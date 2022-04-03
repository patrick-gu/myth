use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project_lite::pin_project;

use super::{FilterExecute, FilterSealed, RequestOutcome};
use crate::{
    generics::tuples::{OneTuple, Tuple},
    request::{Request, RequestState},
    FilterBase,
};

#[derive(Copy, Clone, Debug)]
pub struct Untuple<T>(pub(super) T);

impl<T> FilterSealed for Untuple<T> {}

impl<'f, T> FilterBase<'f> for Untuple<T>
where
    T: FilterBase<'f>,
    T::Success: OneTuple,
    <T::Success as Tuple>::Inner: Tuple,
{
    type Input = T::Input;

    type Success = <T::Success as Tuple>::Inner;
}

impl<'f, T> FilterExecute<'f> for Untuple<T>
where
    T: FilterExecute<'f>,
    T::Success: OneTuple,
    <T::Success as Tuple>::Inner: Tuple,
{
    type Future = UntupleFuture<'f, T>;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        UntupleFuture {
            future: self.0.execute(request, request_state, input),
        }
    }
}

pin_project! {
    pub struct UntupleFuture<'f, T>
    where
        T: FilterExecute<'f>,
    {
        #[pin]
        future: T::Future,
    }
}

impl<'f, T> Future for UntupleFuture<'f, T>
where
    T: FilterExecute<'f>,
    T::Success: OneTuple,
{
    type Output = RequestOutcome<T::Input, <T::Success as Tuple>::Inner>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project()
            .future
            .poll(cx)
            .map(|request_outcome| request_outcome.map(Tuple::into_inner))
    }
}
