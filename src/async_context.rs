use std::sync::Mutex;
use crossbeam::channel::Sender;
//use crate::client::{CmdChannel};
use crate::msgs::{Cmd, NsqCmd};

pub struct ContextAsync {
    cmd: Sender<Cmd>,
    sentinel: Mutex<Sender<()>>,
}

impl ContextAsync {
    pub fn new(cmd: Sender<Cmd>, sentinel: Sender<()>) -> ContextAsync {
        ContextAsync{ cmd, sentinel: Mutex::new(sentinel) }
    }

    pub fn send<C: NsqCmd>(&mut self, cmd: C) {
        let _ = self.cmd.send(cmd.as_cmd());
        let _ = self.sentinel.lock().unwrap().send(());
    }
}