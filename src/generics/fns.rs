use std::future::Future;

use self::sealed::{AsyncTryFnSealed, TupleFnOnceSealed};
use super::tuples::Tuple;

mod sealed {
    pub trait TupleFnOnceSealed<Args> {}

    pub trait AsyncTryFnSealed<Args> {}
}

pub trait TupleFnOnce<Args: Tuple>: TupleFnOnceSealed<Args> {
    type Return;

    fn call(self, args: Args) -> Self::Return;
}

impl<F, Return> TupleFnOnceSealed<()> for F where F: FnOnce() -> Return {}

impl<F, Return> TupleFnOnce<()> for F
where
    F: FnOnce() -> Return,
{
    type Return = Return;

    fn call(self, (): ()) -> Self::Return {
        (self)()
    }
}

macro_rules! define_tuple_fn_once {
    ($($args:ident),+) => {
        not_last!(define_tuple_fn_once() => $($args,)+);
    };
    (;; $_:ident) => {};
    (; $($args:ident,)+; $_:ident) => {
        not_last!(define_tuple_fn_once() => $($args,)+);

        impl<F, $($args,)* Return> TupleFnOnceSealed<($($args,)*)> for F
        where
            F: FnOnce($($args,)*) -> Return,
        {}

        impl<F, $($args,)* Return> TupleFnOnce<($($args,)*)> for F
        where
            F: FnOnce($($args,)*) -> Return,
        {
            type Return = Return;

            fn call(self, #[allow(non_snake_case)] ($($args,)*): ($($args,)*)) -> Self::Return {
                (self)($($args,)*)
            }
        }
    };
}

define_tuple_fn_once!(Arg1, Arg2, Arg3, Arg4, Arg5, Arg6, Arg7, Arg8, Arg9, Arg10, Arg11, Arg12);

/// An async version of [`Fn`] that takes a [`Tuple`] for arguments and returns a [`Result`]
pub trait AsyncTryFn<Args: Tuple>: AsyncTryFnSealed<Args> {
    type Ok;

    type Err;

    type Future: Future<Output = Result<Self::Ok, Self::Err>>;

    fn call(&self, args: Args) -> Self::Future;
}

impl<F, Fut, Ok, Err> AsyncTryFnSealed<()> for F
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<Ok, Err>>,
{
}

impl<F, Fut, Ok, Err> AsyncTryFn<()> for F
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<Ok, Err>>,
{
    type Ok = Ok;

    type Err = Err;

    type Future = Fut;

    fn call(&self, (): ()) -> Self::Future {
        (self)()
    }
}

macro_rules! define_async_try_fn {
    ($($args:ident),+) => {
        not_last!(define_async_try_fn() => $($args,)+);
    };
    (;; $_:ident) => {};
    (; $($args:ident,)+; $_:ident) => {
        not_last!(define_async_try_fn() => $($args,)+);

        impl<F, $($args,)* Fut> AsyncTryFnSealed<($($args,)*)> for F
        where
            F: Fn($($args,)*) -> Fut,
            Fut: Future,
        {}

        impl<F, $($args,)* Fut, Ok, Err> AsyncTryFn<($($args,)*)> for F
        where
            F: Fn($($args,)*) -> Fut,
            Fut: Future<Output = Result<Ok, Err>>,
        {
            type Ok = Ok;

            type Err = Err;

            type Future = Fut;

            fn call(&self, #[allow(non_snake_case)] ($($args,)*): ($($args,)*)) -> Self::Future {
                (self)($($args,)*)
            }
        }
    };
}

define_async_try_fn!(Arg1, Arg2, Arg3, Arg4, Arg5, Arg6, Arg7, Arg8, Arg9, Arg10, Arg11, Arg12);
