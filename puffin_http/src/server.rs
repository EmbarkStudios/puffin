use anyhow::Context as _;
use puffin::GlobalProfiler;
use std::{
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{mpsc, Arc},
};

/// Maximum size of the backlog of packets to send to a client if they aren't reading fast enough.
const MAX_FRAMES_IN_QUEUE: usize = 30;

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

        let (tx, rx): (mpsc::Sender<Arc<puffin::FrameData>>, _) = mpsc::channel();

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

type Packet = Arc<[u8]>;

/// Listens for incoming connections
/// and streams them puffin profiler data.
struct PuffinServerImpl {
    tcp_listener: TcpListener,
    clients: Vec<(SocketAddr, mpsc::SyncSender<Packet>)>,
}

impl PuffinServerImpl {
    fn accept_new_clients(&mut self) -> anyhow::Result<()> {
        loop {
            match self.tcp_listener.accept() {
                Ok((tcp_stream, client_addr)) => {
                    tcp_stream
                        .set_nonblocking(false)
                        .context("stream.set_nonblocking")?;

                    log::info!("{} connected", client_addr);

                    let (packet_tx, packet_rx) = mpsc::sync_channel(MAX_FRAMES_IN_QUEUE);

                    std::thread::Builder::new()
                        .name("puffin-server-client".to_owned())
                        .spawn(move || client_loop(packet_rx, client_addr, tcp_stream))
                        .context("Couldn't spawn thread")?;

                    self.clients.push((client_addr, packet_tx));
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
        puffin::profile_function!();

        let mut packet = vec![];
        packet
            .write_all(&crate::PROTOCOL_VERSION.to_le_bytes())
            .unwrap();
        frame
            .write_into(&mut packet)
            .context("Encode puffin frame")?;

        let packet: Packet = packet.into();

        self.clients.retain(
            |(client_addr, packet_tx)| match packet_tx.try_send(packet.clone()) {
                Ok(()) => true,
                Err(mpsc::TrySendError::Disconnected(_)) => false,
                Err(mpsc::TrySendError::Full(_)) => {
                    log::info!(
                        "puffin client {} is not accepting data fast enough; dropping a frame",
                        client_addr
                    );
                    true
                }
            },
        );

        Ok(())
    }
}

fn client_loop(
    packet_rx: mpsc::Receiver<Packet>,
    client_addr: SocketAddr,
    mut tcp_stream: TcpStream,
) {
    while let Ok(packet) = packet_rx.recv() {
        if let Err(err) = tcp_stream.write_all(&packet) {
            log::info!(
                "puffin server failed sending to {}: {} (kind: {:?})",
                client_addr,
                err,
                err.kind()
            );
            break;
        }
    }
}
