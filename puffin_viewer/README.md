# puffin viewer

[![Embark](https://img.shields.io/badge/embark-open%20source-blueviolet.svg)](https://embark.dev)
[![Embark](https://img.shields.io/badge/discord-ark-%237289da.svg?logo=discord)](https://discord.gg/dAuKfZS)
[![Crates.io](https://img.shields.io/crates/v/puffin_viewer.svg)](https://crates.io/crates/puffin_viewer)

Use [`puffin_http`](https://github.com/EmbarkStudios/puffin/tree/main/puffin_http) to publish puffin events over TCP. Then connect to it with `puffin_viewer`:

``` sh
cargo install puffin_viewer
puffin_viewer --url 127.0.0.1:8585
```

The puffin icon is based on [a photo by Richard Bartz](https://en.wikipedia.org/wiki/File:Papageitaucher_Fratercula_arctica.jpg).
