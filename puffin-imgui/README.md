# Puffin ImGui Flamegraph

[![Embark](https://img.shields.io/badge/embark-open%20source-blueviolet.svg)](https://embark.dev)
[![Embark](https://img.shields.io/badge/discord-ark-%237289da.svg?logo=discord)](https://discord.gg/dAuKfZS)
[![Crates.io](https://img.shields.io/crates/v/puffin-imgui.svg)](https://crates.io/crates/puffin-imgui)
[![Docs](https://docs.rs/puffin-imgui/badge.svg)](https://docs.rs/puffin-imgui)

This crate provides a flamegraph view of the data collected by the Puffin profiler.

![Example view](flamegraph.png)

``` rust
fn main() {
    puffin::set_scopes_on(true); // you may want to control this with a flag
    let mut puffin_ui = puffin_imgui::ProfilerUi::default();

    // game loop
    loop {
        puffin::GlobalProfiler::lock().new_frame();

        {
            puffin::profile_scope!("slow_code");
            slow_code();
        }

        puffin_ui.window(ui);
    }
}
```
