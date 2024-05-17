use anyhow::Context as _;
use puffin::{FrameView, GlobalProfiler};
use std::{
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

/// Maximum size of the backlog of packets to send to a client if they aren't reading fast enough.
const MAX_FRAMES_IN_QUEUE: usize = 30;

/// Listens for incoming connections
/// and streams them puffin profiler data.
///
/// Drop to stop transmitting and listening for new connections.
#[must_use = "When Server is dropped, the server is closed, so keep it around!"]
pub struct Server {
    sink_id: puffin::FrameSinkId,
    join_handle: Option<std::thread::JoinHandle<()>>,
    num_clients: Arc<AtomicUsize>,
    sink_remove: fn(puffin::FrameSinkId) -> (),
}

impl Server {
    /// Start listening for connections on this addr (e.g. "0.0.0.0:8585")
    ///
    /// Connects to the [GlobalProfiler]
    pub fn new(bind_addr: &str) -> anyhow::Result<Self> {
        fn global_add(sink: puffin::FrameSink) -> puffin::FrameSinkId {
            GlobalProfiler::lock().add_sink(sink)
        }
        fn global_remove(id: puffin::FrameSinkId) {
            GlobalProfiler::lock().remove_sink(id);
        }

        Self::new_custom(bind_addr, global_add, global_remove)
    }

    /// Starts a new puffin server, with a custom function for installing the server's sink
    ///
    /// # Arguments
    /// * `bind_addr` - The address to bind to, when listening for connections
    /// (e.g. "localhost:8585" or "127.0.0.1:8585")
    /// * `sink_install` - A function that installs the [Server]'s sink into
    /// a [GlobalProfiler], and then returns the [FrameSinkId] so that the sink can be removed later
    /// * `sink_remove` - A function that reverts `sink_install`.
    /// This should be a call to remove the sink from the profiler ([GlobalProfiler::remove_sink])
    ///
    /// # Example
    ///
    /// Using this is slightly complicated, but it is possible to use this to set a custom profiler per-thread,
    /// such that threads can be grouped together and profiled separately. E.g. you could have one profiling server
    /// instance for the main UI loop, and another for the background worker loop, and events/frames from those thread(s)
    /// would be completely separated. You can then hook up two separate instances of `puffin_viewer` and profile them separately.
    ///
    /// ## Per-Thread Profiling
    /// ```
    /// # use puffin::GlobalProfiler;
    /// # use puffin::{StreamInfoRef, ThreadInfo, ScopeDetails};
    /// # use puffin_http::Server;
    /// # use puffin::ThreadProfiler;
    /// #
    /// # pub fn main() {
    /// #
    /// #
    /// // Initialise the profiling server for the main app
    /// let default_server = Server::new("localhost:8585").expect("failed to create default profiling server");
    /// puffin::profile_scope!("main_scope");
    ///
    /// // Create a new [GlobalProfiler] instance. This is where we will be sending the events to for our threads.
    /// // [OnceLock] and [Mutex] are there so that we can safely get exclusive mutable access.
    /// static CUSTOM_PROFILER: std::sync::OnceLock<std::sync::Mutex<GlobalProfiler>> = std::sync::OnceLock::new();
    /// // Helper function to access the profiler
    /// fn get_custom_profiler() -> std::sync::MutexGuard<'static, GlobalProfiler> {
    ///    CUSTOM_PROFILER.get_or_init(|| std::sync::Mutex::new(GlobalProfiler::default()))
    ///         .lock().expect("failed to lock custom profiler")
    /// }
    /// // Create the custom profiling server that uses our custom profiler instead of the global/default one
    /// let thread_server = Server::new_custom(
    ///     "localhost:6969",
    ///     // Adds the [Server]'s sink to our custom profiler
    ///     |sink| get_custom_profiler().add_sink(sink),
    ///     // Remove
    ///     |id| _ = get_custom_profiler().remove_sink(id)
    /// );
    /// 
    /// // Create some custom threads where we use the custom profiler and server
    /// std::thread::scope(|scope| {
    ///     scope.spawn(move ||{
    ///         // Tell this thread to use the custom profiler
    ///         let _ = ThreadProfiler::initialize(
    ///             // Use the same time source as default puffin
    ///             puffin::now_ns,
    ///             // However redirect the events to our `custom_profiler`, instead of the default
    ///             // which would be the one returned by [GlobalProfiler::lock()]
    ///             |info: ThreadInfo, details: &[ScopeDetails], stream: &StreamInfoRef<'_>|
    ///                 get_custom_profiler().report(info, details, stream)
    ///         );
    ///
    ///         // Do work
    ///         {
    ///             puffin::profile_scope!("inside_thread");
    ///             println!("hello from the thread");
    ///             std::thread::sleep(std::time::Duration::from_secs(1));
    ///         }
    ///
    ///         // Tell our profiler that we are done with this frame
    ///         // This will be sent to the server on port 6969
    ///         get_custom_profiler().new_frame();
    ///     });
    /// });
    ///
    /// // New frame for the global profiler. This is completely separate from the scopes with the custom profiler
    /// GlobalProfiler::lock().new_frame();
    /// #
    /// #
    /// # }
    /// ```
    ///
    /// ## Helpful Macro
    /// ```rust
    /// # use std::thread::sleep;
    /// # use std::time::Duration;
    /// 
    /// /// This macro makes it much easier to define profilers
    /// ///
    /// /// This macro makes use of the `paste` crate to generate unique identifiers, and `tracing` to log events
    /// macro_rules! profiler {
    ///     ($(
    ///          {name: $name:ident, port: $port:expr $(,install: |$install_var:ident| $install:block, drop: |$drop_var:ident| $drop:block)? $(,)?}
    ///      ),* $(,)?)
    ///     => {
    ///         $(
    ///             profiler!(@inner { name: $name, port: $port $(,install: |$install_var| $install, drop: |$drop_var| $drop)? });
    ///         )*
    ///     };
    /// 
    ///     (@inner { name: $name:ident, port: $port:expr }) => {
    ///         paste::paste!{
    ///             #[doc = concat!("The address to bind the ", std::stringify!([< $name:lower >]), " thread profilers' server to")]
    ///                 pub const [< $name:upper _PROFILER_ADDR >] : &'static str
    ///                     = concat!("127.0.0.1:", $port);
    ///
    ///                 /// Installs the server's sink into the custom profiler
    ///                 #[doc(hidden)]
    ///                 fn [< $name:lower _profiler_server_install >](sink: puffin::FrameSink) -> puffin::FrameSinkId {
    ///                     [< $name:lower _profiler_lock >]().add_sink(sink)
    ///                 }
    ///
    ///                 /// Drops the server's sink and removes from profiler
    ///                 #[doc(hidden)]
    ///                 fn [< $name:lower _profiler_server_drop >](id: puffin::FrameSinkId){
    ///                     [< $name:lower _profiler_lock >]().remove_sink(id);
    ///                 }
    ///
    ///                 #[doc = concat!("The instance of the ", std::stringify!([< $name:lower >]), " thread profiler's server")]
    ///                 pub static [< $name:upper _PROFILER_SERVER >] : once_cell::sync::Lazy<std::sync::Mutex<puffin_http::Server>>
    ///                     = once_cell::sync::Lazy::new(|| {
    ///                         eprintln!(
    ///                             "starting puffin_http server for {} profiler at {}",
    ///                             std::stringify!([<$name:lower>]),
    ///                             [< $name:upper _PROFILER_ADDR >]
    ///                         );
    ///                         std::sync::Mutex::new(
    ///                             puffin_http::Server::new_custom(
    ///                                 [< $name:upper _PROFILER_ADDR >],
    ///                                 // Can't use closures in a const context, use fn-pointers instead
    ///                                 [< $name:lower _profiler_server_install >],
    ///                                 [< $name:lower _profiler_server_drop >],
    ///                             )
    ///                             .expect(&format!("{} puffin_http server failed to start", std::stringify!([<$name:lower>])))
    ///                         )
    ///                     });
    ///
    ///                 #[doc = concat!("A custom reporter for the ", std::stringify!([< $name:lower >]), " thread reporter")]
    ///                 pub fn [< $name:lower _profiler_reporter >] (info: puffin::ThreadInfo, details: &[puffin::ScopeDetails],  stream: &puffin::StreamInfoRef<'_>) {
    ///                     [< $name:lower _profiler_lock >]().report(info, details, stream)
    ///                 }
    ///
    ///                 #[doc = concat!("Accessor for the ", std::stringify!([< $name:lower >]), " thread reporter")]
    ///                 pub fn [< $name:lower _profiler_lock >]() -> std::sync::MutexGuard<'static, puffin::GlobalProfiler> {
    ///                     static [< $name _PROFILER >] : once_cell::sync::Lazy<std::sync::Mutex<puffin::GlobalProfiler>> = once_cell::sync::Lazy::new(Default::default);
    ///                     [< $name _PROFILER >].lock().expect("poisoned std::sync::mutex")
    ///                 }
    ///
    ///                 #[doc = concat!("Initialises the ", std::stringify!([< $name:lower >]), " thread reporter and server.\
    ///                 Call this on each different thread you want to register with this profiler")]
    ///                 pub fn [< $name:lower _profiler_init >]() {
    ///                     eprintln!("init thread profiler \"{}\"", std::stringify!([<$name:lower>]));
    ///                     std::mem::drop([< $name:upper _PROFILER_SERVER >].lock());
    ///                     eprintln!("set thread custom profiler \"{}\"", std::stringify!([<$name:lower>]));
    ///                     puffin::ThreadProfiler::initialize(::puffin::now_ns, [< $name:lower _profiler_reporter >]);
    ///                 }
    ///         }
    ///     };
    /// }
    ///
    /// profiler! {
    ///     { name: UI,          port: "2a" },
    ///     { name: RENDERER,    port: 8586 },
    ///     { name: BACKGROUND,  port: 8587 },
    /// }
    /// 
    /// pub fn demo() {
    ///     std::thread::spawn(|| {
    ///         // Initialise the custom profiler for this thread
    ///         // Now all puffin events are sent to the custom profiling server instead
    ///         //
    ///         background_profiler_init();
    ///
    ///         for i in 0..100{
    ///             puffin::profile_scope!("test");
    ///             sleep(Duration::from_millis(i));
    ///         }
    ///
    ///         // Mark a new frame so the data is flushed to the server
    ///         background_profiler_lock().new_frame();
    ///     });
    /// }
    /// ```
    pub fn new_custom(
        bind_addr: &str,
        sink_install: fn (puffin::FrameSink) -> puffin::FrameSinkId,
        sink_remove: fn (puffin::FrameSinkId) -> (),
    ) -> anyhow::Result<Self> {
        let tcp_listener = TcpListener::bind(bind_addr).context("binding server TCP socket")?;
        tcp_listener
            .set_nonblocking(true)
            .context("TCP set_nonblocking")?;

        // We use crossbeam_channel instead of `mpsc`,
        // because on shutdown we want all frames to be sent.
        // `mpsc::Receiver` stops receiving as soon as the `Sender` is dropped,
        // but `crossbeam_channel` will continue until the channel is empty.
        let (tx, rx): (crossbeam_channel::Sender<Arc<puffin::FrameData>>, _) =
            crossbeam_channel::unbounded();

        let num_clients = Arc::new(AtomicUsize::default());
        let num_clients_cloned = num_clients.clone();

        let join_handle = std::thread::Builder::new()
            .name("puffin-server".to_owned())
            .spawn(move || {
                let mut server_impl = PuffinServerImpl {
                    tcp_listener,
                    clients: Default::default(),
                    num_clients: num_clients_cloned,
                    send_all_scopes: false,
                    frame_view: Default::default(),
                };

                while let Ok(frame) = rx.recv() {
                    server_impl.frame_view.add_frame(frame.clone());
                    if let Err(err) = server_impl.accept_new_clients() {
                        log::warn!("puffin server failure: {}", err);
                    }

                    if let Err(err) = server_impl.send(&frame) {
                        log::warn!("puffin server failure: {}", err);
                    }
                }
            })
            .context("Couldn't spawn thread")?;

        // Call the `install` function to add ourselves as a sink
        let sink_id = sink_install(Box::new(move |frame| {
            tx.send(frame).ok();
        }));

        Ok(Server {
            sink_id,
            join_handle: Some(join_handle),
            num_clients,
            sink_remove,
        })
    }

    /// Number of clients currently connected.
    pub fn num_clients(&self) -> usize {
        self.num_clients.load(Ordering::SeqCst)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Remove ourselves from the profiler
        (self.sink_remove)(self.sink_id);

        // Take care to send everything before we shut down:
        if let Some(join_handle) = self.join_handle.take() {
            join_handle.join().ok();
        }
    }
}

type Packet = Arc<[u8]>;

struct Client {
    client_addr: SocketAddr,
    packet_tx: Option<crossbeam_channel::Sender<Packet>>,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Client {
    fn drop(&mut self) {
        // Take care to send everything before we shut down!

        // Drop the sender to signal to shut down:
        self.packet_tx = None;

        // Wait for the shutdown:
        if let Some(join_handle) = self.join_handle.take() {
            join_handle.join().ok();
        }
    }
}

/// Listens for incoming connections
/// and streams them puffin profiler data.
struct PuffinServerImpl {
    tcp_listener: TcpListener,
    clients: Vec<Client>,
    num_clients: Arc<AtomicUsize>,
    send_all_scopes: bool,
    frame_view: FrameView,
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

                    let (packet_tx, packet_rx) = crossbeam_channel::bounded(MAX_FRAMES_IN_QUEUE);

                    let join_handle = std::thread::Builder::new()
                        .name("puffin-server-client".to_owned())
                        .spawn(move || client_loop(packet_rx, client_addr, tcp_stream))
                        .context("Couldn't spawn thread")?;

                    // Send all scopes when new client connects.
                    self.send_all_scopes = true;
                    self.clients.push(Client {
                        client_addr,
                        packet_tx: Some(packet_tx),
                        join_handle: Some(join_handle),
                    });
                    self.num_clients.store(self.clients.len(), Ordering::SeqCst);
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
            .write_into(
                self.frame_view.scope_collection(),
                self.send_all_scopes,
                &mut packet,
            )
            .context("Encode puffin frame")?;
        self.send_all_scopes = false;

        let packet: Packet = packet.into();

        self.clients.retain(|client| match &client.packet_tx {
            None => false,
            Some(packet_tx) => match packet_tx.try_send(packet.clone()) {
                Ok(()) => true,
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => false,
                Err(crossbeam_channel::TrySendError::Full(_)) => {
                    log::info!(
                        "puffin client {} is not accepting data fast enough; dropping a frame",
                        client.client_addr
                    );
                    true
                }
            },
        });
        self.num_clients.store(self.clients.len(), Ordering::SeqCst);

        Ok(())
    }
}

fn client_loop(
    packet_rx: crossbeam_channel::Receiver<Packet>,
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
