use nsq_rust::{Client, Context, Pub, NsqCmd};
use log::info;
use env_logger;
use std::time::{Instant, Duration};

fn main() {
    env_logger::init();
    let mut c = Client::new("0.0.0.0:4150", "test", "test", 3, None);
    c.with_producer();
    c.run();
    let p = c.producer();

    let now = Instant::now();
    for i in 0..1_000_000 {
        p.send(Pub("test".to_owned(), format!("msg {} sent", i)).as_cmd());
    }
    println!("time: {:#?}", now - Instant::now());
}
