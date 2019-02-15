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

use std::collections::HashMap;

use actix::prelude::*;
use nsqueue::{InFlight, Subscribe, ConsumerService};

use crate::reader::NsqReader;

pub struct Consumer {
    conn: String,
    tot_rdy: u32,
    rdy: u32,
}


impl Consumer {
    fn set_rdy(&mut self, conn: String, rdy: u32) {
        let tot_rdy = self.total_rdy + rdy;
        if tot_rdy > self.max_in_flight {
            return;
        }
        self.total_rdy += rdy;
    }
}

impl Actor for Consumer {
    type Context = Context<Self>;
}

impl Handler<InFlight> for Consumer {
    type Result=();

    fn handle(&mut self, msg: InFlight, ctx: &mut Self::Context) {
        let srv = ConsumerService::from_registry();
        let conn = srv.get_conn(self.conn);
        if msg.0 > self.max_in_flight {
            conn.do_send(Rdy(msg.0 - 1));
        }
    }
}

impl Handler<AddThreadReader> for Consumer {
    type Result=();
    fn handle(&mut self, msg: AddThreadReader, _: &mut Self::Context) {
        let handler = Arbiter::start(|_| NsqReader{}).recipient::<Msg>();
        if let Some(conn) = self.connections.get(&msg.0) {
            conn.do_send(AddHandler(handler));
        }
    }
}

impl Handler<Connect> for Consumer {
    type Result=();

    fn handle(&mut self, msg: Connect, _: &mut Self::Context) {
        let address = msg.address.clone();
        let topic = self.topic.clone();
        let channel = self.channel.clone();
        let addr = Supervisor::start(move |_| Connection::new(
                vec![],
                topic,
                channel,
                msg.address,
                Some(msg.config)));
        self.connections.insert(address, addr);
    }
}
