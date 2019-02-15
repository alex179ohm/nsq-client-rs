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

use codec::Cmd;

pub const VERSION: &str = "  V2";
const PUB: &str = "PUB";
const MPUB: &str = "MPUB";
const DPUB: &str = "DPUB";
const SUB: &str = "SUB";
const TOUCH: &str = "TOUCH";
const RDY: &str = "RDY";
const FIN: &str = "FIN";
const CLS: &str = "CLS";
const NOP: &str = "NOP";
const IDENTIFY: &str = "IDENTIFY";
const REQ: &str = "REQ";

pub fn sub(topic: &str, channel: &str ) -> Cmd {
    Cmd::Command(format!("{} {} {}", SUB, topic, channel))
}

pub fn auth(secret: String) -> Cmd {
    Cmd::Msg("AUTH".to_owned(), secret)
}

pub fn nop() -> Cmd {
    Cmd::Command(NOP.to_owned())
}

pub fn identify(config: String) -> Cmd {
    Cmd::Msg(IDENTIFY.to_owned(), config)
}

pub fn fin(id: &str) -> Cmd {
    Cmd::Command(format!("{} {}", FIN, id))
}

pub fn cls() -> Cmd {
    Cmd::Command(CLS.to_owned())
}

pub fn rdy(i: u32) -> Cmd {
    Cmd::Command(format!("{} {}", RDY, i))
}

pub fn touch(id: &str) -> Cmd {
    Cmd::Command(format!("{} {}", TOUCH, id))
}

pub fn req(id: &str, timeout: Option<u32>) -> Cmd {
    if timeout.is_some() {
        Cmd::Command(format!("{} {} {}", REQ, id, timeout.unwrap()))
    } else {
        Cmd::Command(format!("{} {}", REQ, id))
    }
}

pub fn publish(topic: &str, msg: &str) -> Cmd {
    Cmd::Msg(format!("{} {}", PUB, topic), msg.to_owned())
}

pub fn mpub(topic: &str, msgs: Vec<String>) -> Cmd {
    Cmd::MMsg(format!("{} {}", MPUB, topic), msgs)
}

pub fn dpub(topic: &str, defer_time: &str, msg: &str) -> Cmd {
    Cmd::Msg(format!("{} {} {}", DPUB, topic, defer_time), msg.to_owned())
}
