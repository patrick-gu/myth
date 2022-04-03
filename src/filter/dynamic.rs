use std::{fmt, future::Future, pin::Pin, sync::Arc};

use super::{FilterExecute, FilterSealed};
use crate::{
    generics::tuples::Tuple,
    outcome::RequestOutcome,
    request::{Request, RequestState},
    FilterBase,
};

/// A filter dynamically dispatched at runtime.
///
/// This is created by the [`dynamic`](crate::Filter::dynamic) method on [`Filter`](crate::Filter).
pub struct DynamicFilter<C, S>(
    pub(super) 
        Arc<dyn for<'f> FilterExecute<'f, Input = C, Success = S, Future = BoxedFuture<'f, C, S>>>,
);

impl<C, S> fmt::Debug for DynamicFilter<C, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("BoxedFilter").field(&"_").finish()
    }
}

impl<C, S> Clone for DynamicFilter<C, S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<C, S> FilterSealed for DynamicFilter<C, S> {}

impl<'f, C, S> FilterBase<'f> for DynamicFilter<C, S>
where
    C: Tuple + 'static,
    S: Tuple + 'static,
{
    type Input = C;

    type Success = S;
}

impl<'f, C, S> FilterExecute<'f> for DynamicFilter<C, S>
where
    C: Tuple + 'static,
    S: Tuple + 'static,
{
    type Future = BoxedFuture<'f, C, S>;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        self.0.execute(request, request_state, input)
    }
}

type BoxedFuture<'f, C, S> = Pin<Box<dyn Future<Output = RequestOutcome<C, S>> + Send + 'f>>;

#[derive(Copy, Clone, Debug)]
pub struct BoxedFutureFilter<T>(pub(super) T);

impl<T> FilterSealed for BoxedFutureFilter<T> {}

impl<'f, T> FilterBase<'f> for BoxedFutureFilter<T>
where
    T: FilterBase<'f>,
{
    type Input = T::Input;

    type Success = T::Success;
}

impl<'f, T> FilterExecute<'f> for BoxedFutureFilter<T>
where
    T: FilterExecute<'f>,
{
    type Future = BoxedFuture<'f, Self::Input, Self::Success>;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        Box::pin(self.0.execute(request, request_state, input))
    }
}
