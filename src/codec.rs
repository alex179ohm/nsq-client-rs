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

//! And implementation of the NSQ protocol,
//! Source: https://github.com/alex179ohm/nsqueue/blob/master/src/codec.rs

use std::io::{self, Cursor};
use std::str;

use bytes::{Buf, BufMut, BytesMut};
use tokio_io::codec::{Encoder, Decoder};
use log::error;

use crate::error::Error;
//use crate::message::Msg;

// Header: Size(4-Byte) + FrameType(4-Byte)
const HEADER_LENGTH: usize = 8;

// Frame Types
const FRAME_TYPE_RESPONSE: i32 = 0x00;
const FRAME_TYPE_ERROR: i32 = 0x01;
const FRAME_TYPE_MESSAGE: i32 = 0x02;

const HEARTBEAT: &str = "_heartbeat_";

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Cmd {
    /// nsqd heartbeat msg.
    Heartbeat,

    /// Magic "  V2"
    Magic(&'static str),

    /// Succefull response.
    Response(String),

    /// Error Response E_FIN_FAILED, E_REQ_FAILED, E_TOUCH_FAILED
    ResponseError(String),

    /// Message response.
    ResponseMsg(Vec<(i64, u16, String, String)>),

    /// A simple Command whitch not sends msg.
    Command(String),

    /// A simple message (pub or dpub).
    Msg(String, String),

    /// Multiple message (mpub)
    MMsg(String, Vec<String>),
}

/// NSQ codec
pub struct NsqCodec {}

pub fn decode_msg(buf: &mut BytesMut) -> Option<(i64, u16, String, String)> {
    if buf.len() < 4 {
        None
    } else {
        let frame = buf.clone();
        let mut cursor = Cursor::new(frame);
        let size = cursor.get_i32_be() as usize;
        if buf.len() < size + 4 {
            None
        } else {
            // skip frame_type
            let _ = cursor.get_i32_be();
            let timestamp = cursor.get_i64_be();
            let attemps = cursor.get_u16_be();
            if let Ok(id_body) = str::from_utf8(&cursor.bytes()[..size - HEADER_LENGTH - 6]) {
                let (id, body) = id_body.split_at(16);
                // clean the buffer at frame size
                buf.split_to(size+4);
                Some((timestamp, attemps, id.to_owned(), body.to_owned()))
            } else {
                error!("error deconding utf8 message frame");
                None
            }
        }
    }
}

fn write_n(buf: &mut BytesMut) {
    buf.put_u8(b'\n');
}

fn check_and_reserve(buf: &mut BytesMut, size: usize) {
    let remaining_bytes = buf.remaining_mut();
    if remaining_bytes < size {
        buf.reserve(size);
    }
}

/// write command in buffer and append 0x2 ("\n")
fn write_cmd(buf: &mut BytesMut, cmd: String) {
    let cmd_as_bytes = cmd.as_bytes();
    let size = cmd_as_bytes.len() + 1;
    check_and_reserve(buf, size);
    buf.extend(cmd_as_bytes);
    write_n(buf);
}

/// write command and msg in buffer.
///
/// packet format:
/// <command>\n
/// [ 4 byte size in bytes as BigEndian i64 ][ N-byte binary data ]
///
/// https://nsq.io/clients/tcp_protocol_spec.html.
/// command could be PUB or DPUB or any command witch send a message.
pub fn write_msg(buf: &mut BytesMut, msg: String) {
    let msg_as_bytes = msg.as_bytes();
    let msg_len = msg_as_bytes.len();
    let size = 4 + msg_len;
    check_and_reserve(buf, size);
    buf.put_u32_be(msg_len as u32);
    buf.extend(msg_as_bytes);
}

/// write multiple messages (aka msub command).
pub fn write_mmsg(buf: &mut BytesMut, cmd: String, msgs: Vec<String>) {
    write_cmd(buf, cmd);
    buf.put_u32_be(msgs.len() as u32);
    for msg in msgs {
        write_msg(buf, msg);
    }
}

impl Decoder for NsqCodec {
    type Item = Cmd;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let length = buf.len();

        // if length is less than HEADER_LENGTH there is a problem
        if length < HEADER_LENGTH {
            Ok(None)
        } else {
            let mut cursor = Cursor::new(buf.clone());
            let _size = cursor.get_i32_be() as usize;
            // get frame type
            let frame_type: i32 = cursor.get_i32_be();

            // maybe we have a response type frame
            if frame_type == FRAME_TYPE_RESPONSE {
                // clean the buffer
                buf.take();
                if let Ok(s) = str::from_utf8(&cursor.bytes()) {
                        // check for heartbeat
                    if s == HEARTBEAT {
                        Ok(Some(Cmd::Heartbeat))
                    } else {
                        // return response
                        Ok(Some(Cmd::Response(s.to_owned())))
                    }
                } else {
                    // error parsing bytes as utf8
                    Err(Error::Internal("Invalid UTF-8 Data".to_owned()))
                }
            // maybe it is a error type
            } else if frame_type == FRAME_TYPE_ERROR {
                // clean buffer
                buf.take();
                let s = String::from_utf8_lossy(cursor.bytes());
                    // it's a remote error (E_FIN_FAILED, E_REQ_FAILED, E_TOUCH_FAILED)
                Ok(Some(Cmd::ResponseError(s.to_string())))
            // it's a message
            } else if frame_type == FRAME_TYPE_MESSAGE {
                let mut resp_buf = buf.clone();
                let mut msg_buf: Vec<(i64, u16, String, String)> = Vec::new();
                let mut need_more = false;
                loop {
                    if resp_buf.is_empty() { break };
                    if let Some((ts, at, id, bd)) = decode_msg(&mut resp_buf) {
                        msg_buf.push((ts, at, id.to_owned(), bd.to_owned()));
                    } else {
                        need_more = true;
                        break;
                    }
                }
                if need_more {
                    Ok(None)
                } else {
                    buf.take();
                    Ok(Some(Cmd::ResponseMsg(msg_buf)))
                }
            } else {
                Err(Error::Remote("invalid frame type".to_owned()))
            }
        }
    }
}


impl Encoder for NsqCodec {
    type Item = Cmd;
    type Error = io::Error;

    fn encode(&mut self, msg: Self::Item, buf: &mut BytesMut) -> Result<(), Self::Error> {
        match msg {
            Cmd::Magic(ver) => {
                let bytes_ver = ver.as_bytes();
                check_and_reserve(buf, bytes_ver.len());
                buf.extend(bytes_ver);
                Ok(())
            }
            Cmd::Command(cmd) => {
                write_cmd(buf, cmd);
                Ok(())
            },
            Cmd::Msg(cmd, msg) => {
                write_cmd(buf, cmd);
                write_msg(buf, msg);
                Ok(())
            },
            Cmd::MMsg(cmd, msgs) => {
                write_mmsg(buf, cmd, msgs);
                Ok(())
            },
            _ => {
                Err(io::Error::new(io::ErrorKind::Other, "Failed encoding data"))
            },
        }
    }
}
