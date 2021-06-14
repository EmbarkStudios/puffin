//! Remote puffin viewer, connecting to a [`puffin_http::PuffinServer`].

use eframe::{egui, epi};

/// puffin remote profile viewer.
///
/// Connect to a puffin server and show its profile data.
#[derive(argh::FromArgs)]
struct Arguments {
    /// which server to connect to, e.g. `localhost:8585`.
    #[argh(option, default = "default_url()")]
    url: String,
}

fn default_url() -> String {
    format!("localhost:{}", puffin_http::DEFAULT_PORT)
}

fn main() {
    let opt: Arguments = argh::from_env();

    stderrlog::new()
        .module(module_path!())
        .verbosity(2) // 2 == info
        .init()
        .unwrap();

    puffin::set_scopes_on(true); // quiet warning in `puffin_egui`.
    puffin_http::start_client(&opt.url).unwrap();

    let app = PuffinViewer {};
    let options = Default::default();
    eframe::run_native(Box::new(app), options);
}

pub struct PuffinViewer {}

impl epi::App for PuffinViewer {
    fn name(&self) -> &str {
        "puffin http client viewer"
    }

    fn update(&mut self, ctx: &egui::CtxRef, _frame: &mut epi::Frame<'_>) {
        egui::CentralPanel::default().show(ctx, puffin_egui::profiler_ui);
        ctx.request_repaint(); // we get new profiling data all the time, so let's constantly refresh
    }
}
