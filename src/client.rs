//use std::io::{self, Read, Write};
use std::process;
use std::thread;
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::io::{self, Read, Write};
use mio::net::TcpStream;
use std::fmt::Display;
use std::convert::AsRef;
//use std::time::{Instant, Duration};

use crossbeam::channel::{self, Receiver, Sender};
use crossbeam::atomic::AtomicCell;
use backoff::{backoff::Backoff, ExponentialBackoff};
use log::{debug, error, info};

use mio::{Events, Poll, PollOpt, Ready, Registration, Token};
//use serde_json;

//#[cfg(feature = "async")]
//use crate::async_context::ContextAsync;
use crate::codec::decode_msg;
use crate::conn::{
    Conn,
    //State,
    CONNECTION,
    connect
};
//#[cfg(feature = "async")]
//use futures::executor::LocalPool;
//#[cfg(feature = "async")]
//use std::future::Future;
use crate::config::{
    Config,
    //NsqdConfig,
    ConnConfig
};
use crate::msgs::{
    Cmd,
    Msg,
    //Nop,
    NsqCmd
};
use crate::reader::Consumer;
use crate::producer::Producer;

use bytes::BytesMut;

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

pub struct Client<C, S>
where
    C: Into<String> + Clone,
    S: Into<String> + Clone,
{
    rdy: u32,
    msg_timeout: u32,
    channel: String,
    topic: String,
    addr: String,
    config: Config<C>,
    secret: Option<S>,
    msg_channel: MsgChannel,
    cmd_channel: CmdChannel,
    _conns: HashMap<u32, Sender<Cmd>>,
    sentinel: Sentinel,
    poll: Poll,
    _is_connected: AtomicCell<bool>,
}

impl<C, S> Client<C, S>
where
    C: Into<String> + Clone,
    S: Into<String> + Clone,
{
    pub fn new(
        topic: S,
        channel: S,
        addr: S,
        config: Config<C>,
        secret: Option<S>,
        rdy: u32,
        msg_timeout: u32,
    ) -> Client<C, S> {
        Client {
            topic: topic.into(),
            channel: channel.into(),
            addr: addr.into(),
            config,
            rdy,
            secret,
            msg_timeout,
            msg_channel: MsgChannel::new(),
            cmd_channel: CmdChannel::new(),
            _conns: HashMap::new(),
            sentinel: Sentinel::new(),
            poll: Poll::new().unwrap(),
            _is_connected: AtomicCell::new(false),
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
        let mut socket = connect_stream(self.addr.clone(), self.config.output_buffer_size as usize);
        let mut _stream = Stream::new(&socket);
        let mut conn = Conn::new(
            self.cmd_channel.1.clone(),
            self.msg_channel.0.clone(),
            self.config.verify_server.clone(),
            self.addr.clone(),
            self.config.output_buffer_size as usize,
        );
        let mut poll = Poll::new().unwrap();
        let mut evts = Events::with_capacity(1024);
        conn.register(&mut poll, &socket);
        if let Err(e) = poll.register(&handler, Token(1), Ready::writable(), PollOpt::edge()) {
            error!("registering handler");
            return Err(io::Error::new(io::ErrorKind::Other, e));
        }
        conn.magic();
        let conn_config = ConnConfig::new(self.secret.clone(), self.channel.clone(), self.topic.clone());
        loop {
            if let Err(e) = poll.poll(&mut evts, None) {
                error!("polling events failed");
                return Err(io::Error::new(io::ErrorKind::Other, e));
            }
            for ev in &evts {
                debug!("event: {:?}", ev);
                if ev.token() == CONNECTION {
                    match conn.ready(ev, &mut poll, &mut socket, self.config.clone(), self.rdy, conn_config.clone()) {
                        Ok(_) => continue,
                        Err(e) => {
                            return Err(e);
                        }
                    }
                } else if let Err(e) = conn.write_messages(&mut socket) {
                    return Err(e);
                }
            }
        }
    }

    #[cfg(not(feature = "async"))]
    pub fn spawn<H: Consumer>(&mut self, n_threads: usize, reader: H) {
        for _i in 0..n_threads {
            let mut boxed = Box::new(reader);
            let cmd_chan = self.cmd_channel.0.clone();
            let msg_chan = self.msg_channel.1.clone();
            let sentinel = self.sentinel.0.clone();
            let msg_timeout = self.msg_timeout;
            thread::spawn(move || {
                info!("Handler spawned");
                let mut ctx = Context::new(cmd_chan, sentinel, msg_timeout);
                loop {
                    if let Ok(ref mut msg) = msg_chan.recv() {
                        let msg = decode_msg(msg);
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

    pub fn spawn_producer<H: Producer>(&mut self, n_threads: usize, handler: H) {
        for _i in 0..n_threads {
            let mut boxed = Box::new(handler);
            let cmd_chan = self.cmd_channel.0.clone();
            let sentinel = self.sentinel.0.clone();
            thread::spawn(move || {
                info!("producer spawned");
                let mut ctx = Context::new(cmd_chan, sentinel, 0);
                boxed.send(&mut ctx);
            });
        }
    }
}

// TODO: add AsMut and check if native-tls is compatible.
struct Stream<'a, S>
where
    S: Read + Write,
{
    inner: &'a S,
}

impl<'a, S> Stream<'a, S>
where
    S: Read + Write,
    {
        fn new(socket: &'a S) -> Stream<'a, S> {
            Stream{ inner: socket }
        }
    }

impl<'a, S> AsRef<S> for Stream<'a, S>
where
    S: Read + Write,
    {
        fn as_ref(&self) -> &S {
            self.inner
        }
    }

fn connect_stream<A>(addr: A, output_buffer_size: usize) -> TcpStream
where
    A: ToSocketAddrs + Into<String> + Clone + Display,
{
    let mut addrs = match addr.to_socket_addrs() {
        Ok(addrs) => addrs,
        Err(e) => {
            error!("[{}] error on lookup: {}", addr, e);
            process::exit(1);
        }
    };
    let mut backoff = ExponentialBackoff::default();
    loop {
        let addr = addrs.next().expect("could not resove addr");
        match connect(addr) {
            Ok(stream) => {
                if let Err(e) = stream.set_recv_buffer_size(output_buffer_size)
                {
                    panic!("[{}] error on setting socket buffer size: {:?}", addr, e);
                }
                return stream;
            }
            Err(e) => {
                error!("[{}] error on connect to nsqd: {:?}", addr, e);
                if let Some(timeout) = backoff.next_backoff() {
                    thread::sleep(timeout);
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct Context {
    cmd_s: Sender<Cmd>,
    sentinel: Sender<()>,
    msg_timeout: u32,
}

impl Context {
    fn new(cmd_s: Sender<Cmd>, sentinel: Sender<()>, msg_timeout: u32) -> Context {
        Context {
            cmd_s,
            sentinel,
            msg_timeout,
        }
    }

    pub fn timeout(&self) -> u32 {
        self.msg_timeout
    }

    pub fn send<C: NsqCmd>(&mut self, cmd: C) {
        let cmd = cmd.as_cmd();
        let _ = self.cmd_s.send(cmd);
        let _ = self.sentinel.send(());
    }
}
