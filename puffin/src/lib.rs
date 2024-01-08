//! Usage:
//!
//! ``` no_run
//! fn main() {
//!     puffin::set_scopes_on(true); // you may want to control this with a flag
//!
//!     // game loop
//!     loop {
//!         puffin::GlobalProfiler::lock().new_frame();
//!
//!         {
//!             puffin::profile_scope!("slow_code");
//!             slow_code();
//!         }
//!
//!     }
//! }
//!
//! # fn slow_code(){}
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod data;
mod frame_data;
mod merge;
mod profile_view;
mod scope_details;

pub use data::*;
pub use frame_data::{FrameData, FrameMeta, UnpackedFrameData};
pub use merge::*;
use once_cell::sync::Lazy;
pub use profile_view::{select_slowest, FrameView, GlobalFrameView};
pub use scope_details::{ScopeCollection, ScopeDetails, ScopeType};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

static MACROS_ON: AtomicBool = AtomicBool::new(false);

/// Turn on/off the profiler macros ([`profile_function`], [`profile_scope`] etc).
/// When off, these calls take only 1-2 ns to call (100x faster).
/// This is [`false`] by default.
pub fn set_scopes_on(on: bool) {
    MACROS_ON.store(on, Ordering::Relaxed);
}

/// Are the profiler scope macros turned on?
/// This is [`false`] by default.
pub fn are_scopes_on() -> bool {
    MACROS_ON.load(Ordering::Relaxed)
}

/// All times are expressed as integer nanoseconds since some event.
pub type NanoSecond = i64;

// ----------------------------------------------------------------------------

/// Stream of profiling events from one thread.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct Stream(Vec<u8>);

impl Stream {
    /// Returns if stream is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the length in bytes of this steam.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns the bytes of this steam
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    /// Clears the steam of all bytes.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Extends the stream with the given bytes.
    fn extend(&mut self, bytes: &[u8]) {
        self.0.extend(bytes);
    }
}

impl From<Vec<u8>> for Stream {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

// ----------------------------------------------------------------------------

/// Used when parsing a Stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeRecord<'s> {
    /// The start oif this scope in nanoseconds.
    pub start_ns: NanoSecond,
    /// The duration of this scope in nanoseconds.
    pub duration_ns: NanoSecond,
    /// e.g. function argument, like a mesh name. Optional.
    /// Example: "image.png".
    pub data: &'s str,
}

impl<'s> ScopeRecord<'s> {
    /// The end of this scope in nanoseconds.
    #[inline]
    pub fn stop_ns(&self) -> NanoSecond {
        self.start_ns + self.duration_ns
    }
}

/// Used when parsing a Stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope<'s> {
    /// Unique identifier for the profile scope.
    /// More detailed scope information can be requested via [`FrameView::scope_collection()`].
    pub id: ScopeId,
    /// Some dynamic data that is passed into the profiler scope.
    pub record: ScopeRecord<'s>,
    /// Stream offset for first child.
    pub child_begin_position: u64,
    /// Stream offset after last child.
    pub child_end_position: u64,
    /// Stream offset for next sibling (if any).
    pub next_sibling_position: u64,
}

/// Used to identify one source of profiling data.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct ThreadInfo {
    /// Useful for ordering threads.
    pub start_time_ns: Option<NanoSecond>,
    /// Name of the thread
    pub name: String,
}

// ----------------------------------------------------------------------------

/// An incremental monolithic counter to identify frames.
pub type FrameIndex = u64;

/// A [`Stream`] plus some info about it.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct StreamInfo {
    /// The raw profile data.
    pub stream: Stream,

    /// Total number of scopes in the stream.
    pub num_scopes: usize,

    /// The depth of the deepest scope.
    /// `0` mean no scopes, `1` some scopes without children, etc.
    pub depth: usize,

    /// The smallest and largest nanosecond value in the stream.
    ///
    /// The default value is ([`NanoSecond::MAX`], [`NanoSecond::MIN`]) which indicates an empty stream.
    pub range_ns: (NanoSecond, NanoSecond),
}

impl Default for StreamInfo {
    fn default() -> Self {
        Self {
            stream: Default::default(),
            num_scopes: 0,
            depth: 0,
            range_ns: (NanoSecond::MAX, NanoSecond::MIN),
        }
    }
}

impl StreamInfo {
    /// Parse a stream to count the depth, number of scopes in it etc.
    ///
    /// Try to avoid calling this, and instead keep score while collecting a [`StreamInfo`].
    pub fn parse(stream: Stream) -> Result<StreamInfo> {
        let top_scopes = Reader::from_start(&stream).read_top_scopes()?;
        if top_scopes.is_empty() {
            Ok(StreamInfo {
                stream,
                num_scopes: 0,
                depth: 0,
                range_ns: (NanoSecond::MAX, NanoSecond::MIN),
            })
        } else {
            let (num_scopes, depth) = Reader::count_scope_and_depth(&stream)?;
            let min_ns = top_scopes.first().unwrap().record.start_ns;
            let max_ns = top_scopes.last().unwrap().record.stop_ns();

            Ok(StreamInfo {
                stream,
                num_scopes,
                depth,
                range_ns: (min_ns, max_ns),
            })
        }
    }

    /// Extends this [`StreamInfo`] with another [`StreamInfo`].
    pub fn extend(&mut self, other: &StreamInfoRef<'_>) {
        self.stream.extend(other.stream);
        self.num_scopes += other.num_scopes;
        self.depth = self.depth.max(other.depth);
        self.range_ns.0 = self.range_ns.0.min(other.range_ns.0);
        self.range_ns.1 = self.range_ns.1.max(other.range_ns.1);
    }

    /// Clears the contents of this [`StreamInfo`].
    pub fn clear(&mut self) {
        let Self {
            stream,
            num_scopes,
            depth,
            range_ns,
        } = self;
        stream.clear();
        *num_scopes = 0;
        *depth = 0;
        *range_ns = (NanoSecond::MAX, NanoSecond::MIN);
    }

    /// Returns a reference to the contents of this [`StreamInfo`].
    pub fn as_stream_into_ref(&self) -> StreamInfoRef<'_> {
        StreamInfoRef {
            stream: self.stream.bytes(),
            num_scopes: self.num_scopes,
            depth: self.depth,
            range_ns: self.range_ns,
        }
    }
}

/// A reference to the contents of a [`StreamInfo`].
#[derive(Clone, Copy)]
pub struct StreamInfoRef<'a> {
    /// The raw profile data.
    pub stream: &'a [u8],

    /// Total number of scopes in the stream.
    pub num_scopes: usize,

    /// The depth of the deepest scope.
    /// `0` mean no scopes, `1` some scopes without children, etc.
    pub depth: usize,

    /// The smallest and largest nanosecond value in the stream.
    ///
    /// The default value is ([`NanoSecond::MAX`], [`NanoSecond::MIN`]) which indicates an empty stream.
    pub range_ns: (NanoSecond, NanoSecond),
}

// ----------------------------------------------------------------------------

type NsSource = fn() -> NanoSecond;

// Function interface for reporting thread local scope details.
// The scope details array will contain information about a scope the first time it is seen.
// The stream will always contain the scope timing details.
type ThreadReporter = fn(ThreadInfo, &[ScopeDetails], &StreamInfoRef<'_>);

/// Report a stream of profile data from a thread to the [`GlobalProfiler`] singleton.
/// This is used for internal purposes only
pub(crate) fn internal_profile_reporter(
    info: ThreadInfo,
    scope_details: &[ScopeDetails],
    stream_scope_times: &StreamInfoRef<'_>,
) {
    GlobalProfiler::lock().report(info, scope_details, stream_scope_times);
}
/// Collects profiling data for one thread
pub struct ThreadProfiler {
    stream_info: StreamInfo,
    scope_details: Vec<ScopeDetails>,
    /// Current depth.
    depth: usize,
    now_ns: NsSource,
    reporter: ThreadReporter,
    start_time_ns: Option<NanoSecond>,
}

impl Default for ThreadProfiler {
    fn default() -> Self {
        Self {
            stream_info: Default::default(),
            scope_details: Default::default(),
            depth: 0,
            now_ns: crate::now_ns,
            reporter: internal_profile_reporter,
            start_time_ns: None,
        }
    }
}

impl ThreadProfiler {
    /// Explicit initialize with custom callbacks.
    ///
    /// If not called, each thread will use the default nanosecond source ([`now_ns()`])
    /// and report scopes to the global profiler ([`internal_profile_reporter()`]).
    ///
    /// For instance, when compiling for WASM the default timing function ([`now_ns()`]) won't work,
    /// so you'll want to call `puffin::ThreadProfiler::initialize(my_timing_function, internal_profile_reporter);`.
    pub fn initialize(now_ns: NsSource, reporter: ThreadReporter) {
        ThreadProfiler::call(|tp| {
            tp.now_ns = now_ns;
            tp.reporter = reporter;
        });
    }

    /// Register a function scope.
    #[must_use]
    pub fn register_function_scope(
        &mut self,
        function_name: impl Into<Cow<'static, str>>,
        file_path: impl Into<Cow<'static, str>>,
        line_nr: u32,
    ) -> ScopeId {
        let new_id = fetch_add_scope_id();
        self.scope_details.push(
            ScopeDetails::from_scope_id(new_id)
                .with_function_name(function_name)
                .with_file(file_path)
                .with_line_nr(line_nr),
        );
        new_id
    }

    /// Register a named scope.
    #[must_use]
    pub fn register_named_scope(
        &mut self,
        scope_name: impl Into<Cow<'static, str>>,
        function_name: impl Into<Cow<'static, str>>,
        file_path: impl Into<Cow<'static, str>>,
        line_nr: u32,
    ) -> ScopeId {
        let new_id = fetch_add_scope_id();
        self.scope_details.push(
            ScopeDetails::from_scope_id(new_id)
                .with_scope_name(scope_name)
                .with_function_name(function_name)
                .with_file(file_path)
                .with_line_nr(line_nr),
        );
        new_id
    }

    /// Marks the beginning of the scope.
    /// Returns position where to write scope size once the scope is closed.
    #[must_use]
    pub fn begin_scope(&mut self, scope_id: ScopeId, data: &str) -> usize {
        let now_ns = (self.now_ns)();
        self.start_time_ns = Some(self.start_time_ns.unwrap_or(now_ns));

        self.depth += 1;

        self.stream_info.range_ns.0 = self.stream_info.range_ns.0.min(now_ns);
        self.stream_info.stream.begin_scope(now_ns, scope_id, data)
    }

    /// Marks the end of the scope.
    /// Returns the current depth.
    pub fn end_scope(&mut self, start_offset: usize) {
        let now_ns = (self.now_ns)();
        self.stream_info.depth = self.stream_info.depth.max(self.depth);
        self.stream_info.num_scopes += 1;
        self.stream_info.range_ns.1 = self.stream_info.range_ns.1.max(now_ns);

        if self.depth > 0 {
            self.depth -= 1;
        } else {
            eprintln!("puffin ERROR: Mismatched scope begin/end calls");
        }

        self.stream_info.stream.end_scope(start_offset, now_ns);

        if self.depth == 0 {
            // We have no open scopes.
            // This is a good time to report our profiling stream to the global profiler:
            let info = ThreadInfo {
                start_time_ns: self.start_time_ns,
                name: std::thread::current().name().unwrap_or_default().to_owned(),
            };
            (self.reporter)(
                info,
                &self.scope_details,
                &self.stream_info.as_stream_into_ref(),
            );

            self.scope_details.clear();
            self.stream_info.clear();
        }
    }

    /// Do something with the thread local [`ThreadProfiler`]
    #[inline]
    pub fn call<R>(f: impl Fn(&mut Self) -> R) -> R {
        thread_local! {
            pub static THREAD_PROFILER: std::cell::RefCell<ThreadProfiler> = Default::default();
        }
        THREAD_PROFILER.with(|p| f(&mut p.borrow_mut()))
    }
}

/// Incremental monolithic counter to identify scopes.
static SCOPE_ID_TRACKER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn fetch_add_scope_id() -> ScopeId {
    let new_id = SCOPE_ID_TRACKER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ScopeId(
        NonZeroU32::new(new_id)
            .expect("safe because integer is retrieved from fetch-add atomic operation"),
    )
}

// ----------------------------------------------------------------------------

/// Add these to [`GlobalProfiler`] with [`GlobalProfiler::add_sink()`].
pub type FrameSink = Box<dyn Fn(Arc<FrameData>) + Send>;

/// Identifies a specific [`FrameSink`] when added to [`GlobalProfiler`].
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct FrameSinkId(u64);

/// Singleton. Collects profiling data from multiple threads
/// and passes them on to different [`FrameSink`]s.
pub struct GlobalProfiler {
    current_frame_index: FrameIndex,
    current_frame: BTreeMap<ThreadInfo, StreamInfo>,

    next_sink_id: FrameSinkId,
    sinks: std::collections::HashMap<FrameSinkId, FrameSink>,
    // When true will propagate a full snapshot from `scope_collection` to every sink.
    propagate_all_scope_details: bool,
    // The new scopes' details, or also the first time macro or external library detected a scope.
    new_scopes: Vec<Arc<ScopeDetails>>,
    // Store an absolute collection of scope details such that sinks can request a total state by setting `propagate_all_scope_details`.
    // This should not be mutable accessible to external applications as frame views store there own copy.
    scope_collection: ScopeCollection,
}

impl Default for GlobalProfiler {
    fn default() -> Self {
        Self {
            current_frame_index: 0,
            current_frame: Default::default(),
            next_sink_id: FrameSinkId(1),
            sinks: Default::default(),
            propagate_all_scope_details: Default::default(),
            new_scopes: Default::default(),
            scope_collection: Default::default(),
        }
    }
}

impl GlobalProfiler {
    /// Access to the global profiler singleton.
    pub fn lock() -> parking_lot::MutexGuard<'static, Self> {
        static GLOBAL_PROFILER: Lazy<parking_lot::Mutex<GlobalProfiler>> =
            Lazy::new(Default::default);
        GLOBAL_PROFILER.lock()
    }

    /// You need to call this once at the start of every frame.
    ///
    /// It is fine to call this from within a profile scope.
    ///
    /// This takes all completed profiling scopes from all threads,
    /// and sends it to the sinks.
    pub fn new_frame(&mut self) {
        let current_frame_index = self.current_frame_index;
        self.current_frame_index += 1;

        let mut scope_deltas = Vec::with_capacity(self.new_scopes.len());

        // Firstly add the new registered scopes.
        for scope_detail in self.new_scopes.drain(..) {
            scope_deltas.push(scope_detail);
        }

        let current_frame_scope = std::mem::take(&mut self.current_frame);

        // Secondly add a full snapshot of all scopes if requested.
        // Could potentially do this per sink.
        let propagate_full_delta = std::mem::take(&mut self.propagate_all_scope_details);

        if propagate_full_delta {
            scope_deltas.extend(self.scope_collection.scopes_by_id().values().cloned());
        }

        let new_frame = match FrameData::new(
            current_frame_index,
            current_frame_scope,
            scope_deltas,
            propagate_full_delta,
        ) {
            Ok(new_frame) => Arc::new(new_frame),
            Err(Error::Empty) => {
                return; // don't warn about empty frames, just ignore them
            }
            Err(err) => {
                eprintln!("puffin ERROR: Bad frame: {err:?}");
                return;
            }
        };

        self.add_frame(new_frame);
    }

    /// Manually add frame data.
    pub fn add_frame(&mut self, new_frame: Arc<FrameData>) {
        for delta in &new_frame.scope_delta {
            self.scope_collection.insert(delta.clone());
        }

        for sink in self.sinks.values() {
            sink(new_frame.clone());
        }
    }

    /// Inserts user scopes into puffin.
    /// Returns the scope id for every inserted scope in the same order as input slice.
    ///
    /// Scopes details should only be registered once for each scope and need be inserted before being reported to puffin.
    /// This function is relevant when you're registering measurement not performed using the puffin profiler macros.
    /// Scope id is always supposed to be `None` as it will be set by puffin.
    pub fn register_user_scopes(&mut self, scopes: &[ScopeDetails]) -> Vec<ScopeId> {
        let mut new_scopes = Vec::with_capacity(scopes.len());
        for scope_detail in scopes {
            let new_scope_id = fetch_add_scope_id();
            let scope = self.scope_collection.insert(Arc::new(
                (*scope_detail).clone().with_scope_id(new_scope_id),
            ));
            new_scopes.push(scope);
        }
        let new_scope_ids = new_scopes.iter().filter_map(|x| x.scope_id).collect();
        self.new_scopes.extend(new_scopes);
        new_scope_ids
    }

    /// Reports some profiling data. Called from [`ThreadProfiler`].
    pub(crate) fn report(
        &mut self,
        info: ThreadInfo,
        scope_details: &[ScopeDetails],
        stream_scope_times: &StreamInfoRef<'_>,
    ) {
        if !scope_details.is_empty() {
            // Here we can run slightly heavy logic as its only ran once for each scope.
            self.new_scopes.extend(
                scope_details
                    .iter()
                    .map(|x| Arc::new(x.clone().into_readable().clone())),
            );
        }

        self.current_frame
            .entry(info)
            .or_default()
            .extend(stream_scope_times);
    }

    /// Reports user scopes to puffin profiler.
    /// Every scope reported should first be registered by [`Self::register_user_scopes`].
    pub fn report_user_scopes(&mut self, info: ThreadInfo, stream_scope_times: &StreamInfoRef<'_>) {
        self.current_frame
            .entry(info)
            .or_default()
            .extend(stream_scope_times);
    }

    /// Tells [`GlobalProfiler`] to call this function with each new finished frame.
    ///
    /// The returned [`FrameSinkId`] can be used to remove the sink with [`Self::remove_sink()`].
    /// If the sink is registered later in the application make sure to call [`Self::emit_scope_snapshot()`] to send a snapshot of all scopes.
    pub fn add_sink(&mut self, sink: FrameSink) -> FrameSinkId {
        let id = self.next_sink_id;
        self.next_sink_id.0 += 1;
        self.sinks.insert(id, sink);
        id
    }

    /// Removes a sink from the global profiler.
    pub fn remove_sink(&mut self, id: FrameSinkId) -> Option<FrameSink> {
        self.sinks.remove(&id)
    }

    /// Sends a snapshot of all scopes to all sinks via the frame data.
    /// This is useful for if a sink is initialized after scopes are registered.
    pub fn emit_scope_snapshot(&mut self) {
        self.propagate_all_scope_details = true;
    }
}

// ----------------------------------------------------------------------------

/// Returns a high-precision, monotonically increasing nanosecond count since unix epoch.
#[inline]
#[cfg(any(not(target_arch = "wasm32"), feature = "web"))]
pub fn now_ns() -> NanoSecond {
    #[cfg(target_arch = "wasm32")]
    fn nanos_since_epoch() -> NanoSecond {
        (js_sys::Date::new_0().get_time() * 1e6) as _
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn nanos_since_epoch() -> NanoSecond {
        if let Ok(duration_since_epoch) = std::time::UNIX_EPOCH.elapsed() {
            duration_since_epoch.as_nanos() as NanoSecond
        } else {
            0 // system time is set before 1970. this should be quite rare.
        }
    }

    // This can maybe be optimized

    #[cfg(not(target_arch = "wasm32"))]
    use std::time::Instant;
    #[cfg(target_arch = "wasm32")]
    use web_time::Instant;

    static START_TIME: Lazy<(NanoSecond, Instant)> =
        Lazy::new(|| (nanos_since_epoch(), Instant::now()));
    START_TIME.0 + START_TIME.1.elapsed().as_nanos() as NanoSecond
}

#[inline]
#[cfg(all(target_arch = "wasm32", not(feature = "web")))]
pub fn now_ns() -> NanoSecond {
    // This should be unused.
    panic!("Wasm without the `web` feature requires passing a custom source of time via `ThreadProfiler::initialize`");
}

// ----------------------------------------------------------------------------

// We currently store an Option<ProfilerScope> on the stack (None when profiling is off).
// This currently takes up 16 bytes of stack space. TODO: get this down to 4 bytes.
/// Created by the `puffin::profile*!(...)` macros.
pub struct ProfilerScope {
    start_stream_offset: usize,

    /// Prevent the scope from being sent between threads.
    /// The scope must start/stop on the same thread.
    /// In particular, we do NOT want this to migrate threads in some async code.
    /// Workaround until `impl !Send for ProfilerScope {}` is stable.
    _dont_send_me: std::marker::PhantomData<*const ()>,
}

impl ProfilerScope {
    /// The scope id identifies which scopes' time is being reported.
    /// `data` can be changing, i.e. a name of a mesh or a texture.
    #[inline]
    pub fn new(scope_id: ScopeId, data: impl AsRef<str>) -> Self {
        Self {
            start_stream_offset: ThreadProfiler::call(|tp| tp.begin_scope(scope_id, data.as_ref())),
            _dont_send_me: Default::default(),
        }
    }
}

impl Drop for ProfilerScope {
    #[inline]
    fn drop(&mut self) {
        ThreadProfiler::call(|tp| tp.end_scope(self.start_stream_offset));
    }
}

/// A unique id for each scope and [`ScopeDetails`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct ScopeId(pub NonZeroU32);

impl ScopeId {
    #[cfg(test)]
    pub(crate) fn new(id: u32) -> Self {
        ScopeId(NonZeroU32::new(id).expect("Scope id was not non-zero u32"))
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn type_name_of<T>(_: T) -> &'static str {
    std::any::type_name::<T>()
}

/// Returns the name of the calling function without a long module path prefix.
#[macro_export]
macro_rules! current_function_name {
    () => {{
        fn f() {}
        $crate::type_name_of(f)
    }};
}

#[doc(hidden)]
#[inline(never)]
pub fn clean_function_name(name: &str) -> String {
    let Some(name) = name.strip_suffix(USELESS_SCOPE_NAME_SUFFIX) else {
        // Probably the user registered a user scope name.
        return name.to_owned();
    };

    // "foo::bar::baz" -> "baz"
    fn last_part(name: &str) -> &str {
        if let Some(colon) = name.rfind("::") {
            &name[colon + 2..]
        } else {
            name
        }
    }

    // look for:  <some::ConcreteType as some::Trait>::function_name
    if let Some(end_caret) = name.rfind('>') {
        if let Some(trait_as) = name.rfind(" as ") {
            if trait_as < end_caret {
                let concrete_name = if let Some(start_caret) = name[..trait_as].rfind('<') {
                    &name[start_caret + 1..trait_as]
                } else {
                    name
                };

                let trait_name = &name[trait_as + 4..end_caret];

                let concrete_name = last_part(concrete_name);
                let trait_name = last_part(trait_name);

                let dubcolon_function_name = &name[end_caret + 1..];
                return format!("<{concrete_name} as {trait_name}>{dubcolon_function_name}");
            }
        }
    }

    if let Some(colon) = name.rfind("::") {
        if let Some(colon) = name[..colon].rfind("::") {
            // "foo::bar::baz::function_name" -> "baz::function_name"
            name[colon + 2..].to_owned()
        } else {
            // "foo::function_name" -> "foo::function_name"
            name.to_owned()
        }
    } else {
        name.to_owned()
    }
}

#[test]
fn test_clean_function_name() {
    assert_eq!(clean_function_name(""), "");
    assert_eq!(
        clean_function_name(&format!("foo{}", USELESS_SCOPE_NAME_SUFFIX)),
        "foo"
    );
    assert_eq!(
        clean_function_name(&format!("foo::bar{}", USELESS_SCOPE_NAME_SUFFIX)),
        "foo::bar"
    );
    assert_eq!(
        clean_function_name(&format!("foo::bar::baz{}", USELESS_SCOPE_NAME_SUFFIX)),
        "bar::baz"
    );
    assert_eq!(
        clean_function_name(&format!(
            "some::GenericThing<_, _>::function_name{}",
            USELESS_SCOPE_NAME_SUFFIX
        )),
        "GenericThing<_, _>::function_name"
    );
    assert_eq!(
        clean_function_name(&format!(
            "<some::ConcreteType as some::bloody::Trait>::function_name{}",
            USELESS_SCOPE_NAME_SUFFIX
        )),
        "<ConcreteType as Trait>::function_name"
    );
}

/// Shortens a long `file!()` path to the essentials.
///
/// We want to keep it short for two reasons: readability, and bandwidth
#[doc(hidden)]
#[inline(never)]
pub fn short_file_name(path: &str) -> String {
    if path.is_empty() {
        return "".to_string();
    }

    let path = path.replace('\\', "/"); // Handle Windows
    let components: Vec<&str> = path.split('/').collect();
    if components.len() <= 2 {
        return path;
    }

    // Look for `src` folder:

    let mut src_idx = None;
    for (i, c) in components.iter().enumerate() {
        if *c == "src" {
            src_idx = Some(i);
        }
    }

    if let Some(src_idx) = src_idx {
        // Before `src` comes the name of the crate - let's include that:
        let crate_index = src_idx.saturating_sub(1);
        let file_index = components.len() - 1;

        if crate_index + 2 == file_index {
            // Probably "crate/src/lib.rs" - include it all
            format!(
                "{}/{}/{}",
                components[crate_index],
                components[crate_index + 1],
                components[file_index]
            )
        } else if components[file_index] == "lib.rs" {
            // "lib.rs" is very unhelpful - include folder name:
            let folder_index = file_index - 1;

            if crate_index + 1 == folder_index {
                format!(
                    "{}/{}/{}",
                    components[crate_index], components[folder_index], components[file_index]
                )
            } else {
                // Ellide for brevity:
                format!(
                    "{}/…/{}/{}",
                    components[crate_index], components[folder_index], components[file_index]
                )
            }
        } else {
            // Ellide for brevity:
            format!("{}/…/{}", components[crate_index], components[file_index])
        }
    } else {
        // No `src` directory found - could be an example (`examples/hello_world.rs`).
        // Include the folder and file name.
        let n = components.len();
        // NOTE: we've already checked that n > 1 easily in the function
        format!("{}/{}", components[n - 2], components[n - 1])
    }
}

#[test]
fn test_short_file_name() {
    for (before, after) in [
        ("", ""),
        ("foo.rs", "foo.rs"),
        ("foo/bar.rs", "foo/bar.rs"),
        ("foo/bar/baz.rs", "bar/baz.rs"),
        ("crates/cratename/src/main.rs", "cratename/src/main.rs"),
        ("crates/cratename/src/module/lib.rs", "cratename/…/module/lib.rs"),
        ("workspace/cratename/examples/hello_world.rs", "examples/hello_world.rs"),
        ("/rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/ops/function.rs", "core/…/function.rs"),
        ("/Users/emilk/.cargo/registry/src/github.com-1ecc6299db9ec823/tokio-1.24.1/src/runtime/runtime.rs", "tokio-1.24.1/…/runtime.rs"),
        ]
        {
        assert_eq!(short_file_name(before), after);
    }
}

#[test]
fn profile_macros_test() {
    set_scopes_on(true);

    let frame_view = GlobalFrameView::default();

    GlobalProfiler::lock().add_sink(Box::new(|data| {
        if data.frame_index() == 0 {
            assert_eq!(data.frame_index(), 0);
            assert_eq!(data.meta().num_scopes, 2);
            assert_eq!(data.meta().num_bytes, 62);
        } else if data.frame_index() == 1 {
            assert_eq!(data.frame_index(), 1);
            assert_eq!(data.meta().num_scopes, 2);
            assert_eq!(data.meta().num_bytes, 62);
        } else {
            panic!("Only two frames in this test");
        }
    }));

    let line_nr_fn = line!() + 3;
    let line_nr_scope = line!() + 4;
    fn a() {
        profile_function!();
        {
            profile_scope!("my-scope");
        }
    }

    a();

    // First frame
    GlobalProfiler::lock().new_frame();

    let lock = frame_view.lock();
    let scope_details = lock
        .scope_collection()
        .fetch_by_id(&ScopeId::new(1))
        .unwrap();
    assert_eq!(scope_details.file_path, "puffin/src/lib.rs");
    assert_eq!(scope_details.function_name, "profile_macros_test::a");
    assert_eq!(scope_details.line_nr, line_nr_fn);

    let scope_details = lock
        .scope_collection()
        .fetch_by_id(&ScopeId::new(2))
        .unwrap();

    assert_eq!(scope_details.file_path, "puffin/src/lib.rs");
    assert_eq!(scope_details.function_name, "profile_macros_test::a");
    assert_eq!(scope_details.scope_name, Some(Cow::Borrowed("my-scope")));
    assert_eq!(scope_details.line_nr, line_nr_scope);

    let scope_details = lock.scope_collection().fetch_by_name("my-scope").unwrap();
    assert_eq!(scope_details, &ScopeId::new(2));

    drop(lock);

    // Second frame
    a();

    GlobalProfiler::lock().new_frame();
}

// The macro defines 'f()' at the place where macro is called.
// This code is located at the place of call and two closures deep.
// Strip away this useless suffix.
const USELESS_SCOPE_NAME_SUFFIX: &str = "::{{closure}}::{{closure}}::f";

#[allow(clippy::doc_markdown)] // clippy wants to put "MacBook" in ticks 🙄
/// Automatically name the profiling scope based on function name.
///
/// Names should be descriptive, ASCII and without spaces.
///
/// Example:
/// ```
/// # struct Image {};
/// fn load_image(path: &str) -> Image {
///     puffin::profile_function!();
///     /* … */
///     # let image = Image {};
///     image
/// }
/// ```
///
/// An optional argument can be a string describing e.g. an argument, to help diagnose what was slow.
///
/// ```
/// # struct Image {};
/// fn load_image(path: &str) -> Image {
///     puffin::profile_function!(path);
///     /* … */
///     # let image = Image {};
///     image
/// }
/// ```
///
/// Overhead: around 54 ns on Macbook Pro with Apple M1 Max.
#[macro_export]
macro_rules! profile_function {
    () => {
        $crate::profile_function!("");
    };
    ($data:expr) => {
        let _profiler_scope = if $crate::are_scopes_on() {
            static SCOPE_ID: std::sync::OnceLock<$crate::ScopeId> = std::sync::OnceLock::new();
            let scope_id = SCOPE_ID.get_or_init(|| {
                $crate::ThreadProfiler::call(|tp| {
                    let id = tp.register_function_scope(
                        $crate::current_function_name!(),
                        file!(),
                        line!(),
                    );
                    id
                })
            });

            Some($crate::ProfilerScope::new(*scope_id, $data))
        } else {
            None
        };
    };
}

#[allow(clippy::doc_markdown)] // clippy wants to put "MacBook" in ticks 🙄
/// Profile the current scope with the given name (unique in the parent scope).
///
/// Names should be descriptive, ASCII and without spaces.
///
/// Example: `profile_scope!("load_mesh");`.
///
/// An optional second argument can be a string (e.g. a mesh name) to help diagnose what was slow.
/// Example: `profile_scope!("load_mesh", mesh_name);`
///
/// Overhead: around 54 ns on Macbook Pro with Apple M1 Max.
#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        $crate::profile_scope!($name, "");
    };
    ($name:expr, $data:expr) => {
        let _profiler_scope = if $crate::are_scopes_on() {
            static SCOPE_ID: std::sync::OnceLock<$crate::ScopeId> = std::sync::OnceLock::new();
            let scope_id = SCOPE_ID.get_or_init(|| {
                $crate::ThreadProfiler::call(|tp| {
                    let id = tp.register_named_scope(
                        $name,
                        $crate::current_function_name!(),
                        file!(),
                        line!(),
                    );
                    id
                })
            });
            Some($crate::ProfilerScope::new(*scope_id, $data))
        } else {
            None
        };
    };
}
