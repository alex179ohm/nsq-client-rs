use std::process;
use std::thread;
use std::time::{Duration, Instant};
use std::io;
use std::net::Shutdown;

use crossbeam::channel::{self, Receiver, Sender};
use log::{debug, error, info, warn};
use native_tls::{TlsConnector, HandshakeError, TlsStream};

use mio::{Events, Poll, PollOpt, Ready, Registration, Token};
use serde_json;

use crate::codec::{decode_msg, Response};
use crate::conn::{Conn, State, CONNECTION, connect};
use crate::config::{Config, NsqdConfig};
use crate::msgs::{Cmd, Msg, Nop, NsqCmd, ConnMsg, ConnMsgInfo, ConnInfo, BytesMsg};
use crate::reader::Consumer;

use bytes::BytesMut;
#[cfg(target_os = "windows")]
use std::io::{Read, Write};

const CLIENT_TOKEN: Token = Token(4589);
const CMD_TOKEN: Token = Token(3290);

#[derive(Clone, Debug)]
pub(crate) struct CmdChannel(pub Sender<Cmd>, pub Receiver<Cmd>);

impl CmdChannel {
    pub fn new() -> CmdChannel {
        let (cmd_s, cmd_r) = channel::unbounded();
        CmdChannel(cmd_s, cmd_r)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MsgChannel(pub Sender<BytesMsg>, pub Receiver<BytesMsg>);

impl MsgChannel {
    pub fn new() -> MsgChannel {
        let (msg_s, msg_r) = channel::unbounded();
        MsgChannel(msg_s, msg_r)
    }
}

pub(crate) struct Sentinel(pub Sender<()>, Receiver<()>);

impl Sentinel {
    fn new() -> Sentinel {
        let (s, r) = channel::unbounded();
        Sentinel(s, r)
    }
}

pub struct Client<S>
where
    S: Into<String> + Clone,
{
    rdy: u32,
    max_attemps: u16,
    channel: String,
    topic: String,
    addr: String,
    config: Config,
    secret: Option<S>,
    msg_channel: MsgChannel,
    cmd_channel: CmdChannel,
    sentinel: Sentinel,
    in_cmd: Receiver<ConnMsg>,
    out_info: Sender<ConnMsgInfo>,
    connected_s: Sender<bool>,
    connected_r: Receiver<bool>,
    msg_timeout: u64,
}

impl<S> Client<S>
where
    S: Into<String> + Clone,
{
    pub fn new(
        topic: S,
        channel: S,
        addr: S,
        config: Config,
        secret: Option<S>,
        rdy: u32,
        max_attemps: u16,
        in_cmd: Receiver<ConnMsg>,
        out_info: Sender<ConnMsgInfo>,
    ) -> Client<S> {
        let (s, r): (Sender<bool>, Receiver<bool>) = channel::unbounded();
        Client {
            topic: topic.into(),
            channel: channel.into(),
            addr: addr.into(),
            config,
            rdy,
            secret,
            max_attemps,
            msg_channel: MsgChannel::new(),
            cmd_channel: CmdChannel::new(),
            sentinel: Sentinel::new(),
            in_cmd,
            out_info,
            connected_s: s,
            connected_r: r,
            msg_timeout: 0,
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        let (handler, set_readiness) = Registration::new2();
        let r_sentinel = self.sentinel.1.clone();
        thread::spawn(move || loop {
            if let Ok(_ok) = r_sentinel.recv() {
                if let Err(e) = set_readiness.set_readiness(Ready::writable()) {
                    error!("error on handles waker: {}", e);
                }
            }
        });
        let (cmd_handler, cmd_readiness) = Registration::new2();
        let r_cmd = self.in_cmd.clone();
        let (s_close, r_close): (Sender<u32>, Receiver<u32>) = channel::unbounded();
        thread::spawn(move || loop {
            if let Ok(msg) = r_cmd.recv() {
                println!("connection msg received: {:?}", msg);
                let _ = s_close.send(1);
                cmd_readiness.set_readiness(Ready::readable());
            } 
        });

        let mut conn = Conn::new(
            self.config.clone(),
            self.cmd_channel.1.clone(),
            self.msg_channel.0.clone(),
            self.out_info.clone(),
            self.msg_timeout.clone(),
        );
        let mut poll = Poll::new().unwrap();
        if let Err(e) = poll.register(&handler, CLIENT_TOKEN, Ready::writable(), PollOpt::edge()) {
            error!("registering handler");
            panic!("{}", e);
        }
        if let Err(e) = poll.register(&cmd_handler, CMD_TOKEN, Ready::all(), PollOpt::edge()) {
            error!("registering handler");
            panic!("{}", e);
        }
        let mut nsqd_config: NsqdConfig = NsqdConfig::default();
        let mut last_heartbeat = Instant::now();
        let addr: String = self.addr.clone();
        let mut socket = connect(addr.clone(), self.config.output_buffer_size);
        conn.magic(&mut socket);
        conn.identify(&mut socket);
        if let Err(e) = conn.sync_read(&mut socket) {
            return Err(e);
        }
        let resp = match conn.get_response() {
            Response::Response(s) => s,
            Response::Error(s) => {
                panic!("Error on identify: {}", s);
            }
        };
        nsqd_config = serde_json::from_str(&resp).expect("failed to decode server configuration");
        if nsqd_config.tls_v1 {
            let connector = TlsConnector::new().unwrap();
            let mut tls_stream: TlsStream<mio::net::TcpStream> = match connector.connect(&addr, socket) {
                Err(res) => {
                    match res {
                        HandshakeError::Failure(e) => return Err(io::Error::new(io::ErrorKind::Other, format!("tls handshake failed: {}", e))),
                        HandshakeError::WouldBlock(res) => {
                            warn!("socket would block");
                            thread::sleep(Duration::from_millis(1000));
                            let mut res = res;
                            loop {
                                match res.handshake() {
                                    Ok(s) => break s,
                                    Err(s) => {
                                        match s {
                                            HandshakeError::Failure(e) => return Err(io::Error::new(io::ErrorKind::Other, format!("tls handshake failed: {}", e))),
                                            HandshakeError::WouldBlock(r) => {
                                                warn!("socket would block");
                                                thread::sleep(Duration::from_millis(1000));
                                                res = r;
                                                continue;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Ok(s) => s,
            };
            if let Err(e) = conn.sync_read(&mut tls_stream) {
                return Err(e);
            }
            if let Response::Response(s) = conn.get_response() {
                info!("tls handshake: {}", s);
            }
            if nsqd_config.auth_required {
                if self.secret.is_none() {
                    panic!("Auth secret required");
                }
                if let Some(s) = &self.secret {
                    conn.auth(s.clone().into(), &mut tls_stream);
                }
                if let Err(e) = conn.sync_read(&mut tls_stream) {
                    return Err(e);
                }
                match conn.get_response() {
                    Response::Response(s) => info!("auth: {}", s),
                    Response::Error(s) => panic!("auth failed: {}", s),
                }
            }
            if let Err(e) = conn.subscribe(self.topic.clone(), self.channel.clone(), &mut tls_stream) {
                return Err(e);
            }
            if let Err(e) = conn.sync_read(&mut tls_stream) {
                return Err(e);
            }
            match conn.get_response() {
                Response::Response(s) => info!("subscribe: {} {} {}", self.topic.clone(), self.channel.clone(), s),
                Response::Error(s) => panic!("subscribe failed: {} {} {}", self.topic.clone(), self.channel.clone(), s),
            }
            if let Err(e) = conn.rdy(self.rdy, &mut tls_stream) {
                return Err(e);
            }
            if let Err(e) = poll.register(tls_stream.get_ref(), CONNECTION, Ready::all(), PollOpt::edge()) {
                return Err(io::Error::new(io::ErrorKind::Other, "failed to register tls stream"));
            }
            loop {
                let mut evts = Events::with_capacity(1024);
                if let Err(e) = poll.poll(&mut evts, Some(Duration::new(45, 0))) {
                    panic!("polling events failed: {}", e);
                }
                if last_heartbeat.elapsed() > Duration::new(45, 0) {
                    // send fake message as closed connection event.
                    let _ = self.msg_channel.0.send(BytesMsg(0, BytesMut::new()));
                }
                for ev in evts {
                    if ev.token() == CMD_TOKEN {
                        if let Ok(msg) = r_close.try_recv() {
                            match msg {
                                1 => {
                                    let _ = tls_stream.shutdown();
                                    let _ = self.msg_channel.0.send(BytesMsg(0, BytesMut::new()));
                                    poll.reregister(&cmd_handler, CMD_TOKEN, Ready::all(), PollOpt::edge());
                                    return Ok(());
                                },
                                _ => {},
                            }
                        }
                        continue;
                    }
                    if ev.token() == CONNECTION {
                        if ev.readiness() == Ready::readable() {
                            if conn.heartbeat {
                                last_heartbeat = Instant::now();
                                conn.write_cmd(Nop);
                                if let Err(e) = conn.write(&mut tls_stream) {
                                    error!("writing on socket: {:?}", e);
                                }
                                conn.heartbeat_done();
                            }
                            if let Err(e) = conn.read(&mut tls_stream) {
                                return Err(e);
                            }
                            if let Err(e) = poll.reregister(tls_stream.get_ref(), CONNECTION, Ready::all(), PollOpt::edge()) {
                                panic!("error on reregister tls stream: {}", e);
                            }
                        } else {
                            conn.write_messages(&mut tls_stream);
                            if let Err(e) = poll.reregister(tls_stream.get_ref(), CONNECTION, Ready::all(), PollOpt::edge()) {
                                panic!("error on reregister tls stream: {}", e);
                            }
                        }
                    }
                }
            }
        } else {
            loop {
                let mut evts = Events::with_capacity(1024);
                if let Err(e) = poll.poll(&mut evts, Some(Duration::new(45, 0))) {
                    error!("polling events failed");
                    panic!("{}", e);
                }
                // if last_heartbeat is not seen shutdown occurred.
                if last_heartbeat.elapsed() > Duration::new(45, 0) {
                    // send fake message as closed connection event.
                    let _ = self.msg_channel.0.send(BytesMsg(0, BytesMut::new()));
                }
                for ev in &evts {
                    debug!("event: {:?}", ev);
                    if ev.token() == CMD_TOKEN {
                        if let Ok(msg) = r_close.try_recv() {
                            match msg {
                                1 => {
                                    let _ = socket.shutdown(Shutdown::Both);
                                    let _ = self.msg_channel.0.send(BytesMsg(0, BytesMut::new()));
                                    poll.reregister(&cmd_handler, CMD_TOKEN, Ready::all(), PollOpt::edge());
                                    return Ok(());
                                },
                                _ => {},
                            }
                        }
                        continue;
                    }
                    if conn.heartbeat {
                        last_heartbeat = Instant::now();
                        conn.write_cmd(Nop);
                        if let Err(e) = conn.write(&mut socket) {
                            error!("writing on socket: {:?}", e);
                        }
                        conn.heartbeat_done();
                    }
                    conn.write_messages(&mut socket);
                }
            }

        }
    }

    #[cfg(not(feature = "async"))]
    pub fn spawn<H: Consumer>(&mut self, n_threads: usize, reader: H) {
        for _i in 0..n_threads {
            let mut boxed = Box::new(reader.clone());
            let cmd = self.cmd_channel.0.clone();
            let msg_ch = self.msg_channel.1.clone();
            let sentinel = self.sentinel.0.clone();
            let max_attemps = self.max_attemps;
            let conn_s = self.connected_r.clone();
            thread::spawn(move || {
                let mut ctx = Context::new(cmd, sentinel);
                info!("Handler spawned");
                loop {
                    if let Ok(ref mut msg) = msg_ch.recv() {
                        if msg.1.len() == 0 {
                            boxed.on_close(&mut ctx);
                            continue;
                        };
                        let timeout = msg.0;
                        let msg = decode_msg(&mut msg.1);
                        boxed.on_msg(Msg {
                            timeout,
                            timestamp: msg.0,
                            attemps: msg.1,
                            id: msg.2,
                            body: msg.3,
                        }, &mut ctx);
                    }
                }
            });
        }
    }
}

#[derive(Debug, Clone)]
pub struct Context {
    cmd_s: Sender<Cmd>,
    sentinel: Sender<()>,
}

impl Context {
    fn new(cmd_s: Sender<Cmd>, sentinel: Sender<()>) -> Context {
        Context {
            cmd_s,
            sentinel: sentinel,
        }
    }

    pub fn send<C: NsqCmd>(&mut self, cmd: C) {
        let cmd = cmd.as_cmd();
        let _ = self.cmd_s.send(cmd);
        let _ = self.sentinel.send(());
    }
}

