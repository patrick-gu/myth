//! Basic [`Filter`]s

use std::{
    fmt,
    future::{ready, Ready},
};

use crate::{
    filter::{ready::ready_filter, FilterExecute, FilterSealed},
    impl_Filter,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    Filter, FilterBase, Forwarding,
};

/// A [`Filter`] that always returns successfully
pub fn any() -> impl_Filter!(() => Copy + (fmt::Debug)) {
    ready_filter(|_, _| Outcome::Success(()))
}

pub fn never<T: Send + Sync + 'static>() -> impl_Filter!((T,) => Copy + (fmt::Debug)) {
    ready_filter(|_, _| Outcome::Forward {
        input: (),
        forwarding: Forwarding::NotFound,
    })
}

pub fn cloning<T: Clone + Send + Sync + 'static>(t: T) -> impl_Filter!(T => Clone + (fmt::Debug)) {
    any().handle(move || {
        let t = t.clone();
        async move { Ok(t) }
    })
}

pub fn borrowing<T: Send + Sync + 'static>(t: T) -> impl_Filter!('f, &'f T => (fmt::Debug)) {
    struct BorrowingFilter<T>(T);

    impl<T> fmt::Debug for BorrowingFilter<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_tuple("BorrowingFilter").field(&"_").finish()
        }
    }

    impl<T> FilterSealed for BorrowingFilter<T> {}

    impl<'f, T> FilterBase<'f> for BorrowingFilter<T>
    where
        T: Send + Sync + 'static,
    {
        type Input = ();

        type Success = (&'f T,);
    }

    impl<'f, T> FilterExecute<'f> for BorrowingFilter<T>
    where
        T: Send + Sync + 'static,
    {
        type Future = Ready<RequestOutcome<Self::Input, Self::Success>>;

        fn execute(
            &'f self,
            _: &'f Request,
            request_state: RequestState,
            (): Self::Input,
        ) -> Self::Future {
            ready(RequestOutcome {
                request_state,
                outcome: Outcome::Success((&self.0,)),
            })
        }
    }

    BorrowingFilter(t)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{any, cloning};
    use crate::test;

    #[tokio::test]
    async fn any_always_succeeds() {
        test::get().succeeds(&any()).await;
    }

    #[tokio::test]
    async fn cloning_arc() {
        let arc = Arc::new("hello".to_owned());
        let filter = cloning(arc);
        test::post()
            .success(&filter, |arc: Arc<String>| {
                assert_eq!(&**arc, "hello");
            })
            .await;
    }

    #[tokio::test]
    async fn cloning_u64() {
        let filter = cloning(54321u64);
        test::delete()
            .success(&filter, |number| {
                assert_eq!(number, 54321);
            })
            .await;
    }
}
