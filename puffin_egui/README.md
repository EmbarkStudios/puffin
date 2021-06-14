# Show [`puffin`](https://github.com/EmbarkStudios/puffin/) profiler flamegraph in-game using [`egui`](https://github.com/emilk/egui)

[![Latest version](https://img.shields.io/crates/v/puffin_egui.svg)](https://crates.io/crates/puffin_egui)
[![Documentation](https://docs.rs/puffin_egui/badge.svg)](https://docs.rs/puffin_egui)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
![MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Apache](https://img.shields.io/badge/license-Apache-blue.svg)

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

`puffin_egui` allows you to inspect the resulting profile data using [`egui`](https://github.com/emilk/egui), and immediate mode GUI library, using only one line of code:

``` rust
puffin_egui::profiler_window(egui_ctx);
```

<img src="../puffin_egui.gif">

See the [`examples/`](examples/) folder for how to use it with [`eframe`](docs.rs/eframe) or [`macroquad`](https://github.com/not-fl3/macroquad).

To try it out, run `cargo run --release --example macroquad`
