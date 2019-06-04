use nsq_client::{Client, Context, Pub, Config, Producer, VerifyServerCert};
use std::time::Duration;
use std::thread;
use env_logger;

#[derive(Copy, Clone, Debug)]
struct MyProducer;

impl Producer for MyProducer {
    fn send(&mut self, ctx: &mut Context) {
        thread::sleep(Duration::from_millis(1000));
        ctx.send(Pub("test".to_owned(), "message".to_owned()));
    }
}

fn main() {
    env_logger::init();
    let mut config: Config<String> = Config::default();
    config.tls(VerifyServerCert::None);
    let mut c = Client::new(
        "test",
        "test",
        "localhost:4150",
        config,
        None,
        0,
        0,
    );
    c.spawn_producer(4, MyProducer{});
    c.run();
}
