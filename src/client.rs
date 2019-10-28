use std::process;
use std::thread;
use std::time::{Duration, Instant};
use std::io;
use std::net::Shutdown;
use std::sync::{Mutex, Arc};
use lazy_static::lazy_static;

use crossbeam::channel::{self, Receiver, Sender};
use log::{debug, error, info, warn};
use native_tls::{TlsConnector, HandshakeError, TlsStream};

use mio::{Events, Poll, PollOpt, Ready, Registration, Token};
use serde_json;

use crate::codec::decode_msg;
use crate::conn::{Conn, State, CONNECTION, connect};
use crate::config::{Config, NsqdConfig};
use crate::msgs::{Cmd, Msg, Nop, NsqCmd, ConnMsg, ConnMsgInfo, ConnInfo, BytesMsg};
use crate::reader::Consumer;

use bytes::BytesMut;
#[cfg(target_os = "windows")]
use std::io::{Read, Write};

const CLIENT_TOKEN: Token = Token(4589);
const CMD_TOKEN: Token = Token(3290);

lazy_static!{
    static ref CONNECTED: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));
}

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

        println!("Creating conn");
        let mut conn = Conn::new(
            self.config.clone(),
            self.cmd_channel.1.clone(),
            self.msg_channel.0.clone(),
            self.out_info.clone(),
            self.msg_timeout.clone(),
        );
        println!("Conn created");
        let mut poll = Poll::new().unwrap();
        let mut evts = Events::with_capacity(1024);
        if let Err(e) = poll.register(&handler, CLIENT_TOKEN, Ready::writable(), PollOpt::edge()) {
            error!("registering handler");
            panic!("{}", e);
        }
        if let Err(e) = poll.register(&cmd_handler, CMD_TOKEN, Ready::all(), PollOpt::edge()) {
            error!("registering handler");
            panic!("{}", e);
        }
        conn.magic();
        let mut nsqd_config: NsqdConfig = NsqdConfig::default();
        let mut last_heartbeat = Instant::now();
        let addr: String = self.addr.clone();
        let mut socket = connect(addr, self.config.output_buffer_size);
        poll.register(&socket, CONNECTION, Ready::writable(), PollOpt::edge());
        let mut tls: u8 = 0;
        loop {
            if tls == 1 {
                let connector = TlsConnector::new().unwrap();
                let addr: String = self.addr.clone().split(':').collect::<Vec<&str>>()[0].to_owned();
                let mut tls_stream = match connector.connect(addr.as_str(), socket) {
                    Ok(s) => s,
                    Err(e) => {
                        match e {
                            HandshakeError::Failure(e) => {
                                error!("error on tls handshake: {}", e);
                                return Err(io::Error::new(io::ErrorKind::Other, e));
                            },
                            HandshakeError::WouldBlock(res) => {
                                warn!("socket would block");
                                thread::sleep(Duration::from_millis(1000));
                                let mut res = res;
                                loop {
                                    warn!("try handshake");
                                    match res.handshake() {
                                        Ok(s) => break s,
                                        Err(e) => {
                                            match e {
                                                HandshakeError::Failure(e) => {
                                                    error!("error on tls handshake: {}", e);
                                                    return Err(io::Error::new(io::ErrorKind::Other, e));
                                                },
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
                    }
                };
                poll.reregister(tls_stream.get_ref(), CONNECTION, Ready::readable(), PollOpt::edge());
                loop {
                    //if let Err(e) = poll.poll(&mut evts, Some(Duration::new(45, 0))) {
                    if let Err(e) = poll.poll(&mut evts, None) {
                        error!("polling tls events failed");
                        panic!("{}", e); 
                    }
                    for ev in &evts {
                        debug!("event: {:?}", ev);
                        if ev.token() == CMD_TOKEN {
                            if let Ok(msg) = r_close.try_recv() {
                                match msg {
                                    1 => {
                                        match tls_stream.shutdown() {
                                            Ok(_) => debug!("TLS Connection Closed"),
                                            Err(e) => error!("Error on TLS Closing: {:?}", e),
                                        }
                                        match self.msg_channel.0.send(BytesMsg(0, BytesMut::new())) {
                                            Ok(_) => debug!("Disconnet message sent to agent"),
                                            Err(e) => error!("Error sending closing message: {:?}", e),
                                        }
                                        poll.reregister(&cmd_handler, CMD_TOKEN, Ready::all(), PollOpt::edge());
                                        return Ok(());
                                    },
                                    _ => {},
                                }
                            }
                            continue;
                        }
                        if ev.token() == CONNECTION {
                            if ev.readiness().is_readable() {
                                match conn.read(&mut tls_stream) {
                                    Ok(0) => {
                                        if conn.need_response {
                                            poll.reregister(tls_stream.get_ref(), CONNECTION, Ready::readable(), PollOpt::edge());
                                        }
                                        break;
                                    }
                                    Err(e) => {
                                        if e.kind() != std::io::ErrorKind::WouldBlock {
                                            panic!("Error on reading socket: {:?}", e);
                                        }
                                        if let Err(e) = self.out_info.send(ConnMsgInfo::IsConnected(ConnInfo{ connected: false, last_time: 0 })) {
                                            panic!("{}", e);
                                        }
                                        break;
                                    }
                                    _ => {}
                                };
                                if conn.state != State::Started {
                                    match conn.state {
                                        State::Tls => {
                                            let resp = conn
                                                .get_response(format!(
                                                    "[{}] tls handshake failed",
                                                    self.addr
                                                ))
                                                .unwrap();
                                            info!("[{}] tls connection: {}", self.addr, resp);
                                            if nsqd_config.auth_required {
                                                if self.secret.is_none() {
                                                    error!("[{}] authentication required", self.addr);
                                                    error!("secret token needed");
                                                    process::exit(1)
                                                }
                                                conn.state = State::Auth;
                                            } else {
                                                conn.state = State::Subscribe;
                                            }
                                        }
                                        State::Auth => {
                                            let resp = conn
                                                .get_response(format!(
                                                    "[{}] authentication failed",
                                                    self.addr
                                                ))
                                                .unwrap();
                                            info!("[{}] authentication {}", self.addr, resp);
                                            conn.state = State::Subscribe;
                                        }
                                        State::Subscribe => {
                                            let resp = conn
                                                .get_response(format!(
                                                    "[{}] authentication failed",
                                                    self.addr
                                                ))
                                                .unwrap();
                                            info!(
                                                "[{}] subscribe channel: {} topic: {} {}",
                                                self.addr, self.channel, self.topic, resp
                                            );
                                            conn.state = State::Rdy;
                                        }
                                        _ => {}
                                    }
                                    conn.need_response = false;
                                }
                                poll.reregister(tls_stream.get_ref(), CONNECTION, Ready::writable(), PollOpt::edge());
                            } else if conn.state != State::Started {
                                match conn.state {
                                    State::Auth => match &self.secret {
                                        Some(s) => {
                                            let secret = s.clone();
                                            conn.auth(secret.into());
                                        }
                                        None => {}
                                    },
                                    State::Subscribe => {
                                        conn.subscribe(self.topic.clone(), self.channel.clone());
                                    }
                                    State::Rdy => {
                                        conn.rdy(self.rdy);
                                    }
                                    _ => {}
                                }
                                if let Err(e) = conn.write(&mut tls_stream) {
                                    error!("writing on socket: {:?}", e);
                                };
                                if conn.need_response {
                                    poll.reregister(tls_stream.get_ref(), CONNECTION, Ready::readable(), PollOpt::edge());
                                } else {
                                    poll.reregister(tls_stream.get_ref(), CONNECTION, Ready::writable(), PollOpt::edge());
                                };
                            } else {
                                if conn.heartbeat {
                                    last_heartbeat = Instant::now();
                                    println!("heartbeat received");
                                    conn.write_cmd(Nop);
                                    if let Err(e) = conn.write(&mut tls_stream) {
                                        error!("writing on socket: {:?}", e);
                                    }
                                    println!("NOP sent");
                                    conn.heartbeat_done();
                                }
                                conn.write_messages(&mut tls_stream);
                                poll.reregister(tls_stream.get_ref(), CONNECTION, Ready::readable(), PollOpt::edge());
                            }
                        } else {
                            conn.write_messages(&mut tls_stream);
                        }
                    }
                }
            }
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
                    match r_close.try_recv() {
                        Ok(msg) => {
                            match msg {
                                1 => {
                                    let _ = socket.shutdown(Shutdown::Both);
                                    let _ = self.msg_channel.0.send(BytesMsg(0, BytesMut::new()));
                                    poll.reregister(&cmd_handler, CMD_TOKEN, Ready::all(), PollOpt::edge());
                                    return Ok(());
                                },
                                _ => {},
                            }
                        },
                        Err(e) => error!("error on Disconnect: {:?}", e),
                    }
                    continue;
                }
                if ev.token() == CONNECTION {
                    if ev.readiness().is_readable() {
                        match conn.read(&mut socket) {
                            Ok(0) => {
                                if conn.need_response {
                                    poll.reregister(&socket, CONNECTION, Ready::readable(), PollOpt::edge());
                                }
                                break;
                            }
                            Err(e) => {
                                if e.kind() != std::io::ErrorKind::WouldBlock {
                                    panic!("Error on reading socket: {:?}", e);
                                }
                                if let Err(e) = self.out_info.send(ConnMsgInfo::IsConnected(ConnInfo{ connected: false, last_time: 0 })) {
                                    panic!("{}", e);
                                }
                                break;
                            }
                            _ => {}
                        };
                        if conn.state != State::Started {
                            match conn.state {
                                State::Identify => {
                                    let resp = conn
                                        .get_response(format!(
                                            "[{}] failed to indentify",
                                            self.addr
                                        ))
                                        .unwrap();
                                    nsqd_config = serde_json::from_str(&resp)
                                        .expect("failed to decode identify response");
                                    info!("[{}] configuration: {:#?}", self.addr, nsqd_config);
                                    conn.msg_timeout = nsqd_config.msg_timeout;
                                    if nsqd_config.tls_v1 {
                                        conn.tls_enabled(&mut tls);
                                        //#[cfg(target_os = "windows")]
                                        //poll.deregister(&socket);
                                        break;
                                    };
                                    if nsqd_config.auth_required {
                                        if self.secret.is_none() {
                                            error!("[{}] authentication required", self.addr);
                                            error!("secret token needed");
                                            process::exit(1)
                                        }
                                        conn.state = State::Auth;
                                    } else {
                                        conn.state = State::Subscribe;
                                    }
                                }
                                State::Tls => {
                                    let resp = conn
                                        .get_response(format!(
                                            "[{}] tls handshake failed",
                                            self.addr
                                        ))
                                        .unwrap();
                                    info!("[{}] tls connection: {}", self.addr, resp);
                                    if nsqd_config.auth_required {
                                        if self.secret.is_none() {
                                            error!("[{}] authentication required", self.addr);
                                            error!("secret token needed");
                                            process::exit(1)
                                        }
                                        conn.state = State::Auth;
                                    } else {
                                        conn.state = State::Subscribe;
                                    }
                                }
                                State::Auth => {
                                    let resp = conn
                                        .get_response(format!(
                                            "[{}] authentication failed",
                                            self.addr
                                        ))
                                        .unwrap();
                                    info!("[{}] authentication {}", self.addr, resp);
                                    conn.state = State::Subscribe;
                                }
                                State::Subscribe => {
                                    let resp = conn
                                        .get_response(format!(
                                            "[{}] authentication failed",
                                            self.addr
                                        ))
                                        .unwrap();
                                    info!(
                                        "[{}] subscribe channel: {} topic: {} {}",
                                        self.addr, self.channel, self.topic, resp
                                    );
                                    conn.state = State::Rdy;
                                }
                                _ => {}
                            }
                            conn.need_response = false;
                        }
                        poll.reregister(&socket, CONNECTION, Ready::writable(), PollOpt::edge());
                    } else if conn.state != State::Started {
                        match conn.state {
                            State::Identify => {
                                conn.identify();
                            }
                            State::Auth => match &self.secret {
                                Some(s) => {
                                    let secret = s.clone();
                                    conn.auth(secret.into());
                                }
                                None => {}
                            },
                            State::Subscribe => {
                                conn.subscribe(self.topic.clone(), self.channel.clone());
                            }
                            State::Rdy => {
                                conn.rdy(self.rdy);
                            }
                            _ => {}
                        }
                        if let Err(e) = conn.write(&mut socket) {
                            error!("writing on socket: {:?}", e);
                        };
                        if conn.need_response {
                            poll.reregister(&socket, CONNECTION, Ready::readable(), PollOpt::edge());
                        } else {
                            poll.reregister(&socket, CONNECTION, Ready::writable(), PollOpt::edge());
                        };
                    } else {
                        if conn.heartbeat {
                            last_heartbeat = Instant::now();
                            conn.write_cmd(Nop);
                            if let Err(e) = conn.write(&mut socket) {
                                error!("writing on socket: {:?}", e);
                            }
                            conn.heartbeat_done();
                        }
                        conn.write_messages(&mut socket);
                        poll.reregister(&socket, CONNECTION, Ready::readable(), PollOpt::edge());
                    }
                } else {
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
            let CONNECTED_VAR = CONNECTED.clone();
            thread::spawn(move || {
                let mut ctx = Context::new(cmd, sentinel);
                let lock = &*CONNECTED_VAR;
                info!("Handler spawned");
                loop {
                    let mut connected = lock.lock().unwrap();
                    if *connected == false {
                        debug!("closing thread");
                        break;
                    }
                    if let Ok(ref mut msg) = msg_ch.recv() {
                        if msg.1.len() == 0 {
                            debug!("closing thread");
                            *connected = false;
                            boxed.on_close(&mut ctx);
                            break;
                        };
                        debug!("I'm on loop");
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

