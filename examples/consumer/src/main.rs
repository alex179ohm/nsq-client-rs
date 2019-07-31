use nsq_client::{Client, Consumer, Context, Msg, Fin, Config, ConnMsg, ConnMsgInfo};
use log::info;
use env_logger;
use crossbeam::channel::{self, Receiver, Sender};

#[derive(Copy, Clone, Debug)]
struct MyReader;

impl Consumer for MyReader {
    fn on_msg(&mut self, msg: Msg, ctx: &mut Context) {
        info!("msg received: {:?}", msg);
        ctx.send(Fin(msg.id));
    }
}

fn main() {
    env_logger::init();
    let mut config = Config::default();
    config.tls();
    let (_conn_sender, conn_receiver): (Sender<ConnMsg>, Receiver<ConnMsg>) = channel::unbounded();
    let (info_sender, _info_receiver): (Sender<ConnMsgInfo>, Receiver<ConnMsgInfo>) = channel::unbounded();
    let mut c = Client::new(
        "test", // channel
        "test", // topic
        // "tangram-monitor.tngrm.io:4150"
        "nsq-vodafone-1.tngrm.io:4150", // nsqd address
        config, //configuration
        None, // optional secret for authentication
        500, // rdy
        6, // max_attemps
        conn_receiver,
        info_sender,
        );
    c.spawn(8, MyReader{});
    c.run();
}
