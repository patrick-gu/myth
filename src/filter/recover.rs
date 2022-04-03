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
    generics::{
        fns::AsyncTryFn,
        tuples::{OneTuple, Tuple},
    },
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
    FilterBase,
};

pub struct Recover<T, F, E> {
    pub(super) filter: T,
    pub(super) func: F,
    pub(super) unused: Unused!(E),
}

impl<T, F, E> fmt::Debug for Recover<T, F, E>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Recover")
            .field("filter", &self.filter)
            .finish_non_exhaustive()
    }
}

impl<T, F, E> Clone for Recover<T, F, E>
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

impl<T, F, E> Copy for Recover<T, F, E>
where
    T: Copy,
    F: Copy,
{
}

impl<T, F, E> FilterSealed for Recover<T, F, E> {}

impl<'f, T, F, E> FilterBase<'f> for Recover<T, F, E>
where
    T: FilterBase<'f>,
    F: AsyncTryFn<(E,)> + Send + Sync + 'static,
    E: Recoverable,
{
    type Input = T::Input;

    type Success = T::Success;
}

impl<'f, T, F, E> FilterExecute<'f> for Recover<T, F, E>
where
    T: FilterExecute<'f>,
    T::Success: OneTuple,
    F: AsyncTryFn<(E,), Ok = <T::Success as Tuple>::Inner, Err = BoxedFilterError>
        + Send
        + Sync
        + 'static,
    F::Future: Send,
    E: Recoverable,
{
    type Future = RecoverFuture<'f, T, F, E>;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        let path_index = request_state.current_path_index;
        RecoverFuture {
            state: RecoverFutureState::Filter {
                future: self.filter.execute(request, request_state, input),
                func: &self.func,
            },
            path_index,
        }
    }
}

pin_project! {
    pub struct RecoverFuture<'f, T, F, E>
    where
        T: FilterExecute<'f>,
        F: AsyncTryFn<(E,)>,
    {
        #[pin]
        state: RecoverFutureState<'f, T, F, E>,
        path_index: usize,
    }
}

pin_project! {
    #[project = Proj]
    pub enum RecoverFutureState<'f, T, F, E>
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

impl<'f, T, F, E> Future for RecoverFuture<'f, T, F, E>
where
    T: FilterExecute<'f>,
    T::Success: OneTuple,
    F: AsyncTryFn<(E,), Ok = <T::Success as Tuple>::Inner, Err = BoxedFilterError>,
    E: Recoverable,
{
    type Output = RequestOutcome<T::Input, T::Success>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut proj = self.as_mut().project();
        match proj.state.as_mut().project() {
            Proj::Filter { future, func } => {
                let RequestOutcome {
                    mut request_state,
                    outcome,
                } = ready!(future.poll(cx));
                match outcome {
                    outcome @ Outcome::Success(_) => {
                        request_state.current_path_index = *proj.path_index;
                        Poll::Ready(RequestOutcome {
                            request_state,
                            outcome,
                        })
                    }
                    Outcome::Error(error) => match E::recover(error) {
                        Ok(error) => {
                            request_state.current_path_index = *proj.path_index;
                            let state = RecoverFutureState::Func {
                                future: func.call((error,)),
                                request_state: Some(request_state),
                                unused: Unused,
                            };
                            proj.state.set(state);
                            self.poll(cx)
                        }
                        Err(error) => Poll::Ready(RequestOutcome {
                            request_state,
                            outcome: Outcome::Error(error),
                        }),
                    },
                    outcome @ Outcome::Forward { .. } => Poll::Ready(RequestOutcome {
                        request_state,
                        outcome,
                    }),
                }
            }
            Proj::Func {
                future,
                request_state,
                ..
            } => {
                let result = ready!(future.poll(cx));
                let mut request_state = request_state.take().unwrap();
                let outcome = match result {
                    Ok(success) => {
                        request_state.current_path_index = *proj.path_index;
                        Outcome::Success(Tuple::from_inner(success))
                    }
                    Err(error) => Outcome::Error(error),
                };
                Poll::Ready(RequestOutcome {
                    request_state,
                    outcome,
                })
            }
        }
    }
}
