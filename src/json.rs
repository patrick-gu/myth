//! JSON [request] and [response] bodies

use std::fmt;

use mime::Mime;
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    body,
    errors::FilterError,
    header::{self, content_type, HeaderValue},
    impl_Filter,
    response::default_response,
    Filter, Responder, Response, StatusCode,
};

/// An error for the [request] filter.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The [`Content-Type`](header::CONTENT_TYPE) was not present.
    NoContentType,

    /// [`Content-Type`](header::CONTENT_TYPE) was not `application/json` or not valid.
    WrongContentType(HeaderValue),

    /// An error occured while reading the request body.
    Reading(body::Error),

    /// An error occured while deserializing the request body as JSON.
    Deserializing(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoContentType => write!(f, "missing application/json content type"),
            Self::WrongContentType(mime) => write!(
                f,
                "expected application/json content type, instead got {:?}",
                mime
            ),
            Self::Reading(inner) => {
                write!(f, "error while reading body as JSON: {}", inner)
            }
            Self::Deserializing(inner) => {
                write!(f, "error while deserializing body as JSON: {}", inner)
            }
        }
    }
}

impl FilterError for Error {
    fn into_response(self: Box<Self>) -> Response {
        tracing::debug!("default response for JSON request body error: {}", self);
        match *self {
            Self::NoContentType | Self::WrongContentType(_) => {
                default_response(StatusCode::UNSUPPORTED_MEDIA_TYPE)
            }
            Self::Reading(_) | Self::Deserializing(_) => default_response(StatusCode::BAD_REQUEST),
        }
    }
}

/// Creates a [`Filter`] that matches the JSON body of a request.
pub fn request<T: DeserializeOwned + Send + 'static>() -> impl_Filter!(T => Clone + (fmt::Debug)) {
    async fn handler(option: Option<Mime>, value: Option<&HeaderValue>) -> crate::Result<()> {
        match option {
            Some(mime) if mime.type_() == mime::APPLICATION && mime.subtype() == mime::JSON => {
                Ok(())
            }
            _ => Err(value
                .cloned()
                .map(Error::WrongContentType)
                .unwrap_or(Error::NoContentType)
                .into()),
        }
    }
    content_type()
        .handle(handler)
        .untuple()
        .and(
            body::all()
                .recover(|error: body::Error| async move { Err(Error::Reading(error).into()) }),
        )
        .handle(|readable| async move {
            serde_json::from_reader(readable).map_err(|error| Error::Deserializing(error).into())
        })
}

static APPLICATION_JSON: HeaderValue = HeaderValue::from_static("application/json");

pub fn response<T: Serialize>(value: T) -> Result<Response, serde_json::Error> {
    let vec = serde_json::to_vec(&value)?;
    Ok(vec
        .into_response()
        .with_header(header::CONTENT_TYPE, APPLICATION_JSON.clone()))
}
