//! Request abstractions

use std::{
    mem,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use hyper::{body::HttpBody, header::AsHeaderName, http::request::Parts, upgrade::OnUpgrade};

use crate::{
    body,
    header::{HeaderMap, HeaderValue},
    method::Method,
    uri::Uri,
    version::Version,
    Body, Bytes,
};

pub(crate) type HyperRequest = hyper::Request<Body>;

/// An incoming request with data that is immutable.
#[derive(Debug)]
pub struct Request {
    pub(crate) method: Method,
    pub(crate) uri: Uri,
    pub(crate) version: Version,
    pub(crate) headers: HeaderMap,
    pub(crate) remote_addr: SocketAddr,
}

impl Request {
    pub(crate) fn header(&self, name: impl AsHeaderName) -> Option<&HeaderValue> {
        self.headers.get(name)
    }

    pub(crate) fn header_all(&self, name: impl AsHeaderName) -> impl Iterator<Item = &HeaderValue> {
        self.headers.get_all(name).into_iter()
    }

    /// Returns the full request path
    pub(crate) fn full_path(&self) -> &str {
        self.uri.path()
    }
}

#[derive(Debug)]
pub struct RequestState {
    body: BodyState,
    pub(crate) current_path_index: usize,
    on_upgrade: Option<OnUpgrade>,
}

impl RequestState {
    pub(crate) fn new(body: Body, on_upgrade: Option<OnUpgrade>) -> Self {
        Self {
            body: BodyState::Pending {
                stream: body,
                bytes: Vec::new(),
                len: 0,
            },
            current_path_index: 0,
            on_upgrade,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn on_upgrade(&mut self) -> Option<OnUpgrade> {
        self.on_upgrade.take()
    }

    pub(crate) fn poll_body(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(&[Bytes], usize), body::Error>> {
        match self.body {
            BodyState::Pending {
                ref mut stream,
                ref mut bytes,
                ref mut len,
            } => loop {
                match Pin::new(&mut *stream).as_mut().poll_data(cx) {
                    Poll::Ready(Some(Ok(buf))) => {
                        if !buf.is_empty() {
                            *len += buf.len();
                            bytes.push(buf);
                        }
                    }
                    Poll::Ready(Some(Err(error))) => {
                        self.body = BodyState::Error;
                        break Poll::Ready(Err(body::Error { inner: Some(error) }));
                    }
                    Poll::Ready(None) => {
                        let bytes = mem::take(bytes);
                        self.body = BodyState::Finished { bytes, len: *len };
                        break self.poll_body(cx);
                    }
                    Poll::Pending => break Poll::Pending,
                }
            },
            BodyState::Finished { ref mut bytes, len } => Poll::Ready(Ok((&*bytes, len))),
            BodyState::Error => Poll::Ready(Err(body::Error { inner: None })),
        }
    }

    pub(crate) fn current_path<'f>(&self, request: &'f Request) -> &'f str {
        &request.full_path()[self.current_path_index..]
    }

    pub(crate) fn previous_path<'f>(&self, request: &'f Request) -> &'f str {
        &request.full_path()[..self.current_path_index]
    }

    pub(crate) fn incr_current_path_index(&mut self, index: usize) {
        self.current_path_index += index;
    }

    /// Advances the current path index to the end
    pub(crate) fn end_current_path_index(&mut self, request: &Request) {
        self.current_path_index = request.full_path().len();
    }
}

#[derive(Debug)]
enum BodyState {
    Pending {
        stream: Body,
        bytes: Vec<Bytes>,
        len: usize,
    },
    Finished {
        bytes: Vec<Bytes>,
        len: usize,
    },
    Error,
}

pub(crate) fn from_hyper(
    request: HyperRequest,
    remote_addr: SocketAddr,
) -> (Request, RequestState) {
    let (
        Parts {
            method,
            uri,
            version,
            headers,
            mut extensions,
            ..
        },
        body,
    ) = request.into_parts();

    let state = RequestState::new(body, extensions.remove());
    let request = Request {
        method,
        uri,
        version,
        headers,
        remote_addr,
    };
    (request, state)
}
