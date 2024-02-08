use anyhow::Context as _;
use puffin::{FrameSink, FrameSinkId, GlobalProfiler};
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
    sink_id: FrameSinkId,
    join_handle: Option<std::thread::JoinHandle<()>>,
    num_clients: Arc<AtomicUsize>,
    // The Fn is required to be static, since we need to be able to call for
    // any possible lifetime up to `&'static`. The other alternative is a
    // lifetime parameter: `Box< T + 'life>`, or `&'life GlobalProfiler`
    // However a lifetime param would not be backwards compatible
    // (cannot give it a default, so it must be specified)
    // sink_remove: &'profiler GlobalProfiler,
    sink_remove: Option<Box<dyn FnOnce(FrameSinkId) -> () + Send + 'static>>,
}

impl Server {
    /// Start listening for connections on this addr (e.g. "0.0.0.0:8585")
    ///
    /// Connects to the [GlobalProfiler]
    pub fn new(bind_addr: &str) -> anyhow::Result<Self> {
        fn global_add(sink: FrameSink) -> FrameSinkId {
            GlobalProfiler::lock().add_sink(sink)
        }
        fn global_remove(id: FrameSinkId) {
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
    /// # use std::sync::Mutex;
    /// # use std::thread::sleep;
    /// # use std::time::Duration;
    /// # use puffin::{StreamInfoRef, ThreadInfo};
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
    /// // Create some custom threads
    /// std::thread::scope(|scope| {
    ///     // Create a new [GlobalProfiler] instance. This is where we will be sending the events to for our threads
    ///     // In a real app this would be `static`, and you would need to use a [Mutex] anyway
    ///     let mut custom_profiler = Mutex::new(GlobalProfiler::default());
    ///     // Create the custom profiling server that uses our custom profiler instead of the global/default one
    ///     let thread_server = Server::new_custom("localhost:6969", |sink| custom_profiler.lock().unwrap().add_sink(sink), |id| {custom_profiler.lock().unwrap().remove_sink(id);});
    ///
    ///     // Spawn some threads where we use the custom profiler and server
    ///     scope.spawn(||{
    ///         // Tell this thread to use the custom profiler
    ///         let _ = ThreadProfiler::initialize(
    ///             // Use the same time source as default puffin
    ///             puffin::now_ns,
    ///             // However redirect the events to our `custom_profiler`, instead of the default
    ///             // which would be the one returned by [GlobalProfiler::lock()]
    ///             |info: ThreadInfo, stream: &StreamInfoRef<'_>| custom_profiler.report(info, stream)
    ///         );
    ///
    ///         // Do work
    ///         {
    ///             puffin::profile_scope!("inside_thread");
    ///             println!("hello from the thread");
    ///             sleep(Duration::from_secs(1));
    ///         }
    ///
    ///         // Tell our profiler that we are done with this frame
    ///         // This will be sent to the server on port 6969
    ///         custom_profiler.lock().unwrap().new_frame();
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
    /// ```
    /// /// This macro makes it much easier to define profilers
    /// ///
    /// /// This macro makes use of the `paste` crate to generate unique identifiers, and `tracing` to log events
    /// use std::thread::sleep;
    /// use std::time::Duration;
    /// macro_rules! profiler {
    ///     ($(
    ///         {name: $name:ident, port: $port:expr $(,install: |$install_var:ident| $install:block, drop: |$drop_var:ident| $drop:block)? $(,)?}),*
    ///     $(,)?)
    ///     => {
    ///         $(
    ///             $crate::profiler!(@inner {name: $name, port: $port $(,install: |$install_var| $install, drop: |$drop_var| $drop)?});
    ///         )*
    ///     };
    ///
    ///     (@inner {name: $name:ident, port: $port:expr}) => {
    ///         paste::paste!{
    ///             #[doc = concat!("The address to bind the ", std::stringify!([< $name:lower >]), " thread profiler's server to")]
    ///                 pub const [< $name:upper _PROFILER_ADDR >] : &'static str
    ///                     = std::concat!("127.0.0.1:", $port);
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
    ///                         tracing::debug!(
    ///                             "starting puffin_http server for {} profiler at {}",
    ///                             std::stringify!([<$name:lower>]),
    ///                             [< $name:upper _PROFILER_ADDR >])
    ///                         ;
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
    ///                 pub fn [< $name:lower _profiler_reporter >] (info: puffin::ThreadInfo, stream: &puffin::StreamInfoRef<'_>) {
    ///                     [< $name:lower _profiler_lock >]().report(info, stream)
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
    ///                     tracing::trace!("init thread profiler \"{}\"", std::stringify!([<$name:lower>]));
    ///                     std::mem::drop([< $name:upper _PROFILER_SERVER >].lock());
    ///                     tracing::trace!("set thread custom profiler \"{}\"", std::stringify!([<$name:lower>]));
    ///                     puffin::ThreadProfiler::initialize(::puffin::now_ns, [< $name:lower _profiler_reporter >]);
    ///                 }
    ///         }
    ///     };
    /// }
    ///
    ///
    /// profiler! {
    ///     {name: UI,          port: 8585},
    ///     {name: RENDERER,    port: 8586},
    ///     {name: BACKGROUND,  port: 8587},
    /// }
    ///
    /// ```
    ///
    /// ```rust,ignore
    /// pub fn demo() {
    ///     std::thread::spawn(|| {
    ///         // Initialise the custom profiler for this thread
    ///         // Now all puffin events are sent to the custom profiling server instead
    ///         //
    ///         self::background_profiler_init();
    ///
    ///         for i in 0..100{
    ///             puffin::profile_scope!("test");
    ///             sleep(Duration::from_millis(i));
    ///         }
    ///
    ///         // Mark a new frame so the data is flushed to the server
    ///         self::background_profiler_lock().new_frame();
    ///     });
    /// }
    /// ```
    pub fn new_custom(
        bind_addr: &str,
        sink_install: impl FnOnce(FrameSink) -> FrameSinkId,
        sink_remove: impl FnOnce(FrameSinkId) -> () + Send + 'static,
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
                };

                while let Ok(frame) = rx.recv() {
                    if let Err(err) = server_impl.accept_new_clients() {
                        log::warn!("puffin server failure: {}", err);
                    }
                    if let Err(err) = server_impl.send(&frame) {
                        log::warn!("puffin server failure: {}", err);
                    }
                }
            })
            .context("Couldn't spawn thread")?;

        // Call the install function to add ourselves as a sink
        let sink_id = sink_install(Box::new(move |frame| {
            tx.send(frame).ok();
        }));

        Ok(Server {
            sink_id,
            join_handle: Some(join_handle),
            num_clients,
            sink_remove: Some(Box::new(sink_remove)),
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
        match self.sink_remove.take()
        {
            None => log::warn!("puffin server could not remove sink: was `None`"),
            Some(sink_remove) => sink_remove(self.sink_id),
        };

        // Take care to send everything before we shut down:
        log::trace!("puffin server dropped, flushing remaining buffer");
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
            .write_into(&mut packet)
            .context("Encode puffin frame")?;

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
