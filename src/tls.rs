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

use std::io::{self, Read, Write};

use actix::{Context, ActorFuture};
use futures::{Async, Poll};
use native_tls::{
    self,
    Error,
    HandshakeError,
    Protocol,
    Identity,
};
use tokio::io::{AsyncRead, AsyncWrite};
use crate::Connection;

pub type TlsConfig = Identity;


pub fn protocol(s: &str) -> Protocol {
    match s {
        "ssl3.0" => Protocol::Sslv3,
        "tls1.0" => Protocol::Tlsv10,
        "tls1.1" => Protocol::Tlsv11,
        "tls1.2" => Protocol::Tlsv12,
        _ => Protocol::Tlsv12,
    }
}

#[derive(Debug)]
pub struct TlsStream<S> {
    inner: native_tls::TlsStream<S>,
}

impl<S> TlsStream<S> {
    pub fn get_ref(&self) -> &native_tls::TlsStream<S> {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut native_tls::TlsStream<S> {
        &mut self.inner
    }
}

impl<S: Read + Write> Read for TlsStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<S: Read + Write> Write for TlsStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<S: AsyncRead + AsyncWrite> AsyncRead for TlsStream<S> {}

impl<S: AsyncRead + AsyncWrite> AsyncWrite for TlsStream<S> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.inner.get_mut().shutdown()
    }
}



pub struct TlsHandsnake<S> {
    inner: Option<Result<native_tls::TlsStream<S>, HandshakeError<S>>>,
}

impl<S> TlsHandsnake<S> {
    pub fn connect(&self, domain: &str, stream: S, connector: native_tls::TlsConnector) -> TlsHandsnake<S>
    where
        S: AsyncRead + AsyncWrite,
    {
        TlsHandsnake {
            inner: Some(connector.connect(domain, stream)),
        }
    }
}

impl<S> ActorFuture for TlsHandsnake<S>
where
    S: AsyncRead + AsyncWrite,
{
   type Item = TlsStream<S>;
   type Error = Error;
   type Actor = Connection;
   fn poll(
       &mut self,
       _: &mut Connection,
       _: &mut Context<Connection>,
   ) -> Poll<TlsStream<S>, Error> {
       match self.inner.take().expect("could not call Handshake two times") {
           Ok(stream) => Ok(TlsStream { inner: stream }.into()),
           Err(HandshakeError::Failure(e)) => Err(e),
           Err(HandshakeError::WouldBlock(s)) => match s.handshake() {
               Ok(stream) => Ok(TlsStream { inner: stream }.into()),
               Err(HandshakeError::Failure(e)) => Err(e),
               Err(HandshakeError::WouldBlock(s)) => {
                   self.inner = Some(Err(HandshakeError::WouldBlock(s)));
                   Ok(Async::NotReady)
               }
           }
       }
   }
}
