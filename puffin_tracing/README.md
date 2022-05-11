# puffin tracing

[![Embark](https://img.shields.io/badge/embark-open%20source-blueviolet.svg)](https://embark.dev)
[![Embark](https://img.shields.io/badge/discord-ark-%237289da.svg?logo=discord)](https://discord.gg/dAuKfZS)
[![Crates.io](https://img.shields.io/crates/v/puffin_tracing.svg)](https://crates.io/crates/puffin_viewer)

Use [`puffin_tracing`](https://github.com/EmbarkStudios/puffin/tree/main/puffin_tracing) as a layer for tracing-subscriber:

``` rust
use puffin_tracing::PuffinLayer;
use tracing::info_span;
use tracing_subscriber::{layer::SubscriberExt, Registry};

fn main() {
    let subscriber = Registry::default().with(PuffinLayer::new());
    tracing::subscriber::set_global_default(subscriber).unwrap();

    puffin::set_scopes_on(true);

    // ...
}

fn my_function() {
    let _span = info_span!("My Function");
}
```

See the [`tracing.rs`](examples/tracing.rs) example for how to use it with `tracing` and `eframe`.

To try it out, run `cargo run --release --example tracing`
