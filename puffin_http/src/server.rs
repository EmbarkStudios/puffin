use anyhow::Context as _;
use puffin::GlobalProfiler;
use std::{
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
};

/// Listens for incoming connections
/// and streams them puffin profiler data.
///
/// Drop to stop transmitting and listening for new connections.
pub struct Server {
    sink_id: puffin::FrameSinkId,
}

impl Server {
    /// Start listening for connections on this addr (e.g. "0.0.0.0:8585")
    pub fn new(bind_addr: &str) -> anyhow::Result<Self> {
        let tcp_listener = TcpListener::bind(bind_addr).context("binding server TCP socket")?;
        tcp_listener
            .set_nonblocking(true)
            .context("TCP set_nonblocking")?;

        let (tx, rx): (std::sync::mpsc::Sender<Arc<puffin::FrameData>>, _) =
            std::sync::mpsc::channel();

        std::thread::Builder::new()
            .name("puffin-server".to_owned())
            .spawn(move || {
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
            })
            .context("Couldn't spawn thread")?;

        let sink_id = GlobalProfiler::lock().add_sink(Box::new(move |frame| {
            tx.send(frame).ok();
        }));

        Ok(Server { sink_id })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        GlobalProfiler::lock().remove_sink(self.sink_id);
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
        if self.clients.is_empty() {
            return Ok(());
        }

        let mut message = vec![];
        message
            .write_all(&crate::PROTOCOL_VERSION.to_le_bytes())
            .unwrap();
        frame
            .write_into(&mut message)
            .context("Encode puffin frame")?;

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
