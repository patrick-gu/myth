//! HTTP request method

use std::{
    fmt,
    future::{ready, Ready},
};

pub use hyper::Method;

use crate::{
    filter::{ready::ready_filter, FilterExecute, FilterSealed},
    forward::{AttemptedMethods, Forwarding},
    impl_Filter,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    FilterBase,
};

macro_rules! define_method {
    ($fn_name:ident $const_name:ident) => {
        #[doc = concat!(
                    "Returns a [`Filter`](crate::Filter) that returns successfully if the request method was [`",
                    stringify!($const_name),
                    "`](Method::",
                    stringify!($const_name),
                    "), and forwards otherwise"
                )]
        pub fn $fn_name(
        ) -> impl_Filter!(() => Copy + (fmt::Debug)) {
            ready_filter(|request, _| {
                if request.method == Method::$const_name {
                    Outcome::Success(())
                } else {
                    Outcome::Forward {
                        input: (),
                        forwarding: Forwarding::MethodNotAllowed(AttemptedMethods::$const_name),
                    }
                }
            })
        }
    };
}

all_methods!(define_method);

/// Returns a [`Filter`](crate::Filter) that extracts the HTTP request [method](Method) from the request
pub fn method() -> impl_Filter!('f, &'f Method => Copy + (fmt::Debug)) {
    #[derive(Copy, Clone, Debug)]
    struct MethodFilter;

    impl FilterSealed for MethodFilter {}

    impl<'f> FilterBase<'f> for MethodFilter {
        type Input = ();

        type Success = (&'f Method,);
    }

    impl<'f> FilterExecute<'f> for MethodFilter {
        type Future = Ready<RequestOutcome<Self::Input, Self::Success>>;

        fn execute(
            &'f self,
            request: &'f Request,
            request_state: RequestState,
            (): Self::Input,
        ) -> Self::Future {
            ready(RequestOutcome {
                request_state,
                outcome: Outcome::Success((&request.method,)),
            })
        }
    }

    MethodFilter
}

#[cfg(test)]
mod tests {
    use super::{method, Method};
    use crate::test;

    #[tokio::test]
    async fn retrieve_method_patch() {
        fn check(method: &Method) {
            assert_eq!(method, Method::PATCH);
        }
        test::patch().success(&method(), check).await;
    }

    #[tokio::test]
    async fn retrive_method_custom() {
        fn check(method: &Method) {
            assert_eq!(method, "CUSTOM");
        }
        test::RequestBuilder::new()
            .method("CUSTOM")
            .success(&method(), check)
            .await;
    }
}
