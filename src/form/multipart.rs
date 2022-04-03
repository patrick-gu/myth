use std::{fmt, io, io::Read, sync::Arc};

use mime::Mime;
use multipart::server::{FieldHeaders, Multipart, MultipartField};

use crate::{
    body,
    errors::FilterError,
    header::{content_type, HeaderValue},
    impl_Filter,
    response::default_response,
    Filter, Response, StatusCode,
};

/// Creates a [`Filter`] that matches `multipart/form-data` requests
pub fn multipart(
) -> impl_Filter!(impl Iterator<Item = io::Result<Part>> + fmt::Debug => Clone + (fmt::Debug)) {
    async fn handler(option: Option<Mime>, value: Option<&HeaderValue>) -> crate::Result<String> {
        if let Some(mime) = option {
            if mime.type_() == mime::MULTIPART && mime.subtype() == mime::FORM_DATA {
                if let Some(boundary) = mime.get_param(mime::BOUNDARY) {
                    return Ok(boundary.as_str().to_owned());
                }
            }
        }
        Err(value
            .cloned()
            .map(Error::WrongContentType)
            .unwrap_or(Error::NoContentType)
            .into())
    }
    content_type()
        .handle(handler)
        .and(
            body::all()
                .recover(|error: body::Error| async move { Err(Error::Reading(error).into()) }),
        )
        .handle(|boundary, readable| async move {
            Ok(Data {
                inner: Multipart::with_body(readable, boundary),
            })
        })
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The [`Content-Type`](crate::header::CONTENT_TYPE) was not present.
    NoContentType,

    /// [`Content-Type`](crate::header::CONTENT_TYPE) was not `multipart/form-data` or not valid.
    ///
    /// This may occur if the boundary was not present.
    WrongContentType(HeaderValue),

    /// An error occured while reading the request body.
    Reading(body::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoContentType => {
                write!(f, "missing multipart/form-data content type")
            }
            Self::WrongContentType(mime) => write!(
                f,
                "expected multipart/form-data content type, instead got {:?}",
                mime
            ),
            Self::Reading(inner) => {
                write!(f, "error while reading body as multipart data: {}", inner)
            }
        }
    }
}

impl FilterError for Error {
    fn into_response(self: Box<Self>) -> Response {
        tracing::debug!("default response for multipart error: {}", self);
        match *self {
            Self::NoContentType | Self::WrongContentType(_) => {
                default_response(StatusCode::UNSUPPORTED_MEDIA_TYPE)
            }
            Self::Reading(_) => default_response(StatusCode::BAD_REQUEST),
        }
    }
}

/// A `multipart/form-data` body.
///
/// Read by using [`Iterator`].
struct Data<R> {
    inner: Multipart<R>,
}

impl<R> fmt::Debug for Data<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Data").finish_non_exhaustive()
    }
}

impl<R> Iterator for Data<R>
where
    R: Read,
{
    type Item = io::Result<Part>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.read_entry() {
            Ok(option) => match option {
                Some(MultipartField {
                    headers:
                        FieldHeaders {
                            name,
                            filename,
                            content_type,
                        },
                    mut data,
                }) => {
                    let mut buf = Vec::new();
                    match data.read_to_end(&mut buf) {
                        Ok(_) => Some(Ok(Part {
                            name,
                            filename,
                            content_type,
                            data: buf,
                        })),
                        Err(err) => Some(Err(err)),
                    }
                }
                None => None,
            },
            Err(err) => Some(Err(err)),
        }
    }
}

/// A section of `multipart/form-data`.
#[derive(Debug)]
pub struct Part {
    pub name: Arc<str>,
    pub filename: Option<String>,
    pub content_type: Option<Mime>,
    pub data: Vec<u8>,
}
