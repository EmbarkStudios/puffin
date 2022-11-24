use anyhow::Context as _;
use async_executor::{LocalExecutor, Task};
use async_std::{
    io::WriteExt,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
};
use futures_lite::future;
use puffin::GlobalProfiler;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Maximum size of the backlog of packets to send to a client if they aren't reading fast enough.
const MAX_FRAMES_IN_QUEUE: usize = 30;

/// Listens for incoming connections
/// and streams them puffin profiler data.
///
/// Drop to stop transmitting and listening for new connections.
pub struct Server {
    sink_id: puffin::FrameSinkId,
    join_handle: Option<std::thread::JoinHandle<()>>,
    num_clients: Arc<AtomicUsize>,
}

impl Server {
    /// Start listening for connections on this addr (e.g. "0.0.0.0:8585")
    pub fn new(bind_addr: &str) -> anyhow::Result<Self> {
        let bind_addr = String::from(bind_addr);
        // We use crossbeam_channel instead of `mpsc`,
        // because on shutdown we want all frames to be sent.
        // `mpsc::Receiver` stops receiving as soon as the `Sender` is dropped,
        // but `crossbeam_channel` will continue until the channel is empty.
        let (tx, rx): (flume::Sender<Arc<puffin::FrameData>>, _) = flume::unbounded();

        let num_clients = Arc::new(AtomicUsize::default());
        let num_clients_cloned = num_clients.clone();
        let join_handle = std::thread::Builder::new()
            .name("puffin-server".to_owned())
            .spawn(move || {
                Server::run(bind_addr, rx, num_clients_cloned).unwrap();
            })
            .context("Can't start puffin-server thread.")?;

        let sink_id = GlobalProfiler::lock().add_sink(Box::new(move |frame| {
            tx.send(frame).ok();
        }));

        Ok(Server {
            sink_id,
            join_handle: Some(join_handle),
            num_clients,
        })
    }

    /// start and run puffin server service
    pub fn run(
        bind_addr: String,
        rx: flume::Receiver<Arc<puffin::FrameData>>,
        num_clients: Arc<AtomicUsize>,
    ) -> anyhow::Result<()> {
        let executor = Rc::new(LocalExecutor::new());

        let clients = Rc::new(RefCell::new(Vec::new()));
        let clients_cloned = clients.clone();
        let num_clients_cloned = num_clients.clone();

        let executor_cloned = executor.clone();
        let _psconnect_handle = //task::Builder::new()
            //.name("ps-connect".to_owned())
            executor.spawn(async move {
                let tcp_listener = TcpListener::bind(bind_addr)
                    .await
                    .context("binding server TCP socket")
                    .unwrap(); //TODO use ?

                let mut ps_connection = PuffinServerConnection {
                    executor: executor_cloned,
                    tcp_listener,
                    clients: clients_cloned,
                    num_clients: num_clients_cloned,
                };
                if let Err(err) = ps_connection.accept_new_clients().await {
                    log::warn!("puffin server failure: {}", err);
                }
            });
        //.context("Couldn't spawn ps-connect task")?;

        let pssend_handle = //task::Builder::new()
            //.name("ps-send".to_owned())
            executor.spawn(async move {
                let mut ps_send = PuffinServerSend {
                    clients,
                    num_clients,
                };

                while let Ok(frame) = rx.recv_async().await {
                    if let Err(err) = ps_send.send(&frame).await {
                        log::warn!("puffin server failure: {}", err);
                    }
                }
            });
        //.context("Couldn't spawn ps-send task")?;

        future::block_on(executor.run(pssend_handle));
        Ok(())
    }

    /// Number of clients currently connected.
    pub fn num_clients(&self) -> usize {
        self.num_clients.load(Ordering::SeqCst)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        GlobalProfiler::lock().remove_sink(self.sink_id);

        // Take care to send everything before we shut down:
        if let Some(join_handle) = self.join_handle.take() {
            join_handle.join().ok();
        }
    }
}

type Packet = Rc<[u8]>;

struct Client {
    client_addr: SocketAddr,
    packet_tx: Option<flume::Sender<Packet>>,
    join_handle: Option<Task<()>>,
}

impl Drop for Client {
    fn drop(&mut self) {
        // Take care to send everything before we shut down!

        // Drop the sender to signal to shut down:
        self.packet_tx = None;

        // Wait for the shutdown:
        if let Some(join_handle) = self.join_handle.take() {
            future::block_on(join_handle); // .ok()
        }
    }
}

/// Listens for incoming connections
struct PuffinServerConnection<'a> {
    executor: Rc<LocalExecutor<'a>>,
    tcp_listener: TcpListener,
    clients: Rc<RefCell<Vec<Client>>>,
    num_clients: Arc<AtomicUsize>,
}

impl<'a> PuffinServerConnection<'a> {
    async fn accept_new_clients(&mut self) -> anyhow::Result<()> {
        loop {
            match self.tcp_listener.accept().await {
                Ok((tcp_stream, client_addr)) => {
                    puffin::profile_scope!("accept_client");
                    log::info!("{} connected", client_addr);

                    let (packet_tx, packet_rx) = flume::bounded(MAX_FRAMES_IN_QUEUE);

                    let join_handle = //task::Builder::new()
                        //.name("ps-client".to_owned())
                        self.executor.spawn(async move {
                            client_loop(packet_rx, client_addr, tcp_stream).await;
                        });
                    //.context("Couldn't spawn ps-client task")?;

                    self.clients.borrow_mut().push(Client {
                        client_addr,
                        packet_tx: Some(packet_tx),
                        join_handle: Some(join_handle),
                    });
                    self.num_clients
                        .store(self.clients.borrow().len(), Ordering::SeqCst);
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
}

/// streams to client puffin profiler data.
struct PuffinServerSend {
    clients: Rc<RefCell<Vec<Client>>>,
    num_clients: Arc<AtomicUsize>,
}

impl PuffinServerSend {
    pub async fn send(&mut self, frame: &puffin::FrameData) -> anyhow::Result<()> {
        if self.clients.borrow().is_empty() {
            return Ok(());
        }
        puffin::profile_function!();

        let mut packet = vec![];
        packet
            .write_all(&crate::PROTOCOL_VERSION.to_le_bytes())
            .await
            .unwrap();
        frame
            .write_into(&mut packet)
            .context("Encode puffin frame")?;

        let packet: Packet = packet.into();

        // Send frame to clients, remove disconnected clients and update num_clients var
        let mut clients = self.clients.borrow_mut();
        let mut idx_to_remove = Vec::new();
        for (idx, client) in clients.iter().enumerate() {
            if !Self::send_to_client(client, packet.clone()).await {
                idx_to_remove.push(idx);
            }
        }
        idx_to_remove.iter().rev().for_each(|idx| {
            clients.remove(*idx);
        });
        self.num_clients.store(clients.len(), Ordering::SeqCst);

        Ok(())
    }

    async fn send_to_client(client: &Client, packet: Packet) -> bool {
        puffin::profile_function!();
        match &client.packet_tx {
            None => false,
            Some(packet_tx) => match packet_tx.send_async(packet).await {
                Ok(()) => true,
                Err(err) => {
                    log::info!("puffin send error: {} for '{}'", err, client.client_addr);
                    true
                }
            },
        }
    }
}

async fn client_loop(
    packet_rx: flume::Receiver<Packet>,
    client_addr: SocketAddr,
    mut tcp_stream: TcpStream,
) {
    loop {
        match packet_rx.recv_async().await {
            Ok(packet) => {
                puffin::profile_scope!("write frame to client");
                if let Err(err) = tcp_stream.write_all(&packet).await {
                    log::info!(
                        "puffin server failed sending to {}: {} (kind: {:?})",
                        client_addr,
                        err,
                        err.kind()
                    );
                    break;
                }
            }
            Err(err) => {
                log::info!("Error in client_loop: {}", err);
                break;
            }
        }
    }
}
