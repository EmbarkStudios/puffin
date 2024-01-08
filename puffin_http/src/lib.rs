//! `puffin_server` is a library for streaming `puffin` profiler data over TCP.
//!
//! # How to use
//! Add a `puffin_http` `Server` to the profiled application.
//! When the server is started, [`puffin_viewer`](https://crates.io/crates/puffin_viewer) application can connect to it and display profiling information.
//!
//! ```
//! let server_addr = format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT);
//! let _puffin_server = puffin_http::Server::new(&server_addr).unwrap();
//! eprintln!("Serving demo profile data on {server_addr}. Run `puffin_viewer` to view it.");
//! puffin::set_scopes_on(true);
//! ```

/// Bumped on protocol breakage.
pub const PROTOCOL_VERSION: u16 = 2;

/// The default TCP port used.
pub const DEFAULT_PORT: u16 = 8585;

mod client;

#[cfg(not(target_arch = "wasm32"))]
mod server;

pub use client::Client;

#[cfg(not(target_arch = "wasm32"))]
pub use server::Server;
