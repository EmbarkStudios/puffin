use ::std::thread::JoinHandle;
use anyhow::Context as _;
use parking_lot::Mutex;
use puffin::{FrameData, FrameSinkId, GlobalProfiler, ScopeCollection};
use std::{
    collections::HashMap,
    io::{ErrorKind, Write as _},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs as _},
    sync::{
        Arc, LazyLock,
        atomic::{AtomicUsize, Ordering},
        mpsc::{Receiver, Sender, SyncSender, TryRecvError, TrySendError, channel, sync_channel},
    },
    time::Duration,
};

/// Maximum size of the backlog of packets to send to a client if they aren't reading fast enough.
const MAX_FRAMES_IN_QUEUE: usize = 30;

const TCP_PING_TIMEOUT: Duration = Duration::from_millis(50);
const TCP_WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// Listens for incoming connections
/// and streams them puffin profiler data.
///
/// Drop to stop transmitting and listening for new connections.
#[must_use = "When Server is dropped, the server is closed, so keep it around!"]
pub struct Server {
    shared: Arc<Shared>,
    local_addr: SocketAddr,
    listener_handle: Option<std::thread::JoinHandle<()>>,
    fan_out_handle: Option<std::thread::JoinHandle<()>>,
    sink_id: FrameSinkId,
    sink_remove: fn(FrameSinkId) -> (),
}

impl Server {
    /// Start listening for connections on this addr (e.g. "0.0.0.0:8585").
    ///
    /// Port can be set to 0 to use any random unused unprivileged port
    /// (e.g. "127.0.0.1:0").
    ///
    /// Connects to the [`GlobalProfiler`]
    ///
    /// # Errors
    ///
    /// forward error from [`Self::new_custom`] call.
    pub fn new(bind_addr: &str) -> anyhow::Result<Self> {
        fn global_add(sink: puffin::FrameSink) -> FrameSinkId {
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
    ///   (e.g. "localhost:8585" or "127.0.0.1:8585"); port can be set to 0 to use any random unused unprivileged port
    /// * `sink_install` - A function that installs the [Server]'s sink into
    ///   a [`GlobalProfiler`], and then returns the [`FrameSinkId`] so that the sink can be removed later
    /// * `sink_remove` - A function that reverts `sink_install`.
    ///   This should be a call to remove the sink from the profiler ([`GlobalProfiler::remove_sink`])
    ///
    /// # Example
    ///
    /// Using this is slightly complicated, but it is possible to use this to set a custom profiler per-thread,
    /// such that threads can be grouped together and profiled separately. E.g. you could have one profiling server
    /// instance for the main UI loop, and another for the background worker loop, and events/frames from those thread(s)
    /// would be completely separated. You can then hook up two separate instances of `puffin_viewer` and profile them separately.
    ///
    /// # Errors
    ///
    /// Will return an `io::Error` if the [`TcpListener::bind`] fail.
    /// Will return an `io::Error` if the spawn of the thread ,for connection and data send management, fail.
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
    ///                 pub static [< $name:upper _PROFILER_SERVER >] : std::sync::LazyLock<std::sync::Mutex<puffin_http::Server>>
    ///                     = std::sync::LazyLock::new(|| {
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
    ///                     static [< $name _PROFILER >] : std::sync::LazyLock<std::sync::Mutex<puffin::GlobalProfiler>> = std::sync::LazyLock::new(Default::default);
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
        sink_install: fn(puffin::FrameSink) -> FrameSinkId,
        sink_remove: fn(FrameSinkId) -> (),
    ) -> anyhow::Result<Self> {
        let shared = Arc::new(Shared::default());

        let (listener, rx_client_from_listener) = ListenerLoop::new(&shared, bind_addr)?;
        let local_addr = listener.local_addr()?;
        let listener_handle = std::thread::Builder::new()
            .name("puffin-server-listener".to_owned())
            .spawn(|| listener.accept_clients())
            .context("Couldn't spawn listener thread")?;

        let (fan_out, tx_data_to_fan_out) = FanOutLoop::new(&shared, rx_client_from_listener);
        let fan_out_handle = std::thread::Builder::new()
            .name("puffin-server-fan-out".to_owned())
            .spawn(|| fan_out.fan_out_loop())
            .context("Couldn't spawn fan-out thread")?;

        // Call the `install` function to add ourselves as a sink
        let sink_id = sink_install(Box::new(move |frame| {
            tx_data_to_fan_out.send(frame).ok();
        }));

        log::info!("Accepting connections on {local_addr}");

        Ok(Self {
            shared,
            local_addr,
            listener_handle: Some(listener_handle),
            fan_out_handle: Some(fan_out_handle),
            sink_id,
            sink_remove,
        })
    }

    /// Socket address and port of this server.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Number of clients currently connected.
    pub fn num_clients(&self) -> usize {
        self.shared.num_clients()
    }

    /// Set a callback that will be called when first client connects or last client disconnects.
    ///
    /// Callback function must accept a single argument of type `bool`. `true` is passed when the first client connects,
    /// and `false` is passed when the last client disconnects.
    ///
    /// It is guaranteed that:
    ///
    /// * The callback will be called right when it is set if `Server` does not have any existing callback.
    ///   `true` will be passed if `Server` has one or more clients connected already and `false` otherwise.
    /// * The callback will be called when `Server` is dropped if `Server` had active client connections (and
    ///   thus the previous call was `on_state_change(true)`).
    /// * The callback will be called when first client connects even if no profiling data was submitted yet.
    ///   Thus the callback mechanism can be used to implement the "wait for external profiler before starting to
    ///   generate frames" logic.
    /// * The callback will never be called consecutively with the same `bool` argument.
    /// * The callback will be dropped when `Server` is dropped.
    ///
    /// Note that the callback call when the last client disconnects may be delayed until more frame profiling data
    /// is submitted or `Server` is dropped.
    ///
    /// This function is a simplified wrapper around [`Server::replace_on_state_change`].
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use puffin_http::Server;
    /// #
    /// # pub fn main() {
    /// let mut server = Server::new("localhost:0").expect("failed to create default profiling server");
    /// // Enable profiling only when clients are connected.
    /// server.set_on_state_change(|has_clients| puffin::set_scopes_on(has_clients));
    /// # }
    /// ```
    pub fn set_on_state_change<F>(&mut self, on_state_change: F)
    where
        F: FnMut(bool) + Send + 'static,
    {
        self.replace_on_state_change(Some(Box::new(on_state_change)));
    }

    /// Unset the callback previously set by [`Server::set_on_state_change`] (if any).
    ///
    /// This drops the callback without calling it.
    ///
    /// This function is a simplified wrapper around [`Server::replace_on_state_change`].
    pub fn unset_on_state_change(&mut self) {
        self.replace_on_state_change(None);
    }

    /// Replace a callback that will be called when first client connects or last client disconnects.
    ///
    /// Returns the previous callback (if any).
    ///
    /// See [`Server::set_on_state_change`] for the detailed explanation.
    // `self` is `mut` here to prevent a possible deadlock caused by calling
    // this function again from inside of the `on_state_change` callback.
    #[expect(clippy::needless_pass_by_ref_mut)]
    pub fn replace_on_state_change(
        &mut self,
        on_state_change: Option<Box<dyn FnMut(bool) + Send>>,
    ) -> Option<Box<dyn FnMut(bool) + Send>> {
        self.shared.replace_on_state_change(on_state_change).0
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Remove ourselves from the profiler
        (self.sink_remove)(self.sink_id);

        // FrameData Sender was dropped during the sink removal, fan-out thread
        // should notice it and stop.
        self.fan_out_handle
            .take()
            .expect("`fan_out_handle` is None")
            .join()
            .expect("Fan-out thread panicked");

        // Fan-out thread closed its Client Receiver, now we ping the listener thread
        // to make sure that it will notice.
        let listener_handle = self
            .listener_handle
            .take()
            .expect("`listener_handle` is None");

        let (ping_result, _tcp_stream) = tcp_ping_thread(&listener_handle, &self.local_addr);
        if ping_result {
            // Ping succeeded or listener thread already finished on its own.
            listener_handle.join().expect("Listener thread panicked");
        } else {
            // Ping failed and listener thread is still running.
            log::error!(
                "Failed to wake up {} listener thread; leaking it",
                self.local_addr
            );

            let mut leaked_listeners = LEAKED_LISTENERS.lock();
            leaked_listeners.insert(self.local_addr, listener_handle);
        }

        log::info!("Stopped accepting connections on {}", self.local_addr);
    }
}

type OnStateChange = Option<Box<dyn FnMut(bool) + Send>>;
type Packet = Arc<[u8]>;

/// Accepts incoming connections.
struct ListenerLoop {
    shared: Arc<Shared>,
    tcp_listener: TcpListener,
    tx_client_to_fan_out: Sender<Client>,
}

impl ListenerLoop {
    fn new(shared: &Arc<Shared>, bind_addr: &str) -> anyhow::Result<(Self, Receiver<Client>)> {
        // "Manually" resolve and loop over single IP:Port pairs to handle "Address already in use"
        // error for a cases when we know that we previously failed to shut down and leaked a
        // listener with this address.
        let mut tcp_listener = Err(anyhow::anyhow!(
            "No valid socket addresses resolved to bind on {:?}",
            bind_addr
        ));
        for bind_addr in bind_addr
            .to_socket_addrs()
            .context("resolving address to bind a TCP listener")?
        {
            let mut leaked_listeners = LEAKED_LISTENERS.lock();
            tcp_listener = Self::try_bind(&bind_addr, &mut leaked_listeners);
            if tcp_listener.is_ok() {
                break;
            }
        }
        let tcp_listener = tcp_listener?;

        let (tx_client_to_fan_out, rx_client_from_listener) = channel();

        Ok((
            Self {
                shared: shared.clone(),
                tcp_listener,
                tx_client_to_fan_out,
            },
            rx_client_from_listener,
        ))
    }

    /// Bind a new TCP listener socket. Retry on `AddrInUse` if listener with the same address was leaked.
    fn try_bind(
        bind_addr: &SocketAddr,
        leaked_listeners: &mut HashMap<SocketAddr, JoinHandle<()>>,
    ) -> anyhow::Result<TcpListener> {
        match TcpListener::bind(bind_addr) {
            Ok(tcp_listener) => {
                if let Some(listener_handle) = leaked_listeners.remove(
                    &tcp_listener
                        .local_addr()
                        .context("getting local address of listening TCP socket")?,
                ) {
                    // There is a previously leaked listener thread with the same address.
                    // It definitely finished because we managed to bind the socket on the same address.
                    // So it is ok to join its thread handle now.
                    listener_handle.join().expect("Listener thread panicked");
                };

                Ok(tcp_listener)
            }

            Err(err) => {
                if (err.kind() == ErrorKind::AddrInUse) && leaked_listeners.contains_key(bind_addr)
                {
                    // "Address already in use" and listener with the same address was leaked previously.
                    // Try to shut it down again.
                    let (ping_result, _tcp_stream) =
                        tcp_ping_thread(&leaked_listeners[bind_addr], bind_addr);
                    if ping_result {
                        // Ping succeeded or thread finished on its own, we can join the thread handle.
                        leaked_listeners
                            .remove(bind_addr)
                            .expect("leaked `listener_handle` is None")
                            .join()
                            .expect("Listener thread panicked");

                        // Try again with the same bind address.
                        Self::try_bind(bind_addr, leaked_listeners)
                    } else {
                        // Ping failed.
                        Err(err).context("creating listening TCP socket")
                    }
                } else {
                    // No leaked listeners
                    Err(err).context("creating listening TCP socket")
                }
            }
        }
    }

    fn local_addr(&self) -> anyhow::Result<SocketAddr> {
        self.tcp_listener
            .local_addr()
            .context("getting local address of server TCP socket")
    }

    fn accept_clients(self) {
        loop {
            match self.accept_one_client() {
                Err(err) => log::warn!("Failed to accept connection: {err}"),
                Ok(true) => (),
                Ok(false) => break,
            }
        }
    }

    fn accept_one_client(&self) -> anyhow::Result<bool> {
        match self.tcp_listener.accept() {
            Ok((tcp_stream, client_addr)) => {
                let client = Client::new(tcp_stream, client_addr)?;
                self.shared.on_client_connected();

                if self.tx_client_to_fan_out.send(client).is_err() {
                    // Fan-out thread is shutting down.
                    return Ok(false);
                }

                log::info!("{client_addr} connected");
            }

            Err(e) => {
                anyhow::bail!("puffin server TCP error: {:?}", e);
            }
        }
        Ok(true)
    }
}

static LEAKED_LISTENERS: LazyLock<Mutex<HashMap<SocketAddr, JoinHandle<()>>>> =
    LazyLock::new(Default::default);

/// Wake up a listener thread by connecting to a listening socket.
///
/// You must keep the returned `TcpStream` alive until the listener thread is finished.
///
/// At least on macOS, connection may succeed before the listener thread has a chance to `accept()` it.
/// `accept()` will never happen if the "connected" `TcpStream` is closed too soon.
fn tcp_ping_thread(thread_handle: &JoinHandle<()>, addr: &SocketAddr) -> (bool, Option<TcpStream>) {
    if thread_handle.is_finished() {
        (true, None)
    } else {
        match TcpStream::connect_timeout(addr, TCP_PING_TIMEOUT) {
            Ok(tcp_stream) => (true, Some(tcp_stream)),
            Err(_) => (thread_handle.is_finished(), None),
        }
    }
}

/// Streams puffin profiler data to all connected clients.
struct FanOutLoop {
    shared: Arc<Shared>,
    rx_client_from_listener: Option<Receiver<Client>>,
    rx_data_from_sink: Receiver<Arc<FrameData>>,
    max_packet_size: usize,
    clients: Vec<Client>,
    scope_collection: ScopeCollection,
}

impl FanOutLoop {
    fn new(
        shared: &Arc<Shared>,
        rx_client_from_listener: Receiver<Client>,
    ) -> (Self, Sender<Arc<FrameData>>) {
        let (tx_data_to_fan_out, rx_data_from_sink) = channel();

        (
            Self {
                shared: shared.clone(),
                rx_client_from_listener: Some(rx_client_from_listener),
                rx_data_from_sink,
                max_packet_size: 0,
                clients: Vec::new(),
                scope_collection: ScopeCollection::default(),
            },
            tx_data_to_fan_out,
        )
    }

    fn fan_out_loop(mut self) {
        while let Ok(frame) = self.rx_data_from_sink.recv() {
            if let Err(err) = self.send(&frame) {
                log::warn!("Failed to prepare packet: {err}");
            }
        }
        // `recv()` error signals the server shut down.

        // Drop Sender to signal the connection listener thread to shut down.
        self.rx_client_from_listener = None;

        // Call `on_state_change(false)` if `on_state_change(true)` was called before.
        let (on_state_change, had_clients) = self.shared.replace_on_state_change(None);
        if had_clients {
            if let Some(mut on_state_change) = on_state_change {
                on_state_change(false);
            }
        }
    }

    fn send(&mut self, frame: &puffin::FrameData) -> anyhow::Result<()> {
        puffin::profile_function!();

        let send_all_scopes = self.add_clients();

        // Keep scope_collection up-to-date
        for new_scope in &frame.scope_delta {
            self.scope_collection.insert(new_scope.clone());
        }

        // Nothing to send if no clients => Early return.
        if self.clients.is_empty() {
            return Ok(());
        }

        let mut packet = if self.max_packet_size == 0 {
            Vec::new()
        } else {
            Vec::with_capacity(self.max_packet_size)
        };

        packet
            .write_all(&crate::PROTOCOL_VERSION.to_le_bytes())
            .context("Encode puffin `PROTOCOL_VERSION` in packet to be send to client.")?;

        let scope_collection = if send_all_scopes {
            Some(&self.scope_collection)
        } else {
            None
        };

        frame
            .write_into(scope_collection, &mut packet)
            .context("Encode puffin frame")?;

        self.max_packet_size = self.max_packet_size.max(packet.len());
        let packet: Packet = packet.into();

        let n_clients_before = self.clients.len();
        self.clients
            .retain_mut(|client| client.try_send(packet.clone()));
        self.shared
            .on_clients_disconnected(n_clients_before - self.clients.len());

        Ok(())
    }

    fn add_clients(&mut self) -> bool {
        let n_clients_before = self.clients.len();

        loop {
            match self
                .rx_client_from_listener
                .as_ref()
                .expect("`rx_client_from_listener` is None")
                .try_recv()
            {
                Ok(client) => self.clients.push(client),
                Err(TryRecvError::Empty) => break,

                Err(TryRecvError::Disconnected) => {
                    unreachable!("Listener thread exited unexpectedly")
                }
            }
        }

        self.clients.len() != n_clients_before
    }
}

/// Handle of a connected client, with a dedicated packet sending thread.
struct Client {
    client_addr: SocketAddr,
    tx_packet_to_client: Option<SyncSender<Packet>>,
    overrun_warning_shown: bool,
    sender_handle: Option<std::thread::JoinHandle<()>>,
}

impl Client {
    fn new(tcp_stream: TcpStream, client_addr: SocketAddr) -> anyhow::Result<Self> {
        tcp_stream
            .shutdown(Shutdown::Read)
            .context("shutdown TCP read")?;
        tcp_stream
            .set_write_timeout(Some(TCP_WRITE_TIMEOUT))
            .context("set TCP write timeout")?;

        let (tx_packet_to_client, rx_packet_from_fan_out) = sync_channel(MAX_FRAMES_IN_QUEUE);

        let sender_handle = std::thread::Builder::new()
            .name(format!("puffin-server-client-{client_addr}"))
            .spawn(move || {
                send_all_packets_to_client(rx_packet_from_fan_out, client_addr, tcp_stream);
            })
            .context("Couldn't spawn new client thread")?;

        Ok(Self {
            client_addr,
            tx_packet_to_client: Some(tx_packet_to_client),
            overrun_warning_shown: false,
            sender_handle: Some(sender_handle),
        })
    }

    fn try_send(&mut self, packet: Packet) -> bool {
        match self
            .tx_packet_to_client
            .as_ref()
            .expect("tx_packet_to_client is None")
            .try_send(packet)
        {
            Ok(()) => true,
            Err(TrySendError::Disconnected(_)) => false,
            Err(TrySendError::Full(_)) => {
                if !self.overrun_warning_shown {
                    log::warn!(
                        "{} is not accepting data fast enough; dropping a frame (one-time warning)",
                        self.client_addr
                    );
                    self.overrun_warning_shown = true;
                }
                true
            }
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Drop Sender to signal the packet sender thread to shut down.
        self.tx_packet_to_client = None;

        // Wait for all remaining data to be sent.
        self.sender_handle
            .take()
            .expect("`forwarding_handle` is None")
            .join()
            .expect("Server-client thread panicked");
    }
}

#[expect(clippy::needless_pass_by_value)]
fn send_all_packets_to_client(
    rx_packet_from_fan_out: Receiver<Packet>,
    client_addr: SocketAddr,
    mut tcp_stream: TcpStream,
) {
    loop {
        let continue_loop = if let Ok(packet) = rx_packet_from_fan_out.recv() {
            tcp_stream.write_all(&packet).map(|_| true)
        } else {
            // Make sure that all data is sent before closing the connection.
            tcp_stream.shutdown(Shutdown::Write).map(|_| false)
        };

        match continue_loop {
            Err(err) => {
                if (err.kind() == ErrorKind::ConnectionReset)
                    || (err.kind() == ErrorKind::BrokenPipe)
                {
                    log::info!("{client_addr} disconnected");
                } else {
                    log::warn!(
                        "Disconnecting {} after an error: {} (kind: {:?})",
                        client_addr,
                        err,
                        err.kind()
                    );
                }
                break;
            }

            Ok(false) => break,
            Ok(true) => (),
        }
    }
}

/// Fields shared between the `Server` handle, listener thread and fan-out thread.
#[derive(Default)]
struct Shared {
    // `num_clients` is protected by the `on_state_change` mutex, but is still atomic
    // to prevent deadlock when `Server::num_clients()` is called from inside of the
    // `on_state_change` callback.
    num_clients: AtomicUsize,

    on_state_change: Mutex<OnStateChange>,
}

impl Shared {
    #[inline]
    fn num_clients(&self) -> usize {
        self.num_clients.load(Ordering::Relaxed)
    }

    fn replace_on_state_change(&self, on_state_change: OnStateChange) -> (OnStateChange, bool) {
        let mut locked_on_state_change = self.on_state_change.lock();

        let has_clients = self.num_clients() > 0;

        let old_on_state_change = if let Some(mut on_state_change) = on_state_change {
            if locked_on_state_change.is_none() {
                on_state_change(has_clients);
            }
            locked_on_state_change.replace(on_state_change)
        } else {
            locked_on_state_change.take()
        };

        (old_on_state_change, has_clients)
    }

    fn on_client_connected(&self) {
        let mut locked_on_state_change = self.on_state_change.lock();
        if self.num_clients.fetch_add(1, Ordering::Relaxed) == 0 {
            // First client connected.
            if let Some(on_state_change) = locked_on_state_change.as_mut() {
                on_state_change(true);
            }
        }
    }

    fn on_clients_disconnected(&self, num_disconnected: usize) {
        if num_disconnected == 0 {
            return;
        }

        let mut locked_on_state_change = self.on_state_change.lock();
        if self
            .num_clients
            .fetch_sub(num_disconnected, Ordering::Relaxed)
            == num_disconnected
        {
            // Last clients disconnected.
            if let Some(on_state_change) = locked_on_state_change.as_mut() {
                on_state_change(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex;
    use std::{
        net::TcpStream,
        sync::{
            Arc, Barrier,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    use crate::*;

    #[test]
    fn test_addr_in_use() {
        let server = Server::new(ANY_UNUSED_PORT).unwrap();
        let addr = format!("{}", server.local_addr());
        assert!(Server::new(&addr).is_err());
    }

    #[test]
    fn test_on_state_change() {
        let on_change = OnStateChangeMock::new();

        let mut server = Server::new(ANY_UNUSED_PORT).unwrap();
        let on_change_1 = on_change.clone();
        on_change.with_changes(|changes| assert_eq!(changes, []));

        server.set_on_state_change(move |has_clients| on_change_1.on_state_change(has_clients));
        on_change.with_changes(|changes| assert_eq!(changes, [false]));

        drop(server);
        assert_eq!(on_change.num_clones(), 1);
        on_change.with_changes(|changes| assert_eq!(changes, [false]));
    }

    #[test]
    fn test_on_state_change_only_once_on_replace() {
        let on_change = OnStateChangeMock::new();

        let mut server = Server::new(ANY_UNUSED_PORT).unwrap();
        let on_change_1 = on_change.clone();
        let on_change_2 = on_change.clone();
        on_change.with_changes(|changes| assert_eq!(changes, []));

        server.set_on_state_change(move |has_clients| on_change_1.on_state_change(has_clients));
        on_change.with_changes(|changes| assert_eq!(changes, [false]));

        server.set_on_state_change(move |has_clients| on_change_2.on_state_change(has_clients));
        on_change.with_changes(|changes| assert_eq!(changes, [false]));

        drop(server);
        assert_eq!(on_change.num_clones(), 1);
        on_change.with_changes(|changes| assert_eq!(changes, [false]));
    }

    #[test]
    fn test_on_state_change_again_after_unset() {
        let on_change = OnStateChangeMock::new();

        let mut server = Server::new(ANY_UNUSED_PORT).unwrap();
        let on_change_1 = on_change.clone();
        let on_change_2 = on_change.clone();
        on_change.with_changes(|changes| assert_eq!(changes, []));

        server.set_on_state_change(move |has_clients| on_change_1.on_state_change(has_clients));
        on_change.with_changes(|changes| assert_eq!(changes, [false]));

        server.unset_on_state_change();
        on_change.with_changes(|changes| assert_eq!(changes, [false]));

        server.set_on_state_change(move |has_clients| on_change_2.on_state_change(has_clients));
        on_change.with_changes(|changes| assert_eq!(changes, [false, false]));

        drop(server);
        assert_eq!(on_change.num_clones(), 1);
        on_change.with_changes(|changes| assert_eq!(changes, [false, false]));
    }

    #[test]
    fn test_on_state_change_before_connection() {
        let on_change = OnStateChangeMock::new();

        let mut server = Server::new(ANY_UNUSED_PORT).unwrap();
        let on_change_1 = on_change.clone();
        on_change.with_changes(|changes| assert_eq!(changes, []));

        server.set_on_state_change(move |has_clients| on_change_1.on_state_change(has_clients));
        on_change.with_changes(|changes| assert_eq!(changes, [false]));

        let connection = on_change.wait(false, || TcpStream::connect(server.local_addr()).unwrap());
        on_change.with_changes(|changes| assert_eq!(changes, [false, true]));

        on_change.wait(true, || drop(connection));
        on_change.with_changes(|changes| assert_eq!(changes, [false, true, false]));

        drop(server);
        assert_eq!(on_change.num_clones(), 1);
        on_change.with_changes(|changes| assert_eq!(changes, [false, true, false]));
    }

    #[test]
    fn test_on_state_change_after_connection() {
        let on_change = OnStateChangeMock::new();

        let mut server = Server::new(ANY_UNUSED_PORT).unwrap();
        let on_change_1 = on_change.clone();

        server.set_on_state_change(move |has_clients| on_change_1.on_state_change(has_clients));

        let connection = on_change.wait(false, || TcpStream::connect(server.local_addr()).unwrap());
        on_change.with_changes(|changes| assert_eq!(changes, [false, true]));

        server.unset_on_state_change();
        let on_change = OnStateChangeMock::new();
        let on_change_1 = on_change.clone();
        on_change.with_changes(|changes| assert_eq!(changes, []));

        server.set_on_state_change(move |has_clients| on_change_1.on_state_change(has_clients));
        on_change.with_changes(|changes| assert_eq!(changes, [true]));

        on_change.wait(true, || drop(connection));
        on_change.with_changes(|changes| assert_eq!(changes, [true, false]));

        drop(server);
        assert_eq!(on_change.num_clones(), 1);
        on_change.with_changes(|changes| assert_eq!(changes, [true, false]));
    }

    #[test]
    fn test_on_state_change_drop_server_before_connection() {
        let on_change = OnStateChangeMock::new();

        let mut server = Server::new(ANY_UNUSED_PORT).unwrap();
        let on_change_1 = on_change.clone();
        on_change.with_changes(|changes| assert_eq!(changes, []));

        server.set_on_state_change(move |has_clients| on_change_1.on_state_change(has_clients));
        on_change.with_changes(|changes| assert_eq!(changes, [false]));

        let connection = on_change.wait(false, || TcpStream::connect(server.local_addr()).unwrap());
        on_change.with_changes(|changes| assert_eq!(changes, [false, true]));

        drop(server);
        assert_eq!(on_change.num_clones(), 1);
        on_change.with_changes(|changes| assert_eq!(changes, [false, true, false]));

        drop(connection);
    }

    const ANY_UNUSED_PORT: &str = "127.0.0.1:0";

    #[derive(Clone)]
    struct OnStateChangeMock {
        changes: Arc<Mutex<Vec<bool>>>,
        barrier: Arc<Mutex<Option<Arc<Barrier>>>>,
    }

    impl OnStateChangeMock {
        fn new() -> Self {
            puffin::set_scopes_on(true);

            Self {
                changes: Arc::new(Mutex::new(Vec::new())),
                barrier: Arc::new(Mutex::new(None)),
            }
        }

        fn on_state_change(&self, has_clients: bool) {
            self.changes.lock().push(has_clients);

            if let Some(barrier) = self.barrier.lock().take() {
                barrier.wait();
            }
        }

        /// Wait for a callback call after executing `f()`.
        ///
        /// Optionally, submit dummy profiling data (if `with_dummy_frames == true`).
        fn wait<F, R>(&self, with_dummy_frames: bool, f: F) -> R
        where
            F: FnOnce() -> R,
        {
            let barrier = Arc::new(Barrier::new(2));
            *self.barrier.lock() = Some(barrier.clone());

            let produce_dummy_frames = AtomicBool::new(true);
            let result = thread::scope(|scope| {
                if with_dummy_frames {
                    scope.spawn(|| {
                        while produce_dummy_frames.load(Ordering::SeqCst) {
                            puffin::GlobalProfiler::lock().new_frame();
                            puffin::profile_scope!("dummy_frame");
                            thread::sleep(Duration::from_millis(10));
                        }
                    });
                }

                let result = f();
                barrier.wait();
                produce_dummy_frames.store(false, Ordering::SeqCst);
                result
            });

            assert!(self.barrier.lock().is_none());
            result
        }

        fn with_changes<F>(&self, f: F)
        where
            F: FnOnce(&[bool]),
        {
            f(&self.changes.lock());
        }

        fn num_clones(&self) -> usize {
            Arc::strong_count(&self.changes)
        }
    }
}
