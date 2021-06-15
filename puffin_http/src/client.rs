use std::sync::{
    atomic::{AtomicBool, Ordering::SeqCst},
    Arc,
};

/// Connect to a [`crate::Server`], reading profile data
/// and feeding it to [`puffin::GlobalProfiler`].
///
/// Will retry connection until it succeeds, and reconnect on failures.
pub struct Client {
    addr: String,
    connected: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
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
    /// [`puffin_egui`](https://crates.io/crates/puffin_egui) or [`puffin-imgui`](https://crates.io/crates/puffin-imgui).
    ///
    /// ``` no_run
    /// puffin_http::Client::new("127.0.0.1:8585".to_owned());
    /// ```
    pub fn new(addr: String) -> Self {
        let alive = Arc::new(AtomicBool::new(true));
        let connected = Arc::new(AtomicBool::new(false));

        let client = Self {
            addr: addr.clone(),
            connected: connected.clone(),
            alive: alive.clone(),
        };

        std::thread::spawn(move || {
            log::info!("Connecting to {}…", addr);
            while alive.load(SeqCst) {
                match std::net::TcpStream::connect(&addr) {
                    Ok(mut stream) => {
                        log::info!("Connected to {}", addr);
                        connected.store(true, SeqCst);
                        while alive.load(SeqCst) {
                            match consume_message(&mut stream) {
                                Ok(frame_data) => {
                                    puffin::GlobalProfiler::lock()
                                        .add_frame(std::sync::Arc::new(frame_data));
                                }
                                Err(err) => {
                                    log::warn!("Connection to puffin server closed: {}", err);
                                    connected.store(false, SeqCst);
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        log::debug!("Failed to connect to {}: {}", addr, err);
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
            }
        });

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
}

/// Read a `puffin_http` message from a stream.
pub fn consume_message(stream: &mut dyn std::io::Read) -> anyhow::Result<puffin::FrameData> {
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
            "puffin server is using a newer protocol version ({}) than the client ({}). Update puffin_viewer with 'cargo install puffin_viewer'.",
            server_version,
            crate::PROTOCOL_VERSION
        );
        }
    }

    let mut message_len = [0_u8; 4];
    stream.read_exact(&mut message_len)?;
    let message_len = u32::from_le_bytes(message_len);

    let mut bytes = vec![0_u8; message_len as usize];
    stream.read_exact(&mut bytes)?;

    use anyhow::Context as _;
    use bincode::Options as _;

    bincode::options()
        .deserialize(&bytes)
        .context("Failed to decode bincode")
}
