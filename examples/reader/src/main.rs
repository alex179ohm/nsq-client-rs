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

extern crate nsq_client;
extern crate actix;

use std::sync::Arc;

use actix::prelude::*;

use nsq_client::{Connection, Msg, Fin, Subscribe, Config, OnAuth};

struct MyReader(pub Arc<Addr<Connection>>);


impl Actor for MyReader {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe::<Msg>(ctx, self.0.clone());
        self.subscribe::<OnAuth>(ctx, self.0.clone());
    }
}

impl Handler<Msg> for MyReader {
    type Result = ();
    fn handle(&mut self, msg: Msg, _ctx: &mut Self::Context) {
        println!("MyReader: {:?}", msg);
        if let Ok(body) = String::from_utf8(msg.body) {
            println!("utf8 msg: {}", body);
        }
        self.0.do_send(Fin(msg.id));
    }
}

impl Handler<OnAuth> for MyReader {
    type Result = ();
    fn handle(&mut self, msg: OnAuth, _: &mut Self::Context) {
        println!("authenticated: {:#?}", msg.0);
    }
}


fn main() {
    env_logger::init();
    let sys = System::new("nsq-consumer");
    let config = Config::new().client_id("consumer");
    let secret = "".to_owned();
    let c = Supervisor::start(|_| Connection::new(
            "test", // topic
            "test", //channel
            "0.0.0.0:4150", //nsqd tcp address
            Some(config), //config (Optional see mod config for defaults, if None Consumer sets defaults)
            None,// Some(secret), // secret for Auth (Optional)
            Some(2), // rdy (optional if None set rdy to 1)
    ));
    let conn = Arc::new(c);
    let _ = MyReader(conn.clone()).start(); // same thread reader
    sys.run();
}
