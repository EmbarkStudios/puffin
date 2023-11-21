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

// BEGIN - Embark standard lints v5 for Rust 1.55+
// do not change or add/remove here, but one can add exceptions after this section
// for more info see: <https://github.com/EmbarkStudios/rust-ecosystem/issues/59>
#![deny(unsafe_code)]
#![warn(
    clippy::all,
    clippy::await_holding_lock,
    clippy::char_lit_as_u8,
    clippy::checked_conversions,
    clippy::dbg_macro,
    clippy::debug_assert_with_mut_call,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::doc_markdown,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::exit,
    clippy::expl_impl_clone_on_copy,
    clippy::explicit_deref_methods,
    clippy::explicit_into_iter_loop,
    clippy::fallible_impl_from,
    clippy::filter_map_next,
    clippy::flat_map_option,
    clippy::float_cmp_const,
    clippy::fn_params_excessive_bools,
    clippy::from_iter_instead_of_collect,
    clippy::if_let_mutex,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::inefficient_to_string,
    clippy::invalid_upcast_comparisons,
    clippy::large_digit_groups,
    clippy::large_stack_arrays,
    clippy::large_types_passed_by_value,
    clippy::let_unit_value,
    clippy::linkedlist,
    clippy::lossy_float_literal,
    clippy::macro_use_imports,
    clippy::manual_ok_or,
    clippy::map_err_ignore,
    clippy::map_flatten,
    clippy::map_unwrap_or,
    clippy::match_on_vec_items,
    clippy::match_same_arms,
    clippy::match_wild_err_arm,
    clippy::match_wildcard_for_single_variants,
    clippy::mem_forget,
    clippy::mismatched_target_os,
    clippy::missing_enforced_import_renames,
    clippy::mut_mut,
    clippy::mutex_integer,
    clippy::needless_borrow,
    clippy::needless_continue,
    clippy::needless_for_each,
    clippy::option_option,
    clippy::path_buf_push_overwrite,
    clippy::ptr_as_ptr,
    clippy::rc_mutex,
    clippy::ref_option_ref,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::same_functions_in_if_condition,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else,
    clippy::string_add_assign,
    clippy::string_add,
    clippy::string_lit_as_bytes,
    clippy::string_to_string,
    clippy::todo,
    clippy::trait_duplication_in_bounds,
    clippy::unimplemented,
    clippy::unnested_or_patterns,
    clippy::unused_self,
    clippy::useless_transmute,
    clippy::verbose_file_reads,
    clippy::zero_sized_map_values,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms
)]
// END - Embark standard lints v0.5 for Rust 1.55+
// crate-specific exceptions:

mod data;
mod frame_data;
mod merge;
mod profile_view;

pub use data::*;
pub use frame_data::{FrameData, FrameMeta, UnpackedFrameData};
pub use merge::*;
use parking_lot::RwLock;
pub use profile_view::{select_slowest, FrameView, GlobalFrameView};

use std::collections::BTreeMap;
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
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    fn extend(&mut self, bytes: &[u8]) {
        self.0.extend(bytes);
    }
}

impl From<Vec<u8>> for Stream {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl<'a> From<&'a [u8]> for Stream {
    fn from(v: &'a [u8]) -> Self {
        let mut bytes = Vec::with_capacity(v.len());
        bytes.extend_from_slice(v);
        Self(bytes)
    }
}

// ----------------------------------------------------------------------------

/// Used when parsing a Stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeDynamicData<'s> {
    pub start_ns: NanoSecond,
    pub duration_ns: NanoSecond,
    pub data: &'s str,
}

impl<'s> ScopeDynamicData<'s> {
    #[inline]
    pub fn stop_ns(&self) -> NanoSecond {
        self.start_ns + self.duration_ns
    }
}

/// Used when parsing a Stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope<'s> {
    // Identifier for the scope
    pub scope_id: ScopeId,
    // Some dynamic data
    pub scope_dynamic_data: ScopeDynamicData<'s>,
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
            let min_ns = top_scopes.first().unwrap().scope_dynamic_data.start_ns;
            let max_ns = top_scopes.last().unwrap().scope_dynamic_data.stop_ns();

            Ok(StreamInfo {
                stream,
                num_scopes,
                depth,
                range_ns: (min_ns, max_ns),
            })
        }
    }

    pub fn extend(&mut self, other: &StreamInfoRef<'_>) {
        self.stream.extend(other.stream);
        self.num_scopes += other.num_scopes;
        self.depth = self.depth.max(other.depth);
        self.range_ns.0 = self.range_ns.0.min(other.range_ns.0);
        self.range_ns.1 = self.range_ns.1.max(other.range_ns.1);
    }

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

// If there are new scopes the first stream will contain scope details like file name, function name, line nmr etc...
// The second stream always contains the scope time information.
type ThreadReporter = fn(ThreadInfo, &[u8], &StreamInfoRef<'_>);

/// Report a stream of profile data from a thread to the [`GlobalProfiler`] singleton.
pub fn global_reporter(
    info: ThreadInfo,
    stream_scope_details: &[u8],
    stream_scope_times: &StreamInfoRef<'_>,
) {
    GlobalProfiler::lock().report(info, stream_scope_details, stream_scope_times);
}

/// Collects profiling data for one thread
pub struct ThreadProfiler {
    stream_info: StreamInfo,
    scope_details_raw: Stream,
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
            scope_details_raw: Default::default(),
            depth: 0,
            now_ns: crate::now_ns,
            reporter: global_reporter,
            start_time_ns: None,
        }
    }
}

impl ThreadProfiler {
    /// Explicit initialize with custom callbacks.
    ///
    /// If not called, each thread will use the default nanosecond source ([`now_ns()`])
    /// and report scopes to the global profiler ([`global_reporter()`]).
    ///
    /// For instance, when compiling for WASM the default timing function ([`now_ns()`]) won't work,
    /// so you'll want to call `puffin::ThreadProfiler::initialize(my_timing_function, puffin::global_reporter);`.
    pub fn initialize(now_ns: NsSource, reporter: ThreadReporter) {
        ThreadProfiler::call(|tp| {
            tp.now_ns = now_ns;
            tp.reporter = reporter;
        });
    }

    #[must_use]
    pub fn send_scope_info(
        &mut self,
        raw_scope_name: &str,
        file_name: &str,
        line_nr: u32,
    ) -> ScopeId {
        let new_id = fetch_add_scope_id();
        self.scope_details_raw
            .write_scope_info(new_id, raw_scope_name, file_name, line_nr);
        new_id
    }

    /// Returns position where to write scope size once the scope is closed.
    #[must_use]
    pub fn begin_scope(&mut self, scope_id: ScopeId, data: &str) -> usize {
        let now_ns = (self.now_ns)();
        self.start_time_ns = Some(self.start_time_ns.unwrap_or(now_ns));

        self.depth += 1;

        self.stream_info.range_ns.0 = self.stream_info.range_ns.0.min(now_ns);
        self.stream_info.stream.begin_scope(now_ns, scope_id, data)
    }

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
                self.scope_details_raw.bytes(),
                &self.stream_info.as_stream_into_ref(),
            );

            self.scope_details_raw.clear();
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
static SCOPE_ID_TRACKER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn fetch_add_scope_id() -> ScopeId {
    let new_id = SCOPE_ID_TRACKER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    ScopeId(new_id)
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
    current_stream_scope_times: BTreeMap<ThreadInfo, StreamInfo>,
    current_stream_scope_details: Vec<Stream>,

    next_sink_id: FrameSinkId,
    sinks: std::collections::HashMap<FrameSinkId, FrameSink>,
    // Contains detailed information of every registered scope.
    // This data structure can be cloned for fast read access.
    scope_details: ScopeDetails,
}

impl Default for GlobalProfiler {
    fn default() -> Self {
        Self {
            current_frame_index: 0,
            current_stream_scope_times: Default::default(),
            current_stream_scope_details: Default::default(),
            next_sink_id: FrameSinkId(1),
            sinks: Default::default(),
            scope_details: Default::default(),
        }
    }
}

impl GlobalProfiler {
    /// Access to the global profiler singleton.
    #[cfg(feature = "parking_lot")]
    pub fn lock() -> parking_lot::MutexGuard<'static, Self> {
        use once_cell::sync::Lazy;
        static GLOBAL_PROFILER: Lazy<parking_lot::Mutex<GlobalProfiler>> =
            Lazy::new(Default::default);
        GLOBAL_PROFILER.lock()
    }

    /// Access to the global profiler singleton.
    #[cfg(not(feature = "parking_lot"))]
    pub fn lock() -> std::sync::MutexGuard<'static, Self> {
        use once_cell::sync::Lazy;
        static GLOBAL_PROFILER: Lazy<std::sync::Mutex<GlobalProfiler>> =
            Lazy::new(Default::default);
        GLOBAL_PROFILER.lock().expect("poisoned mutex")
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

        let mut scope_details_bytes = 0;
        // Scope details are read before the frame is propagated to sinks. Because:
        // 1. Scopes are only registered once, and we don't need every sink to parse them.
        // 2. Sinks should be able to read scope details
        if !self.current_stream_scope_details.is_empty() {
            for stream_info in self.current_stream_scope_details.drain(..) {
                scope_details_bytes += stream_info.len();

                let new_scopes = Reader::from_start(&stream_info).read_new_scopes().unwrap();
                for (scope_id, scope_detail) in new_scopes {
                    self.scope_details
                        .insert(scope_id, ScopeDetailsOwned::from(scope_detail));
                }
            }
        }

        let current_frame_scope = std::mem::take(&mut self.current_stream_scope_times);

        let new_frame = match FrameData::new(
            current_frame_index,
            current_frame_scope,
            scope_details_bytes,
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
        for sink in self.sinks.values() {
            sink(new_frame.clone());
        }
    }

    /// Report some profiling data. Called from [`ThreadProfiler`].
    pub fn report(
        &mut self,
        info: ThreadInfo,
        stream_scope_details: &[u8],
        stream_scope_times: &StreamInfoRef<'_>,
    ) {
        if !stream_scope_details.is_empty() {
            self.current_stream_scope_details
                .push(Stream::from(stream_scope_details))
        }

        self.current_stream_scope_times
            .entry(info)
            .or_default()
            .extend(stream_scope_times);
    }

    /// Tells [`GlobalProfiler`] to call this function with each new finished frame.
    ///
    /// The returned [`FrameSinkId`] can be used to remove the sink with [`Self::remove_sink()`].
    pub fn add_sink(&mut self, sink: FrameSink) -> FrameSinkId {
        let id = self.next_sink_id;
        self.next_sink_id.0 += 1;
        self.sinks.insert(id, sink);
        id
    }

    pub fn remove_sink(&mut self, id: FrameSinkId) -> Option<FrameSink> {
        self.sinks.remove(&id)
    }

    pub fn scope_details() -> ScopeDetails {
        let lock = Self::lock();
        lock.scope_details.clone()
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

    use once_cell::sync::Lazy;

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
    /// The `id` doesn't need to be static, but it should be unchanging,
    /// and this is a good way to enforce it.
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

#[derive(Debug, Clone, Copy, PartialEq, Hash, PartialOrd, Ord, Eq)]
pub struct ScopeDetailsRef<'s> {
    /// Scope name, or function name (previously called "id")
    pub scope_name: &'s str,
    /// Path to the file containing the profiling macro
    pub file: &'s str,
    /// The line number containing the profiling
    pub line_nr: u32,
}

#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Ord, Eq)]
pub struct ScopeDetailsOwned {
    /// Shorter variant of the raw function name.
    /// This is more descriptive and shorter.
    pub cleaned_scope_name: String,
    /// Raw function name with the entire type path.
    pub raw_scope_name: String,
    /// Shorter variant of the raw file path.
    /// This is cleaned up and made consistent across platforms.
    pub cleaned_file_path: String,
    /// The full raw file path to the file in which the scope was located
    pub raw_file_path: String,
    /// The line number containing the profiling
    pub line_nr: u32,
}

impl<'a> From<ScopeDetailsRef<'a>> for ScopeDetailsOwned {
    fn from(value: ScopeDetailsRef<'a>) -> Self {
        ScopeDetailsOwned {
            raw_scope_name: value.scope_name.to_owned(),
            cleaned_scope_name: clean_function_name(value.scope_name),
            raw_file_path: short_file_name(value.file),
            cleaned_file_path: value.file.to_owned(),
            line_nr: value.line_nr,
        }
    }
}

/// A unique id for each scope and `ScopeInfo`.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScopeId(pub u32);

/// Provides read access to scope details.
#[derive(Default, Clone)]
pub struct ScopeDetails(
    Arc<
        RwLock<(
            std::collections::HashMap<ScopeId, ScopeDetailsOwned>,
            std::collections::HashMap<String, ScopeId>,
        )>,
    >,
);

impl ScopeDetails {
    /// Provides read to the given closure for some scope details.
    pub fn read_by_id<F: FnMut(&ScopeDetailsOwned)>(&self, scope_id: &ScopeId, mut cb: F) {
        if let Some(read) = self.0.read().0.get(scope_id) {
            cb(read)
        }
    }

    pub fn read_by_name<F: FnMut(&ScopeId)>(&self, scope_name: &str, mut cb: F) {
        if let Some(read) = self.0.read().1.get(scope_name) {
            cb(read)
        }
    }

    /// Function is hidden as only puffin library will insert entries.
    pub(crate) fn insert(&self, scope_id: ScopeId, scope_details: ScopeDetailsOwned) {
        // Scope details are collected before scope events are propagated.
        // It is safe to unwrap as every scope
        self.0
            .write()
            .1
            .insert(scope_details.raw_scope_name.clone(), scope_id);
        self.0.write().0.insert(scope_id, scope_details);
    }

    /// Manually insert scope details. After a scope is inserted it can be reported to puffin.
    pub fn insert_scopes(&self, scopes: &[ScopeDetailsRef<'_>]) {
        for scope_detail in scopes {
            let new_scope_id = fetch_add_scope_id();

            // Scope details are collected before scope events are propagated.
            // It is safe to unwrap as every scope
            self.0
                .write()
                .1
                .insert(scope_detail.scope_name.to_owned(), new_scope_id);
            self.0
                .write()
                .0
                .insert(new_scope_id, (*scope_detail).into());
        }
    }

    /// Fetches all registered scopes and their ids.
    /// Useful for fetching scope id by a static scope name.
    pub fn scopes_by_name(
        &self,
        mut existing_scopes: impl FnMut(&std::collections::HashMap<String, ScopeId>),
    ) {
        existing_scopes(&self.0.read().1);
    }

    /// Fetches all registered scopes.
    /// Useful for fetching scope details by a scope id.
    pub fn scopes_by_id(
        &self,
        mut existing_scopes: impl FnMut(&std::collections::HashMap<ScopeId, ScopeDetailsOwned>),
    ) {
        existing_scopes(&self.0.read().0);
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
pub fn clean_function_name<'a>(name: &'a str) -> String {
    let Some(name) = name.strip_suffix(USELESS_SCOPE_NAME_POSTFIX) else {
        // Probably the user registered a custom scope name.
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
        clean_function_name(&format!("foo{}", USELESS_SCOPE_NAME_POSTFIX)),
        "foo"
    );
    assert_eq!(
        clean_function_name(&format!("foo::bar{}", USELESS_SCOPE_NAME_POSTFIX)),
        "foo::bar"
    );
    assert_eq!(
        clean_function_name(&format!("foo::bar::baz{}", USELESS_SCOPE_NAME_POSTFIX)),
        "bar::baz"
    );
    assert_eq!(
        clean_function_name(&format!(
            "some::GenericThing<_, _>::function_name{}",
            USELESS_SCOPE_NAME_POSTFIX
        )),
        "GenericThing<_, _>::function_name"
    );
    assert_eq!(
        clean_function_name(&format!(
            "<some::ConcreteType as some::bloody::Trait>::function_name{}",
            USELESS_SCOPE_NAME_POSTFIX
        )),
        "<ConcreteType as Trait>::function_name"
    );
}

/// Shortens a long `file!()` path to the essentials.
///
/// We want to keep it short for two reasons: readability, and bandwidth
#[doc(hidden)]
#[inline(never)]
pub fn short_file_name<'a>(path: &'a str) -> String {
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

    GlobalProfiler::lock().add_sink(Box::new(|data| {
        if data.frame_index() == 0 {
            assert_eq!(data.frame_index(), 0);
            assert_eq!(data.meta().num_scopes, 2);
            assert_eq!(data.meta().num_bytes, 238);
        } else if data.frame_index() == 1 {
            assert_eq!(data.frame_index(), 1);
            assert_eq!(data.meta().num_scopes, 2);
            assert_eq!(data.meta().num_bytes, 62);
        } else {
            panic!("Only two frames in this test");
        }
    }));

    fn a() {
        profile_function!();
        {
            profile_scope!("my-scope");
        }
    }

    a();

    // First frame
    GlobalProfiler::lock().new_frame();

    let details = GlobalProfiler::scope_details();
    details.read_by_id(&ScopeId(0), |scope| {
        assert_eq!(scope.raw_file_path, "puffin/src/lib.rs");
        assert_eq!(scope.cleaned_file_path, "puffin/src/lib.rs");
        assert_eq!(
            scope.raw_scope_name,
            "puffin::profile_macros_test::a::{{closure}}::{{closure}}::f"
        );
        assert_eq!(scope.cleaned_scope_name, "profile_macros_test::a");
        assert_eq!(scope.line_nr, 1005);
    });
    details.read_by_id(&ScopeId(1), |scope| {
        assert_eq!(scope.raw_file_path, "puffin/src/lib.rs");
        assert_eq!(scope.cleaned_file_path, "puffin/src/lib.rs");
        assert_eq!(
            scope.raw_scope_name,
            "puffin::profile_macros_test::a::{{closure}}::{{closure}}::f"
        );
        assert_eq!(scope.cleaned_scope_name, "profile_macros_test::a");
        assert_eq!(scope.line_nr, 1007);
    });
    details.read_by_name("profile_macros_test::a", |id| assert_eq!(*id, ScopeId(0)));
    details.read_by_name("profile_macros_test::a", |id| assert_eq!(*id, ScopeId(1)));

    // Second frame
    a();

    GlobalProfiler::lock().new_frame();
}

// The macro defines 'f()' at the place where macro is called.
// This code is located at the place of call and two closures deep.
// Strip away this useless postfix.
static USELESS_SCOPE_NAME_POSTFIX: &'static str = "::{{closure}}::{{closure}}::f";

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
/// Overhead: around 210 ns on 2020 Intel MacBook Pro.
#[macro_export]
macro_rules! profile_function {
    () => {
        $crate::profile_function!("");
    };
    ($data:expr) => {
        let _profiler_scope = if $crate::are_scopes_on() {
            static SCOPE_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            static SEND_INFO: std::sync::Once = std::sync::Once::new();
            SEND_INFO.call_once(|| {
                $crate::ThreadProfiler::call(|tp| {
                    let id = tp.send_scope_info($crate::current_function_name!(), file!(), line!());

                    SCOPE_ID.store(id.0, std::sync::atomic::Ordering::SeqCst);
                });
            });

            Some($crate::ProfilerScope::new(
                $crate::ScopeId(SCOPE_ID.load(std::sync::atomic::Ordering::SeqCst)),
                $data,
            ))
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
/// Overhead: around 140 ns on 2020 Intel MacBook Pro.
#[macro_export]
macro_rules! profile_scope {
    ($id:expr) => {
        $crate::profile_scope!($id, "");
    };
    ($id:expr, $data:expr) => {
        let _profiler_scope = if $crate::are_scopes_on() {
            static SCOPE_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            static SEND_INFO: std::sync::Once = std::sync::Once::new();
            SEND_INFO.call_once(|| {
                $crate::ThreadProfiler::call(|tp| {
                    let id = tp.send_scope_info($crate::current_function_name!(), file!(), line!());
                    SCOPE_ID.store(id.0, std::sync::atomic::Ordering::SeqCst);
                });
            });

            Some($crate::ProfilerScope::new(
                $crate::ScopeId(SCOPE_ID.load(std::sync::atomic::Ordering::SeqCst)),
                $data,
            ))
        } else {
            None
        };
    };
}
