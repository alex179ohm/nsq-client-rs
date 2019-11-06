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

use bytes::{BufMut, BytesMut};
use std::str;

use byteorder::{BigEndian, ByteOrder};
use log::error;

pub const HEADER_LENGTH: usize = 8;

// Frame Types
pub const FRAME_TYPE_RESPONSE: i32 = 0x00;
pub const FRAME_TYPE_ERROR: i32 = 0x01;
pub const FRAME_TYPE_MESSAGE: i32 = 0x02;

pub const HEARTBEAT: &str = "_heartbeat_";

#[derive(Debug, PartialEq, Clone)]
pub enum Response {
    Response(String),
    Error(String),
}

pub fn decode_msg(buf: &mut BytesMut) -> (i64, u16, String, Vec<u8>) {
    // skip size and frame type
    let timestamp = BigEndian::read_i64(&buf.split_to(8)[..]);
    let attemps = BigEndian::read_u16(&buf.split_to(2)[..]);
    let id_bytes = &buf.split_to(16)[..];
    let id = match str::from_utf8(id_bytes) {
        Ok(s) => s,
        Err(e) => {
            error!("error deconding utf8 id: {}", e);
            ""
        }
    };
    (timestamp, attemps, id.to_owned(), Vec::from(&buf[..]))
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

pub fn write_magic(buf: &mut BytesMut, cmd: &str) {
    let cmd_as_bytes = cmd.as_bytes();
    let size = cmd_as_bytes.len();
    check_and_reserve(buf, size);
    buf.extend(cmd_as_bytes);
}

/// write command in buffer and append 0x2 ('\n')
pub fn write_cmd(buf: &mut BytesMut, cmd: &str) {
    let cmd_as_bytes = cmd.as_bytes();
    let size = cmd_as_bytes.len() + 1;
    check_and_reserve(buf, size);
    buf.extend(cmd_as_bytes);
    write_n(buf);
}

pub fn write_msg(buf: &mut BytesMut, msg: Vec<u8>) {
    let msg_as_bytes = msg.as_slice();
    let msg_len = msg_as_bytes.len();
    let size = 4 + msg_len;
    check_and_reserve(buf, size);
    buf.put_u32_be(msg_len as u32);
    buf.extend(msg_as_bytes);
}

/// write multiple messages (aka msub command).
pub fn write_mmsg(buf: &mut BytesMut, msgs: Vec<Vec<u8>>) {
    let len = msgs.len();
    let body_size = (msgs.iter().fold(0, |sum, e| sum + (e.len() + 4) as i32) + 4) as usize;
    check_and_reserve(buf, body_size + 4);
    buf.put_u32_be(body_size as u32);
    buf.put_u32_be(len as u32);
    for msg in msgs {
        write_msg(buf, msg);
    }
}
