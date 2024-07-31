# Show [`puffin`](https://github.com/EmbarkStudios/puffin/) profiler flamegraph in-game using [`egui`](https://github.com/emilk/egui)

[![Embark](https://img.shields.io/badge/embark-open%20source-blueviolet.svg)](https://embark.dev)
[![Embark](https://img.shields.io/badge/discord-ark-%237289da.svg?logo=discord)](https://discord.gg/dAuKfZS)
[![Crates.io](https://img.shields.io/crates/v/puffin_egui.svg)](https://crates.io/crates/puffin_egui)
[![Docs](https://docs.rs/puffin_egui/badge.svg)](https://docs.rs/puffin_egui)

[`puffin`](https://github.com/EmbarkStudios/puffin/) is an instrumentation profiler where you opt-in to profile parts of your code:

``` rust
fn my_function() {
    puffin::profile_function!();
    if ... {
        puffin::profile_scope!("load_image", image_name);
        ...
    }
}
```

`puffin_egui` allows you to inspect the resulting profile data using [`egui`](https://github.com/emilk/egui) with only one line of code:

``` rust
puffin_egui::profiler_window(egui_ctx);
```

<img src="../puffin_egui.gif">

See the [`examples/`](examples/) folder for how to use it with [`eframe`](https://docs.rs/eframe).

To try it out, run `cargo run --release --example eframe`
