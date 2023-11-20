//! Remote puffin viewer, connecting to a [`puffin_http::PuffinServer`].

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    /// puffin profile viewer.
    ///
    /// Can either connect remotely to a puffin server
    /// or open a .puffin recording file.
    #[derive(argh::FromArgs)]
    struct Arguments {
        /// which server to connect to, e.g. `127.0.0.1:8585`.
        #[argh(option, default = "default_url()")]
        url: String,

        /// what .puffin file to open, e.g. `my/recording.puffin`.
        #[argh(positional)]
        file: Option<PathBuf>,
    }

    fn default_url() -> String {
        format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT)
    }

    use std::path::PathBuf;

    use puffin::FrameView;
    use puffin_viewer::{PuffinViewer, Source};

    let opt: Arguments = argh::from_env();

    puffin::set_scopes_on(true); // so we can profile ourselves

    let source = if let Some(path) = opt.file {
        let mut file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(err) => {
                log::error!("Failed to open {:?}: {err:#}", path.display());
                std::process::exit(1);
            }
        };

        match FrameView::read(&mut file) {
            Ok(frame_view) => Source::FilePath(path, frame_view),
            Err(err) => {
                log::error!("Failed to load {:?}: {err:#}", path.display());
                std::process::exit(1);
            }
        }
    } else {
        Source::Http(puffin_http::Client::new(opt.url))
    };

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../icon.png")).unwrap();
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_app_id("puffin_viewer")
            .with_drag_and_drop(true)
            .with_window_icon(icon),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "puffin viewer",
        native_options,
        Box::new(|_cc| Box::new(PuffinViewer::new(source))),
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {}
