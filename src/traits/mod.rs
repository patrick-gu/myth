//! Workaround traits for <https://github.com/rust-lang/rust/issues/70263>

use crate::generics::{
    fns::TupleFnOnce,
    tuples::{NonEmptyTuple, Tuple},
};
pub trait TupleFnOnceFor<'a, Args: Tuple>: TupleFnOnce<Args> {}

impl<'a, T: TupleFnOnce<Args>, Args: Tuple> TupleFnOnceFor<'a, Args> for T {}

pub trait NonEmptyTupleFor<'a>: NonEmptyTuple {}

impl<'a, T: NonEmptyTuple> NonEmptyTupleFor<'a> for T {}
