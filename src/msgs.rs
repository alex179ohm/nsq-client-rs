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

use crate::codec::Cmd;
use crate::error::Error;
//use crate::conn::Connection;

pub trait NsqMsg: Message<Result = ()> + Send + 'static {}

impl<M> NsqMsg for M
where
    M: Message<Result = ()> + Send + 'static
{}

#[derive(Message)]
pub struct AddHandler<M: NsqMsg>(pub Recipient<M>);

//#[derive(Message)]
//pub struct Conn(pub Addr<Connection>);

/// Message sent by nsqd
///
/// ## Example
/// ```rust
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
pub struct Msg
{
    /// Timestamp of the message
    pub timestamp: i64,
    /// Number of attemps reader tried to process the message
    pub attemps: u16,
    /// Id of the message
    pub id: String,
    /// Data sent by nsqd
    pub body: String,
}

impl Default for Msg {
    fn default() -> Self {
        Self {
            timestamp: 0,
            attemps: 0,
            id: "".to_owned(),
            body: "".to_owned(),
        }
    }
}

/// Sent by [Connection](struct.Connection.html) every time in_fligth is increased or decreased
#[derive(Message)]
pub struct InFlight(pub u32);

#[derive(Message)]
pub struct Auth;

#[derive(Message)]
pub struct Sub;

/// Allows Consumer to change Connection/s RDY
///
/// # Examples
/// ```rust
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

#[derive(Message)]
pub struct NsqBackoff;

#[derive(Message)]
pub struct Resume;

/// Send FIN command to nsqd
///
/// Args:
/// * id - id of the message
///
/// # Examples
/// ```
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
/// ```
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
pub struct Reqeue(pub String, u32);

/// Send TOUCH command to nsqd (reset timeout for and in-flight message)
/// 
/// Args:
/// * id - id of the message
///
/// # Examples
/// ```
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

#[derive(Message)]
pub struct Cls;

#[derive(Clone, Debug)]
pub struct Pub(pub String);

impl Message for Pub
{
    type Result = Result<Cmd, Error>;
}

