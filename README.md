<div align="center">

# `🐦 puffin`

**The friendly little instrumentation profiler for Rust**

![Puffin photo by Richard Bartz](puffin.jpg)

[(puffin photo by Richard Bartz)](https://en.wikipedia.org/wiki/File:Papageitaucher_Fratercula_arctica.jpg)

[![Embark](https://img.shields.io/badge/embark-open%20source-blueviolet.svg)](https://embark.dev)
[![Embark](https://img.shields.io/badge/discord-ark-%237289da.svg?logo=discord)](https://discord.gg/dAuKfZS)
[![Crates.io](https://img.shields.io/crates/v/puffin.svg)](https://crates.io/crates/puffin)
[![Docs](https://docs.rs/puffin/badge.svg)](https://docs.rs/puffin)
[![dependency status](https://deps.rs/repo/github/EmbarkStudios/puffin/status.svg)](https://deps.rs/repo/github/EmbarkStudios/puffin)
[![Build Status](https://github.com/EmbarkStudios/puffin/workflows/CI/badge.svg)](https://github.com/EmbarkStudios/puffin/actions?workflow=CI)

</div>

## How to use

``` rust
fn my_function() {
    puffin::profile_function!();
    ...
    if ... {
        puffin::profile_scope!("load_image", image_name);
        ...
    }
}
```

The Puffin macros write data to a thread-local data stream. When the outermost scope of a thread is closed, the data stream is sent to a global profiler collector. The scopes are pretty light-weight, costing around 100-200 nanoseconds.

You have to turn on the profiler before it captures any data with a call to `puffin::set_scopes_on(true);`. When the profiler is off the profiler scope macros only has an overhead of 1-2 ns (and some stack space);

Once per frame you need to call `puffin::GlobalProfiler::lock().new_frame();`.

## UI

To view the profile data in-game you can use [`puffin_egui`](https://github.com/EmbarkStudios/puffin/tree/main/puffin_egui).

![Puffin Flamegraph using puffin_egui](puffin_egui.gif)

## Remote profiling

You can use [`puffin_http`](https://github.com/EmbarkStudios/puffin/tree/main/puffin_http) to send profile events over TCP to [`puffin_viewer`](https://github.com/EmbarkStudios/puffin/tree/main/puffin_viewer).

## Other

Also check out the crate [`profiling`](https://crates.io/crates/profiling) which provides a unifying layer of abstraction on top of `puffin` and other profiling crates.

## Contributing

[![Contributor Covenant](https://img.shields.io/badge/contributor%20covenant-v1.4-ff69b4.svg)](CODE_OF_CONDUCT.md)

We welcome community contributions to this project.

Please read our [Contributor Guide](CONTRIBUTING.md) for more information on how to get started.

## License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
