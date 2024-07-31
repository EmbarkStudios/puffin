# puffin viewer

[![Embark](https://img.shields.io/badge/embark-open%20source-blueviolet.svg)](https://embark.dev)
[![Embark](https://img.shields.io/badge/discord-ark-%237289da.svg?logo=discord)](https://discord.gg/dAuKfZS)
[![Crates.io](https://img.shields.io/crates/v/puffin_viewer.svg)](https://crates.io/crates/puffin_viewer)

Use [`puffin_http`](https://github.com/EmbarkStudios/puffin/tree/main/puffin_http) to publish puffin events over TCP. Then connect to it with `puffin_viewer`:

``` sh
cargo install puffin_viewer --locked
puffin_viewer --url 127.0.0.1:8585
```

### On Linux

On Linux gtk3 sources are required for file dialogs. You may install them on Ubuntu using the following command:
```sh
sudo apt install libgtk-3-dev
```

The puffin icon is based on [a photo by Richard Bartz](https://en.wikipedia.org/wiki/File:Papageitaucher_Fratercula_arctica.jpg).
