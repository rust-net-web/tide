//! Tide is a minimal and pragmatic Rust web application framework built for
//! rapid development. It comes with a robust set of features that make
//! building async web applications and APIs easier and more fun.
//!
//! # Getting started
//!
//! In order to build a web app in Rust you need an HTTP server, and an async
//! runtime. After running `cargo init` add the following lines to your
//! `Cargo.toml` file:
//!
//! ```toml
//! # Example, use the version numbers you need
//! tide = "0.14.0"
//! async-std = { version = "1.6.0", features = ["attributes"] }
//! serde = { version = "1.0", features = ["derive"] }
//!```
//!
//! # Examples
//!
//! Create an HTTP server that receives a JSON body, validates it, and responds with a
//! confirmation message.
//!
//! ```no_run
//! use tide::Request;
//! use tide::prelude::*;
//!
//! #[derive(Debug, Deserialize)]
//! struct Animal {
//!     name: String,
//!     legs: u8,
//! }
//!
//! #[async_std::main]
//! async fn main() -> tide::Result<()> {
//!     let mut app = tide::new();
//!     app.at("/orders/shoes").post(order_shoes);
//!     app.listen("127.0.0.1:8080").await?;
//!     Ok(())
//! }
//!
//! async fn order_shoes(mut req: Request<()>) -> tide::Result {
//!     let Animal { name, legs } = req.body_json().await?;
//!     Ok(format!("Hello, {}! I've put in an order for {} shoes", name, legs).into())
//! }
//! ````
//!
//! ```sh
//! $ curl localhost:8080/orders/shoes -d '{ "name": "Chashu", "legs": 4 }'
//! Hello, Chashu! I've put in an order for 4 shoes
//!
//! $ curl localhost:8080/orders/shoes -d '{ "name": "Mary Millipede", "legs": 750 }'
//! number too large to fit in target type
//! ```
//! See more examples in the [examples](https://github.com/http-rs/tide/tree/main/examples) directory.

#![cfg_attr(feature = "docs", feature(doc_cfg))]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations, nonstandard_style)]
#![warn(missing_docs, unreachable_pub, future_incompatible, rust_2018_idioms)]
#![allow(clippy::len_without_is_empty)]
#![doc(test(attr(deny(warnings))))]
#![doc(test(attr(allow(unused_extern_crates, unused_variables))))]
#![doc(html_favicon_url = "https://yoshuawuyts.com/assets/http-rs/favicon.ico")]
#![doc(html_logo_url = "https://yoshuawuyts.com/assets/http-rs/logo-rounded.png")]

#[cfg(feature = "cookies")]
mod cookies;
mod endpoint;
mod fs;
mod middleware;
mod redirect;
mod request;
mod response;
mod response_builder;
mod route;
mod router;
mod server;

pub mod convert;
pub mod listener;
pub mod log;
pub mod prelude;
pub mod security;
pub mod sse;
pub mod utils;

#[cfg(feature = "sessions")]
pub mod sessions;

pub use endpoint::Endpoint;
pub use middleware::{Middleware, Next};
pub use redirect::Redirect;
pub use request::Request;
pub use response::Response;
pub use response_builder::ResponseBuilder;
pub use route::Route;
pub use server::Server;

pub use http_types::{self as http, Body, Error, Status, StatusCode};

pub use stopper;

/// Create a new Tide server.
///
/// # Examples
///
/// ```no_run
/// # use async_std::task::block_on;
/// # fn main() -> Result<(), std::io::Error> { block_on(async {
/// #
/// let mut app = tide::new();
/// app.at("/").get(|_| async { Ok("Hello, world!") });
/// app.listen("127.0.0.1:8080").await?;
/// #
/// # Ok(()) }) }
/// ```
#[must_use]
pub fn new() -> server::Server<()> {
    Server::new()
}

/// Create a new Tide server with shared application scoped state.
///
/// Application scoped state is useful for storing items
///
/// # Examples
///
/// ```no_run
/// # use async_std::task::block_on;
/// # fn main() -> Result<(), std::io::Error> { block_on(async {
/// #
/// use tide::Request;
///
/// /// The shared application state.
/// #[derive(Clone)]
/// struct State {
///     name: String,
/// }
///
/// // Define a new instance of the state.
/// let state = State {
///     name: "Nori".to_string()
/// };
///
/// // Initialize the application with state.
/// let mut app = tide::with_state(state);
/// app.at("/").get(|req: Request<State>| async move {
///     Ok(format!("Hello, {}!", &req.state().name))
/// });
/// app.listen("127.0.0.1:8080").await?;
/// #
/// # Ok(()) }) }
/// ```
pub fn with_state<State>(state: State) -> server::Server<State>
where
    State: Clone + Send + Sync + 'static,
{
    Server::with_state(state)
}

/// A specialized Result type for Tide.
pub type Result<T = Response> = std::result::Result<T, Error>;
