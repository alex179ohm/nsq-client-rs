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

#![feature(try_from, associated_type_defaults)]
extern crate futures;
extern crate tokio_io;
extern crate tokio_codec;
extern crate tokio_tcp;
extern crate bytes;
extern crate hostname;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
extern crate actix;
extern crate backoff;
extern crate log;
//extern crate snap;
extern crate byteorder;
extern crate fnv;


mod codec;
#[allow(dead_code)]
mod commands;
#[allow(dead_code)]
mod error;
mod config;
mod msgs;
mod producer;
mod conn;
mod subscribe;
//mod consumer;

pub use commands::{fin, req, touch};
pub use subscribe::{Subscribe};
pub use config::Config;
pub use producer::{Producer};
pub use conn::{Connection};
pub use codec::{NsqCodec, Cmd};
pub use error::Error;
pub use msgs::{Fin, Msg, Reqeue, Touch, Conn, Pub, NsqMsg, AddHandler, Ready, InFlight};
