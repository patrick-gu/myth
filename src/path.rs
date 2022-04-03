use std::{
    borrow::Cow,
    convert::TryFrom,
    fmt,
    future::{ready, Ready},
    path::PathBuf,
    str::FromStr,
};

use percent_encoding::percent_decode_str;

use crate::{
    errors::FilterError,
    filter::{ready::ready_filter, FilterExecute, FilterSealed},
    header::{self, HeaderValue},
    impl_Filter,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    response::default_response,
    uri::{uri, Uri},
    Filter, FilterBase, Forwarding, Responder, Response, Result, StatusCode,
};

pub fn path() -> impl_Filter!('f, &'f str => Copy + (fmt::Debug)) {
    async fn handler(uri: &Uri) -> Result<&str> {
        let path = uri.path();
        Ok(path)
    }
    uri().handle(handler)
}

pub fn param_str() -> impl_Filter!('f, Cow<'f, str> => Copy + (fmt::Debug)) {
    #[derive(Copy, Clone, Debug)]
    struct ParamStr;

    impl FilterSealed for ParamStr {}

    impl<'f> FilterBase<'f> for ParamStr {
        type Input = ();

        type Success = (Cow<'f, str>,);
    }

    impl<'f> FilterExecute<'f> for ParamStr {
        type Future = Ready<RequestOutcome<Self::Input, Self::Success>>;

        fn execute(
            &'f self,
            request: &'f Request,
            mut request_state: RequestState,
            (): Self::Input,
        ) -> Self::Future {
            let outcome = decoded_segment(request, &mut request_state, |segment| Some((segment,)));
            ready(RequestOutcome {
                request_state,
                outcome,
            })
        }
    }

    ParamStr
}

pub fn param<T: FromStr + Send>() -> impl_Filter!(T => Copy + (fmt::Debug)) {
    #[derive(Debug)]
    struct ForwardParam;

    impl FilterError for ForwardParam {
        fn into_response(self: Box<Self>) -> Response {
            unimplemented!()
        }
    }

    param_str()
        .handle(|segment: Cow<'_, str>| {
            ready(T::from_str(segment.as_ref()).map_err(|_| ForwardParam.into()))
        })
        .recover_forward(|_: ForwardParam| ready(Ok(Forwarding::NotFound)))
}

pub fn literal(value: impl Into<String>) -> impl_Filter!(() => Clone + (fmt::Debug)) {
    let value = value.into();
    assert!(!value.is_empty(), "literal segments cannot be empty");
    assert!(
        !value.contains('/'),
        "literal segments cannot contain a slash"
    );

    ready_filter(move |request, request_state| {
        decoded_segment(request, request_state, |segment| {
            if value == segment {
                Some(())
            } else {
                None
            }
        })
    })
}

fn decoded_segment<'f, F, S>(
    request: &'f Request,
    request_state: &mut RequestState,
    func: F,
) -> Outcome<(), S>
where
    F: FnOnce(Cow<'f, str>) -> Option<S>,
{
    segment(request, request_state)
        .and_then(|(segment, len)| {
            let decoded = percent_decode_str(segment).decode_utf8_lossy();
            let success = func(decoded)?;
            request_state.incr_current_path_index(len);
            Some(Outcome::Success(success))
        })
        .unwrap_or(Outcome::Forward {
            input: (),
            forwarding: Forwarding::NotFound,
        })
}

fn segment<'f>(request: &'f Request, request_state: &RequestState) -> Option<(&'f str, usize)> {
    let current_path = request_state.current_path(request);
    match current_path.as_bytes() {
        [] | [b'/'] => None,
        [b'/', ..] => {
            let current_path = &current_path[1..];
            let index = current_path.find('/').unwrap_or(current_path.len());
            Some((&current_path[..index], index + 1))
        }
        [..] => {
            let index = current_path.find('/').unwrap_or(current_path.len());
            Some((&current_path[..index], index))
        }
    }
}

pub fn end() -> impl_Filter!(() => Copy + (fmt::Debug)) {
    ready_filter(|request, request_state| {
        if matches!(request.full_path(), "" | "/") {
            Outcome::Success(())
        } else {
            let current_path = request_state.current_path(request);
            match current_path {
                "" => Outcome::Success(()),
                "/" => Outcome::Error(
                    Redirect {
                        location: request_state.previous_path(request).to_owned(),
                    }
                    .into(),
                ),
                _ => Outcome::Forward {
                    input: (),
                    forwarding: Forwarding::NotFound,
                },
            }
        }
    })
}

#[derive(Debug)]
pub struct Redirect {
    location: String,
}

impl Redirect {
    pub fn location(&self) -> &str {
        &self.location
    }
}

impl FilterError for Redirect {
    fn into_response(self: Box<Self>) -> Response {
        tracing::debug!(
            "redirecting to {:?} for trailing slash rules",
            self.location
        );
        default_response(StatusCode::PERMANENT_REDIRECT).with_header(
            header::LOCATION,
            HeaderValue::try_from(self.location).expect("redirect location was not valid"),
        )
    }
}

/// Returns the tail
pub fn tail() -> impl_Filter!('f, &'f str => Copy + (fmt::Debug)) {
    #[derive(Copy, Clone, Debug)]
    struct Tail;

    impl FilterSealed for Tail {}

    impl<'f> FilterBase<'f> for Tail {
        type Input = ();

        type Success = (&'f str,);
    }

    impl<'f> FilterExecute<'f> for Tail {
        type Future = Ready<RequestOutcome<Self::Input, Self::Success>>;

        fn execute(
            &'f self,
            request: &'f Request,
            mut request_state: RequestState,
            (): Self::Input,
        ) -> Self::Future {
            let tail = request_state.current_path(request);
            request_state.end_current_path_index(request);

            ready(RequestOutcome {
                request_state,
                outcome: Outcome::Success((tail,)),
            })
        }
    }

    Tail
}

fn sanitize_path(string: &str) -> Option<PathBuf> {
    let decoded = percent_decode_str(string)
        .decode_utf8()
        .map_err(|error| {
            tracing::debug!(
                "sanitize_path: failed to percent decode string {:?} with error {:?}",
                string,
                error
            );
        })
        .ok()?;

    let mut path = PathBuf::new();

    for seg in decoded.split('/') {
        macro_rules! reject_segment {
            ($reason:expr) => {{
                tracing::warn!(
                    concat!(
                        "sanitize_path: rejecting decoded {:?} due to segment ",
                        $reason
                    ),
                    decoded
                );
                return None;
            }};
        }

        if seg.starts_with('.') {
            reject_segment!("starting with a period (.)");
        } else if seg.starts_with('*') {
            reject_segment!("starting with an asterik (*)");
        } else if seg.ends_with(':') {
            reject_segment!("ending with a colon (:)");
        } else if seg.ends_with('>') {
            reject_segment!("ending with a greater than sign (>)");
        } else if seg.ends_with('<') {
            reject_segment!("ending with a less than sign (<)");
        } else if seg.contains('\\') {
            reject_segment!("containing a backslash (\\)");
        } else if seg.contains('\0') {
            reject_segment!("containing a null byte");
        } else if !seg.is_empty() {
            path.push(seg);
        }
    }

    Some(path)
}

/// Returns the tail that has been sanitized
pub fn tail_path() -> impl_Filter!(PathBuf => Copy + (fmt::Debug)) {
    ready_filter(|request, request_state| {
        match sanitize_path(request_state.current_path(request)) {
            Some(sanitized) => {
                request_state.end_current_path_index(request);
                Outcome::Success((sanitized,))
            }
            None => Outcome::Forward {
                input: (),
                forwarding: Forwarding::NotFound,
            },
        }
    })
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, path::PathBuf};

    use super::{end, literal, param, param_str, sanitize_path, Redirect};
    use crate::{test, uri::Uri, Filter};

    #[test]
    fn path_sanitization() {
        // ../aaa
        assert!(sanitize_path("../aaa").is_none());
        assert!(sanitize_path("%2e%2e%2faaa").is_none());
        // ../../aaa
        assert!(sanitize_path("%2e%2e%2f%2e%2e%2faaa").is_none());
        assert!(sanitize_path("..\\").is_none());
        assert!(sanitize_path("./.env").is_none());
        assert!(sanitize_path("/./.env").is_none());
        assert!(sanitize_path("/./..env").is_none());
        assert!(sanitize_path("./../.").is_none());
        assert!(sanitize_path("/etc/passwd%00.png").is_none());
        assert!(sanitize_path("/../aaa.html").is_none());
        assert!(sanitize_path("/C:\\/aaa.html").is_none());
        assert!(sanitize_path("C:\\\\//").is_none());
        assert!(sanitize_path("\\\\//").is_none());
        assert!(sanitize_path("\\").is_none());
        assert!(sanitize_path("e/**/").is_none());
        assert!(sanitize_path("/*y/").is_none());
        assert!(sanitize_path("*/.").is_none());
        assert!(sanitize_path("/.:").is_none());
        assert!(sanitize_path("/aa/eeeee:").is_none());
        assert!(sanitize_path("a/:").is_none());
        assert!(sanitize_path("a/::").is_none());
        assert!(sanitize_path("a/<>").is_none());
        assert!(sanitize_path("./g>").is_none());
        assert!(sanitize_path("//<").is_none());
        assert!(sanitize_path("/eeeee\0ee.txt").is_none());
        assert!(sanitize_path("/././/./").is_none());

        assert_eq!(sanitize_path("//").unwrap(), PathBuf::new());
        assert_eq!(sanitize_path("///").unwrap(), PathBuf::new());
        assert_eq!(sanitize_path("/%2F/").unwrap(), PathBuf::new());
        assert_eq!(
            sanitize_path("/example.html").unwrap(),
            PathBuf::from("example.html")
        );
        assert_eq!(sanitize_path("/").unwrap(), PathBuf::new());
    }

    #[tokio::test]
    async fn root_end() {
        test::get().succeeds(&end()).await;
    }

    #[tokio::test]
    async fn empty_end() {
        let mut parts = Uri::default().into_parts();
        parts.path_and_query = Some("".parse().unwrap());
        let uri = Uri::from_parts(parts).unwrap();
        assert_eq!(uri.path(), "");
        test::get().uri(uri.clone()).succeeds(&end()).await;
        let filter = literal("h")
            .and(end())
            .handle(|| async {
                panic!();
            })
            .untuple()
            .or(end());
        test::get().uri(uri).succeeds(&filter).await;
    }

    #[tokio::test]
    async fn basic_literal() {
        let filter = literal("a");
        test::get().uri("/a").succeeds(&filter).await;
        test::get().uri("/a/foo").succeeds(&filter).await;
        let filter = literal("XYZ").and(end());
        test::put().uri("/XYZ").succeeds(&filter).await;
    }

    #[tokio::test]
    async fn number_param() {
        let filter = literal("foo")
            .and(param::<i32>())
            .and(literal("bar"))
            .and(end());
        test::get()
            .uri("/foo/2345/bar")
            .success(&filter, |num| {
                assert_eq!(num, 2345);
            })
            .await;

        let filter = literal("a").and(param::<i32>()).and(end());
        test::get()
            .uri("/a/2345")
            .success(&filter, |num| {
                assert_eq!(num, 2345);
            })
            .await;
    }

    #[tokio::test]
    async fn utf8_param_str() {
        let filter = param_str().and(end());
        fn checker(str: Cow<'_, str>) {
            assert_eq!(str, "γεια σας");
        }
        test::get()
            .uri("/%CE%B3%CE%B5%CE%B9%CE%B1%20%CF%83%CE%B1%CF%82")
            .success(&filter, checker)
            .await;
    }

    #[tokio::test]
    async fn asterik() {
        let filter = literal("*").and(end());
        test::get().uri("*").succeeds(&filter).await;
    }

    #[tokio::test]
    async fn redirect() {
        let filter = literal("hhhhhhh").and(end());
        let redirect: Redirect = test::get().uri("/hhhhhhh/").error(&filter).await;
        assert_eq!(redirect.location(), "/hhhhhhh");
    }
}
