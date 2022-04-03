use std::{convert::TryInto, fmt, time::Duration};

use crate::{
    any,
    header::{self, HeaderValue},
    impl_Filter, Filter, Responder, Response,
};

pub fn hsts(config: Config) -> impl_Filter!(Response, Response => Clone + (fmt::Debug)) {
    let max_age = config.max_age.as_secs();
    let string = match config.directives {
        Directives::IncludeSubDomains => format!("max-age={}; includeSubDomains", max_age),
        Directives::Preload => {
            if config.max_age <= Duration::from_secs(31_536_000) {
                tracing::warn!("HSTS Preload was specified but the max-age was less than one year");
            }
            format!("max-age={}; includeSubDomains; preload", max_age)
        }
        Directives::None => format!("max-age={}", max_age),
    };
    let header_value: HeaderValue = string
        .try_into()
        .expect("HSTS header value should be valid");

    any()
        .receive::<(Response,)>()
        .handle(move |mut response: Response| {
            response =
                response.with_header(header::STRICT_TRANSPORT_SECURITY, header_value.clone());
            async move { Ok(response) }
        })
}

#[derive(Copy, Clone, Debug)]
pub struct Config {
    /// The time that the browser should remember the HSTS directive.
    ///
    /// Only the seconds are used for the header value.
    pub max_age: Duration,

    /// Extra directives to use.
    pub directives: Directives,
}

/// Extra HSTS directives.
#[derive(Copy, Clone, Debug)]
pub enum Directives {
    /// No extra directives.
    None,

    /// Adds `includeSubDomains`.
    IncludeSubDomains,

    /// Adds `includeSubDomains` and `preload`.
    ///
    /// See <https://hstspreload.org/>
    Preload,
}
