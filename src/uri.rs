//! HTTP request URIs.

use std::{
    fmt,
    future::{ready, Ready},
};

pub use hyper::Uri;

use crate::{
    filter::{FilterExecute, FilterSealed},
    impl_Filter,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    FilterBase,
};

pub fn uri() -> impl_Filter!('f, &'f Uri => Copy + (fmt::Debug)) {
    #[derive(Copy, Clone, Debug)]
    struct UriFilter;

    impl FilterSealed for UriFilter {}

    impl<'f> FilterBase<'f> for UriFilter {
        type Input = ();

        type Success = (&'f Uri,);
    }

    impl<'f> FilterExecute<'f> for UriFilter {
        type Future = Ready<RequestOutcome<Self::Input, Self::Success>>;

        fn execute(
            &'f self,
            request: &'f Request,
            request_state: RequestState,
            (): Self::Input,
        ) -> Self::Future {
            ready(RequestOutcome {
                request_state,
                outcome: Outcome::Success((&request.uri,)),
            })
        }
    }

    UriFilter
}

#[cfg(test)]
mod tests {
    use super::{uri, Uri};
    use crate::test;

    #[tokio::test]
    async fn complex_uri() {
        fn check(uri: &Uri) {
            assert_eq!(uri.path(), "/hello/foo//e/////h/aaa");
            assert_eq!(uri.query(), Some("5=6"));
        }

        test::get()
            .uri("/hello/foo//e/////h/aaa?5=6")
            .success(&uri(), check)
            .await;
    }
}
