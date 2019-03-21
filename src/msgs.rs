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

use actix::prelude::*;

use crate::auth::AuthResp;
use crate::codec::Cmd;
use crate::config::NsqdConfig;
use crate::error::Error;

pub trait NsqMsg: Message<Result = ()> + Send + 'static {}

impl<M> NsqMsg for M where M: Message<Result = ()> + Send + 'static {}

#[derive(Message)]
pub struct AddHandler<M: NsqMsg>(pub Recipient<M>);

/// Message sent by nsqd
///
/// # Examples
/// ```no-run
/// struct Consumer(Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn handle(&mut self, ctx: &mut Self::Context) {
///         self.subscribe::<Msg>(ctx, self.0.clone());
///     }
/// }
///
/// fn Handler<Msg> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: Msg, _: &mut Self::Context) {
///         println!("timestamp: {}", msg.timestamp);
///         println!("attemps: {}", msg.attemps);
///         println!("id: {}", msg.id);
///         println!("data: {}", msg.body);
///         println!("msg debug: {:?}", msg);
///     }
/// }
///
/// ```
#[derive(Clone, Debug, Message)]
pub struct Msg {
    /// Timestamp of the message
    pub timestamp: i64,
    /// Number of attemps reader tried to process the message
    pub attemps: u16,
    /// Id of the message
    pub id: String,
    /// Data sent by nsqd
    pub body: Vec<u8>,
}

impl Default for Msg {
    fn default() -> Self {
        Self {
            timestamp: 0,
            attemps: 0,
            id: "".to_owned(),
            body: Vec::new(),
        }
    }
}

#[derive(Message)]
pub struct Auth;

#[derive(Message)]
pub struct Sub;

/// Allows Consumer to change Connection/s RDY
///
/// # Examples
/// ```no-run
/// struct Consumer(Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, _: &mut Self::Context) {
///         self.conn.do_send(Ready(3));
///     }
/// }
/// ```
#[derive(Message)]
pub struct Ready(pub u32);

/// Allows Consumer to set Connection on backoff state
///
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::{Subscribe, Connection, Backoff, OnBackoff, OnResume, InFlight, Ready};
/// struct Consumer{
///     conn: Addr<Connection>,
///     backoff: bool,
/// };
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, _: &mut Self::Context) {
///         self.subscribe::<OnBackoff>(ctx, self.0.clone());
///         self.subscribe::<OnResume>(ctx, self.0.clone());
///         self.subscribe::<InFlight>(ctx, self.0.clone());
///         self.conn.do_send(Ready(10));
///         self.conn.do_send(Backoff);
///     }
/// }
///
/// impl Handler<OnBackoff> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: OnBackoff, _: &mut Self::Context) {
///         println!("Connection in Backoff");
///         self.backoff = true;
///     }
/// }
///
/// impl Handler<OnResume> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: OnResume, _: &mut Self::Context) {
///         println!("Connection resuming from backoff");
///         self.backoff = false;
///     }
/// }
///
/// impl Handler<OnInflight> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: OnInFlight, _: &mut Self::Context) {
///         match msg.0 {
///             0 => { println!("backoff state: {}", self.backoff) },
///             1 => { println!("resuming from backoff") },
///             _ => { println!("throttle") },
///         }
///     }
/// }
/// ```
#[derive(Message)]
pub struct Backoff;

#[derive(Message)]
pub struct Resume;

/// Send FIN command to nsqd
///
/// Args:
/// * id - id of the message
///
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::{Connection, Subscribe, Msg, Fin};
///
/// struct Consumer(pub Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, ctx: &mut Self::Context) {
///         self.subscribe::<Msg>(ctx, self.0.clone());
///     }
/// }
///
/// impl Handler<Msg> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: Msg, ctx: &mut Self::Conetxt) {
///         self.0.do_send(Fin(msg.id));
///     }
/// }
/// ```
#[derive(Message, Clone)]
pub struct Fin(pub String);

/// Send REQ command to nsqd
///
/// Args:
/// * id - id of the message
/// * timeout - time spent before message is re-sent by nsqd, 0 will not defer requeuing
///
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::{Connection, Subscribe, Requeue, Fin};
///
/// struct Consumer(pub Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, ctx: &mut Self::Context) {
///         self.subscribe::<Msg>(ctx, self.0.clone());
///     }
/// }
///
/// impl Handler<Msg> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: Msg, ctx: &mut Self::Conetxt) {
///         self.0.do_send(Requeue(msg.id, 2));
///     }
/// }
/// ```
#[derive(Message, Clone)]
pub struct Requeue(pub String, pub u32);

/// Send TOUCH command to nsqd (reset timeout for and in-flight message)
///
/// Args:
/// * id - id of the message
///
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::{Connection, Subscribe, Touch, Fin};
///
/// struct Consumer(pub Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, ctx: &mut Self::Context) {
///         self.subscribe::<Msg>(ctx, self.0.clone());
///     }
/// }
///
/// impl Handler<Msg> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: Msg, ctx: &mut Self::Conetxt) {
///         self.0.do_send(Touch(msg.id));
///     }
/// }
/// ```
#[derive(Message, Clone)]
pub struct Touch(pub String);

/// Sent by [Connection](struct.Connection.html) if auth be successful
///
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::{Connection, Subscribe, OnAuth};
///
/// struct Consumer(pub Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, ctx: &mut Self::Context) {
///         self.subscribe::<OnAuth>(ctx, self.0.clone());
///     }
/// }
///
/// impl Handler<OnAuth> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: OnAuth, ctx: &mut Self::Conetxt) {
///         println!("authenticated: {:?}", msg.0);
///     }
/// }
/// ```
#[derive(Message, Clone)]
pub struct OnAuth(pub AuthResp);

/// Sent by [Connection](struct.Connection.html) after identify succeeds
///
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::{Connection, Subscribe, OnIdentify};
///
/// struct Consumer(pub Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, ctx: &mut Self::Context) {
///         self.subscribe::<OnIdentify>(ctx, self.0.clone());
///     }
/// }
///
/// impl Handler<OnIdentify> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: OnIdentify, ctx: &mut Self::Conetxt) {
///         println!("identified: {:?}", msg.0);
///     }
/// }
/// ```
#[derive(Message, Clone)]
pub struct OnIdentify(pub NsqdConfig);

/// Sent by [Connection](struct.Connection.html) after CLS is sent to nsqd
///
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::{Connection, Subscribe, OnClose};
///
/// struct Consumer(pub Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, ctx: &mut Self::Context) {
///         self.subscribe::<OnClose>(ctx, self.0.clone());
///     }
/// }
///
/// impl Handler<OnClose> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: OnClose, ctx: &mut Self::Conetxt) {
///         if msg.0 == true {
///             println!("connection closed");
///         } else {
///             println!("connection closing failed");
///         }
///     }
/// }
/// ```
#[derive(Message, Clone)]
pub struct OnClose(pub bool);

/// Sent by [Connection](struct.Connection.html) after Backoff state is activated
///
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::{Connection, Subscribe, OnBackoff};
///
/// struct Consumer(pub Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, ctx: &mut Self::Context) {
///         self.subscribe::<OnBackoff>(ctx, self.0.clone());
///     }
/// }
///
/// impl Handler<OnBackoff> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: OnBackoff, ctx: &mut Self::Conetxt) {
///         println!("connection backoff activated");
///     }
/// }
/// ```
#[derive(Message, Clone)]
pub struct OnBackoff;

/// Sent by [Connection](struct.Connection.html) after Backoff state is terminated
///
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::{Connection, Subscribe, OnResume};
///
/// struct Consumer(pub Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, ctx: &mut Self::Context) {
///         self.subscribe::<OnResume>(ctx, self.0.clone());
///     }
/// }
///
/// impl Handler<OnResume> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: OnResume, ctx: &mut Self::Conetxt) {
///         println!("resuming connection from backoff state");
///     }
/// }
/// ```
#[derive(Message, Clone)]
pub struct OnResume;

/// Allow Consumer to safefly close nsq [Connection](struct.Connection.html)
///
/// Send nsq CLS connand to nsqd
/// # Examples
/// ```no-run
/// use actix::prelude::*;
/// use nsq_client::{Cls, OnClose, Subscribe, Connection};
///
/// struct Consumer(Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, _: &mut Self::Context) {
///         self.subscribe::<OnClose>(ctx, self.0.clone());
///         self.0.do_send(Cls);
///     }
/// }
///
/// impl Handler<OnClose> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: OnClose, _: &mut Self::Context) {
///         if msg.0 {
///             println!("Connection closed");
///         } else {
///             println!("Cannot close Connection");
///         }
///     }
/// }
/// ```
#[derive(Message)]
pub struct Cls;

#[derive(Clone, Debug)]
pub struct Pub(pub String);

impl Message for Pub {
    type Result = Result<Cmd, Error>;
}
