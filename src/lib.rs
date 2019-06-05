#![feature(async_await, type_ascription)]
extern crate bytes;
extern crate log;
extern crate mio;
extern crate webpki;
extern crate webpki_roots;

//#[cfg(feature = "async")]
//mod async_context;
mod client;
mod codec;
mod config;
mod conn;
mod msgs;
mod producer;
mod reader;
mod tls;

pub use client::{Client, Context};
pub use config::{Config, VerifyServerCert};
pub use conn::Conn;
pub use msgs::{Cls, Dpub, Fin, Mpub, Msg, NsqCmd, Pub, Requeue, Touch};
pub use producer::Producer;
pub use reader::Consumer;
