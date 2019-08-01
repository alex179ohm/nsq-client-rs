use crate::codec::{
    write_cmd, write_magic, write_mmsg, write_msg, Response, FRAME_TYPE_ERROR, FRAME_TYPE_MESSAGE,
    FRAME_TYPE_RESPONSE, HEADER_LENGTH, HEARTBEAT,
};
use crate::config::Config;
use crate::msgs::{Auth, Cmd, Identify, NsqCmd, Rdy, Subscribe, VERSION, ConnMsgInfo, ConnInfo, BytesMsg};
//use crate::tls::TlsSession;
use backoff::{backoff::Backoff, ExponentialBackoff};
use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use crossbeam::channel::{Receiver, Sender};
use log::{debug, error, info};
use mio::{net::TcpStream, Poll, PollOpt, Ready, Token};
use std::fmt::Display;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::process;
use std::thread::{self, Thread};
//use std::sync::{Arc, atomic::{Ordering, AtomicBool}};
use chrono::{DateTime, Utc};
use net2::TcpStreamExt;

pub const CONNECTION: Token = Token(5067);

#[derive(Debug, PartialEq)]
pub enum State {
    Start,
    Identify,
    TlsNegotiating,
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
    s: Sender<BytesMsg>,
    // tcp_stream
    //receive Cmd from readers.
    r: Receiver<Cmd>,
    s_info: Sender<ConnMsgInfo>,
    //heartbeat
    pub heartbeat: bool,
    //responses
    pub responses: Vec<Response>,
    //config
    config: Config,
    //msgs in flight
    in_flight: u32,
    now: std::time::Instant,
    processed: u32,
    pub need_response: bool,
    pub state: State,
    last_time_sent: i64,
    handle: Thread,
    pub msg_timeout: u64,
}

impl Conn {

    pub fn new(config: Config, r: Receiver<Cmd>, s: Sender<BytesMsg>, s_info: Sender<ConnMsgInfo>, msg_timeout: u64) -> Conn {
        Conn {
            r_buf: BytesMut::new(),
            w_buf: BytesMut::new(),
            r,
            s,
            heartbeat: false,
            config,
            responses: Vec::new(),
            in_flight: 0,
            now: std::time::Instant::now(),
            processed: 0,
            need_response: false,
            state: State::Start,
            s_info,
            last_time_sent: 0,
            handle: thread::current(),
            msg_timeout: 0,
        }
    }

//    pub fn close(&mut self) -> io::Result<()> {
//        let _ = self.socket.shutdown(Shutdown::Both);
//        Ok(())
//    }

    pub fn magic<STREAM: Read + Write>(&mut self, s: &mut STREAM) -> io::Result<usize> {
        write_magic(&mut self.w_buf, VERSION);
        self.sync_write(s)
    }

    pub fn identify<STREAM: Read + Write>(&mut self, s: &mut STREAM) -> io::Result<usize> {
        let config = serde_json::to_string(&self.config).unwrap();
        self.write_cmd(Identify(config).as_cmd());
        self.sync_write(s)
    }

    pub fn auth<STREAM: Read + Write>(&mut self, secret: String, s: &mut STREAM) -> io::Result<usize> {
        self.write_cmd(Auth(secret));
        self.sync_write(s)
    }

    pub fn subscribe<STREAM: Read + Write>(&mut self, topic: String, channel: String, s: &mut STREAM) -> io::Result<usize> {
        self.write_cmd(Subscribe(topic, channel));
        self.sync_write(s)
    }

    pub fn rdy<STREAM: Read + Write>(&mut self, rdy: u32, s: &mut STREAM) -> io::Result<usize> {
        self.write_cmd(Rdy(rdy));
        self.sync_write(s)
    }

    pub fn get_response(&mut self) -> Response {
        self.responses.pop().unwrap()
    }

    pub fn heartbeat_done(&mut self) {
        self.heartbeat = false;
    }

    pub fn sync_write<STREAM: Read + Write>(&mut self, s: &mut STREAM) -> io::Result<usize> {
        loop {
            match self.write(s) {
                Ok(0) => Ok(0),
                Ok(n) => Ok(n),
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        continue;
                    }
                    Err(e)
                }
            }
        }
    }

    pub fn sync_read<STREAM: Read + Write>(&mut self, s: &mut STREAM) -> io::Result<usize> {
        loop {
            match self.read_tcp(s) {
                Ok(0) => continue,
                Ok(n) => {
                    self.decode(n);
                    Ok(n)
                },
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        continue;
                    }
                    Err(e)
                },
            };
        }
    }

    pub fn read<STREAM: Read + Write>(&mut self, socket: &mut STREAM) -> io::Result<usize> {
        self.read_tcp(socket)
    }

    pub fn write<STREAM: Read + Write>(&mut self, socket: &mut STREAM) -> io::Result<usize> {
        self.write_tcp(socket)
    }

    pub fn write_messages<STREAM: Read + Write>(&mut self, socket: &mut STREAM) {
        let msgs: Vec<Cmd> = self.r.try_iter().collect();
        for msg in msgs {
            let now: DateTime<Utc> = Utc::now();
            self.write_cmd(msg);
            if let Err(e) = self.write(socket) {
                error!("error writing msg on socket: {:?}", e);
            };
            if let Err(e) = socket.flush() {
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
                let _ = self.s.send(BytesMsg(self.msg_timeout.clone(), frame));
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
                    debug!("error received: {:?}", s.to_owned());
                    self.responses.push(Response::Error(s.to_owned()));
                }
            }
        }
    }

    pub fn read_tcp<STREAM: Read + Write>(&mut self, socket: &mut STREAM) -> io::Result<usize> {
        let mut buf: Vec<u8> = Vec::new();
        buf.resize(self.config.output_buffer_size as usize, 0);
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

    pub fn write_tcp<STREAM: Read + Write>(&mut self, socket: &mut STREAM) -> io::Result<usize> {
        match socket.write(self.w_buf.as_ref()) {
            Ok(0) => {
                self.w_buf.clear();
                Ok(0)
            }
            Ok(n) => {
                debug!("written: {:?}", self.w_buf);
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

pub fn socket_connect(addr: SocketAddr) -> std::io::Result<TcpStream> {
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

pub fn connect<A>(addr: A, output_buffer_size: u64) -> TcpStream
where
    A: ToSocketAddrs + Display + Clone,
{
//    let server_name: String = addr.clone().into();
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
        match socket_connect(addr) {
            Ok(stream) => {
                if let Err(e) = stream.set_recv_buffer_size(output_buffer_size as usize)
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
    }
}

pub fn sync_socket_connect(addr: SocketAddr) -> std::io::Result<std::net::TcpStream> {
    let tcp = if cfg!(windows) {
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
    if let Err(e) = tcp.connect(&addr) {
        return Err(e)
    }
    Ok(tcp)
}

pub fn sync_connect<A>(addr: A, output_buffer_size: u64) -> std::net::TcpStream
where
    A: ToSocketAddrs + Display + Clone,
{
//    let server_name: String = addr.clone().into();
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
        match sync_socket_connect(addr) {
            Ok(stream) => {
                if let Err(e) = stream.set_recv_buffer_size(output_buffer_size as usize)
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
    }
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
