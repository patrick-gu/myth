mod and;
mod dynamic;
mod handle;
mod or;
pub(crate) mod ready;
mod receive;
mod recover;
mod recover_forward;
mod then;
mod untuple;

use std::{future::Future, sync::Arc};

use unused::Unused;

pub use self::dynamic::DynamicFilter;
use self::{
    and::And, dynamic::BoxedFutureFilter, handle::Handle, or::Or, receive::Receive,
    recover::Recover, recover_forward::RecoverForward, then::Then, untuple::Untuple,
};
use crate::{
    generics::tuples::Tuple,
    outcome::RequestOutcome,
    request::{Request, RequestState},
};

pub trait FilterSealed {}

/// A base trait for [`Filter`]
pub trait FilterBase<'f>: FilterSealed + Send + Sync + 'static {
    /// The input for the filter
    type Input: Tuple;

    /// The output for the filter
    type Success: Tuple;
}

pub trait FilterExecute<'f>: FilterBase<'f> {
    /// [Future] returned by [execute](Filter#execute)
    type Future: Future<Output = RequestOutcome<Self::Input, Self::Success>> + Send + 'f;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future;
}

/// The building blocks of request handling
///
/// This trait is sealed
#[cfg_attr(myth_docs, doc(notable_trait))]
#[must_use]
pub trait Filter:
    for<'f> FilterBase<'f> + Send + Sync + 'static + for<'f> FilterExecute<'f>
{
    /// Combines two `Filter`s, joining their success results together. This
    /// Requires both `Filter`s to succeed.
    ///
    /// Only works if [`Self::Input`](FilterBase::Input) is `()`
    /// [`Self::Success`](FilterBase::Success) must be [`Send`]
    /// O must be [`Filter`]
    /// O::Input must be [`Send`]
    /// Combined, Self::Success and O::Success must not exceed 12 elements
    fn and<O>(self, other: O) -> And<Self, O>
    where
        Self: Sized,
        And<Self, O>: Filter,
    {
        And {
            first: self,
            second: other,
        }
    }

    /// Combines another `Filter` after this `Filter` if this one forwards
    ///
    /// Only applies if
    /// [`Other::Input`](FilterBase::Input) is the same as [`Self::Input`](FilterBase::Input) and
    /// [`Other::Success`](FilterBase::Success) is the same as [`Self::Success`](FilterBase::Success)
    fn or<O>(self, other: O) -> Or<Self, O>
    where
        Self: Sized,
        Or<Self, O>: Filter,
    {
        Or {
            first: self,
            second: other,
        }
    }

    /// Combines this filter with an function that takes [`Self::Success`](FilterBase::Success).
    ///
    /// The function should be asynchronous, and resolve to a [`Result<T>`](crate::Result)
    /// The function needs to be [`Send`] + [`Sync`] + `'static`.
    /// The function's [`Future`] needs to be [`Send`].
    fn handle<F>(self, func: F) -> Handle<Self, F>
    where
        Self: Sized,
        Handle<Self, F>: Filter,
    {
        Handle { filter: self, func }
    }

    /// Only works if [`Self::Input`](FilterBase::Input) is `()`
    /// `R` must be able to be combined with [`Self::Success`](FilterBase::Success) and not exceed 12 elements
    /// also, `O` must consume that combined
    /// also, `R` must be [`Send`] and `'static`
    fn then<O, R>(self, other: O) -> Then<Self, O, R>
    where
        Self: Sized,
        Then<Self, O, R>: Filter,
    {
        Then {
            first: self,
            second: other,
            unused: Unused,
        }
    }

    // only works if self returns one-tuple
    // `F` should be async function that takes a [`Recoverable`] and returns Result<Success, BoxedFilterError>
    // `F` needs to be [`Send`] + [`Sync`] + `'static` and its future needs to be [`Send`]
    fn recover<F, E>(self, func: F) -> Recover<Self, F, E>
    where
        Self: Sized,
        Recover<Self, F, E>: Filter,
    {
        Recover {
            filter: self,
            func,
            unused: Unused,
        }
    }

    fn recover_forward<F, E>(self, func: F) -> RecoverForward<Self, F, E>
    where
        Self: Sized,
    {
        RecoverForward {
            filter: self,
            func,
            unused: Unused,
        }
    }

    /// `R` must append [`Self::Input`](FilterBase::Input) and [`Self::Success`](FilterBase::Success)
    /// `R` must be [`Send`] and `'static`
    fn receive<R>(self) -> Receive<Self, R>
    where
        Self: Sized,
        Receive<Self, R>: Filter,
    {
        Receive {
            filter: self,
            unused: Unused,
        }
    }

    /// Destructure into inner tuple
    /// `Self::Success` must be `(T,)` where `T` is a tuple
    fn untuple(self) -> Untuple<Self>
    where
        Self: Sized,
        Untuple<Self>: Sized,
    {
        Untuple(self)
    }

    /// Makes this [`Filter`] be dispatched dynamically
    ///
    /// May reduce compile times
    fn dynamic<C, S>(self) -> DynamicFilter<C, S>
    where
        Self: Sized + for<'f> FilterBase<'f, Input = C, Success = S>,
    {
        DynamicFilter(Arc::new(BoxedFutureFilter(self)))
    }
}

impl<T: ?Sized> Filter for T where T: for<'f> FilterExecute<'f> {}

/// Shortcut for <code>impl [Filter]</code>
#[macro_export]
macro_rules! impl_Filter {
    ($lt:lifetime, $input:ty, ($($success:ty),* $(,)?) $(=> $bound:tt $(+ $bounds:tt)* $(+)?)?) => {
        impl #crate::Filter + for<$lt> $crate::FilterBase::<$lt, Input = ($input,), Success = ($($success,)*)> $(+ $bound $(+ $bounds)*)?
    };
    ($lt:lifetime, $input:ty, $success:ty $(=> $bound:tt $(+ $bounds:tt)* $(+)?)?) => {
        impl $crate::Filter + for<$lt> $crate::FilterBase::<$lt, Input = $input, Success = ($success,)> $(+ $bound $(+ $bounds)*)?
    };
    ($lt:lifetime, ($($success:ty),* $(,)?) $(=> $bound:tt $(+ $bounds:tt)* $(+)?)?) => {
        impl $crate::Filter + for<$lt> $crate::FilterBase::<$lt, Input = (), Success = ($($success,)*)> $(+ $bound $(+ $bounds)*)?
    };
    ($lt:lifetime, $success:ty $(=> $bound:tt $(+ $bounds:tt)* $(+)?)?) => {
        impl $crate::Filter + for<$lt> $crate::FilterBase::<$lt, Input = (), Success = ($success,)> $(+ $bound $(+ $bounds)*)?
    };
    ($input:ty, ($($success:ty),* $(,)?) $(=> $bound:tt $(+ $bounds:tt)* $(+)?)?) => {
        impl $crate::Filter + for<'__filter> $crate::FilterBase::<'__filter, Input = ($input,), Success = ($($success,)*)> $(+ $bound $(+ $bounds)*)?
    };
    ($input:ty, $success:ty $(=> $bound:tt $(+ $bounds:tt)* $(+)?)?) => {
        impl $crate::Filter + for<'__filter> $crate::FilterBase::<'__filter, Input = ($input,), Success = ($success,)> $(+ $bound $(+ $bounds)*)?
    };
    (($($success:ty),* $(,)?) $(=> $bound:tt $(+ $bounds:tt)* $(+)?)?) => {
        impl $crate::Filter + for<'__filter> $crate::FilterBase::<'__filter, Input = (), Success = ($($success,)*)> $(+ $bound $(+ $bounds)*)?
    };
    ($success:ty $(=> $bound:tt $(+ $bounds:tt)* $(+)?)?) => {
        impl $crate::Filter + for<'__filter> $crate::FilterBase::<'__filter, Input = (), Success = ($success,)> $(+ $bound $(+ $bounds)*)?
    };
}

impl<T> FilterSealed for Arc<T> where T: FilterSealed {}

impl<'f, T> FilterBase<'f> for Arc<T>
where
    T: FilterBase<'f>,
{
    type Input = T::Input;

    type Success = T::Success;
}

impl<'f, T> FilterExecute<'f> for Arc<T>
where
    T: FilterExecute<'f>,
{
    type Future = T::Future;

    fn execute(
        &'f self,
        request: &'f Request,
        request_state: RequestState,
        input: Self::Input,
    ) -> Self::Future {
        T::execute(self, request, request_state, input)
    }
}
