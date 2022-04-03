use std::{
    any::{Any, TypeId},
    error::Error as StdError,
    fmt,
};

use self::private::{IsAny, Private, RecoverableSealed};
use crate::{response::default_response, Response, StatusCode};

mod private {
    use std::any::Any;

    use super::{BoxedFilterError, FilterError, Result};

    /// Ensure `as_any` cannot be called from outside crate
    pub struct Private;

    /// Helper trait to downcast boxed by making use of the existing impl on
    /// `Box<dyn Any>`.
    pub trait IsAny: Any {
        fn as_any(self: Box<Self>, _: Private) -> Box<dyn Any>;
    }

    impl<T: Any> IsAny for T {
        fn as_any(self: Box<Self>, _: Private) -> Box<dyn Any> {
            self
        }
    }

    pub trait RecoverableSealed: Sized + 'static {
        fn recover(boxed: BoxedFilterError) -> Result<Self>;
    }

    impl<T: FilterError> RecoverableSealed for T {
        fn recover(boxed: BoxedFilterError) -> Result<Self> {
            boxed.downcast()
        }
    }

    impl RecoverableSealed for BoxedFilterError {
        fn recover(boxed: BoxedFilterError) -> Result<Self> {
            Ok(boxed)
        }
    }
}

pub type Result<T = Response> = std::result::Result<T, BoxedFilterError>;

pub type BoxedFilterError = Box<dyn FilterError>;

pub trait FilterError: fmt::Debug + Send + 'static + IsAny {
    fn into_response(self: Box<Self>) -> Response {
        tracing::debug!("Responding with unhandled error: {:?}", self);
        default_response(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl<T> FilterError for T
where
    T: StdError + Send + 'static,
{
    fn into_response(self: Box<Self>) -> Response {
        tracing::debug!("Responding with unhandled error: {}", self);
        default_response(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl<T: FilterError> From<T> for BoxedFilterError {
    fn from(error: T) -> Self {
        Box::new(error)
    }
}

impl dyn FilterError {
    fn downcast<T: FilterError>(self: Box<Self>) -> Result<T> {
        if Any::type_id(&*self) == TypeId::of::<T>() {
            Ok(*self
                .as_any(Private)
                .downcast()
                .expect("Downcast will never fail because equivalence has been checked"))
        } else {
            Err(self)
        }
    }
}

/// Marker traits for recovery
pub trait Recoverable: RecoverableSealed {}

impl<T: FilterError> Recoverable for T {}

impl Recoverable for BoxedFilterError {}

#[cfg(test)]
mod tests {
    use std::{error::Error as StdError, fmt};

    use super::FilterError;
    use crate::errors::BoxedFilterError;

    #[derive(Default, Debug)]
    struct SomeError {
        data: i32,
    }

    impl fmt::Display for SomeError {
        fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
            unimplemented!()
        }
    }

    impl StdError for SomeError {}

    #[derive(Default, Debug)]
    struct OtherError {
        data: String,
    }

    impl FilterError for OtherError {
        fn into_response(self: Box<Self>) -> crate::Response {
            unimplemented!()
        }
    }

    #[test]
    fn downcast_test() {
        let boxed: BoxedFilterError = Box::new(SomeError { data: 555 });

        let error = boxed.downcast::<SomeError>().unwrap();

        assert_eq!(error.data, 555);

        let boxed: BoxedFilterError = Box::new(OtherError {
            data: "abcdef".to_owned(),
        });

        let obj = boxed.downcast::<SomeError>().unwrap_err();

        let obj = obj.downcast::<SomeError>().unwrap_err();

        let error = obj.downcast::<OtherError>().unwrap();

        assert_eq!(error.data, "abcdef");
    }
}
