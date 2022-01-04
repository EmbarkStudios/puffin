//! Remote puffin viewer, connecting to a [`puffin_http::PuffinServer`].

use puffin::FrameView;
use puffin_viewer::{PuffinViewer, Source};

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
    file: Option<String>,
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

    puffin::set_scopes_on(true); // so we can profile ourselves

    let app = if let Some(file) = opt.file {
        let path = std::path::PathBuf::from(file);
        match FrameView::load_path(&path) {
            Ok(frame_view) => PuffinViewer::new(Source::FilePath(path, frame_view)),
            Err(err) => {
                log::error!("Failed to load {:?}: {}", path.display(), err);
                std::process::exit(1);
            }
        }
    } else {
        PuffinViewer::new(Source::Http(puffin_http::Client::new(opt.url)))
    };

    let options = eframe::NativeOptions {
        drag_and_drop_support: true,
        ..Default::default()
    };
    eframe::run_native(Box::new(app), options);
}
