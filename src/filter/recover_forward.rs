use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::ready;
use pin_project_lite::pin_project;
use unused::Unused;

use super::{FilterExecute, FilterSealed};
use crate::{
    errors::{BoxedFilterError, Recoverable},
    generics::fns::AsyncTryFn,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    FilterBase, Forwarding,
};

pub struct RecoverForward<T, F, E> {
    pub(super) filter: T,
    pub(super) func: F,
    pub(super) unused: Unused!(fn(E)),
}

impl<T, F, E> fmt::Debug for RecoverForward<T, F, E>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecoverForward")
            .field("filter", &self.filter)
            .finish_non_exhaustive()
    }
}

impl<T, F, E> Clone for RecoverForward<T, F, E>
where
    T: Clone,
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            filter: self.filter.clone(),
            func: self.func.clone(),
            unused: self.unused,
        }
    }
}

impl<T, F, E> Copy for RecoverForward<T, F, E>
where
    T: Copy,
    F: Copy,
{
}

impl<T, F, E> FilterSealed for RecoverForward<T, F, E> {}

impl<'f, T, F, E> FilterBase<'f> for RecoverForward<T, F, E>
where
    T: FilterBase<'f>,
    F: AsyncTryFn<(E,)> + Send + Sync + 'static,
    E: Recoverable,
{
    type Input = ();

    type Success = T::Success;
}

impl<'f, T, F, E> FilterExecute<'f> for RecoverForward<T, F, E>
where
    T: FilterExecute<'f, Input = ()>,
    F: AsyncTryFn<(E,), Ok = Forwarding, Err = BoxedFilterError> + Send + Sync + 'static,
    F::Future: Send,
    E: Recoverable,
{
    type Future = RecoverForwardFuture<'f, T, F, E>;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        (): Self::Input,
    ) -> Self::Future {
        RecoverForwardFuture::Filter {
            future: self.filter.execute(request, request_state, ()),
            func: &self.func,
        }
    }
}

pin_project! {
    #[project = Proj]
    pub enum RecoverForwardFuture<'f, T, F, E>
    where
        T: FilterExecute<'f>,
        F: AsyncTryFn<(E,)>,
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
            unused: Unused!(E),
        }
    }
}

impl<'f, T, F, E> Future for RecoverForwardFuture<'f, T, F, E>
where
    T: FilterExecute<'f, Input = ()>,
    F: AsyncTryFn<(E,), Ok = Forwarding, Err = BoxedFilterError>,
    E: Recoverable,
{
    type Output = RequestOutcome<(), T::Success>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            Proj::Filter { future, func } => {
                let RequestOutcome {
                    request_state,
                    outcome,
                } = ready!(future.poll(cx));
                match outcome {
                    Outcome::Error(error) => match E::recover(error) {
                        Ok(error) => {
                            let state = Self::Func {
                                future: func.call((error,)),
                                request_state: Some(request_state),
                                unused: Unused,
                            };
                            self.set(state);
                            self.poll(cx)
                        }
                        Err(error) => Poll::Ready(RequestOutcome {
                            request_state,
                            outcome: Outcome::Error(error),
                        }),
                    },
                    outcome @ (Outcome::Success(_) | Outcome::Forward { .. }) => {
                        Poll::Ready(RequestOutcome {
                            request_state,
                            outcome,
                        })
                    }
                }
            }
            Proj::Func {
                future,
                request_state,
                ..
            } => {
                let result = ready!(future.poll(cx));
                Poll::Ready(RequestOutcome {
                    request_state: request_state.take().unwrap(),
                    outcome: match result {
                        Ok(forwarding) => Outcome::Forward {
                            input: (),
                            forwarding,
                        },
                        Err(error) => Outcome::Error(error),
                    },
                })
            }
        }
    }
}
