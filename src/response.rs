use std::{
    borrow::Cow,
    convert::{Infallible, TryFrom},
};

use hyper::{header::IntoHeaderName, Response as HttpResponse};

use crate::{
    header::{self, HeaderName, HeaderValue},
    Body, StatusCode,
};

/// A response to an HTTP request.
///
/// # Example
///
/// Creating a response:
///
/// ```
/// use myth::{header::HeaderValue, Body, Response};
///
/// let mut response = Response::new(Body::from("Hello World!"));
/// response.headers_mut().insert("Content-Type", HeaderValue::from_static("text/plain; charset=utf-8"));
/// ```
///
/// Creating an equivalent response using a [`Responder`]:
///
/// ```
/// use myth::Responder;
///
/// let response = "Hello World!".into_response();
/// ```
pub type Response = HttpResponse<Body>;

/// Types that can be converted into a [`Response`].
pub trait Responder: Sized {
    /// Creates the [`Response`].
    fn into_response(self) -> Response;

    /// Sets the status code of the response.
    ///
    /// The provided status code may be a [`StatusCode`] or a [`u16`].
    ///
    /// # Panics
    ///
    /// Panics if the status code is provided as a [`u16`] and is not within the range
    /// 100-999, inclusive.
    ///
    /// # Example
    ///
    /// ```
    /// # use myth::{Responder, Response};
    /// let responder = "Could not find the requested page.";
    /// let response: Response = responder.with_status(404);
    /// ```
    ///
    /// This can also be done directly using [`Response::status_mut()`]:
    ///
    /// ```
    /// # use myth::{Responder, Response, StatusCode};
    ///
    /// let mut response: Response = "Could not find the requested page.".into_response();
    /// *response.status_mut() = StatusCode::NOT_FOUND;
    /// ```
    ///
    fn with_status(self, status: impl IntoStatusCode) -> Response {
        let mut response = self.into_response();
        *response.status_mut() = status.into_status_code();
        response
    }

    /// Sets a header of the response.
    ///
    /// If the header name is already present in the response, then all existing existing values are
    /// removed before the value is added.
    ///
    /// The header value may be one of:
    ///  - [`HeaderValue`]
    ///  - <code>&[HeaderValue]</code>
    ///  - [`HeaderName`]
    ///  - [`String`]
    ///  - <code>&'static [str]</code>
    ///  - <code>[Vec]\<[u8]></code>
    ///  - <code>[Box]\<[[u8]]></code>
    ///  - [`u16`]
    ///  - [`i16`]
    ///  - [`u32`]
    ///  - [`i32`]
    ///  - [`u64`]
    ///  - [`i64`]
    ///  - [`usize`]
    ///  - [`isize`]
    ///
    /// # Panics
    ///
    /// Panics if the header value is provided as one of:
    ///  - [`String`]
    ///  - <code>&'static [str]</code>
    ///  - <code>[Vec]\<[u8]></code>
    ///  - <code>[Box]\<[[u8]]></code>
    ///
    /// and the value is invalid.
    ///
    /// # Example
    ///
    /// ```
    /// # use myth::{Responder, Response};
    /// let responder = "Hello World!";
    /// let response: Response = responder.with_header("My-Custom-Header", "Something");
    /// ```
    ///
    /// This can also be done directly using [`Response::headers_mut()`]:
    ///
    /// ```
    /// # use myth::{header::HeaderValue, Responder, Response};
    ///
    /// let mut response: Response = "Hello World!".into_response();
    /// response.headers_mut().insert("My-Custom-Header", HeaderValue::from_static("Something"));
    /// ```
    fn with_header<N, V>(self, name: N, value: V) -> Response
    where
        N: IntoHeaderName,
        V: IntoHeaderValue,
    {
        let mut response = self.into_response();
        response
            .headers_mut()
            .insert(name, value.into_header_value());
        response
    }

    /// Adds a header to the response.
    ///
    /// The header value is added in addition to any existing values of the header name.
    ///
    /// The header value may be one of:
    ///  - [`HeaderValue`]
    ///  - <code>&[HeaderValue]</code>
    ///  - [`HeaderName`]
    ///  - [`String`]
    ///  - <code>&'static [str]</code>
    ///  - <code>[Vec]\<[u8]></code>
    ///  - <code>[Box]\<[[u8]]></code>
    ///  - [`u16`]
    ///  - [`i16`]
    ///  - [`u32`]
    ///  - [`i32`]
    ///  - [`u64`]
    ///  - [`i64`]
    ///  - [`usize`]
    ///  - [`isize`]
    ///
    /// # Panics
    ///
    /// Panics if the header value is provided as one of:
    ///  - [`String`]
    ///  - <code>&'static [str]</code>
    ///  - <code>[Vec]\<[u8]></code>
    ///  - <code>[Box]\<[[u8]]></code>
    ///
    /// and the value is invalid.
    ///
    /// # Example
    ///
    /// ```
    /// # use myth::{Responder, Response};
    /// let responder = "Hello World!";
    /// let response: Response = responder.add_header("My-Custom-Header", "Something")
    ///     .add_header("My-Custom-Header", "Another-Thing");
    /// ```
    ///
    /// This can also be done directly using [`Response::headers_mut()`]:
    ///
    /// ```
    /// # use myth::{header::HeaderValue, Responder, Response};
    ///
    /// let mut response: Response = "Hello World!".into_response();
    /// response.headers_mut().append("My-Custom-Header", HeaderValue::from_static("Something"));
    /// response.headers_mut().append("My-Custom-Header", HeaderValue::from_static("Another-Thing"));
    /// ```
    fn add_header<N, V>(self, name: N, value: V) -> Response
    where
        N: IntoHeaderName,
        V: IntoHeaderValue,
    {
        let mut response = self.into_response();
        response
            .headers_mut()
            .append(name, value.into_header_value());
        response
    }
}

pub trait IntoStatusCode {
    fn into_status_code(self) -> StatusCode;
}

impl IntoStatusCode for StatusCode {
    fn into_status_code(self) -> StatusCode {
        self
    }
}

impl IntoStatusCode for u16 {
    fn into_status_code(self) -> StatusCode {
        StatusCode::from_u16(self).expect("invalid status code")
    }
}

pub trait IntoHeaderValue {
    fn into_header_value(self) -> HeaderValue;
}

impl IntoHeaderValue for HeaderValue {
    fn into_header_value(self) -> HeaderValue {
        self
    }
}

impl IntoHeaderValue for &HeaderValue {
    fn into_header_value(self) -> HeaderValue {
        self.clone()
    }
}

impl IntoHeaderValue for HeaderName {
    fn into_header_value(self) -> HeaderValue {
        HeaderValue::from_name(self)
    }
}

impl IntoHeaderValue for String {
    fn into_header_value(self) -> HeaderValue {
        HeaderValue::try_from(self).expect("invalid header value")
    }
}

impl IntoHeaderValue for &'static str {
    fn into_header_value(self) -> HeaderValue {
        HeaderValue::from_static(self)
    }
}

impl IntoHeaderValue for Vec<u8> {
    fn into_header_value(self) -> HeaderValue {
        HeaderValue::try_from(self).expect("invalid header value")
    }
}

impl IntoHeaderValue for Box<[u8]> {
    fn into_header_value(self) -> HeaderValue {
        Vec::<u8>::from(self).into_header_value()
    }
}

macro_rules! into_header_value_integers {
    ($($integer:ident),+) => {
        $(
            impl IntoHeaderValue for $integer {
                fn into_header_value(self) -> HeaderValue {
                    HeaderValue::from(self)
                }
            }
        )+
    };
}

into_header_value_integers!(u16, i16, u32, i32, u64, i64, usize, isize);

impl Responder for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl Responder for Infallible {
    fn into_response(self) -> Response {
        match self {}
    }
}

static TEXT_PLAIN: HeaderValue = HeaderValue::from_static("text/plain; charset=utf-8");
static APPLICATION_OCTET_STREAM: HeaderValue = HeaderValue::from_static("application/octet-stream");

macro_rules! responder_impls {
    ($content_type:ident => $($type:ty),+) => {
        $(
            impl Responder for $type {
                fn into_response(self) -> Response {
                    Response::new(self.into())
                        .with_header(header::CONTENT_TYPE, $content_type.clone())
                }
            }
        )+
    };
}

responder_impls!(TEXT_PLAIN => &'static str, String, Cow<'static, str>);
responder_impls!(APPLICATION_OCTET_STREAM => &'static [u8], Vec<u8>, Cow<'static, [u8]>);

impl<R: Responder> Responder for (StatusCode, R) {
    fn into_response(self) -> Response {
        self.1.with_status(self.0)
    }
}

pub(crate) fn default_response(status: StatusCode) -> Response {
    status
        .canonical_reason()
        .expect("Status code should have a canonical reason")
        .with_status(status)
}

static TEXT_HTML: HeaderValue = HeaderValue::from_static("text/html; charset=utf-8");

/// Creates a [`Response`] with a `Content-Type` of `text/html; charset=utf-8`
///
/// # Example
/// ```
/// use myth::{html, Response};
///
/// let body = "<h1>Hello!</h1>";
///
/// let response: Response = html(body);
/// ```
pub fn html(responder: impl Responder) -> Response {
    responder.with_header(header::CONTENT_TYPE, TEXT_HTML.clone())
}
