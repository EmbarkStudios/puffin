//! Remote puffin viewer, connecting to a [`puffin_http::PuffinServer`].

use eframe::{egui, epi};

/// puffin remote profile viewer.
///
/// Connect to a puffin server and show its profile data.
#[derive(argh::FromArgs)]
struct Arguments {
    /// which server to connect to, e.g. `127.0.0.1:8585`.
    #[argh(option, default = "default_url()")]
    url: String,
}

fn default_url() -> String {
    format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT)
}

fn main() {
    let opt: Arguments = argh::from_env();

    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .ok();

    puffin::set_scopes_on(true); // quiet warning in `puffin_egui`.
    let client = puffin_http::Client::new(opt.url);

    let app = PuffinViewer { client };
    let options = Default::default();
    eframe::run_native(Box::new(app), options);
}

pub struct PuffinViewer {
    client: puffin_http::Client,
}

impl epi::App for PuffinViewer {
    fn name(&self) -> &str {
        "puffin http client viewer"
    }

    fn update(&mut self, ctx: &egui::CtxRef, _frame: &mut epi::Frame<'_>) {
        egui::TopPanel::top("top_panel").show(ctx, |ui| {
            if self.client.connected() {
                ui.label(format!("Connected to {}", self.client.addr()));
            } else {
                ui.label(format!("Connecting to {}…", self.client.addr()));
            }
        });

        egui::CentralPanel::default().show(ctx, puffin_egui::profiler_ui);
    }
}
