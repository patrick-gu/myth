use std::fmt;

use serde::de::DeserializeOwned;
use serde_urlencoded::de;

use crate::{
    errors::FilterError,
    impl_Filter,
    response::default_response,
    uri::{uri, Uri},
    Filter, Response, Result, StatusCode,
};

pub fn optional() -> impl_Filter!('f, Option<&'f str> => (Copy) + (fmt::Debug)) {
    async fn handler(uri: &Uri) -> Result<Option<&str>> {
        Ok(uri.query())
    }

    uri().handle(handler)
}

#[derive(Debug)]
pub enum DeserializeError {
    NoQuery,
    Deserializing(de::Error),
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeError::NoQuery => write!(f, "no query was present"),
            DeserializeError::Deserializing(error) => write!(f, "failed to deserialize: {}", error),
        }
    }
}

impl FilterError for DeserializeError {
    fn into_response(self: Box<Self>) -> Response {
        tracing::debug!("default response for query deserialization error: {}", self);
        default_response(StatusCode::BAD_REQUEST)
    }
}

pub fn deserialize<T: DeserializeOwned + Send + 'static>() -> impl_Filter!(T => Copy + (fmt::Debug))
{
    async fn handler<T: DeserializeOwned + Send>(option: Option<&str>) -> Result<T> {
        let query = option.ok_or(DeserializeError::NoQuery)?;
        let t = serde_urlencoded::from_str(query).map_err(DeserializeError::Deserializing)?;
        Ok(t)
    }

    optional().handle(handler)
}
