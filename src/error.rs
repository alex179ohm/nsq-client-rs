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

use std::str;
use std::{error, fmt, io};
use std::time::Duration;

use futures::sync::{mpsc, oneshot};

use codec;

#[derive(Debug)]
pub enum Error {
    /// A Non-Specific internal error than prevented and operation from completing
    Internal(String),

    /// An IO error
    IO(io::Error),

    /// An parsing/serialising error occurred
    Value(String, Option<codec::Cmd>),

    /// An critical Unexpected Error
    Unexpected(String),

    /// End of Stream connection is broken
    EndOfStream,

    /// receive error during reconnecting
    NotConnected,

    /// Cancel all writers after connection get dropped
    Disconnected,

    /// Remote error
    Remote(String),

    /// Processing error on Handler
    Processing(Duration, String),
}

pub fn internal<T: Into<String>>(msg: T) -> Error {
    Error::Internal(msg.into())
}

pub fn value<T: Into<String>>(msg: T, val: codec::Cmd) -> Error {
    Error::Value(msg.into(), Some(val))
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IO(err)
    }
}

impl From<oneshot::Canceled> for Error {
    fn from(err: oneshot::Canceled) -> Error {
        Error::Unexpected(format!("Oneshot was cancelled before use: {}", err))
    }
}

impl<T: 'static + Send> From<mpsc::SendError<T>> for Error {
    fn from(err: mpsc::SendError<T>) -> Error {
        Error::Unexpected(format!("Cannot write to channel: {}", err))
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::IO(ref err) => err.description(),
            Error::Value(ref s, _) => s,
            Error::Unexpected(ref s) => s,
            Error::Internal(ref s) => s,
            Error::EndOfStream => "End of Stream",
            Error::Remote(ref s) => s,
            Error::NotConnected => "Not Connected",
            Error::Disconnected => "Disconnected",
            Error::Processing(_, ref s) => s,
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::IO(ref err) => Some(err),
            Error::Value(_, _) => None,
            Error::Internal(_) => None,
            Error::Unexpected(_) => None,
            Error::EndOfStream => None,
            Error::Remote(_) => None,
            Error::NotConnected => None,
            Error::Disconnected => None,
            Error::Processing(_, _) => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use std::error::Error;
        fmt::Display::fmt(self.description(), f)
    }
}
