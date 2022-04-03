use core::fmt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::ready;
use pin_project_lite::pin_project;
use unused::Unused;

use super::{FilterExecute, FilterSealed, RequestOutcome};
use crate::{
    generics::tuples::Append,
    outcome::Outcome,
    request::{Request, RequestState},
    FilterBase,
};

pub struct Receive<T, R> {
    pub(super) filter: T,
    pub(super) unused: Unused!(R),
}

impl<T, R> fmt::Debug for Receive<T, R>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Receive")
            .field("filter", &self.filter)
            .finish_non_exhaustive()
    }
}

impl<T, R> Clone for Receive<T, R>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            filter: self.filter.clone(),
            unused: self.unused,
        }
    }
}

impl<T, R> Copy for Receive<T, R> where T: Copy {}

impl<T, R> FilterSealed for Receive<T, R> {}

impl<'f, T, R> FilterBase<'f> for Receive<T, R>
where
    T: FilterBase<'f>,
    R: Append<T::Input> + Append<T::Success> + 'static,
{
    type Input = <R as Append<T::Input>>::Appended;

    type Success = <R as Append<T::Success>>::Appended;
}

impl<'f, T, R> FilterExecute<'f> for Receive<T, R>
where
    T: FilterExecute<'f>,
    R: Append<T::Input> + Append<T::Success> + Send + 'static,
{
    type Future = ReceiveFuture<'f, T, R>;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        let (receive, input) = R::remove(input);
        ReceiveFuture {
            future: self.filter.execute(request, request_state, input),
            receive: Some(receive),
        }
    }
}

pin_project! {
    pub struct ReceiveFuture<'f, F, R>
    where
        F: FilterExecute<'f>,
    {
        #[pin]
        future: F::Future,
        receive: Option<R>,
    }
}

impl<'f, T, R> Future for ReceiveFuture<'f, T, R>
where
    T: FilterExecute<'f>,
    R: Append<T::Input> + Append<T::Success>,
{
    type Output =
        RequestOutcome<<R as Append<T::Input>>::Appended, <R as Append<T::Success>>::Appended>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let RequestOutcome {
            request_state,
            outcome,
        } = ready!(self.as_mut().project().future.poll(cx));

        let receive = self.project().receive.take().unwrap();

        Poll::Ready(RequestOutcome {
            request_state,
            outcome: match outcome {
                Outcome::Success(success) => Outcome::Success(receive.append(success)),
                Outcome::Error(error) => Outcome::Error(error),
                Outcome::Forward { input, forwarding } => Outcome::Forward {
                    input: receive.append(input),
                    forwarding,
                },
            },
        })
    }
}
