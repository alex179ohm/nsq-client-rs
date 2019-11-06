use crossbeam::channel::{self, Receiver, Sender};
use env_logger;
use log::info;
use nsq_client::{Client, Cmd, Config, ConnMsg, ConnMsgInfo, Mpub, NsqCmd, Producer};
use std::thread;
use std::time::Duration;

#[derive(Copy, Clone)]
struct MyProducer;

impl Producer for MyProducer {
    fn publish(&self) -> Cmd {
        thread::sleep(Duration::new(1, 0));
        let mut msgs: Vec<Vec<u8>> = Vec::new();
        for i in 0..100 {
            let msg_byte = format!("msg-{}", i);
            msgs.push(Vec::from(msg_byte.as_bytes()));
        }
        Mpub("test".to_owned(), msgs).as_cmd()
    }
}

fn main() {
    env_logger::init();
    let mut config = Config::default();
    config.tls();
    let (_conn_sender, conn_receiver): (Sender<ConnMsg>, Receiver<ConnMsg>) = channel::unbounded();
    let (info_sender, _info_receiver): (Sender<ConnMsgInfo>, Receiver<ConnMsgInfo>) =
        channel::unbounded();
    let mut c = Client::new(
        // no channel and topic means Producer
        "", // channel
        "", // topic
        "tangram-monitor.tngrm.io:4150",
        //"nsq-vodafone-1.tngrm.io:4150", // nsqd address
        config, //configuration
        None,   // optional secret for authentication
        500,    // rdy unused on Producer
        6,      // max_attemps unused on Producer
        conn_receiver,
        info_sender,
    );
    c.spawn_producer(1, MyProducer {});
    c.run();
}
