use crate::codec::{
    write_cmd, write_magic, write_mmsg, write_msg, Response, FRAME_TYPE_ERROR,
    FRAME_TYPE_MESSAGE, FRAME_TYPE_RESPONSE, HEADER_LENGTH, HEARTBEAT,
};
use crate::config::Config;
use crate::msgs::{Cmd, Identify, NsqCmd, VERSION};
#[cfg(feature = "tls")]
use crate::tls::TlsSession;
use backoff::{backoff::Backoff, ExponentialBackoff};
use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use crossbeam::channel::{Receiver, Sender};
use log::{debug, error, info};
use mio::{net::TcpStream, Poll, PollOpt, Ready, Token};
#[cfg(feature = "tls")]
use rustls::Session;
use std::io::{self, Read, Write};
use std::net::ToSocketAddrs;
use std::process;
use std::thread;

#[derive(Debug)]
pub struct Conn {
    //the addr of the nsqd.
    addr: String,
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
    //heartbeat
    pub heartbeat: bool,
    //responses
    pub responses: Vec<Response>,
    //config
    config: Config,
    //msgs in flight
    in_flight: u32,
    //tls_session
    #[cfg(feature = "tls")]
    tls_sess: TlsSession,
    //tls connection enabled/disabled (needed because we not start chatting on encrypted connection)
    #[cfg(feature = "tls")]
    tls: bool,
    now: std::time::Instant,
    processed: u32,
}

impl Conn {
    #[allow(clippy::too_many_arguments)]
    #[cfg(not(feature = "tls"))]
    pub fn new(addr: String, config: Config, r: Receiver<Cmd>, s: Sender<Msg>) -> Conn {
        let socket = connect(addr.as_str());
        Conn {
            addr,
            socket,
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
        }
    }

    #[cfg(feature = "tls")]
    pub fn new(
        addr: String,
        config: Config,
        r: Receiver<Cmd>,
//        s: Sender<Msg>,
        s: Sender<BytesMut>,
        hostname: &str,
        verify_server_cert: bool,
    ) -> Conn {
        let socket = connect(addr.as_str(), config.output_buffer_size as usize);
        Conn {
            addr,
            socket,
            r_buf: BytesMut::new(),
            w_buf: BytesMut::new(),
            r,
            s,
            heartbeat: false,
            config,
            responses: Vec::new(),
            tls_sess: TlsSession::new(hostname, verify_server_cert),
            tls: false,
            in_flight: 0,
            now: std::time::Instant::now(),
            processed: 0,
        }
    }
    //lookup for address and connect to nsqd server.

    pub fn start(&mut self) {
        write_magic(&mut self.w_buf, VERSION);
        let _ = self.write();
        let config = serde_json::to_string(&self.config).unwrap();
        self.write_cmd(Identify(config).as_cmd());
        let _ = self.write();
    }

    #[cfg(feature = "tls")]
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
    }

    pub fn register(&mut self, poll: &mut Poll, token: Token) {
        poll.register(&self.socket, token, Ready::all(), PollOpt::edge())
            .expect("cannot register socket on poll");
    }

    pub fn get_response(&mut self, on_err: String) -> Result<String, ()> {
        self.poll_response();
        //if self.tls {
        //    info!("get resonse tls");
        //    if let Some(r) = self.responses.pop() {
        //        get_response(r, on_err.clone());
        //    }
        //    return Ok("Ok".to_owned());
        //}
        get_response(self.responses.pop().unwrap(), on_err)
    }

    pub fn poll_response(&mut self) {
        loop {
            let res = self.read();
            match res {
                Ok(0) => {
                    continue;
                }
                Ok(n) => {
                    info!("read n bytes: {}", n);
                    //self.decode(n);
                    if !self.responses.is_empty() {
                        info!("response read");
                        return;
                    } else {
                        continue;
                    }
                }
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        continue;
                    }
                    return;
                }
            }
        }
    }

    pub fn heartbeat_done(&mut self) {
        self.heartbeat = false;
    }

    #[cfg(feature = "tls")]
    pub fn read(&mut self) -> io::Result<usize> {
        if self.tls {
            self.read_tls()
        } else {
            self.read_tcp()
        }
    }

    #[cfg(not(feature = "tls"))]
    pub fn read(&mut self) -> io::Result<usize> {
        self.read_tcp()
    }

    #[cfg(feature = "tls")]
    pub fn write(&mut self) -> io::Result<usize> {
        if self.tls {
            self.write_tls()
        } else {
            self.write_tcp()
        }
    }

    #[cfg(not(feature = "tls"))]
    pub fn write(&mut self) -> io::Result<usize> {
        self.write_tcp()
    }

    pub fn write_messages(&mut self) {
        let msgs: Vec<Cmd> = self.r.try_iter().collect();
        for msg in msgs {
            self.write_cmd(msg);
            let _ = self.write();
            self.in_flight -= 1;
            self.processed += 1;
        }
        info!("processed {}", self.processed);
        if self.processed == 2000 {
            info!("time: {:?}", std::time::Instant::now().duration_since(self.now));
            std::process::exit(0);
        }
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

    #[cfg(feature = "tls")]
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
        let mut buf: Vec<u8> = vec![0; self.config.output_buffer_size as usize];
        //let mut n: usize = 0;
        loop {
            match self.tls_sess.0.read(buf.as_mut_slice()) {
                Ok(0) => return Ok(0),
                Ok(b) => {
                    debug!("read: {}", b);
                    self.r_buf.extend_from_slice(&buf.as_slice()[..b]);
                    self.decode(b);
                    buf.clear();
                    return Ok(b);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }

    pub fn read_tcp(&mut self) -> io::Result<usize> {
        let mut buf: Vec<u8> = vec![0; self.config.output_buffer_size as usize];
        let n: usize = 0;
        loop {
            match self.socket.read(buf.as_mut_slice()) {
                Ok(0) => return Ok(0),
                Ok(b) => {
                    self.r_buf.extend_from_slice(&buf.as_slice()[..b]);
                    self.decode(b);
                    buf.clear();
                    return Ok(b);
                }
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        return Ok(n);
                    }
                    return Err(e);
                }
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

    #[cfg(feature = "tls")]
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

pub fn connect(addr: &str, buffer_size: usize) -> TcpStream {
    let mut backoff = ExponentialBackoff::default();
    let mut addrs = match addr.to_socket_addrs() {
        Ok(addrs) => addrs,
        Err(e) => {
            error!("[{}] error on lookup: {}", addr, e);
            process::exit(1);
        }
    };
    info!(addrs);
    loop {
        info!("trying to connect to nsqd server");
        if let Some(addr) = addrs.next() {
            match TcpStream::connect(&addr) {
                Ok(stream) => {
                    if let Err(e) = stream.peer_addr() {
                        error!("[{}] nsqd not connected: {}", addr, e);
                        if let Some(timeout) = backoff.next_backoff() {
                            thread::sleep(timeout);
                            continue;
                        }
                    }
                    info!("[{}] connected", stream.peer_addr().unwrap());
                    let _ = stream.set_recv_buffer_size(buffer_size);
                    return stream;
                }
                Err(err) => {
                    error!("could not connect to nsqd: {}", err);
                    if let Some(timeout) = backoff.next_backoff() {
                        thread::sleep(timeout);
                    }
                }
            }
        } else {
            error!("could not resolve {}", addr);
            if let Some(timeout) = backoff.next_backoff() {
                thread::sleep(timeout);
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
