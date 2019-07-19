use crate::codec::{
    write_cmd, write_magic, write_mmsg, write_msg, Response, FRAME_TYPE_ERROR, FRAME_TYPE_MESSAGE,
    FRAME_TYPE_RESPONSE, HEADER_LENGTH, HEARTBEAT,
};
use crate::config::Config;
use crate::msgs::{Auth, Cmd, Identify, NsqCmd, Rdy, Subscribe, VERSION, ConnMsgInfo, ConnInfo};
use crate::tls::TlsSession;
use backoff::{backoff::Backoff, ExponentialBackoff};
use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use crossbeam::channel::{Receiver, Sender};
use log::{debug, error, info};
use mio::{net::TcpStream, Poll, PollOpt, Ready, Token};
use rustls::Session;
use std::fmt::Display;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::process;
use std::thread::{self, Thread};
use std::net::Shutdown;
use std::sync::{Arc, atomic::{Ordering, AtomicBool}};
use chrono::{DateTime, Utc};

pub const CONNECTION: Token = Token(0);

#[derive(Debug, PartialEq)]
pub enum State {
    Start,
    Identify,
    Tls,
    Auth,
    Subscribe,
    Rdy,
    Started,
}

#[derive(Debug)]
pub struct Conn<S>
where
    S: Into<String> + Clone,
{
    //writing buffer where commands are written.
    w_buf: BytesMut,
    //read buffer where data is decoded.
    r_buf: BytesMut,
    //send message to readers.
    //s: Sender<Msg>,
    s: Sender<BytesMut>,
    // tcp_stream
    socket: TcpStream,
    //receive Cmd from readers.
    r: Receiver<Cmd>,
    s_info: Sender<ConnMsgInfo>,
    //heartbeat
    pub heartbeat: bool,
    //responses
    pub responses: Vec<Response>,
    //config
    config: Config<S>,
    //msgs in flight
    in_flight: u32,
    //tls_session
    tls_sess: TlsSession,
    //tls connection enabled/disabled (needed because we not start chatting on encrypted connection)
    tls: bool,
    now: std::time::Instant,
    processed: u32,
    pub need_response: bool,
    pub state: State,
    last_time_sent: i64,
    handle: Thread,
}

impl<S> Conn<S>
where
    S: Into<String> + Clone,
{

    pub fn new<A>(addr: A, config: Config<S>, r: Receiver<Cmd>, s: Sender<BytesMut>, s_info: Sender<ConnMsgInfo>) -> Conn<S>
    where
        A: ToSocketAddrs + Into<String> + Display + Clone,
    {
        let server_name: String = addr.clone().into();
        let mut addrs = match addr.to_socket_addrs() {
            Ok(addrs) => addrs,
            Err(e) => {
                error!("[{}] error on lookup: {}", addr, e);
                process::exit(1);
            }
        };
        let mut backoff = ExponentialBackoff::default();
        let socket = loop {
            let addr = addrs.next().expect("could not resove addr");
            match connect(addr) {
                Ok(stream) => {
                    if let Err(e) = stream.set_recv_buffer_size(config.output_buffer_size as usize)
                    {
                        panic!("[{}] error on setting socket buffer size: {:?}", addr, e);
                    }
                    break stream;
                }
                Err(e) => {
                    error!("[{}] error on connect to nsqd: {:?}", addr, e);
                    if let Some(timeout) = backoff.next_backoff() {
                        thread::sleep(timeout);
                    }
                }
            }
        };
        let verify_server_cert = config.verify_server.clone();
        Conn {
            socket,
            r_buf: BytesMut::new(),
            w_buf: BytesMut::new(),
            r,
            s,
            heartbeat: false,
            config,
            responses: Vec::new(),
            tls_sess: TlsSession::new(
                server_name.split(':').collect::<Vec<&str>>()[0],
                verify_server_cert,
            ),
            tls: false,
            in_flight: 0,
            now: std::time::Instant::now(),
            processed: 0,
            need_response: false,
            state: State::Start,
            s_info,
            last_time_sent: 0,
            handle: thread::current(),
        }
    }

    pub fn close(&mut self) -> io::Result<()> {
        let _ = self.socket.shutdown(Shutdown::Both);
        self.s_info.send(ConnMsgInfo::IsConnected(ConnInfo{ connected: false, last_time: 0 }));
        Ok(())
    }

    pub fn magic(&mut self) {
        write_magic(&mut self.w_buf, VERSION);
        self.state = State::Identify;
    }

    pub fn identify(&mut self) {
        let config = serde_json::to_string(&self.config).unwrap();
        self.write_cmd(Identify(config).as_cmd());
        self.state = State::Identify;
        self.need_response = true;
    }

    pub fn auth(&mut self, secret: String) {
        self.write_cmd(Auth(secret));
        self.state = State::Auth;
        self.need_response = true;
    }

    pub fn subscribe(&mut self, topic: String, channel: String) {
        self.write_cmd(Subscribe(topic, channel));
        self.state = State::Subscribe;
        self.need_response = true;
    }

    pub fn rdy(&mut self, rdy: u32) {
        self.write_cmd(Rdy(rdy));
        self.state = State::Started;
        self.need_response = false;
    }

    pub fn tls_enabled(&mut self) {
        self.tls = true;
        debug!("tls enabled");
        let _ = self.tls_sess.0.complete_io(&mut self.socket);
        if self.tls_sess.0.wants_write() {
            let _ = self.write_tls();
        }
        if self.tls_sess.0.wants_read() {
            let _ = self.read_tls();
        }
        self.state = State::Tls;
    }

    pub fn register(&mut self, poll: &mut Poll) {
        poll.register(&self.socket, CONNECTION, Ready::writable(), PollOpt::edge())
            .expect("cannot register socket on poll");
    }

    pub fn reregister(&mut self, poll: &mut Poll, interest: Ready) {
        poll.reregister(&self.socket, CONNECTION, interest, PollOpt::edge())
            .expect("cannot reregister socket on poll")
    }

    pub fn get_response(&mut self, on_err: String) -> Result<String, ()> {
        //self.poll_response();
        get_response(self.responses.pop().unwrap(), on_err)
    }

    pub fn heartbeat_done(&mut self) {
        self.heartbeat = false;
    }

    pub fn read(&mut self) -> io::Result<usize> {
        if self.tls {
            self.read_tls()
        } else {
            self.read_tcp()
        }
    }

    pub fn write(&mut self) -> io::Result<usize> {
        if self.tls {
            self.write_tls()
        } else {
            self.write_tcp()
        }
    }

    pub fn write_messages(&mut self) {
        let msgs: Vec<Cmd> = self.r.try_iter().collect();
        for msg in msgs {
            let now: DateTime<Utc> = Utc::now();
            self.write_cmd(msg);
            if let Err(e) = self.write() {
                error!("error writing msg on socket: {:?}", e);
            };
            if let Err(e) = self.socket.flush() {
                error!("error flushing socket: {:?}", e);
            };
            self.last_time_sent = now.timestamp();
            self.in_flight -= 1;
            self.processed += 1;
        }
        info!("inflight: {}", self.in_flight);
        info!("processed {}", self.processed);
    }

    pub fn decode(&mut self, size: usize) {
        loop {
            //buffer totally consumed
            if self.r_buf.is_empty() {
                return;
            }
            //readed socket bytes are not enought
            if size < HEADER_LENGTH {
                return;
            }
            let buf_len = self.r_buf.len();
            if buf_len < HEADER_LENGTH {
                return;
            }
            //read and check the frame size.
            let frame_size = BigEndian::read_i32(&self.r_buf.as_ref()[..4]) as usize;
            if size < frame_size {
                return;
            }
            if buf_len < frame_size {
                return;
            }
            //there is no more bytes to read for socket, split size and start decoding frame.
            let _ = self.r_buf.split_to(4);
            let frame_type = BigEndian::read_i32(&self.r_buf.split_to(4));
            //take the whole frame for buffer.
            let frame = self.r_buf.split_to(frame_size - 4);
            if frame_type == FRAME_TYPE_MESSAGE {
                let _ = self.s.send(frame);
                self.in_flight += 1;
                continue;
            } else {
                let s = std::str::from_utf8(frame.as_ref()).unwrap();
                if s == HEARTBEAT {
                    self.heartbeat = true;
                    continue;
                }
                if frame_type == FRAME_TYPE_RESPONSE {
                    self.responses.push(Response::Response(s.to_owned()));
                } else if frame_type == FRAME_TYPE_ERROR {
                    self.responses.push(Response::Error(s.to_owned()));
                }
            }
        }
    }

    pub fn read_tls(&mut self) -> io::Result<usize> {
        if self.tls_sess.0.is_handshaking() {
            self.tls_sess.0.complete_io(&mut self.socket)?;
        }
        if self.tls_sess.0.wants_write() {
            self.tls_sess.0.complete_io(&mut self.socket)?;
        }
        while self.tls_sess.0.wants_read() && self.tls_sess.0.complete_io(&mut self.socket)?.0 != 0
        {
        }
        let mut buf: Vec<u8> = Vec::new();
        buf.resize(self.config.output_buffer_size as usize, 0);
        //let mut n: usize = 0;
        match self.tls_sess.0.read(&mut buf) {
            Ok(0) => Ok(0),
            Ok(b) => {
                debug!("read: {}", b);
                self.r_buf.extend_from_slice(&buf.as_slice()[..b]);
                self.decode(b);
                //buf.clear();
                Ok(b)
            }
            Err(e) => Err(e),
        }
    }

    pub fn read_tcp(&mut self) -> io::Result<usize> {
        let mut buf: Vec<u8> = Vec::new();
        buf.resize(self.config.output_buffer_size as usize, 0);
        match self.socket.read(&mut buf) {
            Ok(0) => Ok(0),
            Ok(b) => {
                self.r_buf.extend_from_slice(&buf.as_slice()[..b]);
                self.decode(b);
                //buf.clear();
                Ok(b)
            }
            Err(e) => {
                //if e.kind() == io::ErrorKind::WouldBlock {
                //    return Ok(n);
                //}
                Err(e)
            }
        }
    }

    pub fn write_cmd<C: NsqCmd>(&mut self, msg: C) {
        let msg = msg.as_cmd();
        debug!("{:?}", msg);
        write_cmd(&mut self.w_buf, &msg.cmd);
        if msg.msg.is_empty() {
            return;
        }
        if msg.msg.len() == 1 {
            write_msg(&mut self.w_buf, &msg.msg[0]);
            return;
        }
        write_mmsg(&mut self.w_buf, &msg.msg);
    }

    pub fn write_tls(&mut self) -> io::Result<usize> {
        if self.tls_sess.0.is_handshaking() {
            self.tls_sess.0.complete_io(&mut self.socket)?;
        }
        if self.tls_sess.0.wants_write() {
            self.tls_sess.0.complete_io(&mut self.socket)?;
        }
        let mut n: usize = 0;
        match self.tls_sess.0.write(self.w_buf.as_mut()) {
            Ok(0) => {
                self.w_buf.clear();
            }
            Ok(b) => {
                self.w_buf.clear();
                n = b;
            }
            Err(e) => {
                error!("writing on tls socket");
                self.w_buf.clear();
                return Err(e);
            }
        }
        let _ = self.tls_sess.0.complete_io(&mut self.socket);
        Ok(n)
    }

    pub fn write_tcp(&mut self) -> io::Result<usize> {
        match self.socket.write(self.w_buf.as_ref()) {
            Ok(0) => {
                self.w_buf.clear();
                Ok(0)
            }
            Ok(n) => {
                self.w_buf.clear();
                Ok(n)
            }
            Err(e) => {
                self.w_buf.clear();
                Err(e)
            }
        }
    }
}

pub fn connect(addr: SocketAddr) -> std::io::Result<TcpStream> {
    let tcpstream = if cfg!(windows) {
        let tcp_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 4150);
        debug!("{:?}", tcp_addr);
        net2::TcpBuilder::new_v4()
            .unwrap()
            .bind(tcp_addr)
            .expect("failed to create and bind tcp stream")
            .to_tcp_stream()
            .unwrap()
    } else {
        net2::TcpBuilder::new_v4()
            .expect("failed to create tcp stream")
            .to_tcp_stream()
            .unwrap()
    };
    info!("[{}] trying to connect to nsqd server", addr);
    TcpStream::connect_stream(tcpstream, &addr)
}

pub fn get_response(resp: Response, expect: String) -> Result<String, ()> {
    match resp {
        Response::Response(r) => Ok(r),
        Response::Error(e) => {
            error!("{}", expect);
            error!("error on response: {}", e);
            Err(())
        }
    }
}
