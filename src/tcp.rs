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

use std::net::SocketAddr;
use std::time::Duration;
use std::vec::IntoIter;

use actix::prelude::*;
use actix::clock;
use tokio::timer::Delay;
use tokio::net::tcp::{
    TcpStream,
    ConnectFuture,
};
use futures::{Future, Async, Poll};

use crate::conn::{
    Connection,
    ConnectionError,
};

pub struct TcpConnector {
    addrs: IntoIter<SocketAddr>,
    timeout: Delay,
    stream: Option<ConnectFuture>
}

impl TcpConnector {
    pub fn new(addrs: IntoIter<SocketAddr>) -> TcpConnector {
        TcpConnector::with_timeout(addrs, Duration::from_secs(2))
    }

    fn with_timeout(addrs: IntoIter<SocketAddr>, timeout: Duration) -> TcpConnector {
        TcpConnector {
            addrs,
            stream: None,
            timeout: Delay::new(clock::now() + timeout),
        }
    }
}

impl ActorFuture for TcpConnector {
    type Item = TcpStream;
    type Error = ConnectionError;
    type Actor = Connection;

    fn poll(
        &mut self,
        _: &mut Connection,
        _: &mut Context<Connection>,
    ) -> Poll<Self::Item, Self::Error> {
        if let Ok(Async::Ready(_)) = self.timeout.poll() {
            return Err(ConnectionError::Timeout);
        }

        // connect
        loop {
            if let Some(new) = self.stream.as_mut() {
                match new.poll() {
                    Ok(Async::Ready(sock)) => return Ok(Async::Ready(sock)),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(err) => {
                        if self.addrs.as_slice().is_empty() {
                            return Err(ConnectionError::IoError(err));
                        }
                    }
                }
            }

            let addr = self.addrs.next().unwrap();
            self.stream = Some(TcpStream::connect(&addr));
        }
    }
}
