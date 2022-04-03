use std::fmt;

use mime::Mime;
use serde::de::DeserializeOwned;

use crate::{
    body,
    errors::FilterError,
    header::{content_type, HeaderValue},
    impl_Filter,
    response::default_response,
    Filter, Response, StatusCode,
};

/// An error for the [request] filter.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The [`Content-Type`](crate::header::CONTENT_TYPE) was not present.
    NoContentType,

    /// [`Content-Type`](crate::header::CONTENT_TYPE) was not `application/x-www-form-urlencoded` or not valid.
    WrongContentType(HeaderValue),

    /// An error occured while reading the request body.
    Reading(body::Error),

    /// An error occured while deserializing the request body as urlencoded data.
    Deserializing(serde_urlencoded::de::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoContentType => {
                write!(f, "missing application/x-www-form-urlencoded content type")
            }
            Self::WrongContentType(mime) => write!(
                f,
                "expected application/x-www-form-urlencoded content type, instead got {:?}",
                mime
            ),
            Self::Reading(inner) => {
                write!(f, "error while reading body as urlencoded data: {}", inner)
            }
            Self::Deserializing(inner) => {
                write!(
                    f,
                    "error while deserializing body as urlencoded data: {}",
                    inner
                )
            }
        }
    }
}

impl FilterError for Error {
    fn into_response(self: Box<Self>) -> Response {
        tracing::debug!(
            "default response for urlencoded request body error: {}",
            self
        );
        match *self {
            Self::NoContentType | Self::WrongContentType(_) => {
                default_response(StatusCode::UNSUPPORTED_MEDIA_TYPE)
            }
            Self::Reading(_) | Self::Deserializing(_) => default_response(StatusCode::BAD_REQUEST),
        }
    }
}

pub fn request<T: DeserializeOwned + Send + 'static>() -> impl_Filter!(T => Clone + (fmt::Debug)) {
    async fn handler(option: Option<Mime>, value: Option<&HeaderValue>) -> crate::Result<()> {
        match option {
            Some(mime)
                if mime.type_() == mime::APPLICATION
                    && mime.subtype() == mime::WWW_FORM_URLENCODED =>
            {
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
            serde_urlencoded::from_reader(readable)
                .map_err(|error| Error::Deserializing(error).into())
        })
}
