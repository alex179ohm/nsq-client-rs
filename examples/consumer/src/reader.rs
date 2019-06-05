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
use std::time::Duration;

use actix::prelude::*;
use chrono::DateTime;
use nsq_client::{Msg, OnIdentify, Fin, Connection, Subscribe};

pub struct NsqReader {
    pub conn: Arc<Addr<Connection>>,
    pub timeout: Duration,
}

impl Actor for NsqReader {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        println!("Reader started");
        self.subscribe::<Msg>(ctx, self.conn.clone());
        self.subscribe::<OnIdentify>(ctx, self.conn.clone());
    }
}

impl Handler<OnIdentify> for NsqReader {
    type Result = ();
    fn handle(&mut self, msg: OnIdentify, _: &mut Self::Context) {
        let config = msg.0;
        println!("nofify: {:?}", config);
    }
}

impl Handler<Msg> for NsqReader {
    type Result = ();
    // on identify
    fn handle(&mut self, msg: Msg, _ctx: &mut Self::Context) {
        println!("MyReader: {:?}", msg);
        let t = DateTime::parse_from_str(msg.timestamp.to_str(), "%s");
        self.conn.do_send(Fin(msg.id));
    }
}
