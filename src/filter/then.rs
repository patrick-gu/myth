use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::ready;
use pin_project_lite::pin_project;
use unused::Unused;

use super::{FilterExecute, FilterSealed, RequestOutcome};
use crate::{
    generics::tuples::{Append, Tuple},
    outcome::Outcome,
    request::{Request, RequestState},
    FilterBase,
};

pub struct Then<A, B, R> {
    pub(super) first: A,
    pub(super) second: B,
    pub(super) unused: Unused!(R),
}

impl<A, B, R> fmt::Debug for Then<A, B, R>
where
    A: fmt::Debug,
    B: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Then")
            .field("first", &self.first)
            .field("second", &self.second)
            .finish_non_exhaustive()
    }
}

impl<A, B, R> Clone for Then<A, B, R>
where
    A: Clone,
    B: Clone,
{
    fn clone(&self) -> Self {
        Self {
            first: self.first.clone(),
            second: self.second.clone(),
            unused: self.unused,
        }
    }
}

impl<A, B, R> Copy for Then<A, B, R>
where
    A: Copy,
    B: Copy,
{
}

impl<A, B, R> FilterSealed for Then<A, B, R> {}

impl<'f, A, B, R> FilterBase<'f> for Then<A, B, R>
where
    A: FilterBase<'f>,
    B: FilterBase<'f>,
    R: Tuple + 'static,
{
    type Input = R;

    type Success = B::Success;
}

impl<'f, A, B, R> FilterExecute<'f> for Then<A, B, R>
where
    A: FilterExecute<'f, Input = ()>,
    B: FilterExecute<'f, Input = <R as Append<A::Success>>::Appended>,
    R: Append<A::Success> + Send + 'static,
{
    type Future = ConsumeFuture<'f, A, B, R>;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        ConsumeFuture::First {
            future: self.first.execute(request, request_state, ()),
            second: &self.second,
            request,
            prepend: Some(input),
        }
    }
}

pin_project! {
    #[project = Proj]
    pub enum ConsumeFuture<'f, A, B, R>
    where
        A: FilterExecute<'f>,
        B: FilterExecute<'f>,
    {
        First {
            #[pin]
            future: A::Future,
            second: &'f B,
            request: &'f Request,
            prepend: Option<R>,
        },
        Second {
            #[pin]
            future: B::Future,
        }
    }
}

impl<'f, A, B, R> Future for ConsumeFuture<'f, A, B, R>
where
    A: FilterExecute<'f>,
    B: FilterExecute<'f, Input = <R as Append<A::Success>>::Appended>,
    R: Append<A::Success>,
{
    type Output = RequestOutcome<R, B::Success>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            Proj::First {
                future,
                second,
                request,
                prepend,
            } => {
                let RequestOutcome {
                    request_state,
                    outcome,
                } = ready!(future.poll(cx));
                let prepend = prepend.take().unwrap();
                match outcome {
                    Outcome::Success(success) => {
                        let state = Self::Second {
                            future: second.execute(request, request_state, prepend.append(success)),
                        };
                        self.set(state);
                        self.poll(cx)
                    }
                    Outcome::Error(error) => Poll::Ready(RequestOutcome {
                        request_state,
                        outcome: Outcome::Error(error),
                    }),
                    Outcome::Forward { forwarding, .. } => Poll::Ready(RequestOutcome {
                        request_state,
                        outcome: Outcome::Forward {
                            input: prepend,
                            forwarding,
                        },
                    }),
                }
            }
            Proj::Second { future } => future.poll(cx).map(|request_outcome| {
                request_outcome.map_input(|input| <R as Append<A::Success>>::remove(input).0)
            }),
        }
    }
}
