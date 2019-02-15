#[cfg(test)]
extern crate bytes;
extern crate nsq_client;
extern crate tokio_io;

use bytes::{BufMut, BytesMut};
use nsq_client::{NsqCodec, Cmd};
use tokio_io::codec::{Encoder, Decoder};

const FRAME_TYPE_RESPONSE: i32 = 0x00;
const FRAME_TYPE_ERROR: i32 = 0x01;
const FRAME_TYPE_MESSAGE: i32 = 0x02;

fn gen_frame_header(buf: &mut BytesMut, size: usize, frame_type: i32) {
    if buf.remaining_mut() < size + 8 {
        buf.reserve(size);
    }
    buf.put_i32_be(size as i32);
    buf.put_i32_be(frame_type);
}

fn gen_frame_response(buf: &mut BytesMut, resp: &str ) {
    let bytes = resp.as_bytes();
    gen_frame_header(buf, bytes.len(), FRAME_TYPE_RESPONSE);
    buf.extend(bytes);
}

fn gen_frame_error(buf: &mut BytesMut, err: &str) {
    let bytes = err.as_bytes();
    gen_frame_header(buf, bytes.len(), FRAME_TYPE_ERROR);
    buf.extend(bytes);
}

fn gen_frame_message(buf: &mut BytesMut, msg: &str, id: &str) {
    let bytes = msg.as_bytes();
    let bytes_id = id.as_bytes();
    gen_frame_header(buf, bytes.len() + 26, FRAME_TYPE_MESSAGE);
    buf.put_i64_be(0); // timestamp (8 bytes)
    buf.put_u16_be(0); // attemps (2 bytes)
    buf.extend(bytes_id); // message ID (16 byte)
    buf.extend(bytes);
}

macro_rules! encode_cmd_tests {
    ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (input, exp) = $value;
                let i = Cmd::Command(input.to_owned());
                let mut codec = NsqCodec{};
                let mut buf = BytesMut::new();
                if codec.encode(i, &mut buf).is_ok() {
                    assert_eq!(exp, &buf[..]);
                }
            }
        )*

    }
}

macro_rules! encode_msg_tests {
    ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (cmd, msg, exp) = $value;
                let i = Cmd::Msg(cmd.to_owned(), msg.to_owned());
                let mut codec = NsqCodec{};
                let mut buf = BytesMut::new();
                if codec.encode(i, &mut buf).is_ok() {
                    assert_eq!(exp, &buf[..]);
                }
            }
        )*
    }
}

macro_rules! encode_mmsg_tests {
    ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (cmd, msg, exp) = $value;
                let msgs: Vec<String> = msg.into_iter().map(|x| x.to_owned()).collect();
                let i = Cmd::MMsg(cmd.to_owned(), msgs);
                let mut codec = NsqCodec{};
                let mut buf = BytesMut::new();
                if codec.encode(i, &mut buf).is_ok() {
                    assert_eq!(exp, &buf[..]);
                }
            }
        )*
    }
}

encode_cmd_tests! {
    encode_sub_test: ("SUB topic channel", b"SUB topic channel\n"),
    encode_v2_test: ("  V2", b"  V2\n"),
    encode_nop_test: ("NOP", b"NOP\n"),
    encode_cls_test: ("CLS", b"CLS\n"),
    encode_req_test: ("REQ 03ff", b"REQ 03ff\n"),
    encode_dry_test: ("DRY 0", b"DRY 0\n"),
}

encode_msg_tests! {
    encode_indentify_test: ("IDENTIFY", "{}", &[73, 68, 69, 78, 84, 73, 70, 89, 10, 0, 0, 0, 2, 123, 125]),
    encode_pub_test: ("PUB test", "hello world", &[80, 85, 66, 32, 116, 101, 115, 116, 10, 0, 0, 0, 11, 104, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100]),
    encode_pub_utf8_test: ("PUB test", "©¥£", &[80, 85, 66, 32, 116, 101, 115, 116, 10, 0, 0, 0, 6, 194, 169, 194, 165, 194, 163]),
}

encode_mmsg_tests! {
    encode_mpub_test: ("MPUB test", vec!["hello", "world"], &[77, 80, 85, 66, 32, 116, 101, 115, 116, 10, 0, 0, 0, 2, 0, 0, 0, 5, 104, 101, 108, 108, 111, 0, 0, 0, 5, 119, 111, 114, 108, 100]),
}

#[test]
fn decode_resp_test() {
    let mut buf = BytesMut::new();
    let mut codec = NsqCodec{};
    gen_frame_response(&mut buf, "OK");
    match codec.decode(&mut buf) {
        Ok(r) => {
            match r.unwrap() {
                Cmd::Response(_) => {
                    ()
                },
                _ => { panic!("Expected Response found ResponseMsg"); },
            }
        },
        Err(err) => { panic!("Expected Ok found: {:?}", err) },
    }
}

#[test]
fn decode_error_test() {
    let mut buf = BytesMut::new();
    let mut codec = NsqCodec{};
    gen_frame_error(&mut buf, "E_INVALID");
    match codec.decode(&mut buf) {
        Ok(r) => { panic!("expected Error found: {:?}", r) },
        Err(_) => { () },
    }
}

#[test]
fn decode_msg_test() {
    let mut buf = BytesMut::new();
    let mut codec = NsqCodec{};
    gen_frame_message(&mut buf, "Hello World", "00000000000000ff");
    match codec.decode(&mut buf) {
        Ok(r) => {
            match r.unwrap() {
                Cmd::ResponseMsg(_) => {
                    ()
                },
                _ => { panic!("Expected Message found Response") },
            }
        },
        Err(err) => {
            panic!("Expected Ok found: {:?}", err);
        }
    }
}

#[test]
fn decode_msg_utf8_test() {
    let mut buf = BytesMut::new();
    let mut codec = NsqCodec{};
    gen_frame_message(&mut buf, "®µ¶", "00000000000000ff");
    match codec.decode(&mut buf) {
        Ok(r) => {
            match r.unwrap() {
                Cmd::ResponseMsg(_) => {
                    ()
                },
                _ => { panic!("Expected Message found Response") },
            }
        },
        Err(err) => {
            panic!("Expected Ok found: {:?}", err);
        }
    }
}
