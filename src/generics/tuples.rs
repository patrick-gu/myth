use self::sealed::TupleSealed;

mod sealed {
    pub trait TupleSealed {}
}

/// A [`Sized`] tuple of up to length 12
pub trait Tuple: TupleSealed + Sized {
    /// For `(T,)` this is T. Otherwise, this is `Self`.
    type Inner;

    /// Converts this tuple to [inner](Self::Inner) value
    fn into_inner(self) -> Self::Inner;

    /// Converts the [inner](Self::Inner) value into this tuple
    fn from_inner(inner: Self::Inner) -> Self;
}

/// A [`Tuple`] that is not [`()`](unit)
pub trait NonEmptyTuple: Tuple {}

/// A [`Tuple`] that can have one element appended to its end
pub trait AppendOne<Elem>: Tuple {
    type Appended: RemoveOne<Removed = Self, Elem = Elem>;

    fn append_one(self, elem: Elem) -> Self::Appended;
}

/// A [`Tuple`] that can have one element removed from its end
pub trait RemoveOne: Tuple {
    type Removed: AppendOne<Self::Elem>;

    type Elem;

    fn remove_one(self) -> (Self::Removed, Self::Elem);
}

pub trait PushOne<Elem>: Tuple {
    type Pushed: PopOne<Popped = Self, Elem = Elem>;

    fn push_one(self, elem: Elem) -> Self::Pushed;
}

/// A [`Tuple`] that can have one element removed from its front
pub trait PopOne: Tuple {
    type Popped: PushOne<Self::Elem, Pushed = Self>;

    type Elem;

    fn pop_one(self) -> (Self::Popped, Self::Elem);
}

/// A [`Tuple`] that can append an `Ending` [`Tuple`] to its end
pub trait Append<Ending: Tuple>: Tuple {
    type Appended: Tuple;

    fn append(self, ending: Ending) -> Self::Appended;

    fn remove(appended: Self::Appended) -> (Self, Ending);
}

impl<T> Append<()> for T
where
    T: Tuple,
{
    type Appended = Self;

    fn append(self, (): ()) -> Self::Appended {
        self
    }

    fn remove(appended: Self::Appended) -> (Self, ()) {
        (appended, ())
    }
}

impl<T, Ending> Append<Ending> for T
where
    Self: AppendOne<Ending::Elem>,
    Ending: PopOne,
    <Self as AppendOne<Ending::Elem>>::Appended: Append<Ending::Popped>,
{
    type Appended =
        <<Self as AppendOne<Ending::Elem>>::Appended as Append<Ending::Popped>>::Appended;

    fn append(self, ending: Ending) -> Self::Appended {
        let (ending, elem) = ending.pop_one();
        self.append_one(elem).append(ending)
    }

    fn remove(appended: Self::Appended) -> (Self, Ending) {
        let (appended, ending) = <<Self as AppendOne<Ending::Elem>>::Appended as Append<
            Ending::Popped,
        >>::remove(appended);
        let (beginning, elem) = appended.remove_one();
        (beginning, ending.push_one(elem))
    }
}

macro_rules! define_tuple {
    ($($elems:ident),+) => {
        define_tuple!(; $($elems,)+; __);
    };
    (;; $_:ident) => {};
    (; $($elems:ident,)+; $_:ident) => {
        not_last!(define_tuple() => $($elems,)+);
        define_tuple!(basic; $($elems,)+);
        define_tuple!(non_empty; $($elems,)+);
        not_last!(define_tuple(append_one) => $($elems,)+);
        define_tuple!(push_one; $($elems,)+);
    };
    (basic; $elem:ident,) => {};
    (basic; $($elems:ident,)*) => {
        impl<$($elems,)*> TupleSealed for ($($elems,)*) {}

        impl<$($elems,)*> Tuple for ($($elems,)*) {
            type Inner = Self;

            fn into_inner(self) -> Self::Inner {
                self
            }

            fn from_inner(inner: Self::Inner) -> Self {
                inner
            }
        }
    };
    (non_empty; $($elems:ident,)+) => {
        impl<$($elems,)+> NonEmptyTuple for ($($elems,)+) {}
    };
    (append_one; $($elems:ident,)*; $last:ident) => {
        impl<$($elems,)* $last> AppendOne<$last> for ($($elems,)*) {
            type Appended = ($($elems,)* $last,);

            fn append_one(self, elem: $last) -> Self::Appended {
                #[allow(non_snake_case)]
                let ($($elems,)*) = self;
                ($($elems,)* elem,)
            }
        }

        impl<$($elems,)* $last> RemoveOne for ($($elems,)* $last,) {
            type Removed = ($($elems,)*);

            type Elem = $last;

            fn remove_one(self) -> (Self::Removed, Self::Elem) {
                #[allow(non_snake_case)]
                let ($($elems,)* last,) = self;
                (($($elems,)*), last)
            }
        }
    };
    (push_one; $first:ident, $($elems:ident,)*) => {
        impl<$first, $($elems,)*> PushOne<$first> for ($($elems,)*) {
            type Pushed = ($first, $($elems,)*);

            fn push_one(self, elem: $first) -> Self::Pushed {
                #[allow(non_snake_case)]
                let ($($elems,)*) = self;
                (elem, $($elems,)*)
            }
        }

        impl<$first, $($elems,)*> PopOne for ($first, $($elems,)*) {
            type Popped = ($($elems,)*);

            type Elem = $first;

            fn pop_one(self) -> (Self::Popped, Self::Elem) {
                #[allow(non_snake_case)]
                let (elem, $($elems,)*) = self;
                (($($elems,)*), elem)
            }
        }
    };
}

define_tuple!(basic;);

impl<T> TupleSealed for (T,) {}

impl<T> Tuple for (T,) {
    type Inner = T;

    fn into_inner(self) -> Self::Inner {
        self.0
    }

    fn from_inner(inner: Self::Inner) -> Self {
        (inner,)
    }
}

define_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

pub trait OneTuple: NonEmptyTuple {}

impl<T> OneTuple for (T,) {}

#[cfg(test)]
mod tests {
    use super::Append;

    #[test]
    fn append() {
        let tup1 = (5, 6, false);
        let string = "a".to_owned();
        let tup2 = (&*string, 8.0, true);
        let tup = tup1.append(tup2);
        assert_eq!(tup, (5, 6, false, "a", 8.0, true));

        let tup1 = (7, 8, "a".to_string());
        let tup1_clone = tup1.clone();
        let tup2 = ();
        let tup = tup1.append(tup2);
        assert_eq!(tup, tup1_clone);

        let tup1 = ();
        let tup2 = ();
        let _tup: () = tup1.append(tup2);

        let tup1 = ("e".to_owned(),);
        let tup2 = ();
        let tup = tup1.append(tup2);
        assert_eq!(tup, ("e".to_owned(),));

        let tup1 = ();
        let tup2 = (6, 6, 6, 6, 6, 6, 6, 6, 7);
        let tup2_copy = tup2;
        let tup = tup1.append(tup2);
        assert_eq!(tup, tup2_copy);

        let tup1 = ('a', 'b', 'c', 'd', 'e', 'f');
        let tup2 = ('g', 'h', 'i', 'j');
        let tup = tup1.append(tup2);
        assert_eq!(tup, ('a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'));
    }
}
