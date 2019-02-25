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
//! Source: https://github.com/alex179ohm/nsq-client-rs/blob/master/src/codec.rs

use std::io::{self, Cursor};
use std::str;

use bytes::{Buf, BufMut, BytesMut};
use log::{error, info};
use tokio_io::codec::{Decoder, Encoder};

use crate::error::Error;

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
    ResponseMsg(Vec<(i64, u16, String, Vec<u8>)>),

    /// A simple Command whitch not sends msg.
    Command(String),

    /// A simple message (pub or dpub).
    Msg(String, String),

    /// Multiple message (mpub)
    MMsg(String, Vec<String>),
}

/// NSQ codec
pub struct NsqCodec;

pub fn decode_msg(buf: &mut BytesMut) -> Option<(i64, u16, String, Vec<u8>)> {
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
            let id_body_bytes = &cursor.bytes()[..size - HEADER_LENGTH - 6];
            if id_body_bytes.len() < 16 {
                return None;
            }
            let (id_bytes, body_bytes) = id_body_bytes.split_at(16);
            let id = match str::from_utf8(id_bytes) {
                Ok(s) => s,
                Err(e) => {
                    error!("error deconding utf8 id: {}", e);
                    return None;
                }
            };
            // clean the buffer at frame size
            buf.split_to(size + 4);
            Some((timestamp, attemps, id.to_owned(), Vec::from(body_bytes)))
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

/// write command in buffer and append 0x2 ('\n')
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
                match str::from_utf8(&cursor.bytes()) {
                    // check for heartbeat
                    Ok(s) => {
                    if s == HEARTBEAT {
                        info!("heartbeat");
                        return Ok(Some(Cmd::Heartbeat))
                    } else {
                        // return response
                        return Ok(Some(Cmd::Response(s.to_owned())))
                    }
                },
                Err(e) => {
                    // error parsing bytes as utf8
                    return Err(Error::Internal(format!("Invalid UTF-8 Data: {}", e)));
                },
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
                let mut msg_buf: Vec<(i64, u16, String, Vec<u8>)> = Vec::new();
                let mut need_more = false;
                info!("new message arrived: {:?}", resp_buf);
                loop {
                    if resp_buf.is_empty() {
                        break;
                    };
                    if let Some((ts, at, id, bd)) = decode_msg(&mut resp_buf) {
                        msg_buf.push((ts, at, id.to_owned(), bd));
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
                info!("fin sent codec");
                Ok(())
            }
            Cmd::Msg(cmd, msg) => {
                write_cmd(buf, cmd);
                write_msg(buf, msg);
                Ok(())
            }
            Cmd::MMsg(cmd, msgs) => {
                write_mmsg(buf, cmd, msgs);
                Ok(())
            }
            _ => Err(io::Error::new(io::ErrorKind::Other, "Failed encoding data")),
        }
    }
}
