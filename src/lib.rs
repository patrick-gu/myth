#![forbid(unsafe_code)]
#![cfg_attr(myth_docs, feature(doc_cfg, doc_notable_trait))]

//! # Myth
//!
//! A web framework.
//!
//! ## Modularity
//!
//! Myth is heavily inspired by [warp](https://docs.rs/warp), in that it uses a system of
//! [`Filter`]s to handle requests.
//!
//! A [`Filter`] optionally takes some input and either produces a successful output, gives an
//! [error](self::errors::FilterError), or [forwards](Forwarding).
//!
//! A simple filter may look like this:
//!
//! ```
//! use myth::Filter;
//!
//! let filter = myth::any()
//!     .handle(|| async {
//!         Ok("Hello World!")
//!     });
//! ```
//!
//! We can run this basic [`Filter`] as a server:
//!
//! ```no_run
//! # use myth::Filter;
//! # #[tokio::main] async fn main() {
//! # let filter = myth::any()
//! #     .handle(|| async {
//! #         Ok("Hello World!")
//! #     });
//! // Run our server on port 8080
//! myth::serve(filter).bind(([127, 0, 0, 1], 8080)).run().await;
//! # }
//! ```
//!
//! The [`any()`] filter takes an empty input and always produces an empty, successful output.
//! The [`handle`](Filter::handle) method takes a [`Filter`]'s output and attaches a handler
//! function, that returns a <code>[Result]\<Output></code>.
//!
//! The input and output of a [`Filter`] are represented as the tuples [`FilterBase::Input`] and
//! [`FilterBase::Success`]. Using a [base trait](FilterBase) with a lifetime `'f` means that we
//! can express data borrowed from other [`Filter`]s, such as `&'f str`.

#[macro_use]
mod macros;

mod addr;
mod basic;
pub mod body;
pub mod cache;
pub mod errors;
mod filter;
pub mod form;
mod forward;
pub mod generics;
pub mod header;
#[cfg(feature = "json")]
#[cfg_attr(myth_docs, doc(cfg(feature = "json")))]
pub mod json;
pub mod method;
mod outcome;
pub mod path;
pub mod query;
mod request;
mod response;
pub mod security;
mod server;
pub mod service;
pub mod test;
#[cfg(feature = "tls")]
mod tls;
mod traits;
pub mod uri;
mod util;
pub mod version;

pub use hyper::{body::Bytes, Body, StatusCode};

#[cfg(feature = "tls")]
pub use self::tls::TlsConfig;
pub use self::{
    addr::remote_addr,
    basic::{any, borrowing, cloning, never},
    errors::Result,
    filter::{DynamicFilter, Filter, FilterBase},
    forward::Forwarding,
    response::{html, Responder, Response},
    server::{serve, Server},
};
