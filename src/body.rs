//! [`Filter`]s that extract the body of a request.
//!
//! For more, see JSON or forms.

use std::{
    collections::VecDeque,
    fmt,
    future::Future,
    io,
    io::Read,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::ready;
use hyper::{body::Buf, Error as HyperError};

use crate::{
    cloning,
    errors::{BoxedFilterError, FilterError},
    filter::{FilterExecute, FilterSealed},
    header::{self, HeaderValue},
    impl_Filter,
    outcome::RequestOutcome,
    request::{Request, RequestState},
    response::default_response,
    Bytes, Filter, FilterBase, Response, Result, StatusCode,
};

/// An error that occured while extracting the body of a request
#[derive(Debug)]
pub struct Error {
    pub(crate) inner: Option<HyperError>,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.inner {
            Some(error) => write!(f, "error while reading request body: {}", error),
            None => write!(f, "error occured previously while request reading body"),
        }
    }
}

impl Error {
    #[must_use]
    pub fn into_inner(self) -> Option<HyperError> {
        self.inner
    }
}

impl FilterError for Error {
    fn into_response(self: Box<Self>) -> Response {
        tracing::debug!("Default response for unhandled {}", self);
        default_response(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

/// Extracts the raw body of the request
pub fn all() -> impl_Filter!(impl Buf + Read => Copy + (fmt::Debug)) {
    #[derive(Copy, Clone, Debug)]
    struct TakeBody;

    impl FilterSealed for TakeBody {}

    impl<'f> FilterBase<'f> for TakeBody {
        type Input = ();

        type Success = (BytesBuf,);
    }

    impl<'f> FilterExecute<'f> for TakeBody {
        type Future = TakeBodyFuture;

        fn execute(
            &'f self,
            _: &'f Request,
            request_state: RequestState,
            (): Self::Input,
        ) -> Self::Future {
            TakeBodyFuture {
                request_state: Some(request_state),
            }
        }
    }

    struct TakeBodyFuture {
        request_state: Option<RequestState>,
    }

    impl Future for TakeBodyFuture {
        type Output = RequestOutcome<(), (BytesBuf,)>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let result = ready!(self.request_state.as_mut().unwrap().poll_body(cx));
            let outcome = result
                .map(|(bytes, len)| {
                    (BytesBuf {
                        bytes: bytes.iter().cloned().collect(),
                        len,
                    },)
                })
                .map_err(BoxedFilterError::from)
                .into();

            Poll::Ready(RequestOutcome {
                request_state: self.request_state.take().unwrap(),
                outcome,
            })
        }
    }

    TakeBody
}

#[derive(Clone, Debug)]
struct BytesBuf {
    bytes: VecDeque<Bytes>,
    len: usize,
}

impl Buf for BytesBuf {
    fn remaining(&self) -> usize {
        self.len
    }

    fn chunk(&self) -> &[u8] {
        self.bytes.front().map(Bytes::as_ref).unwrap_or_default()
    }

    fn advance(&mut self, mut cnt: usize) {
        if cnt > self.len {
            panic!("cannot advance past end of buffer")
        }
        self.len -= cnt;
        while let Some(front) = self.bytes.front_mut() {
            if cnt < front.len() {
                front.advance(cnt);
                break;
            }
            cnt -= front.len();
            self.bytes.pop_front();
        }
    }
}

impl Read for BytesBuf {
    fn read(&mut self, mut buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining() < buf.len() {
            buf = &mut buf[..self.remaining()];
        }
        self.copy_to_slice(buf);
        Ok(buf.len())
    }
}

#[derive(Debug)]
pub struct ContentLengthError {
    length: usize,
}

impl ContentLengthError {
    #[must_use]
    pub fn length(&self) -> usize {
        self.length
    }
}

impl FilterError for ContentLengthError {
    fn into_response(self: Box<Self>) -> Response {
        tracing::debug!("content-length was too large ({})", self.length);
        default_response(StatusCode::PAYLOAD_TOO_LARGE)
    }
}

pub fn content_length_limit(limit: usize) -> impl_Filter!(() => Clone + (fmt::Debug)) {
    async fn handler(option: Option<&HeaderValue>, limit: usize) -> Result<()> {
        match option {
            Some(value) => {
                let length = value
                    .to_str()
                    .ok()
                    .and_then(|str| str.parse::<usize>().ok())
                    .expect("content-Length should have been checked by Hyper");
                if length <= limit {
                    Ok(())
                } else {
                    Err(ContentLengthError { length }.into())
                }
            }
            None => Ok(()),
        }
    }
    header::value_optional(header::CONTENT_LENGTH)
        .and(cloning(limit))
        .handle(handler)
        .untuple()
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, io, io::Read};

    use super::BytesBuf;
    use crate::Bytes;

    #[test]
    fn read_bytes_buf() -> io::Result<()> {
        let bytes: &[&'static [u8]] = &[
            b"abcdefg", b"hij", b"klmn", b"o", b"pq", b"r", b"s", b"tuv", b"w", b"x", b"yz",
        ];
        let bytes: VecDeque<Bytes> = bytes.iter().copied().map(Bytes::from_static).collect();
        let len = bytes.iter().map(Bytes::len).sum();
        let mut reader = BytesBuf { bytes, len };

        let mut buf = [0; 4];
        assert_eq!(reader.read(&mut buf)?, 4);
        assert_eq!(&buf, b"abcd");

        let mut buf = [8; 3];
        assert_eq!(reader.read(&mut buf)?, 3);
        assert_eq!(&buf, b"efg");

        assert_eq!(reader.read(&mut [])?, 0);

        let mut buf = [0; 2];
        assert_eq!(reader.read(&mut buf)?, 2);
        assert_eq!(&buf, b"hi");

        let mut buf = [0; 6];
        assert_eq!(reader.read(&mut buf)?, 6);
        assert_eq!(&buf, b"jklmno");

        let mut buf = [1; 3];
        assert_eq!(reader.read(&mut buf)?, 3);
        assert_eq!(&buf, b"pqr");

        let mut buf = [0; 5];
        assert_eq!(reader.read(&mut buf)?, 5);
        assert_eq!(&buf, b"stuvw");

        assert_eq!(reader.read(&mut [])?, 0);

        let mut buf = [0; 3];
        assert_eq!(reader.read(&mut buf)?, 3);
        assert_eq!(&buf, b"xyz");

        assert_eq!(reader.read(&mut [])?, 0);

        assert_eq!(reader.read(&mut [0; 6])?, 0);

        let bytes: VecDeque<Bytes> = IntoIterator::into_iter(["12345", "678", "90"])
            .map(str::to_owned)
            .map(Bytes::from)
            .collect();
        let mut reader = BytesBuf { bytes, len: 10 };

        let mut buf = [0; 2];
        assert_eq!(reader.read(&mut buf)?, 2);
        assert_eq!(&buf, b"12");

        let mut buf = [b'-'; 10];
        assert_eq!(reader.read(&mut buf)?, 8);
        assert_eq!(&buf, b"34567890--");

        assert_eq!(reader.read(&mut [b'$'; 10])?, 0);

        assert_eq!(reader.read(&mut [])?, 0);

        Ok(())
    }
}
