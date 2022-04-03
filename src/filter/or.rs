use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::ready;
use pin_project_lite::pin_project;

use super::{FilterExecute, FilterSealed, RequestOutcome};
use crate::{
    outcome::Outcome,
    request::{Request, RequestState},
    FilterBase, Forwarding,
};

#[derive(Copy, Clone, Debug)]
pub struct Or<A, B> {
    pub(super) first: A,
    pub(super) second: B,
}

impl<A, B> FilterSealed for Or<A, B> {}

impl<'f, A, B> FilterBase<'f> for Or<A, B>
where
    A: FilterBase<'f>,
    B: FilterBase<'f>,
{
    type Input = A::Input;

    type Success = A::Success;
}

impl<'f, A, B> FilterExecute<'f> for Or<A, B>
where
    A: FilterExecute<'f>,
    B: FilterExecute<'f, Input = A::Input, Success = A::Success>,
{
    type Future = OrFuture<'f, A, B>;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        let path_index = request_state.current_path_index;
        OrFuture {
            state: OrFutureState::First {
                future: self.first.execute(request, request_state, input),
                second: &self.second,
                request,
            },
            path_index,
        }
    }
}

pin_project! {
    pub struct OrFuture<'f, A, B>
    where
        A: FilterExecute<'f>,
        B: FilterExecute<'f>,
    {
        #[pin]
        state: OrFutureState<'f, A, B>,
        path_index: usize,
    }
}

pin_project! {
    #[project = Proj]
    pub enum OrFutureState<'f, A, B>
    where
        A: FilterExecute<'f>,
        B: FilterExecute<'f>,
    {
        First {
            #[pin]
            future: A::Future,
            second: &'f B,
            request: &'f Request,
        },
        Second {
            #[pin]
            future: B::Future,
            first_forwarding: Option<Forwarding>
        },
    }
}

impl<'f, A, B> Future for OrFuture<'f, A, B>
where
    A: FilterExecute<'f>,
    B: FilterExecute<'f, Input = A::Input, Success = A::Success>,
{
    type Output = RequestOutcome<A::Input, A::Success>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut proj = self.as_mut().project();
        match proj.state.as_mut().project() {
            Proj::First {
                future,
                second,
                request,
            } => {
                let RequestOutcome {
                    mut request_state,
                    outcome,
                } = ready!(future.poll(cx));
                match outcome {
                    Outcome::Success(success) => Poll::Ready(RequestOutcome {
                        request_state,
                        outcome: Outcome::Success(success),
                    }),
                    Outcome::Error(error) => Poll::Ready(RequestOutcome {
                        request_state,
                        outcome: Outcome::Error(error),
                    }),
                    Outcome::Forward { input, forwarding } => {
                        request_state.current_path_index = *proj.path_index;
                        let state = OrFutureState::Second {
                            future: second.execute(request, request_state, input),
                            first_forwarding: Some(forwarding),
                        };
                        proj.state.set(state);
                        self.poll(cx)
                    }
                }
            }
            Proj::Second {
                future,
                first_forwarding,
            } => {
                let RequestOutcome {
                    mut request_state,
                    mut outcome,
                } = ready!(future.poll(cx));
                outcome = match outcome {
                    outcome @ (Outcome::Success(_) | Outcome::Error(_)) => outcome,
                    Outcome::Forward { input, forwarding } => {
                        request_state.current_path_index = *proj.path_index;
                        Outcome::Forward {
                            input,
                            forwarding: first_forwarding.take().unwrap().combine(forwarding),
                        }
                    }
                };
                Poll::Ready(RequestOutcome {
                    request_state,
                    outcome,
                })
            }
        }
    }
}
