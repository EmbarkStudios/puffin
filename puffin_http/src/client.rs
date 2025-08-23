use anyhow::Context as _;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::SeqCst},
};

use puffin::{DataHeader, FrameData, FrameView, ScopeCollection};

enum MessageContent {
    FrameData(FrameData),
    ScopeCollection(ScopeCollection),
}

/// Connect to a [`crate::Server`], reading profile data
/// and feeding it to a [`puffin::FrameView`].
///
/// Will retry connection until it succeeds, and reconnect on failures.
pub struct Client {
    addr: String,
    connected: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    frame_view: Arc<parking_lot::Mutex<FrameView>>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.alive.store(false, SeqCst);
    }
}

impl Client {
    /// Connects to the given http address receives puffin profile data
    /// that is then fed to [`puffin::GlobalProfiler`].
    ///
    /// You can then view the data with
    /// [`puffin_egui`](https://crates.io/crates/puffin_egui).
    ///
    /// ``` no_run
    /// puffin_http::Client::new("127.0.0.1:8585".to_owned());
    /// ```
    pub fn new(addr: String) -> Self {
        let alive = Arc::new(AtomicBool::new(true));
        let connected = Arc::new(AtomicBool::new(false));
        let frame_view = Arc::new(parking_lot::Mutex::new(FrameView::default()));

        let client = Self {
            addr: addr.clone(),
            connected: connected.clone(),
            alive: alive.clone(),
            frame_view: frame_view.clone(),
        };

        let _: std::thread::JoinHandle<()> = std::thread::Builder::new()
            .name("http_client_thread".to_owned())
            .spawn(move || {
                log::info!("Connecting to {addr}…");
                while alive.load(SeqCst) {
                    match std::net::TcpStream::connect(&addr) {
                        Ok(mut stream) => {
                            *frame_view.lock() = FrameView::default();
                            log::info!("Connected to {addr}");
                            connected.store(true, SeqCst);
                            while alive.load(SeqCst) {
                                match consume_message(&mut stream) {
                                    Ok(frame_data) => {
                                        frame_view
                                            .lock()
                                            .add_frame(std::sync::Arc::new(frame_data));
                                    }
                                    Err(err) => {
                                        log::warn!(
                                            "Connection to puffin server closed: {}",
                                            error_display_chain(err.as_ref())
                                        );
                                        connected.store(false, SeqCst);
                                        break;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            log::debug!("Failed to connect to {addr}: {err}");
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                }
            })
            .expect("Failed to spawn client thread");

        client
    }

    /// The address we are connected to or trying to connect to.
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Are we currently connect to the server?
    pub fn connected(&self) -> bool {
        self.connected.load(SeqCst)
    }

    /// Get the current data.
    pub fn frame_view(&self) -> parking_lot::MutexGuard<'_, FrameView> {
        self.frame_view.lock()
    }
}

/// Read a `puffin_http` message from a stream.
pub fn consume_message(stream: &mut impl std::io::Read) -> anyhow::Result<MessageContent> {
    let mut server_version = [0_u8; 2];
    stream.read_exact(&mut server_version)?;
    let server_version = u16::from_le_bytes(server_version);

    match server_version.cmp(&crate::PROTOCOL_VERSION) {
        std::cmp::Ordering::Less => {
            anyhow::bail!(
                "puffin server is using an older protocol version ({}) than the client ({}).",
                server_version,
                crate::PROTOCOL_VERSION
            );
        }
        std::cmp::Ordering::Equal => {}
        std::cmp::Ordering::Greater => {
            anyhow::bail!(
                "puffin server is using a newer protocol version ({}) than the client ({}). Update puffin_viewer with 'cargo install puffin_viewer --locked'.",
                server_version,
                crate::PROTOCOL_VERSION
            );
        }
    }

    todo!("handle scope collection");

    let frame_data = FrameData::read_next(stream)
        .context("Failed to parse FrameData")?
        .ok_or_else(|| anyhow::format_err!("End of stream"))?;
    Ok(MessageContent::FrameData(frame_data))
}

/// Show full cause chain in a single line
fn error_display_chain(error: &dyn std::error::Error) -> String {
    let mut s = error.to_string();
    if let Some(source) = error.source() {
        s.push_str(" -> ");
        s.push_str(&error_display_chain(source));
    }
    s
}
