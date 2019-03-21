use std::io::{self, Read, Write};

use actix::{Context, ActorFuture};
use futures::{Async, Future, Poll, try_ready};
use native_tls::{self, Error, HandshakeError};
use tokio::io::{AsyncRead, AsyncWrite};
use crate::Connection;

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
        //try_ready!(self.inner.shutdown());
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

impl<S: AsyncRead + AsyncWrite> ActorFuture for TlsHandsnake<S> {
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