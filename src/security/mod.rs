//! Security

pub mod hsts;
pub mod origin;

use std::{
    fmt,
    future::{ready, Ready},
};

use crate::{
    filter::{FilterExecute, FilterSealed},
    header::{self, HeaderValue},
    impl_Filter,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    FilterBase, Responder, Response,
};

static NOSNIFF: HeaderValue = HeaderValue::from_static("nosniff");

/// Sets `X-Content-Type-Options: nosniff`
pub fn x_content_type_options() -> impl_Filter!(Response, Response => Copy + (fmt::Debug)) {
    #[derive(Copy, Clone, Debug)]
    pub struct XContentTypeOptions;

    impl FilterSealed for XContentTypeOptions {}

    impl<'f> FilterBase<'f> for XContentTypeOptions {
        type Input = (Response,);

        type Success = (Response,);
    }

    impl<'f> FilterExecute<'f> for XContentTypeOptions {
        type Future = Ready<RequestOutcome<Self::Input, Self::Success>>;

        fn execute(
            &'f self,
            _: &'f Request,
            request_state: RequestState,
            (response,): Self::Input,
        ) -> Self::Future {
            ready(RequestOutcome {
                request_state,
                outcome: Outcome::Success((
                    response.with_header(header::X_CONTENT_TYPE_OPTIONS, NOSNIFF.clone()),
                )),
            })
        }
    }

    XContentTypeOptions
}
