use std::{
    fmt,
    future::{ready, Ready},
};

use unused::Unused;

use super::{FilterBase, FilterExecute, FilterSealed};
use crate::{
    generics::tuples::Tuple,
    outcome::{Outcome, RequestOutcome},
    request::{Request, RequestState},
};

pub(crate) fn ready_filter<F, S>(func: F) -> ReadyFilter<F, S>
where
    F: Fn(&Request, &mut RequestState) -> Outcome<(), S> + Send + Sync + 'static,
    S: Tuple + Send + 'static,
{
    ReadyFilter {
        func,
        unused: Unused,
    }
}

pub(crate) struct ReadyFilter<F, S> {
    func: F,
    unused: Unused!(S),
}

impl<F, S> fmt::Debug for ReadyFilter<F, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadyFilter").finish_non_exhaustive()
    }
}

impl<F, S> Clone for ReadyFilter<F, S>
where
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            func: self.func.clone(),
            unused: self.unused,
        }
    }
}

impl<F, S> Copy for ReadyFilter<F, S> where F: Copy {}

impl<F, S> FilterSealed for ReadyFilter<F, S> {}

impl<'f, F, S> FilterBase<'f> for ReadyFilter<F, S>
where
    F: Send + Sync + 'static,
    S: Tuple + 'static,
{
    type Input = ();

    type Success = S;
}

impl<'f, F, S> FilterExecute<'f> for ReadyFilter<F, S>
where
    F: Fn(&Request, &mut RequestState) -> Outcome<(), S> + Send + Sync + 'static,
    S: Tuple + Send + 'static,
{
    type Future = Ready<RequestOutcome<Self::Input, Self::Success>>;

    fn execute(
        &'f self,
        request: &'f Request,
        mut request_state: RequestState,
        (): Self::Input,
    ) -> Self::Future {
        let outcome = (self.func)(request, &mut request_state);
        ready(RequestOutcome {
            request_state,
            outcome,
        })
    }
}
