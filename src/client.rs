//use std::io::{self, Read, Write};
use std::process;
use std::sync::Mutex;
use std::thread;

use crossbeam::channel::{self, Receiver, Sender};
use log::{debug, error, info};

use mio::{Events, Poll, PollOpt, Ready, Registration, Token};
use serde_json;


#[cfg(feature = "async")]
use crate::async_context::ContextAsync;
use crate::codec::decode_msg;
use crate::conn::{Conn, CONNECTION, State};
#[cfg(feature = "async")]
use futures::executor::LocalPool;
#[cfg(feature = "async")]
use std::future::Future;
//use crate::handler::Handler;
use crate::msgs::{Auth, Cmd, Msg, Nop, NsqCmd, Rdy, Subscribe};
use crate::reader::Consumer;
use crate::config::{Config, NsqdConfig};


use bytes::BytesMut;

//const CONNECTION: Token = Token(19_791_988);

#[derive(Clone, Debug)]
pub(crate) struct CmdChannel(pub Sender<Cmd>, pub Receiver<Cmd>);

impl CmdChannel {
    pub fn new() -> CmdChannel {
        let (cmd_s, cmd_r) = channel::unbounded();
        CmdChannel(cmd_s, cmd_r)
    }
}

#[derive(Clone, Debug)]
//pub(crate) struct MsgChannel(pub Sender<Msg>, pub Receiver<Msg>);
pub(crate) struct MsgChannel(pub Sender<BytesMut>, pub Receiver<BytesMut>);

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

pub struct Client {
    rdy: u32,
    max_attemps: u16,
    channel: String,
    topic: String,
    addr: String,
    config: Config,
    secret: Option<String>,
    msg_channel: MsgChannel,
    cmd_channel: CmdChannel,
    sentinel: Sentinel,
    //    conn: Connection,
}

impl Client {
    pub fn new<S: Into<String>>(
        topic: S,
        channel: S,
        addr: S,
        config: Config,
        secret: Option<String>,
        rdy: u32,
        max_attemps: u16,
    ) -> Client {
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
        }
    }

    pub fn run(&mut self) {
        let (handler, set_readiness) = Registration::new2();
        let r_sentinel = self.sentinel.1.clone();
        thread::spawn(move || loop {
            if let Ok(_ok) = r_sentinel.recv() {
                if let Err(e) = set_readiness.set_readiness(Ready::writable()) {
                    error!("error on handles waker: {}", e);
                }
            }
        });
        let secret = if let Some(secret) = &self.secret {
            secret.clone()
        } else {
            String::new()
        };

        let mut conn = Conn::new(
            self.addr.clone(),
            self.config.clone(),
            self.cmd_channel.1.clone(),
            self.msg_channel.0.clone(),
            #[cfg(feature = "tls")]
            "localhost",
            #[cfg(feature = "tls")]
            false,
        );
        //conn.start();
        let mut poll = Poll::new().unwrap();
        let mut evts = Events::with_capacity(1024);
        conn.register(&mut poll);
        if let Err(e) = poll.register(&handler, Token(1), Ready::writable(), PollOpt::edge()) {
            error!("registering handler");
            panic!("{}", e);
        }
        conn.magic();
        loop {
            if let Err(e) = poll.poll(&mut evts, None) {
                error!("polling events failed");
                panic!("{}", e);
            }
            for ev in &evts {
                debug!("event: {:?}", ev);
                if ev.token() == CONNECTION {
                    if ev.readiness().is_readable() {
                        match conn.read() {
                            Ok(0) => {
                                if conn.need_response {
                                    conn.reregister(&mut poll, Ready::readable());
                                }
                                break;
                            },
                            Err(e) => {
                                if e.kind() != std::io::ErrorKind::WouldBlock {
                                    panic!("Error on reading socket: {:?}", e);
                                }
                                break;
                            },
                            _ => {},
                        };
                        if conn.state != State::Started {
                            match conn.state {
                                State::Identify => {
                                    let resp = conn
                                        .get_response(format!("[{}] failed to indentify", self.addr))
                                        .unwrap();
                                    let nsqd_config: NsqdConfig =
                                        serde_json::from_str(&resp).expect("failed to decode identify response");
                                    info!("[{}] configuration: {:#?}", self.addr, nsqd_config);
                                    if nsqd_config.tls_v1 {
                                        #[cfg(feature = "tls")]
                                        conn.tls_enabled("localhost", false);
                                        let resp = conn
                                            .get_response(format!("[{}] tls handshake failed", self.addr))
                                            .unwrap();
                                        info!("[{}] tls connection: {}", self.addr, resp)
                                    }
                                    if nsqd_config.auth_required {
                                        if secret.is_empty() {
                                            error!("[{}] authentication required", self.addr);
                                            error!("secret token needed");
                                            process::exit(1)
                                        }
                                        conn.state = State::Auth;
                                    } else {
                                        conn.state = State::Subscribe;
                                    }
                                },
                                State::Auth => {
                                    debug!("reading auth");
                                    let resp = conn
                                        .get_response(format!("[{}] authentication failed", self.addr))
                                        .unwrap();
                                    info!("[{}] authentication {}", self.addr, resp);
                                    conn.state = State::Subscribe;

                                },
                                State::Subscribe => {
                                    let resp = conn
                                        .get_response(format!("[{}] authentication failed", self.addr))
                                        .unwrap();
                                    info!(
                                        "[{}] subscribe channel: {} topic: {} {}",
                                        self.addr, self.channel, self.topic, resp
                                    );
                                    conn.state = State::Rdy;
                                },
                                _ => {},
                            }
                            conn.need_response = false;
                        }
                        conn.reregister(&mut poll, Ready::writable());
                    } else {
                        if conn.state != State::Started {
                            match conn.state {
                                State::Identify => {
                                    conn.identify();
                                },
                                State::Auth => {
                                    debug!("writing auth");
                                    conn.auth(secret.clone());
                                },
                                State::Subscribe => {
                                    conn.subscribe(self.topic.clone(), self.channel.clone());
                                },
                                State::Rdy => {
                                    conn.rdy(self.rdy);
                                },
                                _ => {},
                            }
                            if let Err(e) = conn.write() {
                                error!("writing on socket: {:?}", e);
                            };
                            if conn.need_response {
                                conn.reregister(&mut poll, Ready::readable());
                            } else {
                                conn.reregister(&mut poll, Ready::writable());
                            };
                        } else {
                            if conn.heartbeat {
                                conn.write_cmd(Nop);
                                if let Err(e) = conn.write() {
                                    error!("writing on socket: {:?}", e);
                                }
                                conn.heartbeat_done();
                            }
                            conn.write_messages();
                            conn.reregister(&mut poll, Ready::readable());
                        }
                    }
                } else {
                    conn.write_messages();
                }
            }
        }
    }

    #[cfg(not(feature = "async"))]
    pub fn spawn<C: Consumer>(&mut self, n_threads: usize, reader: C) {
        for _i in 0..n_threads {
            let mut boxed = Box::new(reader);
            let cmd = self.cmd_channel.0.clone();
            let msg = self.msg_channel.1.clone();
            let sentinel = self.sentinel.0.clone();
            let max_attemps = self.max_attemps;
            thread::spawn(move || {
                info!("Handler spawned");
                let mut ctx = Context::new(cmd, sentinel);
                loop {
                    if let Ok(ref mut msg) = msg.recv() {
                        let msg = decode_msg(msg);
                        if msg.1 >= max_attemps {
                            boxed.on_max_attemps(
                                Msg {
                                    timestamp: msg.0,
                                    attemps: msg.1,
                                    id: msg.2,
                                    body: msg.3,
                                },
                                &mut ctx,
                            );
                            continue;
                        }
                        boxed.handle(
                            Msg {
                                timestamp: msg.0,
                                attemps: msg.1,
                                id: msg.2,
                                body: msg.3,
                            },
                            &mut ctx,
                        );
                    }
                }
            });
        }
    }

    #[cfg(feature = "async")]
    pub fn spawn<C: Consumer>(&mut self, n_threads: usize, reader: C) {
        for i in 0..n_threads {
            let boxed = Box::new(reader);
            let msg = self.msg_channel.1.clone();
            let cmd = self.cmd_channel.0.clone();
            let sentinel = self.sentinel.0.clone();
            let max_attemps = self.max_attemps;
            thread::spawn(move || {
                let ctx = ContextAsync::new(cmd, sentinel);
                loop {
                    if let Ok(msg) = msg.recv() {
                        LocalPool::new().run_until(boxed.handle(msg, ctx));
                    }
                }
            });
        }
    }
}

#[derive(Debug)]
pub struct Context {
    cmd_s: Sender<Cmd>,
    sentinel: Mutex<Sender<()>>,
    //sentinel: Sender<()>,
}

impl Context {
    fn new(cmd_s: Sender<Cmd>, sentinel: Sender<()>) -> Context {
        Context {
            cmd_s,
            sentinel: Mutex::new(sentinel),
            //sentinel,
        }
    }

    pub fn send<C: NsqCmd>(&mut self, cmd: C) {
        let cmd = cmd.as_cmd();
        let _ = self.cmd_s.send(cmd);
        let _ = self.sentinel.lock().unwrap().send(());
        //let _ = self.sentinel.send(());
        //the sentinel is unlocked where goes out of scope.
    }
}
