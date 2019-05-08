use nsq_client::{Client, Consumer, Context, Msg, Fin, Config};
use log::info;
use env_logger;

#[derive(Copy, Clone, Debug)]
struct MyReader;

impl Consumer for MyReader {
    fn handle(&mut self, msg: Msg, ctx: &mut Context) {
        info!("msg received: {:?}", msg);
        ctx.send(Fin(msg.id));
    }
}

fn main() {
    env_logger::init();
    let mut config = Config::default();
    config.tls();
    let mut c = Client::new(
        "test", // channel
        "test", // topic
        "localhost:4150", // nsqd address
        config, //configuration
        None, // optional secret for authentication
        500, // rdy
        6, // max_attemps
        );
    c.spawn(8, MyReader{});
    c.run();
}
