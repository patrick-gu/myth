//! HTTP request version

use std::fmt;

pub use hyper::Version;

use crate::{filter::ready::ready_filter, impl_Filter, outcome::Outcome};

/// Creates a [`Filter`](crate::Filter) that extracts the HTTP [Version]
pub fn version() -> impl_Filter!(Version => Copy + (fmt::Debug)) {
    ready_filter(|request, _| Outcome::Success((request.version,)))
}

#[cfg(test)]
mod tests {
    use super::{version, Version};
    use crate::test;

    #[tokio::test]
    async fn http_11() {
        test::get()
            .version(Version::HTTP_11)
            .success(&version(), |version| {
                assert_eq!(version, Version::HTTP_11);
            })
            .await;
    }

    #[tokio::test]
    async fn http_2() {
        test::post()
            .version(Version::HTTP_2)
            .success(&version(), |version| {
                assert_eq!(version, Version::HTTP_2);
            })
            .await;
    }
}
