use crate::codec::{
    write_cmd, write_magic, write_mmsg, write_msg, Response, FRAME_TYPE_ERROR, FRAME_TYPE_MESSAGE,
    FRAME_TYPE_RESPONSE, HEADER_LENGTH, HEARTBEAT,
};
use crate::config::{VerifyServerCert, NsqdConfig, Config, ConnConfig};
use crate::msgs::{Auth, Cmd, Identify, NsqCmd, Rdy, Subscribe, VERSION, Nop};
use crate::tls::rustls::TlsSession;
//use backoff::{backoff::Backoff, ExponentialBackoff};
use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use crossbeam::channel::{Receiver, Sender};
use log::{debug, error, info};
use mio::{net::TcpStream, Poll, PollOpt, Ready, Token, Evented, Event};
use rustls::Session;
//use std::fmt::Display;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[cfg(feature = "nativetls")]
use native_tls::{TlsConnector, TlsStream};

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
pub struct Conn {
    //writing buffer where commands are written.
    w_buf: BytesMut,
    //read buffer where data is decoded.
    r_buf: BytesMut,
    //send message to readers.
    //s: Sender<Msg>,
    s: Sender<BytesMut>,
    buffer_size: usize,
    //receive Cmd from readers.
    r: Receiver<Cmd>,
    //heartbeat
    pub heartbeat: bool,
    //responses
    pub responses: Vec<Response>,
    server_name: String,
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
}

impl Conn {

    pub fn new<ST>(
        r: Receiver<Cmd>,
        s: Sender<BytesMut>,
        verify_server_cert: VerifyServerCert<ST>,
        server_name: String,
        buffer_size: usize,
        ) -> Conn
    where
        ST: Into<String> + Clone,
    {
       let server_name = &server_name.split(':').map(|x| x.to_owned()).collect::<Vec<String>>()[0]; 
        Conn {
            r_buf: BytesMut::new(),
            w_buf: BytesMut::new(),
            r,
            s,
            heartbeat: false,
            responses: Vec::new(),
            tls_sess: TlsSession::new(
                server_name.as_str(),
                verify_server_cert,
            ),
            server_name: server_name.to_owned(),
            buffer_size,
            tls: false,
            in_flight: 0,
            now: std::time::Instant::now(),
            processed: 0,
            need_response: false,
            state: State::Start,
        }
    }

    pub fn ready<S, C, H>(
        &mut self,
        ev: Event,
        poll: &mut Poll,
        socket: &mut S,
        config: Config<C>,
        rdy: u32,
        conn_config: ConnConfig<H>,
        ) -> io::Result<()>
    where
        S: Read + Write + Evented,
        C: Into<String> + Clone,
        H: Into<String> + Clone,
    {
        let mut nsqd_config: NsqdConfig = NsqdConfig::default();
        if ev.readiness().is_readable() {
            match self.read(socket) {
                Ok(0) => {
                    if self.need_response {
                        self.reregister(poll, socket, Ready::readable());
                    }
                    return Ok(());
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::WouldBlock {
                        return Err(e);
                    }
                    return Ok(());
                }
                _ => {}
            };
            if self.state != State::Started {
                match self.state {
                    State::Identify => {
                        let resp = match self.get_response() {
                            Ok(r) => r,
                            Err(e) => {
                                error!("[{}] failed to identify", self.server_name);
                                return Err(io::Error::new(io::ErrorKind::Other, e));
                            }
                        };
                        nsqd_config = serde_json::from_str(&resp)
                            .expect("failed to decode identify response");
                        info!("[{}] configuration: {:#?}", self.server_name, nsqd_config);
                        if nsqd_config.tls_v1 {
                            self.tls_enabled(socket);
                            self.reregister(poll, socket, Ready::readable());
                            return Ok(());
                        };
                        if nsqd_config.auth_required {
                            if conn_config.secret.is_none() {
                                error!("[{}] authentication required", self.server_name);
                                error!("secret token needed");
                                return Err(io::Error::new(io::ErrorKind::Other, "Secret token required"));
                            }
                            self.state = State::Auth;
                        } else {
                            self.state = State::Subscribe;
                        }
                    }
                    State::Tls => {
                        let resp = match self.get_response() {
                            Ok(r) => r,
                            Err(e) => {
                                error!("[{}] {}", self.server_name, e);
                                return Err(io::Error::new(io::ErrorKind::Other, e));
                            }
                        };
                        info!("[{}] tls connection: {}", self.server_name, resp);
                        if nsqd_config.auth_required {
                            if conn_config.secret.is_none() {
                                error!("[{}] authentication required", self.server_name);
                                return Err(io::Error::new(io::ErrorKind::Other, "secret token required"));
                            }
                            self.state = State::Auth;
                        } else {
                            self.state = State::Subscribe;
                        }
                    }
                    State::Auth => {
                        let resp = match self.get_response() {
                            Ok(r) => r,
                            Err(e) => {
                                error!("[{}] Authentication failed", self.server_name);
                                return Err(io::Error::new(io::ErrorKind::Other, e));
                            }
                        };
                        info!("[{}] authentication {}", self.server_name, resp);
                        self.state = State::Subscribe;
                    }
                    State::Subscribe => {
                        let resp = match self.get_response() {
                            Ok(r) => r,
                            Err(e) => {
                                error!("[{}] {}", self.server_name, e);
                                return Err(io::Error::new(io::ErrorKind::Other, e));
                            }
                        };
                        info!(
                            "[{}] subscribe: {}",
                            self.server_name, resp
                        );
                        self.state = State::Rdy;
                    }
                    _ => {}
                }
                self.need_response = false;
            }
            self.reregister(poll, socket, Ready::writable());
        } else if self.state != State::Started {
            match self.state {
                State::Identify => {
                    let config = serde_json::to_string(&config).unwrap();
                    self.identify(config);
                }
                State::Auth => match &conn_config.secret {
                    Some(s) => {
                        let secret = s.clone();
                        self.auth(secret.into());
                    }
                    None => {}
                },
                State::Subscribe => {
                    self.subscribe(conn_config.topic, conn_config.channel);
                }
                State::Rdy => {
                    self.rdy(rdy);
                }
                _ => {}
            }
            if let Err(e) = self.write(socket) {
                error!("writing on socket: {:?}", e);
            };
            if self.need_response {
                self.reregister(poll, socket, Ready::readable());
            } else {
                self.reregister(poll, socket, Ready::writable());
            };
        } else {
            if self.heartbeat {
                self.write_cmd(Nop);
                if let Err(e) = self.write(socket) {
                    error!("writing on socket: {:?}", e);
                    return Err(e);
                }
                self.heartbeat_done();
            }
            if let Err(e) = self.write_messages(socket) {
                return Err(e);
            }
            self.reregister(poll, socket, Ready::readable());
        }
        Ok(())
    }

    pub fn magic(&mut self) {
        write_magic(&mut self.w_buf, VERSION);
        self.state = State::Identify;
    }

    pub fn identify(&mut self, config: String) {
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

    pub fn tls_enabled<S: Read + Write>(&mut self, socket: &mut S) {
        self.tls = true;
        debug!("tls enabled");
        let _ = self.tls_sess.0.complete_io(socket);
        if self.tls_sess.0.wants_write() {
            let _ = self.write_tls(socket);
        }
        if self.tls_sess.0.wants_read() {
            let _ = self.read_tls(socket);
        }
        self.state = State::Tls;
    }

    pub fn register<S: Evented>(&mut self, poll: &mut Poll, socket: &S) {
        poll.register(socket, CONNECTION, Ready::writable(), PollOpt::edge())
            .expect("cannot register socket on poll");
    }

    pub fn reregister<S: Evented>(&mut self, poll: &mut Poll, socket: &S, interest: Ready) {
        poll.reregister(socket, CONNECTION, interest, PollOpt::edge())
            .expect("cannot reregister socket on poll")
    }

    pub fn get_response(&mut self) -> io::Result<String> {
        get_response(self.responses.pop().unwrap())
    }

    pub fn heartbeat_done(&mut self) {
        self.heartbeat = false;
    }

    pub fn read<S: Read + Write>(&mut self, socket: &mut S) -> io::Result<usize> {
        if self.tls {
            self.read_tls(socket)
        } else {
            self.read_tcp(socket)
        }
    }

    pub fn write<S: Read + Write>(&mut self, socket: &mut S) -> io::Result<usize> {
        if self.tls {
            self.write_tls(socket)
        } else {
            self.write_tcp(socket)
        }
    }

    pub fn write_messages<S: Read + Write>(&mut self, socket: &mut S) -> io::Result<()> {
        let msgs: Vec<Cmd> = self.r.try_iter().collect();
        for msg in msgs {
            self.write_cmd(msg);
            if let Err(e) = self.write(socket) {
                error!("error writing msg on socket: {:?}", e);
                return Err(e);
            };
            if let Err(e) = socket.flush() {
                error!("error flushing socket: {:?}", e);
                return Err(e);
            };
            if self.in_flight != 0 {
                self.in_flight -= 1;
            }
            self.processed += 1;
        }
        info!("inflight: {}", self.in_flight);
        info!("processed {}", self.processed);
        //if self.processed == 2000 {
        //    info!("time: {:?}", std::time::Instant::now().duration_since(self.now));
        //    std::process::exit(0);
        //}
        Ok(())
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

    pub fn read_tls<S: Read + Write>(&mut self, mut socket: S) -> io::Result<usize> {
        if self.tls_sess.0.is_handshaking() {
            self.tls_sess.0.complete_io(&mut socket)?;
        }
        if self.tls_sess.0.wants_write() {
            self.tls_sess.0.complete_io(&mut socket)?;
        }
        while self.tls_sess.0.wants_read() && self.tls_sess.0.complete_io(&mut socket)?.0 != 0
        {
        }
        let mut buf: Vec<u8> = Vec::new();
        buf.resize(self.buffer_size, 0);
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

    pub fn read_tcp<S: Read + Write>(&mut self, socket: &mut S) -> io::Result<usize> {
        let mut buf: Vec<u8> = Vec::new();
        buf.resize(self.buffer_size, 0);
        match socket.read(&mut buf) {
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

    pub fn write_cmd<CMD: NsqCmd>(&mut self, msg: CMD) {
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

    pub fn write_tls<S: Read + Write>(&mut self, socket: &mut S) -> io::Result<usize> {
        if self.tls_sess.0.is_handshaking() {
            self.tls_sess.0.complete_io(socket)?;
        }
        if self.tls_sess.0.wants_write() {
            self.tls_sess.0.complete_io(socket)?;
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
        let _ = self.tls_sess.0.complete_io(socket);
        Ok(n)
    }

    pub fn write_tcp<S: Read + Write>(&mut self, socket: &mut S) -> io::Result<usize> {
        match socket.write(self.w_buf.as_ref()) {
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

pub fn get_response(resp: Response) -> io::Result<String> {
    match resp {
        Response::Response(r) => Ok(r),
        Response::Error(e) => {
            error!("error on response: {}", e);
            Err(io::Error::new(io::ErrorKind::Other, e))
        }
    }
}
