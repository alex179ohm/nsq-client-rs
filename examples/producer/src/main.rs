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
extern crate futures;

use actix::prelude::*;
use std::time::Duration;
use nsq_client::{Producer, Pub, Config};

struct MyClient ( Addr<Producer> );

impl Actor for MyClient {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        let dur = Duration::from_secs(1);
        ctx.run_later(dur, |act, _ctx| {
            for i in 0..200 {
                act.0.do_send(Pub(format!("test {}‰ msg sent ™", i)));
            }
        });
    }
}

fn main() {
    let config = Config::default().client_id("Producer".to_owned());
    System::run(|| {
        let p = Supervisor::start(|_|
            Producer::new("test", "test", "0.0.0.0:4150", Some(config), "")
        );
        MyClient(p).start();
    });
}
