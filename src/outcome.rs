use crate::{errors::BoxedFilterError, request::RequestState, Forwarding};

/// Outcome of a [Filter](crate::Filter)
#[derive(Debug)]
pub enum Outcome<C, S> {
    /// The success case
    Success(S),

    /// The error case
    Error(BoxedFilterError),

    /// The forwarding case
    Forward { input: C, forwarding: Forwarding },
}

impl<C, S> Outcome<C, S> {
    pub(crate) fn map<F, S1>(self, func: F) -> Outcome<C, S1>
    where
        F: FnOnce(S) -> S1,
    {
        match self {
            Self::Success(success) => Outcome::Success(func(success)),
            Self::Error(error) => Outcome::Error(error),
            Self::Forward { input, forwarding } => Outcome::Forward { input, forwarding },
        }
    }

    pub(crate) fn map_input<F, C1>(self, func: F) -> Outcome<C1, S>
    where
        F: FnOnce(C) -> C1,
    {
        match self {
            Self::Success(success) => Outcome::Success(success),
            Self::Error(error) => Outcome::Error(error),
            Self::Forward { input, forwarding } => Outcome::Forward {
                input: func(input),
                forwarding,
            },
        }
    }
}

impl<C, S> From<Result<S, BoxedFilterError>> for Outcome<C, S> {
    fn from(result: Result<S, BoxedFilterError>) -> Self {
        match result {
            Ok(success) => Self::Success(success),
            Err(error) => Self::Error(error),
        }
    }
}

#[derive(Debug)]
pub struct RequestOutcome<C, S> {
    pub(crate) request_state: RequestState,
    pub(crate) outcome: Outcome<C, S>,
}

impl<C, S> RequestOutcome<C, S> {
    pub(crate) fn map<F, S1>(self, func: F) -> RequestOutcome<C, S1>
    where
        F: FnOnce(S) -> S1,
    {
        RequestOutcome {
            request_state: self.request_state,
            outcome: self.outcome.map(func),
        }
    }

    pub(crate) fn map_input<F, C1>(self, func: F) -> RequestOutcome<C1, S>
    where
        F: FnOnce(C) -> C1,
    {
        RequestOutcome {
            request_state: self.request_state,
            outcome: self.outcome.map_input(func),
        }
    }
}
