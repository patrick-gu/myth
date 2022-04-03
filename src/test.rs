//! Utilities to test [`Filter`]s.

use std::{convert::TryInto, net::SocketAddr};

use crate::{
    errors::Recoverable,
    header::{self, HeaderMap, HeaderName, HeaderValue},
    method::Method,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    traits::{NonEmptyTupleFor, TupleFnOnceFor},
    uri::Uri,
    version::Version,
    Body, Bytes, Filter, FilterBase, Forwarding, Responder,
};

#[derive(Debug)]
#[must_use]
pub struct RequestBuilder<Input> {
    method: Method,
    uri: Uri,
    version: Version,
    headers: HeaderMap,
    remote_addr: SocketAddr,
    body: Body,
    input: Input,
}

impl RequestBuilder<()> {
    /// Creates a new, default [`RequestBuilder`].
    /// The request uses `GET / HTTP/1.1`.
    pub fn new() -> Self {
        Self {
            method: Method::GET,
            uri: Uri::from_static("/"),
            version: Version::HTTP_11,
            headers: HeaderMap::new(),
            remote_addr: ([0, 0, 0, 0], 0).into(),
            body: Body::empty(),
            input: (),
        }
    }

    /// Set what the [`Filter`] will [consume](FilterBase::Input)
    pub fn input<Input>(self, input: Input) -> RequestBuilder<Input> {
        RequestBuilder {
            method: self.method,
            uri: self.uri,
            version: self.version,
            headers: self.headers,
            remote_addr: self.remote_addr,
            body: self.body,
            input,
        }
    }
}

impl Default for RequestBuilder<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Input> RequestBuilder<Input> {
    /// Sets the HTTP request [method](Method).
    ///
    /// Defaults to [`GET`](Method::GET).
    ///
    /// # Panics
    /// Panics if the method provided was not valid.
    ///
    /// # Example
    /// ```
    /// # use myth::test::RequestBuilder;
    /// RequestBuilder::new()
    ///     .method("POST");
    /// ```
    pub fn method(mut self, method: impl TryInto<Method>) -> Self {
        self.method = match method.try_into() {
            Ok(method) => method,
            Err(_) => panic!("Invalid request method provided"),
        };
        self
    }

    /// Sets the request [uri](Uri).
    ///
    /// # Panics
    /// Panics if the URI provided was not valid.
    ///
    /// # Example
    /// ```
    /// # use myth::test::RequestBuilder;
    /// RequestBuilder::new()
    ///     .uri("/foo");
    /// ```
    pub fn uri(mut self, uri: impl TryInto<Uri>) -> Self {
        self.uri = match uri.try_into() {
            Ok(uri) => uri,
            Err(_) => panic!("Invalid request URI provided"),
        };
        self
    }

    /// Sets the HTTP request [version](Version).
    ///
    /// # Example
    /// ```
    /// # use myth::test::RequestBuilder;
    /// use myth::version::Version;
    ///
    /// RequestBuilder::new()
    ///     .version(Version::HTTP_10);
    /// ```
    pub fn version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    /// Appends a header to this request.
    ///
    /// This will not replace an existing header with the same name.
    ///
    /// # Panics
    ///
    /// Panics if the provided header name or value was not valid.
    ///
    /// # Example
    /// ```
    /// # use myth::test::RequestBuilder;
    /// use myth::version::Version;
    ///
    /// RequestBuilder::new()
    ///     .header("User-Agent", "User");
    /// ```
    pub fn header(
        mut self,
        name: impl TryInto<HeaderName>,
        value: impl TryInto<HeaderValue>,
    ) -> Self {
        let name = match name.try_into() {
            Ok(name) => name,
            Err(_) => panic!("Invalid request header name provided"),
        };
        let value = match value.try_into() {
            Ok(value) => value,
            Err(_) => panic!("Invalid request header value provided"),
        };
        self.headers.append(name, value);
        self
    }

    /// Sets the request's origin remote address.
    ///
    /// # Example
    /// ```
    /// # use myth::test::RequestBuilder;
    /// use myth::version::Version;
    ///
    /// RequestBuilder::new()
    ///     .remote_addr(([127, 0, 0, 1], 12345));
    /// ```
    pub fn remote_addr(mut self, addr: impl Into<SocketAddr>) -> Self {
        self.remote_addr = addr.into();
        self
    }

    /// Sets the request's body.
    ///
    /// # Example
    /// ```
    /// use myth::test;
    ///
    /// test::post()
    ///     .body("foo");
    /// ```
    pub fn body(mut self, body: impl Into<Body>) -> Self {
        self.body = body.into();
        self
    }

    /// Sets the request's body to JSON, and sets the `Content-Type` to `application/json
    ///
    /// # Example
    /// ```
    /// use myth::test;
    /// use serde_json::json;
    ///
    /// test::post()
    ///     .json(json!({ "foo": [8, 9] }));
    /// ```
    #[cfg(feature = "json")]
    #[cfg_attr(myth_docs, doc(cfg(feature = "json")))]
    pub fn json(self, data: impl serde::Serialize) -> Self {
        let string = serde_json::to_string(&data).expect("Failed to serialize JSON request body");
        self.body(string)
            .header(header::CONTENT_TYPE, "application/json")
    }

    pub async fn success<T, F>(self, filter: &T, func: F)
    where
        T: Filter + for<'f> FilterBase<'f, Input = Input>,
        for<'f> <T as FilterBase<'f>>::Success: NonEmptyTupleFor<'f>,
        F: for<'f> TupleFnOnceFor<'f, <T as FilterBase<'f>>::Success>,
    {
        let (request, request_state, input) = self.into_args();
        let RequestOutcome { outcome, .. } = filter.execute(&request, request_state, input).await;
        match_outcome(outcome, move |success| {
            func.call(success);
        });
    }

    pub async fn succeeds<T>(self, filter: &T)
    where
        T: Filter + for<'f> FilterBase<'f, Input = Input, Success = ()>,
    {
        let (request, request_state, input) = self.into_args();
        let RequestOutcome { outcome, .. } = filter.execute(&request, request_state, input).await;
        match_outcome(outcome, |_| ());
    }

    /// Gets the response for a [`Filter`].
    ///
    /// # Example
    /// ```
    /// # use myth::Filter;
    /// # #[tokio::main]
    /// # async fn main() {
    /// let filter = myth::any().handle(|| async { Ok("Hello!") });
    ///
    /// let response = myth::test::get()
    ///     .response(&filter)
    ///     .await;
    ///
    /// assert_eq!(response.status(), 200);
    /// assert_eq!(response.body(), "Hello!");
    /// # }
    /// ```
    pub async fn response<T, R>(self, filter: &T) -> hyper::Response<Bytes>
    where
        T: Filter + for<'f> FilterBase<'f, Input = Input, Success = (R,)>,
        R: Responder,
    {
        let (request, request_state, input) = self.into_args();
        let RequestOutcome { outcome, .. } = filter.execute(&request, request_state, input).await;
        let response = match_outcome(outcome, |(responder,)| responder.into_response());
        let (parts, body) = response.into_parts();
        let bytes = hyper::body::to_bytes(body)
            .await
            .expect("Failed to read body as bytes");
        hyper::Response::from_parts(parts, bytes)
    }

    pub async fn error<T, R>(self, filter: &T) -> R
    where
        T: Filter + for<'f> FilterBase<'f, Input = Input>,
        R: Recoverable,
    {
        let (request, request_state, input) = self.into_args();
        let RequestOutcome { outcome, .. } = filter.execute(&request, request_state, input).await;
        match outcome {
            Outcome::Success(_) => panic!("Expected error, instead got success"),
            Outcome::Error(error) => R::recover(error).expect("Got wrong error type"),
            Outcome::Forward { forwarding, .. } => {
                panic!("Expected error, instead got forwarding {:?}", forwarding)
            }
        }
    }

    pub async fn not_found<T>(self, filter: &T)
    where
        T: Filter + for<'f> FilterBase<'f, Input = Input>,
    {
        let (request, request_state, input) = self.into_args();
        let RequestOutcome { outcome, .. } = filter.execute(&request, request_state, input).await;
        match outcome {
            Outcome::Success(_) => panic!("Expected not-found forwarding, instead got success"),
            Outcome::Error(error) => panic!(
                "Expected not-found forwarding, instead got error {:?}",
                error,
            ),
            Outcome::Forward { forwarding, .. } => {
                assert!(
                    matches!(forwarding, Forwarding::NotFound),
                    "Expected not-found forwarding, instead got forwarding {:?}",
                    forwarding
                );
            }
        }
    }

    fn into_args(self) -> (Request, RequestState, Input) {
        let Self {
            method,
            uri,
            version,
            headers,
            remote_addr,
            body,
            input,
        } = self;
        let request_state = RequestState::new(body, None);
        let request = Request {
            method,
            uri,
            version,
            headers,
            remote_addr,
        };
        (request, request_state, input)
    }
}

fn match_outcome<C, S, F, R>(outcome: Outcome<C, S>, on_success: F) -> R
where
    F: FnOnce(S) -> R,
{
    match outcome {
        Outcome::Success(success) => on_success(success),
        Outcome::Error(error) => {
            panic!("Expected success, instead got error {:?}", error)
        }
        Outcome::Forward { forwarding, .. } => {
            panic!("Expected success, instead got forwarding {:?}", forwarding)
        }
    }
}

macro_rules! define_method {
    ($fn_name:ident $const_name:ident) => {
        #[doc = concat!(
                                            "Creates a new [`RequestBuilder`] with a method of [`",
                                            stringify!($const_name),
                                            "`](Method::",
                                            stringify!($const_name),
                                            ")."
                                        )]
        pub fn $fn_name() -> RequestBuilder<()> {
            RequestBuilder::new().method(Method::$const_name)
        }
    };
}

all_methods!(define_method);
