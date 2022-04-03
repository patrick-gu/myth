use std::{convert::Infallible, error::Error as StdError, future::Future, net::SocketAddr};
use futures_util::Stream;
use hyper::{
    server::{
        accept::Accept,
        conn::{AddrIncoming, AddrStream},
    },
    service::{service_fn, Service},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::Instrument;

use crate::{
    outcome::Outcome, request, request::HyperRequest, Filter, FilterBase, Responder, Response,
};

/// An incoming stream of connections that can be used by a [`Server`](crate::Server).
///
/// This should be able to [`Accept`] incoming requests and produce a [`RequestStream`].
pub trait Incoming: Accept
where
    <Self as Accept>::Conn: RequestStream,
    <Self as Accept>::Error: StdError + Send + Sync + 'static,
{
    /// Returns the address that this incoming stream is bound to.
    fn local_addr(&self) -> SocketAddr;
}

/// A stream of requests produced by an [`Incoming`] that can be read by a [`Server`](crate::Server).
pub trait RequestStream: AsyncRead + AsyncWrite + Unpin + Send + 'static {
    /// Returns the remote address of the client.
    fn remote_addr(&self) -> SocketAddr;
}

impl Incoming for AddrIncoming {
    fn local_addr(&self) -> SocketAddr {
        Self::local_addr(self)
    }
}

impl RequestStream for AddrStream {
    fn remote_addr(&self) -> SocketAddr {
        Self::remote_addr(self)
    }
}

/// Creates a [`Service`] to handle requests, given a [`Filter`] and the [`remote_addr`](crate::remote_addr)
/// of the request.
///
/// # Example
/// ```no_run
/// use std::{net::SocketAddr, sync::Arc};
///
/// use myth::Filter;
/// use tower::make;
///
/// # #[tokio::main] async fn main() -> hyper::Result<()> {
/// let filter = myth::any().handle(|| async { Ok("Hello from a service!") });
/// // Wrap our `filter` with an `Arc`. This is necessary for `Filter`s that cannot be cloned.
/// let filter = Arc::new(filter);
/// // Create a service with a placeholder `remote_addr` of `0.0.0.0:0`
/// let service = myth::service::handle_requests(filter, SocketAddr::from(([0, 0, 0, 0], 0)));
/// // Run the service on a `hyper` server.
/// hyper::Server::bind(&SocketAddr::from(([127, 0, 0, 1], 3000)))
///     .serve(make::Shared::new(service))
///     .await?;
/// # Ok(()) }
/// ```
pub fn handle_requests<F, R>(
    filter_wrap: impl AsRef<F> + Clone + Send + 'static,
    remote_addr: SocketAddr,
) -> impl Service<
    HyperRequest,
    Response = Response,
    Error = Infallible,
    Future = impl Future<Output = Result<Response, Infallible>> + Send,
> + Clone
       + Send
       + 'static
where
    F: Filter + for<'f> FilterBase<'f, Input = (), Success = (R,)>,
    R: Responder + 'static,
{
    service_fn(move |request: HyperRequest| {
        let filter_wrap = filter_wrap.clone();
        let (request, request_state) = request::from_hyper(request, remote_addr);

        async move {
            let span = tracing::trace_span!(
                "Incoming request",
                method = %request.method,
                uri = %request.uri,
                remote_addr = %request.remote_addr,
            );
            let filter = filter_wrap.as_ref();
            let future = filter.execute(&request, request_state, ()).instrument(span);
            let response = match future.await.outcome {
                Outcome::Success((responder,)) => responder.into_response(),
                Outcome::Error(error) => error.into_response(),
                Outcome::Forward { forwarding, .. } => forwarding.into_response(),
            };
            Ok::<_, Infallible>(response)
        }
    })
}
