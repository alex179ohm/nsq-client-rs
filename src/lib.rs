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

//! Nsq-client is the [actix](https://actix.rs) based client implementation of the nsq protocol.
//!
//! This crate is intended as a swiss-knife base implementation for more
//! complex nsq client applications, it supports even single or multiple connections, single or
//! multiple async readers.
//!
//! Due the actors model, readers and connections are distinct entities witch communicate
//! each other throught messages, so one reader could receive messages from multiple connections and multiple
//! connections could easily send messages to multiple readers.
//!
//!
//! # Examples
//! ```no-run
//! use actix::prelude::*;
//! use nsq_client::{Connection, Msg, Subscribe, Fin};
//!
//! struct MyReader{
//!     conn: Arc<Addr<Connection>>,
//! };
//!
//! impl Actor for MyReader {
//!     type Context = Context<Self>;
//!     fn started(&mut self, _: &mut Self::Context) {
//!         self.subscribe::<Msg>(ctx, self.conn.clone());
//!     }
//! }
//!
//! impl Handler<Msg> for MyReader {
//!     type Result = ();
//!     fn handle(&mut self, msg: Msg, ctx: &mut Self::Context) {
//!         let conn = msg.conn.clone();
//!         let msg = msg.msg;
//!         info!("MyReader received: {:?}", msg);
//!         conn.do_send(Fin(msg.id));
//!     }
//! }
//! ```

#![feature(associated_type_defaults)]
extern crate actix;
extern crate backoff;
extern crate byteorder;
extern crate bytes;
extern crate fnv;
extern crate futures;
extern crate hostname;
extern crate log;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
extern crate tokio;

mod auth;
mod codec;
#[allow(dead_code)]
mod commands;
mod config;
mod conn;
#[allow(dead_code)]
mod error;
mod msgs;
mod producer;
mod subscribe;

pub use config::Config;
pub use conn::Connection;
pub use error::Error;
pub use msgs::{
    Backoff, Fin, Msg, OnAuth, OnBackoff, OnClose, OnIdentify, OnResume, Pub, Ready,
    Requeue, Touch,
};
pub use producer::Producer;
pub use subscribe::Subscribe;
