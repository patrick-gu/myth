use std::net::SocketAddr;

use myth::{header::HeaderValue, method::Method, uri::Uri, Filter};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("myth=trace".parse().unwrap()),
        )
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NEW)
        .init();

    async fn handler(
        uri: &Uri,
        remote_addr: SocketAddr,
        method: &Method,
        cookie: Option<&HeaderValue>,
    ) -> myth::Result<String> {
        Ok(format!(
            "{} {:?}\nRemote address: {}\nCookie: {:?}\n",
            method,
            uri.path(),
            remote_addr,
            cookie
        ))
    }

    let filter = myth::uri::uri()
        .and(myth::remote_addr())
        .and(myth::method::method())
        .and(myth::header::value_optional("cookie"))
        .handle(handler);

    myth::serve(filter).bind(([127, 0, 0, 1], 8080)).run().await;
}
