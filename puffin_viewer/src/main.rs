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
        file: Option<String>,
    }

    fn default_url() -> String {
        format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT)
    }

    use puffin::FrameView;
    use puffin_viewer::{PuffinViewer, Source};

    let opt: Arguments = argh::from_env();

    puffin::set_scopes_on(true); // so we can profile ourselves

    let source = if let Some(file) = opt.file {
        let path = std::path::PathBuf::from(file);
        match FrameView::load_path(&path) {
            Ok(frame_view) => Source::FilePath(path, frame_view),
            Err(err) => {
                log::error!("Failed to load {:?}: {}", path.display(), err);
                std::process::exit(1);
            }
        }
    } else {
        Source::Http(puffin_http::Client::new(opt.url))
    };

    let native_options = eframe::NativeOptions {
        app_id: Some("puffin_viewer".to_owned()),
        icon_data: Some(
            eframe::IconData::try_from_png_bytes(include_bytes!("../icon.png")).unwrap(),
        ),
        drag_and_drop_support: true,
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
