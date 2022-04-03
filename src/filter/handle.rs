use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::ready;
use pin_project_lite::pin_project;

use super::{FilterBase, FilterExecute, FilterSealed, RequestOutcome};
use crate::{
    errors::BoxedFilterError,
    generics::fns::AsyncTryFn,
    outcome::Outcome,
    request::{Request, RequestState},
};

#[derive(Copy, Clone)]
pub struct Handle<T, F> {
    pub(super) filter: T,
    pub(super) func: F,
}

impl<T, F> fmt::Debug for Handle<T, F>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handle")
            .field("filter", &self.filter)
            .finish_non_exhaustive()
    }
}

impl<T, F> FilterSealed for Handle<T, F> {}

impl<'f, T, F> FilterBase<'f> for Handle<T, F>
where
    T: FilterBase<'f>,
    F: AsyncTryFn<T::Success> + Send + Sync + 'static,
{
    type Input = T::Input;

    type Success = (<F as AsyncTryFn<T::Success>>::Ok,);
}

impl<'f, T, F> FilterExecute<'f> for Handle<T, F>
where
    T: FilterExecute<'f>,
    F: AsyncTryFn<T::Success, Err = BoxedFilterError> + Send + Sync + 'static,
    F::Future: Send,
{
    type Future = HandleFuture<'f, T, F>;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        HandleFuture::Filter {
            future: self.filter.execute(request, request_state, input),
            func: &self.func,
        }
    }
}

pin_project! {
    #[project = Proj]
    pub enum HandleFuture<'f, T, F>
    where
        T: FilterExecute<'f>,
        F: AsyncTryFn<T::Success>,
    {
        Filter {
            #[pin]
            future: T::Future,
            func: &'f F,
        },
        Func {
            #[pin]
            future: F::Future,
            request_state: Option<RequestState>,
        },
    }
}

impl<'f, T, F> Future for HandleFuture<'f, T, F>
where
    T: FilterExecute<'f>,
    F: AsyncTryFn<T::Success, Err = BoxedFilterError>,
{
    type Output = RequestOutcome<T::Input, (F::Ok,)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            Proj::Filter { future, func } => {
                let RequestOutcome {
                    request_state,
                    outcome,
                } = ready!(future.poll(cx));
                match outcome {
                    Outcome::Success(success) => {
                        let future = func.call(success);
                        self.set(Self::Func {
                            future,
                            request_state: Some(request_state),
                        });
                        self.poll(cx)
                    }
                    Outcome::Error(error) => Poll::Ready(RequestOutcome {
                        request_state,
                        outcome: Outcome::Error(error),
                    }),
                    Outcome::Forward { input, forwarding } => Poll::Ready(RequestOutcome {
                        request_state,
                        outcome: Outcome::Forward { input, forwarding },
                    }),
                }
            }
            Proj::Func {
                future,
                request_state,
            } => {
                let result = ready!(future.poll(cx));
                Poll::Ready(RequestOutcome {
                    request_state: request_state.take().unwrap(),
                    outcome: result.map(|success| (success,)).into(),
                })
            }
        }
    }
}
