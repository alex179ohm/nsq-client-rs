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

use actix::prelude::*;
use nsq_client::{Connection, Subscribe, InFlight, OnAuth, OnIdentify, OnBackoff, OnResume, Ready};

pub struct Consumer {
    conn: Arc<Addr<Connection>>,
    total_rdy: u32,
    rdy: u32,
    max_in_flight: u32,
}

#[derive(Message)]
pub struct SetRDY(pub u32);

impl Consumer {
    pub fn new(conn: Arc<Addr<Connection>>, rdy: u32, max_in_flight: u32) -> Self {
        Consumer {
            conn,
            rdy,
            max_in_flight,
            total_rdy: 2500,
        }
    }
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
    fn started(&mut self, ctx: &mut Self::Context) {
        println!("Consumer Started");
        self.subscribe::<InFlight>(ctx, self.conn.clone());
        self.subscribe::<OnAuth>(ctx, self.conn.clone());
        self.subscribe::<OnIdentify>(ctx, self.conn.clone());
    }
}

impl Handler<InFlight> for Consumer {
    type Result=();

    fn handle(&mut self, msg: InFlight, _: &mut Self::Context) {
        println!("in_flight: {}", msg.0);
    }
}

impl Handler<OnAuth> for Consumer {
    type Result = ();
    fn handle(&mut self, msg: OnAuth, _: &mut Self::Context) {
        println!("authenticated: {:#?}", msg.0);
    }
}

//impl Handler<OnIdentify> for Consumer {
//    type Result = ();
//    fn handle(&mut self, msg: OnIdentify, _: &mut Self::Context) {
//        println!("idetified: {:#?}", msg.0);
//    }
//}

impl Handler<OnBackoff> for Consumer {
    type Result = ();
    fn handle(&mut self, msg: OnBackoff, _: &mut Self::Context) {
        println!("backoff activated");
    }
}

impl Handler<OnResume> for Consumer {
    type Result = ();
    fn handle(&mut self, msg: OnResume, _: &mut Self::Context) {
        println!("resuming");
    }
}

impl Handler<SetRDY> for Consumer {
    type Result = ();
    fn handle(&mut self, msg: SetRDY, _: &mut Self::Context) {
        self.conn.do_send(Ready(msg.0))
    }
}
