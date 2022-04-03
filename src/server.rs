//! Provides [`Server`], which is used to actually run a
//! [`Filter`] as an HTTP server

use std::{
    convert::Infallible,
    error::Error as StdError,
    fmt,
    future::{ready, Future},
    net::SocketAddr,
    sync::Arc,
};

use futures_util::FutureExt;
use hyper::{
    server::{conn::AddrIncoming, Server as HyperServer},
    service::make_service_fn,
    Error as HyperError,
};
use tracing::Instrument;

use crate::{
    service::{handle_requests, Incoming, RequestStream},
    Filter, FilterBase, Responder,
};

macro_rules! make_service {
    ($filter:expr) => {{
        let filter = Arc::new($filter);
        make_service_fn(move |stream| {
            let filter = Arc::clone(&filter);
            let remote_addr = RequestStream::remote_addr(stream);
            let request_service = handle_requests(filter, remote_addr);
            ready(Ok::<_, Infallible>(request_service))
        })
    }};
}

/// A server that uses a [`Filter`] to handle requests.
///
/// # Example
/// ```no_run
/// use myth::{Filter, Server};
///
/// # #[tokio::main] async fn main() {
/// // Create a `Filter`.
/// let filter = myth::any().handle(|| async { Ok("Hello world!") });
///
/// // Create a new `Server`.
/// let server = Server::new(filter);
///
/// server
///     // Bind to public TCP port 80.
///     .bind(([0, 0, 0, 0], 80))
///     // Run the server forever.
///     .run()
///     // Await the `Future`
///     .await;
/// # }
/// ```
#[derive(Debug)]
pub struct Server<I, F> {
    incoming: I,
    filter: F,
}

impl<I, F, R> Server<I, F>
where
    I: Incoming,
    I::Conn: RequestStream,
    I::Error: StdError + Send + Sync + 'static,
    F: Filter + for<'f> FilterBase<'f, Input = (), Success = (R,)>,
    R: Responder + 'static,
{
    /// Runs the server until either a Ctrl-C signal is received or an error occurs.
    ///
    /// # Panics
    ///
    /// Panics if an error occured while preparing or running the server.
    pub async fn run(self) {
        if let Err(error) = self.try_run().await {
            panic!("{}", error);
        }
    }

    /// Attempts to run the server until either a Ctrl-C signal is received or an error occurs.
    ///
    /// # Errors
    ///
    /// Returns an error if either the Ctrl-C signal failed to install, or an error
    /// occurred while running the server.
    pub async fn try_run(self) -> Result {
        let signal = tokio::signal::ctrl_c().map(|result| {
            if let Err(error) = result {
                tracing::error!("Failed to install ctrl-c shutdown signal: {}", error);
            }
        });
        self.run_with_graceful_shutdown(signal).await
    }

    pub async fn run_with_graceful_shutdown(self, signal: impl Future<Output = ()>) -> Result {
        let addr = &*self.local_addr().to_string();
        HyperServer::builder(self.incoming)
            .serve(make_service!(self.filter))
            .with_graceful_shutdown(signal)
            .instrument(tracing::info_span!("Running server", addr))
            .await
            .map_err(Error::Running)
    }

    pub async fn run_without_graceful_shutdown(self) -> Result {
        let addr = &*self.local_addr().to_string();
        HyperServer::builder(self.incoming)
            .serve(make_service!(self.filter))
            .instrument(tracing::info_span!(
                "Running server without graceful shutdown",
                addr
            ))
            .await
            .map_err(Error::Running)
    }

    /// Returns the local address that this server is bound to.
    ///
    /// # Example
    ///
    /// ```
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    /// # use myth::{Filter, Server};
    ///
    /// # #[tokio::main] async fn main() {
    /// let filter = myth::any().handle(|| async { Ok("Hello world!") });
    /// let server = Server::new(filter).bind(([127, 0, 0, 1], 8080));
    /// assert_eq!(server.local_addr(), SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)));
    /// # }
    /// ```
    pub fn local_addr(&self) -> SocketAddr {
        self.incoming.local_addr()
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Running(HyperError),
    Bind(HyperError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running(error) => write!(f, "error while running server: {}", error),
            Self::Bind(error) => write!(f, "error binding server: {}", error),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(match self {
            Self::Running(error) | Self::Bind(error) => error,
        })
    }
}

impl From<HyperError> for Error {
    fn from(error: HyperError) -> Self {
        Self::Running(error)
    }
}

pub type Result<T = ()> = std::result::Result<T, Error>;

impl<F, R> Server<(), F>
where
    F: Filter + for<'f> FilterBase<'f, Input = (), Success = (R,)>,
    R: Responder + 'static,
{
    pub fn new(filter: F) -> Self {
        Self {
            incoming: (),
            filter,
        }
    }

    pub fn bind(self, addr: impl Into<SocketAddr>) -> Server<AddrIncoming, F> {
        match self.try_bind(addr) {
            Ok(server) => server,
            Err(error) => {
                panic!("{}", error);
            }
        }
    }

    pub fn try_bind(self, addr: impl Into<SocketAddr>) -> Result<Server<AddrIncoming, F>> {
        let addr = addr.into();
        AddrIncoming::bind(&addr)
            .map(|incoming| {
                tracing::trace!("Bound server to http://{}", addr);
                Server {
                    incoming,
                    filter: self.filter,
                }
            })
            .map_err(Error::Bind)
    }
}

#[cfg(feature = "tls")]
impl<F, R> Server<AddrIncoming, F>
where
    F: Filter + for<'f> FilterBase<'f, Input = (), Success = (R,)>,
    R: Responder + 'static,
{
    #[cfg_attr(myth_docs, doc(cfg(feature = "tls")))]
    pub fn with_tls(
        self,
        config: crate::TlsConfig,
    ) -> Server<crate::tls::TlsAcceptor<AddrIncoming>, F> {
        Server {
            incoming: crate::tls::TlsAcceptor {
                acceptor: Arc::new(config.config).into(),
                incoming: self.incoming,
            },
            filter: self.filter,
        }
    }
}

/// Creates a new [`Server`] from a [`Filter`].
///
/// This is equivalent to calling [`Server::new()`].
pub fn serve<F, R>(filter: F) -> Server<(), F>
where
    F: Filter + for<'f> FilterBase<'f, Input = (), Success = (R,)>,
    R: Responder + 'static,
{
    Server::new(filter)
}
