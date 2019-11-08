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

use bytes::BytesMut;

pub const VERSION: &str = "  V2";
const PUB: &str = "PUB";
const MPUB: &str = "MPUB";
const DPUB: &str = "DPUB";
const SUB: &str = "SUB";
const TOUCH: &str = "TOUCH";
const RDY: &str = "RDY";
const FIN: &str = "FIN";
const CLS: &str = "CLS";
const AUTH: &str = "AUTH";
pub const NOP: &str = "NOP";
const IDENTIFY: &str = "IDENTIFY";
const REQ: &str = "REQ";

pub trait NsqCmd: Send {
    fn cmd(&self) -> String;
    fn msg(&self) -> Vec<Vec<u8>> {
        vec![]
    }
    fn as_cmd(&self) -> Cmd {
        Cmd::new(self.cmd(), self.msg())
    }
}

#[derive(Debug, Clone)]
pub struct Cmd {
    pub cmd: String,
    pub msg: Vec<Vec<u8>>,
}

impl Cmd {
    fn new(cmd: String, msg: Vec<Vec<u8>>) -> Cmd {
        Cmd { cmd, msg }
    }
}

impl NsqCmd for Cmd {
    fn cmd(&self) -> String {
        self.cmd.clone()
    }

    fn msg(&self) -> Vec<Vec<u8>> {
        self.msg.clone()
    }
}

pub trait Message: Send + 'static {}

pub struct Fin(pub String);
pub struct Touch(pub String);
pub struct Requeue(pub String, pub u32);
pub struct Pub(pub String, pub Vec<u8>);
pub struct Mpub(pub String, pub Vec<Vec<u8>>);
pub struct Dpub(pub String, pub u32, pub Vec<u8>);
pub struct Identify(pub String);
pub struct Subscribe(pub String, pub String);
pub struct Auth(pub String);
pub struct Nop;
pub struct Cls;
pub struct Rdy(pub u32);

#[derive(Debug, Clone)]
pub struct Msg {
    pub timeout: u64,
    pub timestamp: i64,
    pub attemps: u16,
    pub id: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct BytesMsg(pub u64, pub BytesMut);

impl NsqCmd for Auth {
    fn cmd(&self) -> String {
        AUTH.to_owned()
    }

    fn msg(&self) -> Vec<Vec<u8>> {
        vec![Vec::from(self.0.as_bytes())]
    }
}

impl NsqCmd for Nop {
    fn cmd(&self) -> String {
        NOP.to_owned()
    }
}

impl NsqCmd for Subscribe {
    fn cmd(&self) -> String {
        format!("{} {} {}", SUB, self.0, self.1)
    }
}

impl NsqCmd for Rdy {
    fn cmd(&self) -> String {
        format!("{} {}", RDY, self.0)
    }
}

impl NsqCmd for Cls {
    fn cmd(&self) -> String {
        CLS.to_owned()
    }
}

impl NsqCmd for Identify {
    fn cmd(&self) -> String {
        IDENTIFY.to_owned()
    }

    fn msg(&self) -> Vec<Vec<u8>> {
        vec![Vec::from(self.0.as_bytes())]
    }
}

impl NsqCmd for Fin {
    fn cmd(&self) -> String {
        format!("{} {}", FIN, self.0)
    }
}

impl NsqCmd for Touch {
    fn cmd(&self) -> String {
        format!("{} {}", TOUCH, self.0)
    }
}

impl NsqCmd for Requeue {
    fn cmd(&self) -> String {
        format!("{} {} {}", REQ, self.0, self.1)
    }
}

impl NsqCmd for Pub {
    fn cmd(&self) -> String {
        format!("{} {}", PUB, self.0)
    }

    fn msg(&self) -> Vec<Vec<u8>> {
        vec![self.1.clone()]
    }
}

impl NsqCmd for Mpub {
    fn cmd(&self) -> String {
        format!("{} {}", MPUB, self.0)
    }

    fn msg(&self) -> Vec<Vec<u8>> {
        self.1.clone()
    }
}

impl NsqCmd for Dpub {
    fn cmd(&self) -> String {
        format!("{} {} {}", DPUB, self.0, self.1)
    }

    fn msg(&self) -> Vec<Vec<u8>> {
        vec![self.2.clone()]
    }
}

#[derive(Debug)]
pub enum ConnMsg {
    Close,
    Connect(String),
    GetIsConnected,
}

#[derive(Debug)]
pub struct ConnInfo {
    pub connected: bool,
    pub last_time: i64,
}

#[derive(Debug)]
pub struct MsgTimeInfo {
    pub last_time_recv: i64,
    pub last_time_sent: u32,
}

#[derive(Debug)]
pub enum ConnMsgInfo {
    IsConnected(ConnInfo),
    MsgInfo(MsgTimeInfo),
}
