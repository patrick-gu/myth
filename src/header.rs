//! HTTP headers and [`Filters`](Filter) that match them

use std::{
    convert::TryInto,
    fmt,
    future::{ready, Ready},
    str::FromStr,
};

pub use hyper::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_CHARSET, ACCEPT_ENCODING, ACCEPT_LANGUAGE,
    ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS,
    ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS,
    ACCESS_CONTROL_MAX_AGE, ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, AGE,
    ALLOW, ALT_SVC, AUTHORIZATION, CACHE_CONTROL, CONNECTION, CONTENT_DISPOSITION,
    CONTENT_ENCODING, CONTENT_LANGUAGE, CONTENT_LENGTH, CONTENT_LOCATION, CONTENT_RANGE,
    CONTENT_SECURITY_POLICY, CONTENT_SECURITY_POLICY_REPORT_ONLY, CONTENT_TYPE, COOKIE, DATE, DNT,
    ETAG, EXPECT, EXPIRES, FORWARDED, FROM, HOST, IF_MATCH, IF_MODIFIED_SINCE, IF_NONE_MATCH,
    IF_RANGE, IF_UNMODIFIED_SINCE, LAST_MODIFIED, LINK, LOCATION, MAX_FORWARDS, ORIGIN, PRAGMA,
    PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, PUBLIC_KEY_PINS, PUBLIC_KEY_PINS_REPORT_ONLY, RANGE,
    REFERER, REFERRER_POLICY, REFRESH, RETRY_AFTER, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_EXTENSIONS,
    SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL, SEC_WEBSOCKET_VERSION, SERVER, SET_COOKIE,
    STRICT_TRANSPORT_SECURITY, TE, TRAILER, TRANSFER_ENCODING, UPGRADE, UPGRADE_INSECURE_REQUESTS,
    USER_AGENT, VARY, VIA, WARNING, WWW_AUTHENTICATE, X_CONTENT_TYPE_OPTIONS,
    X_DNS_PREFETCH_CONTROL, X_FRAME_OPTIONS, X_XSS_PROTECTION,
};
use mime::Mime;

use crate::{
    errors::FilterError,
    filter::{FilterExecute, FilterSealed},
    impl_Filter,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    Filter, FilterBase, Forwarding, Response,
};

fn unwrap_header_name(name: impl TryInto<HeaderName>) -> HeaderName {
    match name.try_into() {
        Ok(name) => name,
        Err(_) => panic!("The provided header name was not valid"),
    }
}

pub fn all() -> impl_Filter!('f, &'f HeaderMap => Copy + (fmt::Debug)) {
    #[derive(Copy, Clone, Debug)]
    struct AllHeaders;

    impl FilterSealed for AllHeaders {}

    impl<'f> FilterBase<'f> for AllHeaders {
        type Input = ();

        type Success = (&'f HeaderMap,);
    }

    impl<'f> FilterExecute<'f> for AllHeaders {
        type Future = Ready<RequestOutcome<Self::Input, Self::Success>>;

        fn execute(
            &'f self,
            request: &'f Request,
            request_state: RequestState,
            (): Self::Input,
        ) -> Self::Future {
            ready(RequestOutcome {
                request_state,
                outcome: Outcome::Success((&request.headers,)),
            })
        }
    }

    AllHeaders
}

/// Creates a [`Filter`] that extracts the corresponding [`HeaderValue`] for a certain header name, or returns
/// [`None`] if the header was not present.
///
/// # Panics
///
/// Panics if the provided header name is not valid
pub fn value_optional(
    name: impl TryInto<HeaderName>,
) -> impl_Filter!('f, Option<&'f HeaderValue> => Clone + (fmt::Debug)) {
    #[derive(Clone, Debug)]
    struct HeaderValueFilter(HeaderName);

    impl FilterSealed for HeaderValueFilter {}

    impl<'f> FilterBase<'f> for HeaderValueFilter {
        type Input = ();

        type Success = (Option<&'f HeaderValue>,);
    }

    impl<'f> FilterExecute<'f> for HeaderValueFilter {
        type Future = Ready<RequestOutcome<Self::Input, Self::Success>>;

        fn execute(
            &'f self,
            request: &'f Request,
            request_state: RequestState,
            (): Self::Input,
        ) -> Self::Future {
            ready(RequestOutcome {
                request_state,
                outcome: Outcome::Success((request.header(&self.0),)),
            })
        }
    }

    HeaderValueFilter(unwrap_header_name(name))
}

pub fn value(
    name: impl TryInto<HeaderName>,
) -> impl_Filter!('f, &'f HeaderValue => Clone + (fmt::Debug)) {
    let name = unwrap_header_name(name);

    #[derive(Debug)]
    struct HeaderMissing;

    impl FilterError for HeaderMissing {
        fn into_response(self: Box<Self>) -> Response {
            unreachable!("Should have been recovered")
        }
    }

    async fn handler(option: Option<&HeaderValue>) -> crate::Result<&HeaderValue> {
        option.ok_or_else(|| HeaderMissing.into())
    }

    value_optional(name)
        .handle(handler)
        .recover_forward(|_: HeaderMissing| async { Ok(Forwarding::NotFound) })
}

#[allow(dead_code)]
pub(super) fn content_type(
) -> impl_Filter!('f, (Option<Mime>, Option<&'f HeaderValue>) => Clone + (fmt::Debug)) {
    async fn handler(
        value: Option<&HeaderValue>,
    ) -> crate::Result<(Option<Mime>, Option<&HeaderValue>)> {
        let mime = value
            .and_then(|value| value.to_str().ok())
            .and_then(|str| Mime::from_str(str).ok());
        Ok((mime, value))
    }

    value_optional(CONTENT_TYPE).handle(handler).untuple()
}
