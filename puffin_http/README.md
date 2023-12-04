# puffin_http

[![Embark](https://img.shields.io/badge/embark-open%20source-blueviolet.svg)](https://embark.dev)
[![Embark](https://img.shields.io/badge/discord-ark-%237289da.svg?logo=discord)](https://discord.gg/dAuKfZS)
[![Crates.io](https://img.shields.io/crates/v/puffin_http.svg)](https://crates.io/crates/puffin_http)
[![Docs](https://docs.rs/puffin_http/badge.svg)](https://docs.rs/puffin_http)

A HTTP server/client for communicating [`puffin`](https://github.com/EmbarkStudios/puffin) profiling events.

You can view them using [`puffin_viewer`](https://github.com/EmbarkStudios/puffin/tree/main/puffin_viewer).

## How to use
Add a `puffin_http` `Server` to the profiled application
When the server is started, [`puffin_viewer`](https://crates.io/crates/puffin_viewer) application can connect to it and display profiling information.

``` rust
fn main() {
    let server_addr = format!("0.0.0.0:{}", puffin_http::DEFAULT_PORT);
    let _puffin_server = puffin_http::Server::new(&server_addr).unwrap();
    eprintln!("Serving demo profile data on {server_addr}. Run `puffin_viewer` to view it.");
    puffin::set_scopes_on(true);

    // …
}
```

You can checkout the examples/server.rs for a more complete example.
