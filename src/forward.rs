use std::{convert::TryInto, ops};

use crate::{
    header, header::HeaderValue, method::Method, response::default_response, Responder, Response,
    StatusCode,
};

/// Provides data about [`Filter`](crate::Filter)s that fail to match
#[derive(Debug)]
#[non_exhaustive]
pub enum Forwarding {
    /// Represents a resource not being found.
    ///
    /// By default, this will return a [404](StatusCode::NOT_FOUND)
    NotFound,

    /// Represents a resource that is found, but is accessed with a method that is not allowed.
    ///
    /// By default, this will return a [405](StatusCode::METHOD_NOT_ALLOWED)
    MethodNotAllowed(AttemptedMethods),
}

impl Responder for Forwarding {
    fn into_response(self) -> Response {
        match self {
            Forwarding::NotFound => default_response(StatusCode::NOT_FOUND),
            Forwarding::MethodNotAllowed(attempted) => {
                default_response(StatusCode::METHOD_NOT_ALLOWED)
                    .with_header(header::ALLOW, attempted.into_header_value())
            }
        }
    }
}

impl Forwarding {
    pub(crate) fn combine(self, other: Self) -> Self {
        match self {
            Self::NotFound => other,
            Self::MethodNotAllowed(attempted) => match other {
                Self::NotFound => Self::MethodNotAllowed(attempted),
                Self::MethodNotAllowed(other_attempted) => {
                    Self::MethodNotAllowed(attempted | other_attempted)
                }
            },
        }
    }
}

/// Represents methods that were attempted.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AttemptedMethods(u16);

impl AttemptedMethods {
    pub const NONE: Self = Self(0);
    pub const GET: Self = Self(1 << 0);
    pub const POST: Self = Self(1 << 1);
    pub const PUT: Self = Self(1 << 2);
    pub const DELETE: Self = Self(1 << 3);
    pub const HEAD: Self = Self(1 << 4);
    pub const OPTIONS: Self = Self(1 << 5);
    pub const CONNECT: Self = Self(1 << 6);
    pub const PATCH: Self = Self(1 << 7);
    pub const TRACE: Self = Self(1 << 8);

    fn into_header_value(self) -> HeaderValue {
        let mut string = String::with_capacity(10);
        macro_rules! check_method {
            ($method:ident) => {{
                if (self & Self::$method) != Self::NONE {
                    if !string.is_empty() {
                        string += ", ";
                    }
                    string += Method::$method.as_str();
                }
            }};
        }
        check_method!(GET);
        check_method!(POST);
        check_method!(PUT);
        check_method!(DELETE);
        check_method!(HEAD);
        check_method!(OPTIONS);
        check_method!(CONNECT);
        check_method!(PATCH);
        check_method!(TRACE);
        string
            .try_into()
            .expect("Constructed header value must be correct")
    }
}

impl ops::BitOr for AttemptedMethods {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl ops::BitAnd for AttemptedMethods {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}
