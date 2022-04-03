use std::{
    future::{ready, Ready},
    time::SystemTime,
};

use httpdate::parse_http_date;

use crate::{
    errors::FilterError,
    filter::{FilterExecute, FilterSealed},
    header::{self, HeaderValue},
    impl_Filter,
    method::Method,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    response::default_response,
    FilterBase, Forwarding, Responder, Response, StatusCode,
};

/// `If-Unmodified-Since` header handling.
///
/// # Example
///
/// ```
/// use std::time::{Duration, SystemTime};
///
/// use myth::{html, Filter};
///
/// // A `Filter` that returns some HTML.
/// let filter = myth::any().handle(|| async {
///     Ok(html("<h1>This response might be cached</h1>"))
/// });
///
/// // Updated 5 seconds ago.
/// let updated_time = SystemTime::now() - Duration::from_secs(5);
///
/// // Clone `updated_time` and pass it to `if_unmodified_since`.
/// // This will return a 304 Not Modified if `If-Modified-Since` is after `updated_time`.
/// let filter_cached = myth::cloning(updated_time)
///     .consume(myth::cache::if_unmodified_since());
///
/// // Check for a cached version first, and only if that fails, continue to the original `filter`.
/// let filter = filter_cached.or(filter);
/// ```

pub fn if_unmodified_since() -> impl_Filter!(SystemTime, Response) {
    #[derive(Copy, Clone, Debug)]
    struct IfUnmodifiedSince;

    impl FilterSealed for IfUnmodifiedSince {}

    impl<'f> FilterBase<'f> for IfUnmodifiedSince {
        type Input = (SystemTime,);

        type Success = (Response,);
    }

    impl<'f> FilterExecute<'f> for IfUnmodifiedSince {
        type Future = Ready<RequestOutcome<Self::Input, Self::Success>>;

        fn execute(
            &'f self,
            request: &'f Request,
            request_state: RequestState,
            (modified_time,): Self::Input,
        ) -> Self::Future {
            macro_rules! not_found {
                () => {
                    Outcome::Forward {
                        input: (modified_time,),
                        forwarding: Forwarding::NotFound,
                    }
                };
            }
            let outcome = if request.method == Method::GET || request.method == Method::HEAD {
                if let Some(value) = request.header(header::IF_MODIFIED_SINCE) {
                    match value
                        .to_str()
                        .ok()
                        .and_then(|str| parse_http_date(str).ok())
                    {
                        Some(cached_time) if cached_time > modified_time => Outcome::Success((
                            Response::default().with_status(StatusCode::NOT_MODIFIED),
                        )),
                        Some(_) => not_found!(),
                        None => Outcome::Error(
                            InvalidIfUnmodifiedSince {
                                value: value.clone(),
                            }
                            .into(),
                        ),
                    }
                } else {
                    not_found!()
                }
            } else {
                not_found!()
            };
            ready(RequestOutcome {
                request_state,
                outcome,
            })
        }
    }

    IfUnmodifiedSince
}

#[derive(Debug)]
struct InvalidIfUnmodifiedSince {
    value: HeaderValue,
}

impl FilterError for InvalidIfUnmodifiedSince {
    fn into_response(self: Box<Self>) -> Response {
        tracing::debug!("invalid If-Unmodified-Since: {:?}", self.value);
        default_response(StatusCode::BAD_REQUEST)
    }
}
