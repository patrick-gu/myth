use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::ready;
use pin_project_lite::pin_project;

use super::{FilterExecute, FilterSealed, RequestOutcome};
use crate::{
    generics::tuples::Append,
    outcome::Outcome,
    request::{Request, RequestState},
    FilterBase,
};

#[derive(Copy, Clone, Debug)]
pub struct And<A, B> {
    pub(super) first: A,
    pub(super) second: B,
}

impl<A, B> FilterSealed for And<A, B> {}

impl<'f, A, B> FilterBase<'f> for And<A, B>
where
    A: FilterBase<'f>,
    A::Success: Append<B::Success>,
    B: FilterBase<'f>,
{
    type Input = B::Input;

    type Success = <A::Success as Append<B::Success>>::Appended;
}

impl<'f, A, B> FilterExecute<'f> for And<A, B>
where
    A: FilterExecute<'f, Input = ()>,
    A::Success: Append<B::Success> + Send,
    B: FilterExecute<'f>,
    B::Input: Send,
{
    type Future = AndFuture<'f, A, B>;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        AndFuture::First {
            future: self.first.execute(request, request_state, ()),
            second: &self.second,
            request,
            input: Some(input),
        }
    }
}

pin_project! {
    #[project = Proj]
    pub enum AndFuture<'f, A, B>
    where
        A: FilterExecute<'f>,
        B: FilterExecute<'f>,
    {
        First {
            #[pin]
            future: A::Future,
            second: &'f B,
            request: &'f Request,
            input: Option<B::Input>,
        },
        Second {
            #[pin]
            future: B::Future,
            first_success: Option<A::Success>,
        },
    }
}

impl<'f, A, B> Future for AndFuture<'f, A, B>
where
    A: FilterExecute<'f, Input = ()>,
    A::Success: Append<B::Success>,
    B: FilterExecute<'f>,
{
    type Output = RequestOutcome<B::Input, <A::Success as Append<B::Success>>::Appended>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            Proj::First {
                future,
                second,
                request,
                input,
            } => {
                let RequestOutcome {
                    request_state,
                    outcome,
                } = ready!(future.poll(cx));
                let input = input.take().unwrap();
                match outcome {
                    Outcome::Success(success) => {
                        let state = Self::Second {
                            future: second.execute(request, request_state, input),
                            first_success: Some(success),
                        };
                        self.as_mut().set(state);
                        self.poll(cx)
                    }
                    Outcome::Error(error) => Poll::Ready(RequestOutcome {
                        request_state,
                        outcome: Outcome::Error(error),
                    }),
                    Outcome::Forward { forwarding, .. } => Poll::Ready(RequestOutcome {
                        request_state,
                        outcome: Outcome::Forward { forwarding, input },
                    }),
                }
            }
            Proj::Second {
                future,
                first_success,
            } => future.poll(cx).map(|request_outcome| {
                request_outcome
                    .map(|second_success| first_success.take().unwrap().append(second_success))
            }),
        }
    }
}
