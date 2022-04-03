//! Remote [`SocketAddr`]s

use std::{fmt, net::SocketAddr};

use crate::{filter::ready::ready_filter, impl_Filter, outcome::Outcome};

/// Creates a [`Filter`](crate) that extracts the remote [`SocketAddr`] of the client connecting to
/// the server.
pub fn remote_addr() -> impl_Filter!(SocketAddr => Copy + (fmt::Debug)) {
    ready_filter(|request, _| Outcome::Success((request.remote_addr,)))
}

#[cfg(test)]
mod tests {
    use super::remote_addr;
    use crate::test;

    #[tokio::test]
    async fn extract_remote_addr() {
        test::get()
            .remote_addr(([127, 0, 0, 1], 12345))
            .success(&remote_addr(), |addr| {
                assert_eq!(addr, "127.0.0.1:12345".parse().unwrap());
            })
            .await;
    }
}
