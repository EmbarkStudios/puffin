use anyhow::Context as _;
use std::{
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
};

/// Listens for incoming connections
/// and streams them puffin profiler data.
pub struct PuffinServer {
    tx: std::sync::mpsc::Sender<Arc<puffin::FrameData>>,
}

impl PuffinServer {
    /// Start listening for connections on this addr (e.g. "0.0.0.0:8585")
    pub fn new(bind_addr: &str) -> anyhow::Result<Self> {
        let tcp_listener = TcpListener::bind(bind_addr).context("binding server TCP socket")?;
        tcp_listener
            .set_nonblocking(true)
            .context("TCP set_nonblocking")?;

        let (tx, rx) = std::sync::mpsc::channel();

        let server = PuffinServer { tx };

        std::thread::spawn(move || {
            let mut server_impl = PuffinServerImpl {
                tcp_listener,
                clients: Default::default(),
            };

            while let Ok(frame) = rx.recv() {
                if let Err(err) = server_impl.accept_new_clients() {
                    log::warn!("puffin server failure: {}", err);
                }
                if let Err(err) = server_impl.send(&*frame) {
                    log::warn!("puffin server failure: {}", err);
                }
            }
        });

        Ok(server)
    }

    /// Call this once per frame, right after calling [`puffin::GlobalProfiler::new_frame`].
    pub fn update(&self) {
        let latest_frame = puffin::GlobalProfiler::lock().latest_frame();
        if let Some(latest_frame) = latest_frame {
            self.tx.send(latest_frame).ok();
        }
    }
}

/// Listens for incoming connections
/// and streams them puffin profiler data.
struct PuffinServerImpl {
    tcp_listener: TcpListener,
    clients: Vec<(SocketAddr, TcpStream)>,
}

impl PuffinServerImpl {
    fn accept_new_clients(&mut self) -> anyhow::Result<()> {
        loop {
            match self.tcp_listener.accept() {
                Ok((stream, client_addr)) => {
                    stream
                        .set_nonblocking(true)
                        .context("stream.set_nonblocking")?;

                    log::info!("{} connected", client_addr);
                    self.clients.push((client_addr, stream));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    break; // Nothing to do for now.
                }
                Err(e) => {
                    anyhow::bail!("puffin server TCP error: {:?}", e);
                }
            }
        }
        Ok(())
    }

    pub fn send(&mut self, frame: &puffin::FrameData) -> anyhow::Result<()> {
        use bincode::Options as _;
        let mut encoded = bincode::options()
            .serialize(frame)
            .context("Encode puffin frame")?;

        let mut message = vec![];
        message
            .write_all(&crate::PROTOCOL_VERSION.to_le_bytes())
            .unwrap();
        message
            .write_all(&(encoded.len() as u32).to_le_bytes())
            .unwrap();
        message.append(&mut encoded);

        use retain_mut::RetainMut as _;
        self.clients
            .retain_mut(|(addr, stream)| match stream.write_all(&message) {
                Ok(()) => true,
                Err(err) => {
                    log::info!("Failed sending to {}: {}", addr, err);
                    false
                }
            });

        Ok(())
    }
}
