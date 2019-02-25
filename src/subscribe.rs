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

use std::sync::Arc;

use actix::dev::ToEnvelope;
use actix::prelude::*;

use crate::conn::Connection;
use crate::msgs::{AddHandler, NsqMsg};

/// Allows differents consumers to subscribe to the desired msgs sent by connections.
///
/// # Example
/// ```no-run
/// struct Consumer(pub Addr<Connection>);
///
/// impl Actor for Consumer {
///     type Context = Context<Self>;
///     fn started(&mut self, ctx: &mut Self::Context) {
///         self.subscribe::<Msg>(ctx, self.0.clone());
///         self.subsctibe::<InFligth>(ctx, self.0.clone());
///     }
/// }
///
/// impl Handler<Msg> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: Msg, _: &mut Self::Context) {
///         // process Msg
///     }
/// }
///
/// impl Handler<InFligth> for Consumer {
///     type Result = ();
///     fn handle(&mut self, msg: InFligth, _: &mut Self::Context) {
///         // do something every time in_fligth is increased or decreased
///     }
/// }
/// ```
pub trait Subscribe
where
    Self: Actor,
    <Self as Actor>::Context: AsyncContext<Self>,
{
    fn subscribe<M: NsqMsg>(&self, ctx: &mut Self::Context, addr: Arc<Addr<Connection>>)
    where
        Self: Handler<M>,
        <Self as Actor>::Context: ToEnvelope<Self, M>,
    {
        let recp = ctx.address().recipient::<M>();
        addr.do_send(AddHandler(recp));
    }
}

impl<A> Subscribe for A
where
    A: Actor,
    <Self as Actor>::Context: AsyncContext<A>,
{
}
