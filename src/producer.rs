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

use std::io;
use std::collections::VecDeque;

use actix::actors::resolver::{Connect, Resolver};
use actix::prelude::*;
use futures::unsync::oneshot;
use futures::Future;
use backoff::backoff::Backoff;
use backoff::ExponentialBackoff;
use log::{error, info, debug};
use serde_json;
use tokio_codec::FramedRead;
use tokio_io::io::WriteHalf;
use tokio_io::AsyncRead;
use tokio_tcp::TcpStream;
//use bytes::BytesMut;

use crate::codec::{NsqCodec, Cmd};
use crate::commands::{identify, nop, auth, sub, rdy, publish, VERSION};
use crate::config::{Config, NsqdConfig};
use crate::error::Error;
use crate::msgs::{Auth, Pub, Sub, Ready};
use crate::conn::ConnState;

pub struct Producer
{
    topic: String,
    channel: String,
    addr: String,
    config: Config,
    backoff: ExponentialBackoff,
    cell: Option<actix::io::FramedWrite<WriteHalf<TcpStream>, NsqCodec>>,
    state: ConnState,
    queue: VecDeque<oneshot::Sender<Result<Cmd, Error>>>,
    auth: String,
//    rdy: u32,
}

impl Default for Producer
{
    fn default() -> Producer {
        Producer {
            topic: String::new(),
            channel: String::new(),
            addr: String::new(),
            config: Config::default(),
            backoff: ExponentialBackoff::default(),
            cell: None,
            state: ConnState::Neg,
            queue: VecDeque::new(),
            auth: String::new(),
 //           rdy: 0,
        }
    }
}

impl Producer
{
    pub fn new<S: Into<String>>(
        topic: S,
        channel: S,
        addr: S,
        config: Option<Config>,
        auth: S,
 //       rdy: Option<u32>,
    ) -> Producer {
        let mut backoff = ExponentialBackoff::default();
        let mut _rdy = 0;
        //if rdy.is_some() { _rdy = rdy.unwrap() };
        let cfg = match config {
            Some(cfg) => cfg,
            None => Config::default(),
        };
        backoff.max_elapsed_time = None;
        Producer {
            addr: addr.into(),
            config: cfg,
            backoff,
            cell: None,
            topic: topic.into(),
            channel: channel.into(),
            state: ConnState::Neg,
            queue: VecDeque::new(),
            auth: auth.into(),
  //          rdy: _rdy,
        }
    }
}

impl Actor for Producer
{
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        Resolver::from_registry()
            .send(Connect::host(self.addr.as_str()))
            .into_actor(self)
            .map(|res, act, ctx| match res {
                Ok(stream) => {
                    info!("Connected to nsqd: {}", act.addr);

                    let (r, w) = stream.split();

                    // write connection
                    let mut framed =
                        actix::io::FramedWrite::new(w, NsqCodec{}, ctx);
                    let mut rx = FramedRead::new(r, NsqCodec{});

                    // send magic version
                    framed.write(Cmd::Magic(VERSION));
                    // send configuration to nsqd
                    let json = match serde_json::to_string(&act.config) {
                        Ok(s) => s,
                        Err(e) => {
                            error!("Config cannot be formatted as json string: {}", e);
                            return ctx.stop();
                        }
                    };
                    // read connection
                    ctx.add_stream(rx);
                    framed.write(identify(json));
                    act.cell = Some(framed);

                    // reset backoff
                    act.backoff.reset();
                }
                Err(err) => {
                    error!("Can not connect to nsqd: {}", err);
                    if let Some(timeout) = act.backoff.next_backoff() {
                        ctx.run_later(timeout, |_, ctx| ctx.stop());
                    }
                }
            })
            .map_err(|err, act, ctx| {
                error!("Can not connect to nsqd: {}", err);
                if let Some(timeout) = act.backoff.next_backoff() {
                    ctx.run_later(timeout, |_, ctx| ctx.stop());
                }
            })
            .wait(ctx);
    }
}

impl actix::io::WriteHandler<io::Error> for Producer
{
    fn error(&mut self, err: io::Error, _: &mut Self::Context) -> Running {
        error!("nsqd connection dropped: {} error: {}", self.addr, err);
        Running::Stop
    }
}

// TODO: Implement error
impl StreamHandler<Cmd, Error> for Producer
{
    fn error(&mut self, err: Error, _ctx: &mut Self::Context) -> Running {
        match err {
            Error::Remote(err) => {
                if let Some(tx) = self.queue.pop_front() {
                    let _ = tx.send(Err(Error::Remote(err)));
                }
                return Running::Continue
            },
            _ => {
                error!("Something goes wrong decoding message");
            },
        };
        Running::Stop
    }

    fn handle(&mut self, msg: Cmd, ctx: &mut Self::Context)
    {
        match msg {
            Cmd::Heartbeat => {
                debug!("received heartbeat");
                if let Some(ref mut cell) = self.cell {
                    cell.write(nop());
                } else {
                    error!("Nsqd connection dropped. trying reconnecting");
                    ctx.stop();
                }
            }
            Cmd::Response(s) => {
                match self.state {
                    ConnState::Neg => {
                        let config: NsqdConfig = match serde_json::from_str(s.as_str()) {
                            Ok(s) => s,
                            Err(err) => {
                                error!("Negotiating json response invalid: {:?}", err);
                                return ctx.stop();
                            }
                        };
                        debug!("json response: {:?}", config);
                        if config.auth_required {
                            ctx.notify(Auth);
                        } else {
                            //ctx.notify(Sub);
                            self.state = ConnState::Started;
                        }
                    },
                    ConnState::Sub => {
                        ctx.notify(Sub);
                    },
                    ConnState::Ready => {
                        debug!("sub response: {}", s);
                        ctx.notify(Ready(0));
                    }
                    _ => {
                        debug!("response: {}", s);
                        if let Some(tx) = self.queue.pop_front() {
                            let _ = tx.send(Ok(Cmd::Response(s)));
                        }
                    },
                }
            },
            _ => {},
        }
    }
}

impl Handler<Auth> for Producer
{
    type Result = ();
    fn handle(&mut self, _msg: Auth, ctx: &mut Self::Context) {
        if let Some(ref mut cell) = self.cell {
            cell.write(auth(self.auth.clone()));
        } else {
            error!("Unable to identify nsqd connection dropped");
            ctx.stop();
        }
        self.state = ConnState::Sub;
    }

}

impl Handler<Sub> for Producer
{
    type Result = ();

    fn handle(&mut self, _msg: Sub, _ctx: &mut Self::Context) {
        if let Some(ref mut cell) = self.cell {
            let topic = self.topic.clone();
            let channel = self.channel.clone();
            cell.write(sub(topic.as_str(), channel.as_str()));
        }
        self.state = ConnState::Ready
    }
}

impl Handler<Ready> for Producer
{
    type Result = ();

    fn handle(&mut self, msg: Ready, _ctx: &mut Self::Context) {
        if let Some(ref mut cell) = self.cell {
            cell.write(rdy(msg.0));
        }
        if self.state != ConnState::Started { self.state = ConnState::Started }
    }
}


impl Handler<Pub> for Producer
{
    type Result = ResponseFuture<Cmd, Error>;

    fn handle(&mut self, msg: Pub, _ctx: &mut Self::Context) -> Self::Result {
        let (tx, rx) = oneshot::channel();
        println!("on Pub");
        if let Some(ref mut cell) = self.cell {
            self.queue.push_back(tx);
            let topic = self.topic.clone();
            println!("publish data");
            cell.write(publish(topic.as_str(), &msg.0));
        } else {
            let _ = tx.send(Err(Error::NotConnected));
        }
        Box::new(rx.map_err(|_| Error::Disconnected).and_then(|res| res))
    }
}


impl Supervised for Producer
{
    fn restarting(&mut self, _: &mut Self::Context) {}
}
