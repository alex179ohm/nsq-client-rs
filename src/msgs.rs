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
use crate::conn::Connection;

pub trait NsqMsg: Message<Result = ()> + Send + 'static {}

impl<M> NsqMsg for M
where
    M: Message<Result = ()> + Send + 'static
{}

#[derive(Message)]
pub struct AddHandler<M: NsqMsg>(pub Recipient<M>);

#[derive(Message)]
pub struct Conn(pub Addr<Connection>);

#[derive(Clone, Debug, Message)]
pub struct Msg
{
    pub timestamp: i64,
    pub attemps: u16,
    pub id: String,
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

#[derive(Message)]
pub struct InFlight(pub u32);

#[derive(Message)]
pub struct Auth;

#[derive(Message)]
pub struct Sub;

#[derive(Message)]
pub struct Ready(pub u32);

#[derive(Message)]
pub struct NsqBackoff;

#[derive(Message)]
pub struct Resume;

#[derive(Message, Clone)]
pub struct Fin(pub String);

#[derive(Message, Clone)]
pub struct Reqeue(pub String, u32);

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

