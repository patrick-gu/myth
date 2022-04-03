//! `Origin` checking.
//!
//! A [`Filter`] wrapped by a [`Config`] implements Cross-Origin-Resource-Sharing as defined by the
//! [fetch specification](https://fetch.spec.whatwg.org/).
//!
//! It responds to [preflight requests](https://developer.mozilla.org/en-US/docs/Glossary/Preflight_request)
//! with a response of either [204 No Content](StatusCode::NO_CONTENT),
//! [403 Forbidden](StatusCode::FORBIDDEN), or [400 Bad Request](StatusCode::BAD_REQUEST).
//!
//! Additionally, a non-preflight request with an origin or method that is disallowed by the [`Config`]
//! will receive a forbidden response, and will not proceed to the wrapped [`Filter`]. This can help to
//! prevent Cross-Site Request Forgery attacks.
//!
//! See [`Config`] for usage.

use std::{
    convert::{TryFrom, TryInto},
    future::{ready, Ready},
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures_util::{future::Either, ready, Future};
use pin_project_lite::pin_project;

use crate::{
    filter::{FilterExecute, FilterSealed},
    header,
    header::{HeaderMap, HeaderName, HeaderValue},
    method::Method,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    response::default_response,
    util::StrExt,
    Filter, FilterBase, Responder, Response, StatusCode,
};

/// Represents configuration for origin checking.
///
/// # Example
///
/// ```
/// use std::time::Duration;
///
/// use myth::{security::origin, Filter};
///
/// // Create a new `Config`.
/// let config = origin::Config::new()
///     // Allow an origin of `https://example.com`.
///     .origin("https://example.com")
///     // Allow `GET` and `HEAD` requests.
///     .method("GET")
///     .method("HEAD")
///     // Allow for custom `Content-Type`s.
///     .allow_header("Content-Type")
///     // Expose a custom header.
///     .expose_header("X-Custom-Header")
///     // Permit credentialed requests.
///     .credentials()
///     // Configure max age for caching preflight requests.
///     .max_age(Duration::from_secs(60 * 60));
///
/// // Create a `Filter` that returns a `Responder`.
/// let filter = myth::any().handle(|| async { Ok("Hello, user at https://example.com!") });
///
/// // Wrap our `filter` with CORS.
/// let filter = config.apply(filter);
/// ```
#[derive(Clone, Debug)]
pub struct Config {
    origins: Option<Vec<String>>,
    methods: Vec<Method>,
    allow_headers: Vec<HeaderName>,
    expose_headers: Vec<HeaderName>,
    max_age: Option<Duration>,
    credentials: bool,
}

impl Config {
    /// Creates a new origin configuration.
    ///
    /// By default, this allows no origins, no credentials, no methods, no allowed headers, no exposed
    /// headers, and no max age.
    ///
    /// # Example
    ///
    /// ```
    /// use myth::security::origin::Config;
    ///
    /// // Completely strict config.
    /// let mut config = Config::new();
    ///
    /// // Modify the `config` to be more permissive.
    /// config = config.any_origin().method("GET");
    /// ```
    pub fn new() -> Self {
        Self {
            origins: Some(Vec::new()),
            methods: Vec::new(),
            allow_headers: Vec::new(),
            expose_headers: Vec::new(),
            max_age: None,
            credentials: false,
        }
    }

    /// Adds a single origin to the list of allowed origins.
    ///
    /// If [`any_origin`](Self::any_origin) was previously called, this overrides it.
    ///
    /// # Example
    ///
    /// ```
    /// # use myth::security::origin::Config;
    /// let config = Config::new()
    ///     // Allow "example.com" over both HTTP and HTTPS.
    ///     .origin("https://example.com")
    ///     .origin("http://example.com")
    ///     // Allow cases where the `Origin` is set to "null".
    ///     .origin("null");
    /// ```
    pub fn origin(mut self, origin: impl Into<String>) -> Self {
        self.origins
            .get_or_insert_with(Vec::new)
            .push(origin.into());
        self
    }

    /// Allows any origin to access the resource.
    ///
    /// If [`origin`](Self::origin) was previously called, this overrides it.
    ///
    /// # Example
    ///
    /// ```
    /// # use myth::security::origin::Config;
    /// let config = Config::new()
    ///     .any_origin();
    /// ```
    pub fn any_origin(mut self) -> Self {
        self.origins = None;
        self
    }

    /// Allows a method to access the resource.
    ///
    /// These are set in [`Access-Control-Allow-Methods`](header::ACCESS_CONTROL_ALLOW_METHODS).
    ///
    /// Does nothing if the same method was previously added.
    ///
    /// # Panics
    ///
    /// Panics if the provided method is invalid, or if the method is [`OPTIONS`](Method::OPTIONS).
    ///
    /// # Example
    ///
    /// ```
    /// # use myth::security::origin::Config;
    /// let config = Config::new()
    ///     .method("GET")
    ///     .method("PUT");
    /// ```
    pub fn method(mut self, method: impl TryInto<Method>) -> Self {
        let method = match method.try_into() {
            Ok(method) => method,
            Err(_) => panic!("Invalid method provided"),
        };
        if method == Method::OPTIONS {
            panic!("Method cannot be OPTIONS");
        }
        if !self.methods.contains(&method) {
            self.methods.push(method);
        }
        self
    }

    /// Allows a header that may be in the [`Access-Control-Allow-Headers`](header::ACCESS_CONTROL_ALLOW_HEADERS).
    ///
    /// These headers will not be checked for simple requests.
    ///
    /// Does nothing if the same header was already allowed.
    ///
    /// # Panics
    ///
    /// Panics if the provided header name was invalid.
    ///
    /// # Example
    ///
    /// ```
    /// # use myth::security::origin::Config;
    /// let config = Config::new()
    ///     .allow_header("X-Custom-Header");
    /// ```
    pub fn allow_header(mut self, header_name: impl TryInto<HeaderName>) -> Self {
        let header_name = match header_name.try_into() {
            Ok(header_name) => header_name,
            Err(_) => panic!("Invalid header name provided"),
        };
        if !self.allow_headers.contains(&header_name) {
            self.allow_headers.push(header_name);
        }
        self
    }

    /// Adds a header to [`Access-Control-Expose-Headers`].
    ///
    /// Does nothing if the same header was already exposed.
    ///
    /// # Panics
    ///
    /// Panics if the provided header name was invalid.
    ///
    /// # Example
    ///
    /// ```
    /// # use myth::security::origin::Config;
    /// let config = Config::new()
    ///     .expose_header("X-Custom-Header");
    /// ```
    pub fn expose_header(mut self, header_name: impl TryInto<HeaderName>) -> Self {
        let header_name = match header_name.try_into() {
            Ok(header_name) => header_name,
            Err(_) => panic!("Invalid header name provided"),
        };
        if !self.expose_headers.contains(&header_name) {
            self.expose_headers.push(header_name);
        }
        self
    }

    /// Sets the [`Access-Control-Max-Age`](header::ACCESS_CONTROL_MAX_AGE)
    /// for preflight requests.
    ///
    /// Only the whole seconds of the [`Duration`] are used.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// # use myth::security::origin::Config;
    /// let config = Config::new()
    ///     // Duration of one day.
    ///     .max_age(Duration::from_secs(24 * 60 * 60));
    /// ```
    pub fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = Some(max_age);
        self
    }

    /// Sets <code>[Access-Control-Allow-Credentials](header::ACCESS_CONTROL_ALLOW_CREDENTIALS): true</code>,
    /// which permits sharing the response of requests with
    /// [credentials](https://fetch.spec.whatwg.org/#credentials).
    ///
    /// # Example
    ///
    /// ```
    /// # use myth::security::origin::Config;
    /// let config = Config::new()
    ///     .origin("https://example.com:12345")
    ///     .method("POST")
    ///     .credentials();
    /// ```
    pub fn credentials(mut self) -> Self {
        self.credentials = true;
        self
    }

    /// Wraps an inner [`Filter`] with this configuration.
    ///
    /// Note that this will not apply headers if `filter` produces an unsuccessful result.
    ///
    /// # Panics
    ///
    /// Panics if:
    ///  - [`method`](Self::method) was not called, allowing no valid methods.
    ///  - Neither [`origin`](Self::origin) nor [`any_origin`](Self::any_origin) was called, leaving
    ///    no valid origins.
    pub fn apply<F, I, R>(
        self,
        filter: F,
    ) -> impl Filter + for<'f> FilterBase<'f, Input = I, Success = (Response,)>
    where
        F: Filter + for<'f> FilterBase<'f, Input = I, Success = (R,)>,
        I: Send,
        R: Responder,
    {
        if let Some(origins) = &self.origins {
            assert!(
                !origins.is_empty(),
                "Neither `origin` or `any_origin` was called, so no origins are allowed."
            );
        }
        assert!(
            !self.methods.is_empty(),
            "`method` was not called, so no methods are allowed."
        );
        Cors::new(self, filter)
    }

    fn preflight_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        match self.origins {
            Some(_) => {
                static VARY_HEADERS: HeaderValue = HeaderValue::from_static(
                    "Origin, Access-Control-Request-Method, Access-Control-Request-Headers",
                );
                // Access-Control-Allow-Origin set during the request.
                headers.insert(header::VARY, VARY_HEADERS.clone());
            }
            None => {
                headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HEADER_ASTERIK.clone());
                static VARY_HEADERS: HeaderValue = HeaderValue::from_static(
                    "Access-Control-Request-Method, Access-Control-Request-Headers",
                );
                headers.insert(header::VARY, VARY_HEADERS.clone());
            }
        }

        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            join_to_header_value(&self.methods),
        );

        if !self.allow_headers.is_empty() {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                join_to_header_value(&self.allow_headers),
            );
        }

        if let Some(expose_headers) = self.expose_headers() {
            headers.insert(header::ACCESS_CONTROL_EXPOSE_HEADERS, expose_headers);
        }

        if self.credentials {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
                HEADER_TRUE.clone(),
            );
        }

        if let Some(max_age) = self.max_age {
            headers.insert(
                header::ACCESS_CONTROL_MAX_AGE,
                HeaderValue::from(max_age.as_secs()),
            );
        }

        headers
    }

    fn expose_headers(&self) -> Option<HeaderValue> {
        if self.expose_headers.is_empty() {
            None
        } else {
            Some(join_to_header_value(&self.expose_headers))
        }
    }
}

static HEADER_TRUE: HeaderValue = HeaderValue::from_static("true");
static HEADER_ASTERIK: HeaderValue = HeaderValue::from_static("*");

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

struct Cors<T> {
    filter: T,
    origins: Option<Vec<String>>,
    methods: Vec<Method>,
    allow_headers: Vec<HeaderName>,
    expose_headers: Option<HeaderValue>,
    preflight_headers: HeaderMap,
    credentials: bool,
}

impl<T> FilterSealed for Cors<T> {}

impl<'f, T> FilterBase<'f> for Cors<T>
where
    T: FilterBase<'f>,
{
    type Input = T::Input;

    type Success = (Response,);
}

impl<'f, T, R> FilterExecute<'f> for Cors<T>
where
    T: FilterExecute<'f, Success = (R,)>,
    T::Input: Send,
    R: Responder,
{
    type Future = Either<
        Either<Ready<RequestOutcome<Self::Input, Self::Success>>, ApplyHeaders<T::Future>>,
        VaryOrigin<T::Future>,
    >;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        let origin = match request.header(header::ORIGIN) {
            Some(origin) => origin,
            None => {
                return Either::Right(VaryOrigin {
                    future: self.filter.execute(request, request_state, input),
                });
            }
        };

        if request.method == Method::OPTIONS {
            let response = self.preflight(request, origin);
            Either::Left(Either::Left(ready(RequestOutcome {
                request_state,
                outcome: Outcome::Success((response,)),
            })))
        } else {
            macro_rules! forbidden {
                () => {{
                    let mut response = default_response(StatusCode::FORBIDDEN);
                    vary_origin(response.headers_mut());
                    Either::Left(Either::Left(ready(RequestOutcome {
                        request_state,
                        outcome: Outcome::Success((response,)),
                    })))
                }};
            }

            let (origin, vary) = match self.check_origin(origin) {
                Origin::Allowed => (origin.clone(), true),
                Origin::Disallowed => {
                    tracing::debug!("Request with origin {:?} that is not allowed", origin);
                    return forbidden!();
                }
                Origin::Any => (HEADER_ASTERIK.clone(), false),
            };

            if self.methods.contains(&request.method) {
                Either::Left(Either::Right(ApplyHeaders {
                    future: self.filter.execute(request, request_state, input),
                    origin,
                    expose_headers: self.expose_headers.clone(),
                    credentials: self.credentials,
                    vary,
                }))
            } else {
                tracing::debug!(
                    "Request with method {:?} that is not allowed",
                    request.method
                );
                forbidden!()
            }
        }
    }
}

impl<T> Cors<T> {
    fn new(config: Config, filter: T) -> Self {
        let preflight_headers = config.preflight_headers();
        let expose_headers = config.expose_headers();
        Self {
            filter,
            origins: config.origins,
            methods: config.methods,
            allow_headers: config.allow_headers,
            expose_headers,
            preflight_headers,
            credentials: config.credentials,
        }
    }

    fn preflight(&self, request: &Request, origin: &HeaderValue) -> Response {
        let mut headers = self.preflight_headers.clone();
        let status = self
            .preflight_impl(&mut headers, request, origin)
            .err()
            .unwrap_or(StatusCode::NO_CONTENT);

        let mut response = Response::default().with_status(status);
        *response.headers_mut() = headers;
        response
    }

    fn preflight_impl(
        &self,
        headers: &mut HeaderMap,
        request: &Request,
        origin: &HeaderValue,
    ) -> Result<(), StatusCode> {
        match self.check_origin(origin) {
            Origin::Allowed => {
                headers.append(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.clone());
            }
            Origin::Disallowed => return Err(StatusCode::FORBIDDEN),
            Origin::Any => (), // already added
        }

        let request_method = request
            .header(header::ACCESS_CONTROL_REQUEST_METHOD)
            .ok_or_else(|| {
                tracing::debug!("Preflight request missing Access-Control-Request-Method");
                StatusCode::BAD_REQUEST
            })?;

        if !self
            .methods
            .iter()
            .any(|method| request_method == method.as_ref())
        {
            tracing::debug!(
                "Preflight request with method {:?} that is not allowed",
                request_method
            );
            return Err(StatusCode::FORBIDDEN);
        }

        for value in request.header_all(header::ACCESS_CONTROL_REQUEST_HEADERS) {
            let value = value.to_str().map_err(|_| {
                tracing::debug!("Preflight request has invalid Access-Control-Request-Headers");
                StatusCode::FORBIDDEN
            })?;
            for padded in value.split(',') {
                let request_header = padded.trim_spaces_tabs();
                if !self
                    .allow_headers
                    .iter()
                    .any(|header| header == request_header)
                {
                    tracing::debug!(
                        "Preflight request has request header {:?} that is not allowed",
                        request_header
                    );
                    return Err(StatusCode::FORBIDDEN);
                }
            }
            for request_header in value.split_whitespace() {
                if !self
                    .allow_headers
                    .iter()
                    .any(|header| header == request_header)
                {
                    tracing::debug!(
                        "Preflight request has request header {:?} that is not allowed",
                        request_header
                    );
                    return Err(StatusCode::FORBIDDEN);
                }
            }
        }

        Ok(())
    }

    fn check_origin(&self, origin: &HeaderValue) -> Origin {
        if let Some(vec) = &self.origins {
            if vec.iter().any(|allowed| allowed == origin) {
                Origin::Allowed
            } else {
                tracing::debug!("CORS request with disallowed origin: {:?}", origin);
                Origin::Disallowed
            }
        } else {
            Origin::Any
        }
    }
}

fn join_to_header_value<T: AsRef<str>>(values: &[T]) -> HeaderValue {
    assert!(!values.is_empty());
    let mut string = values[0].as_ref().to_owned();
    for value in &values[1..] {
        string += ", ";
        string += value.as_ref();
    }
    HeaderValue::try_from(string).unwrap()
}

pin_project! {
    pub struct ApplyHeaders<F> {
        #[pin]
        future: F,
        origin: HeaderValue,
        expose_headers: Option<HeaderValue>,
        credentials: bool,
        vary: bool,
    }
}

impl<F, I, R> Future for ApplyHeaders<F>
where
    F: Future<Output = RequestOutcome<I, (R,)>>,
    R: Responder,
{
    type Output = RequestOutcome<I, (Response,)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let RequestOutcome {
            request_state,
            outcome,
        } = ready!(self.as_mut().project().future.poll(cx));
        let outcome = match outcome {
            Outcome::Success((responder,)) => {
                let mut response =
                    responder.with_header(header::ACCESS_CONTROL_ALLOW_ORIGIN, self.origin.clone());
                if let Some(value) = &self.expose_headers {
                    response =
                        response.with_header(header::ACCESS_CONTROL_EXPOSE_HEADERS, value.clone());
                }
                if self.credentials {
                    response = response.with_header(
                        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
                        HEADER_TRUE.clone(),
                    );
                }
                if self.vary {
                    vary_origin(response.headers_mut());
                }
                Outcome::Success((response,))
            }
            Outcome::Error(error) => {
                tracing::info!("Not applying CORS headers to error outcome");
                Outcome::Error(error)
            }
            Outcome::Forward { input, forwarding } => {
                tracing::info!("Not applying CORS headers to forwarding outcome");
                Outcome::Forward { input, forwarding }
            }
        };
        Poll::Ready(RequestOutcome {
            request_state,
            outcome,
        })
    }
}

pin_project! {
    pub struct VaryOrigin<F> {
        #[pin]
        future: F,
    }
}

impl<F, I, R> Future for VaryOrigin<F>
where
    F: Future<Output = RequestOutcome<I, (R,)>>,
    R: Responder,
{
    type Output = RequestOutcome<I, (Response,)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let RequestOutcome {
            request_state,
            outcome,
        } = ready!(self.project().future.poll(cx));
        let outcome = match outcome {
            Outcome::Success((responder,)) => {
                let mut response = responder.into_response();
                vary_origin(response.headers_mut());
                Outcome::Success((response,))
            }
            Outcome::Error(error) => {
                tracing::info!("Not applying Vary: Origin to error outcome");
                Outcome::Error(error)
            }
            Outcome::Forward { input, forwarding } => {
                tracing::info!("Not applying Vary: Origin to forwarding outcome");
                Outcome::Forward { input, forwarding }
            }
        };
        Poll::Ready(RequestOutcome {
            request_state,
            outcome,
        })
    }
}

fn vary_origin(headers: &mut HeaderMap) {
    headers.append(header::VARY, HeaderValue::from_name(header::ORIGIN));
}

enum Origin {
    Allowed,
    Disallowed,
    Any,
}

#[cfg(test)]
mod tests {
    use super::Config;
    use crate::{any, impl_Filter, test, Bytes, Filter, Responder, Response};

    fn creates_response() -> impl_Filter!(Response) {
        any().handle(|| async { Ok("Success".into_response()) })
    }

    fn simple_with_origin() -> impl_Filter!(Response) {
        Config::new()
            .method("GET")
            .method("POST")
            .origin("https://example.com")
            .apply(creates_response())
    }

    #[tokio::test]
    async fn not_cors() {
        let response = test::patch().response(&simple_with_origin()).await;
        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), "Success");
        let response = test::options()
            .header("Referrer", "http://localhost")
            .response(&simple_with_origin())
            .await;
        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), "Success");
    }

    fn assert_forbidden_normal(response: &hyper::Response<Bytes>) {
        assert_eq!(response.status(), 403);
        assert_eq!(response.body(), "Forbidden");
        let vary = response
            .headers()
            .get_all("Vary")
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(vary.len(), 1);
        assert!(vary[0].to_str().unwrap().eq_ignore_ascii_case("Origin"));
    }

    #[tokio::test]
    async fn forbid_origin() {
        let response = test::get()
            .header("Origin", "http://localhost")
            .header("Host", "https://example.com")
            .header("Cookie", "token=5")
            .response(&simple_with_origin())
            .await;
        assert_forbidden_normal(&response);
        let response = test::get()
            .header("Origin", "null")
            .header("Referrer", "null")
            .response(&simple_with_origin())
            .await;
        assert_forbidden_normal(&response);
    }

    #[tokio::test]
    async fn forbid_method_normal() {
        let response = test::delete()
            .header("Origin", "http://example.com")
            .response(&simple_with_origin())
            .await;
        assert_forbidden_normal(&response);
    }

    #[tokio::test]
    async fn preflight_origin() {
        let filter = Config::new()
            .method("PUT")
            .origin("http://example.org")
            .origin("https://example.org")
            .origin("https://example.com")
            .apply(creates_response());
        let response = test::options()
            .header("Origin", "http://example.org")
            .header("Access-Control-Request-Method", "PUT")
            .response(&filter)
            .await;
        assert_eq!(response.status(), 204);
        assert!(response.body().is_empty());
        assert_vary(&response);

        let response = test::options()
            .header("Origin", "http://0.0.0.0:80")
            .header("Access-Control-Request-Method", "PUT")
            .response(&filter)
            .await;
        assert_eq!(response.status(), 403);
        assert!(response.body().is_empty());
        assert_vary(&response);
    }

    fn assert_vary(response: &hyper::Response<Bytes>) {
        let vary = response
            .headers()
            .get_all("Vary")
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(vary.len(), 1);
        assert_eq!(
            vary[0],
            "Origin, Access-Control-Request-Method, Access-Control-Request-Headers"
        );
    }

    #[tokio::test]
    async fn preflight_method_and_headers() {
        let filter = Config::new()
            .method("PATCH")
            .method("PUT")
            .origin("https://example.com:12345")
            .allow_header("Content-Type")
            .allow_header("X-Custom-Header")
            .apply(creates_response());

        let response = test::options()
            .header("Origin", "https://example.com:12345")
            .header("Access-Control-Request-Method", "PUT")
            .header("Access-Control-Request-Headers", "X-Custom-Header")
            .response(&filter)
            .await;

        assert_eq!(response.status(), 204);
        assert!(response.body().is_empty());
        assert_vary(&response);

        let allow_headers = response
            .headers()
            .get_all("Access-Control-Allow-Headers")
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(allow_headers.len(), 1);
        assert_eq!(allow_headers[0], "content-type, x-custom-header");

        let allow_methods = response
            .headers()
            .get_all("Access-Control-Allow-Methods")
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(allow_methods.len(), 1);
        assert_eq!(allow_methods[0], "PATCH, PUT");

        let response = test::options()
            .header("Origin", "https://example.com:12345")
            .header("Access-Control-Request-Method", "GET")
            .header("Access-Control-Request-Headers", "X-Custom-Header")
            .response(&filter)
            .await;

        assert_eq!(response.status(), 403);

        let response = test::options()
            .header("Origin", "https://example.com:12345")
            .header("Access-Control-Request-Method", "PATCH")
            .header(
                "Access-Control-Request-Headers",
                "X-Custom-Header, X-Other-Custom-Header",
            )
            .response(&filter)
            .await;

        assert_eq!(response.status(), 403);
    }

    #[tokio::test]
    async fn simple_successful_preflight_any_origin() {
        let filter = Config::new()
            .method("DELETE")
            .any_origin()
            .apply(creates_response());
        let response = test::options()
            .header("Origin", "http://example.org")
            .header("Access-Control-Request-Method", "DELETE")
            .response(&filter)
            .await;
        assert_eq!(response.status(), 204);
        assert!(response.body().is_empty());
        let vary = response
            .headers()
            .get_all("Vary")
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(vary.len(), 1);
        assert_eq!(
            vary[0],
            "Access-Control-Request-Method, Access-Control-Request-Headers"
        );
    }
}
