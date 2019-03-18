// MIT License
//
// Copyright (c) 2019-2021 Alessandro Cresto Miseroglio <alex179ohm@gmail.com>
// Copyright (c) 2019-2021 Tangram Technologies S.R.L. <https://tngrm.io>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::any::{Any, TypeId};
use std::io;
use std::time::Duration;

use actix::actors::resolver::{Connect, Resolver};
use actix::prelude::*;
use backoff::backoff::Backoff as TcpBackoff;
use backoff::ExponentialBackoff;
use fnv::FnvHashMap;
use futures::stream::once;
use log::{error, info};
use serde_json;
use tokio_codec::FramedRead;
use tokio_io::io::WriteHalf;
use tokio_io::AsyncRead;
use tokio_tcp::TcpStream;

use crate::auth::AuthResp;
use crate::codec::{Cmd, NsqCodec};
use crate::commands::{auth, fin, identify, nop, rdy, sub, VERSION};
use crate::config::{Config, NsqdConfig};
use crate::error::Error;
use crate::msgs::{
    AddHandler, Auth, Backoff, Cls, Fin, InFlight, Msg, NsqMsg, OnAuth, OnBackoff, OnClose,
    OnIdentify, OnResume, Ready, Resume, Sub,
};

#[derive(Message, Clone)]
pub struct TcpConnect(pub String);

#[derive(Message)]
pub struct SendMsg;

#[derive(Debug, PartialEq)]
pub enum ConnState {
    Neg,
    Auth,
    Sub,
    Ready,
    Started,
    Backoff,
    Resume,
    Closing,
    Stopped,
}

/// Tcp Connection to NSQ system.
///
/// Tries to connect to nsqd early as started:
///
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::Connection;
///
/// fn main() {
///     let sys = System::new("consumer");
///     Supervisor::start(|_| Connection::new(
///         "test", // <- topic
///         "test", // <- channel
///         "0.0.0.0:4150", // <- nsqd tcp address
///         None, // <- config (Optional)
///         None, // <- secret used by Auth (Optional)
///         Some(1) // <- Initial RDY setting for the Connection
///     ));
///     sys.run();
/// }
/// ```
pub struct Connection {
    msgs: Vec<(i64, u16, String, Vec<u8>)>,
    addr: String,
    handlers: Vec<Box<Any>>,
    handlers_busy: FnvHashMap<String, Box<Any>>,
    info_hashmap: FnvHashMap<TypeId, Box<Any>>,
    topic: String,
    channel: String,
    config: Config,
    secret: String,
    tcp_backoff: ExponentialBackoff,
    backoff: ExponentialBackoff,
    cell: Option<actix::io::FramedWrite<WriteHalf<TcpStream>, NsqCodec>>,
    state: ConnState,
    rdy: u32,
    in_flight: u32,
    handler_ready: usize,
}

impl Default for Connection {
    fn default() -> Connection {
        Connection {
            msgs: Vec::new(),
            handlers: Vec::new(),
            handlers_busy: FnvHashMap::default(),
            info_hashmap: FnvHashMap::default(),
            topic: String::new(),
            channel: String::new(),
            config: Config::default(),
            secret: String::new(),
            tcp_backoff: ExponentialBackoff::default(),
            backoff: ExponentialBackoff::default(),
            cell: None,
            state: ConnState::Neg,
            addr: String::new(),
            rdy: 1,
            in_flight: 0,
            handler_ready: 0,
        }
    }
}

impl Connection {
    /// Return a Tcp Connection to nsqd.
    ///
    /// * `topic`    - Topic String
    /// * `channel`  - Channel String
    /// * `addr`     - Tcp address of nsqd
    /// * `config`   - Optional [`Config`]
    /// * `secret`   - Optional String used to autenticate to nsqd
    /// * `rdy`      - Optional initial RDY setting
    pub fn new<S: Into<String>>(
        topic: S,
        channel: S,
        addr: S,
        config: Option<Config>,
        secret: Option<String>,
        rdy: Option<u32>,
    ) -> Connection {
        let mut tcp_backoff = ExponentialBackoff::default();
        let backoff = ExponentialBackoff::default();
        let cfg = match config {
            Some(cfg) => cfg,
            None => Config::default(),
        };
        let mut scrt = String::new();
        if let Some(sec) = secret {
            scrt = sec;
        }
        let rdy = match rdy {
            Some(r) => r,
            None => 1,
        };
        tcp_backoff.max_elapsed_time = None;
        Connection {
            msgs: Vec::new(),
            config: cfg,
            secret: scrt,
            tcp_backoff,
            backoff,
            cell: None,
            topic: topic.into(),
            channel: channel.into(),
            state: ConnState::Neg,
            handlers: Vec::new(),
            handlers_busy: FnvHashMap::default(),
            info_hashmap: FnvHashMap::default(),
            addr: addr.into(),
            rdy,
            in_flight: 0,
            handler_ready: 0,
        }
    }
}

impl Connection {
    fn info_in_flight(&self, n: u32) {
        if let Some(box_handler) = self.info_hashmap.get(&TypeId::of::<Recipient<InFlight>>()) {
            if let Some(handler) = box_handler.downcast_ref::<Recipient<InFlight>>() {
                match handler.do_send(InFlight(n)) {
                    Ok(_) => {}
                    Err(e) => error!("sending InFlight: {}", e),
                }
            }
        }
    }

    fn info_on_auth(&self, resp: AuthResp) {
        if let Some(box_handler) = self.info_hashmap.get(&TypeId::of::<Recipient<OnAuth>>()) {
            if let Some(handler) = box_handler.downcast_ref::<Recipient<OnAuth>>() {
                match handler.do_send(OnAuth(resp)) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("sending OnAuth: {}", e);
                    }
                }
            }
        }
    }

    fn info_on_identify(&self, resp: NsqdConfig) {
        if let Some(box_handler) = self
            .info_hashmap
            .get(&TypeId::of::<Recipient<OnIdentify>>())
        {
            if let Some(handler) = box_handler.downcast_ref::<Recipient<OnIdentify>>() {
                match handler.do_send(OnIdentify(resp)) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("sending OnIdentify: {}", e);
                    }
                }
            }
        }
    }

    fn info_on_close(&self, resp: bool) {
        if let Some(box_handler) = self.info_hashmap.get(&TypeId::of::<Recipient<OnClose>>()) {
            if let Some(handler) = box_handler.downcast_ref::<Recipient<OnClose>>() {
                match handler.do_send(OnClose(resp)) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("sending OnClose: {}", e);
                    }
                }
            }
        }
    }

    fn info_on_backoff(&self) {
        if let Some(box_handler) = self.info_hashmap.get(&TypeId::of::<Recipient<OnBackoff>>()) {
            if let Some(handler) = box_handler.downcast_ref::<Recipient<OnBackoff>>() {
                match handler.do_send(OnBackoff) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("sending OnBackoff: {}", e);
                    }
                }
            }
        }
    }

    fn info_on_resume(&self) {
        if let Some(box_handler) = self.info_hashmap.get(&TypeId::of::<Recipient<OnResume>>()) {
            if let Some(handler) = box_handler.downcast_ref::<Recipient<OnResume>>() {
                match handler.do_send(OnResume) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("sending OnBackoff: {}", e);
                    }
                }
            }
        }
    }
}

impl Actor for Connection {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        info!("trying to connect [{}]", self.addr);
        self.handler_ready = self.handlers.len();
        ctx.add_message_stream(once(Ok(TcpConnect(self.addr.to_owned()))));
    }
}

impl actix::io::WriteHandler<io::Error> for Connection {
    fn error(&mut self, err: io::Error, _: &mut Self::Context) -> Running {
        error!("Nsqd connection dropped: {}", err);
        Running::Stop
    }
}

// TODO: implement error
impl StreamHandler<Vec<Cmd>, Error> for Connection {
    fn finished(&mut self, ctx: &mut Self::Context) {
        error!("Nsqd connection dropped");
        ctx.stop();
    }

    fn error(&mut self, err: Error, _ctx: &mut Self::Context) -> Running {
        error!("Something goes wrong decoding message: {}", err);
        Running::Stop
    }

    fn handle(&mut self, msgs: Vec<Cmd>, ctx: &mut Self::Context) {
        info!("msg: {:?}", msgs);
        for msg in msgs {
            match msg {
                Cmd::Heartbeat => {
                    if let Some(ref mut cell) = self.cell {
                        cell.write(nop());
                    } else {
                        error!("Nsqd connection dropped. trying reconnecting");
                        ctx.stop();
                    }
                }
                Cmd::Response(s) => match self.state {
                    ConnState::Neg => {
                        info!("trying negotiation [{}]", self.addr);
                        let config: NsqdConfig = match serde_json::from_str(s.as_str()) {
                            Ok(s) => s,
                            Err(err) => {
                                error!("Negotiating json response invalid: {:?}", err);
                                return ctx.stop();
                            }
                        };
                        info!("configuration [{}] {:#?}", self.addr, config);
                        self.info_on_identify(config.clone());
                        if config.auth_required {
                            info!("trying authentication [{}]", self.addr);
                            ctx.notify(Auth);
                        } else {
                            info!(
                                "subscribing [{}] topic: {} channel: {}",
                                self.addr, self.topic, self.channel
                            );
                            ctx.notify(Sub);
                        }
                    }
                    ConnState::Auth => {
                        let auth_resp: AuthResp = match serde_json::from_str(s.as_str()) {
                            Ok(s) => s,
                            Err(err) => {
                                error!("Auth json response invalid: {:?}", err);
                                return ctx.stop();
                            }
                        };
                        info!("authenticated [{}] {:#?}", self.addr, auth_resp);
                        self.info_on_auth(auth_resp);
                        ctx.notify(Sub);
                    }
                    ConnState::Sub => {
                        ctx.notify(Sub);
                    }
                    ConnState::Ready => {
                        ctx.notify(Ready(self.rdy));
                    }
                    ConnState::Closing => {
                        self.info_on_close(true);
                        self.state = ConnState::Stopped;
                    }
                    _ => {}
                },
                // TODO: implement msg_queue and tumable RDY for fast processing multiple msgs
                Cmd::ResponseMsg(timestamp, attemps, id, body) => {
                    self.msgs.push((timestamp, attemps, id, body));
                    println!("send msgs");
                    ctx.notify(SendMsg);
                }
                Cmd::ResponseError(s) => {
                    if self.state == ConnState::Closing {
                        error!("Closing connection: {}", s);
                        self.info_on_close(false);
                        self.state = ConnState::Started;
                    }
                    error!("failed: {}", s);
                }
                Cmd::Command(_) => {
                    if let Some(ref mut cell) = self.cell {
                        cell.write(rdy(1));
                    }
                }
                _ => {}
            }
        }
    }
}

impl Handler<TcpConnect> for Connection {
    type Result = ();
    fn handle(&mut self, msg: TcpConnect, ctx: &mut Self::Context) {
        Resolver::from_registry()
            .send(Connect::host(msg.0.as_str()).timeout(Duration::new(5, 0)))
            .into_actor(self)
            .map(move |res, act, ctx| match res {
                Ok(stream) => {
                    info!("connected [{}]", msg.0);
                    //stream.set_recv_buffer_size(act.config.output_buffer_size as usize);

                    let (r, w) = stream.split();

                    // configure write side of the connection
                    let mut framed = actix::io::FramedWrite::new(w, NsqCodec{ msgs: Vec::new() }, ctx);
                    let mut rx = FramedRead::new(r, NsqCodec{ msgs: Vec::new() });
                    framed.write(Cmd::Magic(VERSION));
                    // send configuration to nsqd
                    let json = match serde_json::to_string(&act.config) {
                        Ok(s) => s,
                        Err(e) => {
                            error!("config cannot be formatted as json string: {}", e);
                            return ctx.stop();
                        }
                    };
                    // read connection
                    ctx.add_stream(rx);
                    framed.write(identify(json));
                    act.cell = Some(framed);

                    act.backoff.reset();
                    act.state = ConnState::Neg;
                }
                Err(err) => {
                    error!("can not connect [{}]", err);
                    // re-connect with backoff time.
                    // we stop current context, supervisor will restart it.
                    if let Some(timeout) = act.tcp_backoff.next_backoff() {
                        ctx.run_later(timeout, |_, ctx| ctx.stop());
                    }
                }
            })
            .map_err(|err, act, ctx| {
                error!("can not connect [{}]", err);
                // re-connect with backoff time.
                // we stop current context, supervisor will restart it.
                if let Some(timeout) = act.tcp_backoff.next_backoff() {
                    ctx.run_later(timeout, |_, ctx| ctx.stop());
                }
            })
            .wait(ctx);
    }
}

impl Handler<SendMsg> for Connection {
     type Result = ();
     fn handle(&mut self, _msg: SendMsg, _ctx: &mut Self::Context) {
        info!("handlers: {:?}", self.handlers);
        info!("busy: {:?}", self.handlers_busy);
        info!("msgs: {:?}", self.msgs);
        let len = self.handlers.len();
        info!("handlers len: {}", len);
        if len == 0 {
            return;
        }
        let mut sent = false;
        if let Some(handler) = self.handlers.get(len - 1) {
            if let Some(rec) = handler.downcast_ref::<Recipient<Msg>>() {
                if let Some((timestamp, attemps, id, body)) = self.msgs.pop() {
                    let id_cloned = id.clone();
                    let rec_cloned = rec.clone();
                    let _ = rec.do_send(Msg {
                        timestamp,
                        attemps,
                        id,
                        body,
                    });
                    self.handlers_busy.insert(id_cloned, Box::new(rec_cloned));
                    sent = true;
                }
                self.in_flight += 1;
                self.info_in_flight(self.in_flight);
            }
        }
        if sent {
            let _ = self.handlers.pop();
        }
     }
}

impl Handler<Cls> for Connection {
    type Result = ();
    fn handle(&mut self, _msg: Cls, ctx: &mut Self::Context) {
        self.state = ConnState::Closing;
        ctx.stop();
    }
}

impl Handler<Fin> for Connection {
    type Result = ();
    fn handle(&mut self, msg: Fin, ctx: &mut Self::Context) {
        // discard the in_flight messages
        let id = msg.0.clone();
        if let Some(ref mut cell) = self.cell {
            cell.write(fin(&msg.0));
        }
        if let Some(r) = self.handlers_busy.get(&id) {
            let rec = r.downcast_ref::<Recipient<Msg>>().unwrap();
            self.handlers.push(Box::new(rec.clone()));
            if !self.msgs.is_empty() {
                ctx.notify(SendMsg);
            }
        }
        self.handlers_busy.remove(&id);
        self.in_flight -= 1;
        self.info_in_flight(self.in_flight);
        if self.state == ConnState::Resume {
            ctx.notify(Ready(self.rdy));
            self.state = ConnState::Started;
        }
    }
}

impl Handler<Ready> for Connection {
    type Result = ();

    fn handle(&mut self, msg: Ready, _ctx: &mut Self::Context) {
        if self.state != ConnState::Ready {
            self.rdy = msg.0;
            return;
        }
        if let Some(ref mut cell) = self.cell {
            cell.write(rdy(msg.0));
        }
        if self.state == ConnState::Started {
            self.rdy = msg.0;
            info!("rdy updated [{}]", self.addr);
        } else {
            self.state = ConnState::Started;
            info!("Ready to go [{}] RDY: {}", self.addr, msg.0);
        }
    }
}

impl Handler<Auth> for Connection {
    type Result = ();
    fn handle(&mut self, _msg: Auth, ctx: &mut Self::Context) {
        info!("I'm on auth handler");
        if let Some(ref mut cell) = self.cell {
            println!("trying to write");
            cell.write(auth(self.secret.clone()));
            println!("Writed!");
        } else {
            error!("unable to identify: connection dropped [{}]", self.addr);
            ctx.stop();
        }
        self.state = ConnState::Auth;
    }
}

impl Handler<Sub> for Connection {
    type Result = ();
    fn handle(&mut self, _msg: Sub, ctx: &mut Self::Context) {
        if let Some(ref mut cell) = self.cell {
            cell.write(sub(&self.topic, &self.channel));
        } else {
            error!("unable to subscribing: connection dropped [{}]", self.addr);
            ctx.stop();
        }
        self.state = ConnState::Ready;
        info!(
            "subscribed [{}] topic: {} channel: {}",
            self.addr, self.topic, self.channel
        );
    }
}

impl Handler<Backoff> for Connection {
    type Result = ();
    fn handle(&mut self, _msg: Backoff, ctx: &mut Self::Context) {
        if let Some(timeout) = self.backoff.next_backoff() {
            if let Some(ref mut cell) = self.cell {
                cell.write(rdy(0));
                ctx.run_later(timeout, |_, ctx| ctx.notify(Resume));
                self.state = ConnState::Backoff;
            } else {
                error!("backoff failed: connection dropped [{}]", self.addr);
                Self::add_stream(once::<Vec<Cmd>, Error>(Err(Error::NotConnected)), ctx);
            }
            self.info_in_flight(0);
            self.info_on_backoff();
        }
    }
}

impl Handler<Resume> for Connection {
    type Result = ();
    fn handle(&mut self, _msg: Resume, ctx: &mut Self::Context) {
        if let Some(ref mut cell) = self.cell {
            cell.write(rdy(1));
            self.state = ConnState::Resume;
        } else {
            error!("resume failed: connection dropped [{}]", self.addr);
            Self::add_stream(once::<Vec<Cmd>, Error>(Err(Error::NotConnected)), ctx);
        }
        self.info_in_flight(1);
        self.info_on_resume();
    }
}

impl<M: NsqMsg> Handler<AddHandler<M>> for Connection {
    type Result = ();
    fn handle(&mut self, msg: AddHandler<M>, _: &mut Self::Context) {
        let msg_id = TypeId::of::<Recipient<M>>();
        if msg_id == TypeId::of::<Recipient<Msg>>() {
            self.handlers.push(Box::new(msg.0));
            info!("Reader added");
        } else {
            self.info_hashmap.insert(msg_id, Box::new(msg.0));
            info!("info handler added");
        }
    }
}

impl Supervised for Connection {
    fn restarting(&mut self, ctx: &mut Self::Context) {
        if self.state == ConnState::Stopped {
            ctx.stop();
        }
    }
}
