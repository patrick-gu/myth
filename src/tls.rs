use std::{
    error::Error as StdError,
    fmt,
    fs::File,
    io::{self, BufRead, BufReader},
    net::SocketAddr,
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::{ready, FutureExt};
use hyper::server::accept::Accept;
use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::rustls::{self, Certificate, PrivateKey};

use crate::service::{Incoming, RequestStream};

/// A configuration for [Rustls](rustls) TLS, to be used with
/// [`Server::with_tls()`](crate::Server::with_tls).
///
/// By default, this uses ALPN protocols for `h2` and `http/1.1`.
///
/// # Custom Configuration
///
/// This can be created from a [`rustls::ServerConfig`] for more fine-grained configuration.
///
/// ```
/// # use myth::TlsConfig;
/// # use tokio_rustls::rustls;
/// # fn config() -> Result<(), rustls::Error> {
/// let cert = rustls::Certificate(b"CERTIFICATE_GOES_HERE".to_vec());
/// let key = rustls::PrivateKey(b"PRIVATE_KEY_GOES_HERE".to_vec());
///
/// let config = rustls::ServerConfig::builder()
///     .with_safe_defaults()
///     .with_no_client_auth()
///     .with_single_cert(vec![cert], key)?;
///
/// let config = TlsConfig::from(config);
/// # Ok(())
/// # }
/// # assert!(config().is_err());
/// ```
#[cfg_attr(myth_docs, doc(cfg(feature = "tls")))]
pub struct TlsConfig {
    pub(crate) config: rustls::ServerConfig,
}

impl TlsConfig {
    /// Creates a new TLS config using the provided certificate chain, private key,
    /// and ALPN protocols.
    ///
    /// # Errors
    ///
    /// Returns an error if the provided private key was invalid.
    fn try_new_with_alpn(
        cert_chain: Vec<Certificate>,
        key: PrivateKey,
        alpn_protocols: Vec<Vec<u8>>,
    ) -> Result<Self, rustls::Error> {
        let mut config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)?;
        config.alpn_protocols = alpn_protocols;
        Ok(Self { config })
    }

    /// Creates a new TLS config using the provided certificate chain and private key.
    ///
    /// # Errors
    ///
    /// Returns an error if the provided private key was invalid
    fn try_new(cert_chain: Vec<Certificate>, key: PrivateKey) -> Result<Self, rustls::Error> {
        Self::try_new_with_alpn(cert_chain, key, vec![b"h2".to_vec(), b"http/1.1".to_vec()])
    }

    /// Creates a new TLS config using the provided certificate chain and private key.
    ///
    /// # Panics
    ///
    /// Panics if the provided private key was invalid.
    pub fn new(cert_chain: Vec<Certificate>, key: PrivateKey) -> Self {
        Self::try_new(cert_chain, key)
            .unwrap_or_else(|error| panic!("invalid private key: {}", error))
    }

    /// Creates a new TLS config by reading a certificate chain and a PKCS8 or RSA private key
    /// from the provided buffers.
    ///
    /// # Panics
    ///
    /// Panics upon failure to read a valid certificate chain or private key.
    pub fn read(cert_chain_read: &mut dyn BufRead, key_read: &mut dyn BufRead) -> Self {
        fn read_cert_chain(cert_chain_read: &mut dyn BufRead) -> Vec<Certificate> {
            rustls_pemfile::certs(cert_chain_read)
                .unwrap_or_else(|error| panic!("error reading cert chain: {}", error))
                .into_iter()
                .map(Certificate)
                .collect()
        }

        fn read_key(key_read: &mut dyn BufRead) -> PrivateKey {
            let item = rustls_pemfile::read_one(key_read)
                .unwrap_or_else(|error| panic!("error reading private key: {}", error))
                .expect("no private key found");
            PrivateKey(match item {
                rustls_pemfile::Item::PKCS8Key(key) | rustls_pemfile::Item::RSAKey(key) => key,
                rustls_pemfile::Item::X509Certificate(_) => {
                    panic!("expected a PKCS8 or RSA private key, instead found an x509 certificate")
                }
            })
        }

        let cert_chain = read_cert_chain(cert_chain_read);
        let key = read_key(key_read);
        Self::new(cert_chain, key)
    }

    /// Creates a new TLS config by reading a certificate chain and a PKCS8 or RSA private key
    /// from the specified files.
    ///
    /// # Panics
    ///
    /// Panics upon failure to read a valid certificate chain or private key.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use myth::TlsConfig;
    /// let config = TlsConfig::read_file("/path/to/certificate.pem", "/path/to/private/key.pem");
    /// ```
    pub fn read_file(cert_chain_path: impl AsRef<Path>, key_path: impl AsRef<Path>) -> Self {
        let mut cert_chain_read = BufReader::new(
            File::open(cert_chain_path)
                .unwrap_or_else(|error| panic!("failed to open cert chain path: {}", error)),
        );
        let mut key_read = BufReader::new(
            File::open(key_path)
                .unwrap_or_else(|error| panic!("failed to open private key path: {}", error)),
        );
        Self::read(&mut cert_chain_read, &mut key_read)
    }
}

impl From<rustls::ServerConfig> for TlsConfig {
    fn from(config: rustls::ServerConfig) -> Self {
        Self { config }
    }
}

pin_project! {
    pub struct TlsAcceptor<I> {
        pub(crate) acceptor: tokio_rustls::TlsAcceptor,
        #[pin]
        pub(crate) incoming: I,
    }
}

impl<I> Incoming for TlsAcceptor<I>
where
    I: Incoming,
    I::Conn: RequestStream,
    I::Error: StdError + Send + Sync + 'static,
{
    fn local_addr(&self) -> SocketAddr {
        self.incoming.local_addr()
    }
}

impl<I> Accept for TlsAcceptor<I>
where
    I: Incoming,
    I::Conn: RequestStream,
    I::Error: StdError + Send + Sync + 'static,
{
    type Conn = TlsStream<I::Conn>;

    type Error = I::Error;

    fn poll_accept(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        self.as_mut().project().incoming.poll_accept(cx).map(|opt| {
            opt.map(|res| {
                res.map(|request_stream| {
                    let remote_addr = request_stream.remote_addr();
                    TlsStream {
                        state: TlsStreamState::Handshaking(self.acceptor.accept(request_stream)),
                        remote_addr,
                    }
                })
            })
        })
    }
}

#[derive(Debug)]
pub struct TlsStream<S> {
    pub(crate) state: TlsStreamState<S>,
    pub(crate) remote_addr: SocketAddr,
}

pub(crate) enum TlsStreamState<S> {
    Handshaking(tokio_rustls::Accept<S>),
    Streaming(tokio_rustls::server::TlsStream<S>),
}

impl<S> fmt::Debug for TlsStreamState<S>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Handshaking(_) => f.debug_tuple("Handshaking").field(&"_").finish(),
            Self::Streaming(inner) => f.debug_tuple("Streaming").field(&inner).finish(),
        }
    }
}

impl<S> RequestStream for TlsStream<S>
where
    S: RequestStream,
{
    fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }
}

impl<S> TlsStream<S>
where
    S: RequestStream,
{
    fn poll_read_write<
        F: FnOnce(Pin<&mut tokio_rustls::server::TlsStream<S>>, &mut Context) -> Poll<io::Result<R>>,
        R,
    >(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        f: F,
    ) -> Poll<io::Result<R>> {
        match &mut self.state {
            TlsStreamState::Handshaking(future) => match ready!(future.poll_unpin(cx)) {
                Ok(stream) => {
                    self.state = TlsStreamState::Streaming(stream);
                    self.poll_read_write(cx, f)
                }
                Err(error) => Poll::Ready(Err(error)),
            },
            TlsStreamState::Streaming(stream) => f(Pin::new(stream), cx),
        }
    }

    fn poll_flush_shutdown(
        mut self: Pin<&mut Self>,
        f: impl FnOnce(Pin<&mut tokio_rustls::server::TlsStream<S>>) -> Poll<io::Result<()>>,
    ) -> Poll<io::Result<()>> {
        match &mut self.state {
            TlsStreamState::Handshaking(_) => Poll::Ready(Ok(())),
            TlsStreamState::Streaming(stream) => f(Pin::new(stream)),
        }
    }
}

impl<S> AsyncRead for TlsStream<S>
where
    S: RequestStream,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_read_write(cx, |pin, cx| pin.poll_read(cx, buf))
    }
}

impl<S> AsyncWrite for TlsStream<S>
where
    S: RequestStream,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_read_write(cx, |pin, cx| pin.poll_write(cx, buf))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush_shutdown(|pin| pin.poll_flush(cx))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush_shutdown(|pin| pin.poll_shutdown(cx))
    }
}
