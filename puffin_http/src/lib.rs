//! `puffin_server` is a library for streaming `puffin` profiler data over http.
pub const PROTOCOL_VERSION: u16 = 1;
pub const DEFAULT_PORT: u16 = 8585;

pub mod client;
mod server;

pub use client::start_client;
pub use server::PuffinServer;
