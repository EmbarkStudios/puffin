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

// BEGIN - Embark standard lints v0.3
// do not change or add/remove here, but one can add exceptions after this section
// for more info see: <https://github.com/EmbarkStudios/rust-ecosystem/issues/59>
#![deny(unsafe_code)]
#![warn(
    clippy::all,
    clippy::await_holding_lock,
    clippy::dbg_macro,
    clippy::debug_assert_with_mut_call,
    clippy::doc_markdown,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::exit,
    clippy::explicit_into_iter_loop,
    clippy::filter_map_next,
    clippy::fn_params_excessive_bools,
    clippy::if_let_mutex,
    clippy::imprecise_flops,
    clippy::inefficient_to_string,
    clippy::large_types_passed_by_value,
    clippy::let_unit_value,
    clippy::linkedlist,
    clippy::lossy_float_literal,
    clippy::macro_use_imports,
    clippy::map_err_ignore,
    clippy::map_flatten,
    clippy::map_unwrap_or,
    clippy::match_on_vec_items,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::mem_forget,
    clippy::mismatched_target_os,
    clippy::needless_borrow,
    clippy::needless_continue,
    clippy::option_option,
    clippy::pub_enum_variant_names,
    clippy::ref_option_ref,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::string_add_assign,
    clippy::string_add,
    clippy::string_to_string,
    clippy::suboptimal_flops,
    clippy::todo,
    clippy::unimplemented,
    clippy::unnested_or_patterns,
    clippy::unused_self,
    clippy::verbose_file_reads,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms
)]

mod data;
mod merge;

pub use data::*;
pub use merge::*;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

static MACROS_ON: AtomicBool = AtomicBool::new(false);

/// Turn on/off the profiler macros (`profile_function`, `profile_scope` etc).
/// When off, these calls take only 1-2 ns to call (100x faster).
/// This is `false` by default.
pub fn set_scopes_on(on: bool) {
    MACROS_ON.store(on, Ordering::Relaxed);
}

/// Are the profiler scope macros turned on?
/// This is `false` by default.
pub fn are_scopes_on() -> bool {
    MACROS_ON.load(Ordering::Relaxed)
}

/// All times are expressed as integer nanoseconds since some event.
pub type NanoSecond = i64;

/// Stream of profiling events from one thread.
#[derive(Clone, Default)]
#[cfg_attr(feature = "with_serde", derive(serde::Deserialize, serde::Serialize))]
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

    pub fn append(&mut self, mut other: Self) {
        self.0.append(&mut other.0);
    }
}

impl From<Vec<u8>> for Stream {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

/// Used when parsing a Stream.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record<'s> {
    pub start_ns: NanoSecond,
    pub duration_ns: NanoSecond,

    /// e.g. function name. Mandatory. Used to identify records.
    /// Does not need to be globally unique, just unique in the parent scope.
    /// Example: "load_image"
    pub id: &'s str,

    /// e.g. file name. Optional. Used for finding the location of the profiler scope.
    /// Example: "my_library/image_loader.rs:52"
    pub location: &'s str,

    /// e.g. function argument, like a mesh name. Optional.
    /// Example: "image.png".
    pub data: &'s str,
}

impl<'s> Record<'s> {
    pub fn stop_ns(&self) -> NanoSecond {
        self.start_ns + self.duration_ns
    }
}

/// Used when parsing a Stream.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scope<'s> {
    pub record: Record<'s>,
    /// Stream offset for first child.
    pub child_begin_position: u64,
    /// Stream offset after last child.
    pub child_end_position: u64,
    /// Stream offset for next sibling (if any).
    pub next_sibling_position: u64,
}

/// Used to identify one source of profiling data.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[cfg_attr(feature = "with_serde", derive(serde::Deserialize, serde::Serialize))]
pub struct ThreadInfo {
    /// Useful for ordering threads.
    pub start_time_ns: Option<NanoSecond>,
    /// Name of the thread
    pub name: String,
}

// ----------------------------------------------------------------------------

pub type FrameIndex = u64;

/// One frame worth of profile data, collected from many sources.
#[derive(Clone)]
#[cfg_attr(feature = "with_serde", derive(serde::Deserialize, serde::Serialize))]
pub struct FrameData {
    pub frame_index: FrameIndex,
    pub thread_streams: BTreeMap<ThreadInfo, Stream>,
    pub range_ns: (NanoSecond, NanoSecond),
    pub num_bytes: usize,
    pub num_scopes: usize,
}

impl FrameData {
    pub fn new(
        frame_index: FrameIndex,
        thread_streams: BTreeMap<ThreadInfo, Stream>,
    ) -> Result<Self> {
        let mut num_bytes = 0;
        let mut num_scopes = 0;

        let mut min_ns = NanoSecond::MAX;
        let mut max_ns = NanoSecond::MIN;
        for stream in thread_streams.values() {
            num_bytes += stream.len();
            num_scopes += Reader::count_all_scopes(stream)?;

            let top_scopes = Reader::from_start(stream).read_top_scopes()?;
            if !top_scopes.is_empty() {
                min_ns = min_ns.min(top_scopes.first().unwrap().record.start_ns);
                max_ns = max_ns.max(top_scopes.last().unwrap().record.stop_ns());
            }
        }

        if min_ns <= max_ns {
            Ok(Self {
                frame_index,
                thread_streams,
                range_ns: (min_ns, max_ns),
                num_bytes,
                num_scopes,
            })
        } else {
            Err(Error::Empty)
        }
    }

    pub fn duration_ns(&self) -> NanoSecond {
        let (min, max) = self.range_ns;
        max - min
    }
}

// ----------------------------------------------------------------------------

type NsSource = fn() -> NanoSecond;
type ThreadReporter = fn(ThreadInfo, Stream);

/// Report a stream of profile data from a thread to the `GlobalProfiler` singleton.
pub fn global_reporter(info: ThreadInfo, stream: Stream) {
    GlobalProfiler::lock().report(info, stream)
}

/// Collects profiling data for one thread
pub struct ThreadProfiler {
    stream: Stream,
    depth: usize,
    now_ns: NsSource,
    reporter: ThreadReporter,
    start_time_ns: Option<NanoSecond>,
}

impl Default for ThreadProfiler {
    fn default() -> Self {
        Self {
            stream: Default::default(),
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
    /// If not called, each thread will use the default nanosecond source
    /// and report scopes to the global profiler.
    ///
    /// For instance, when compiling for WASM the default timing function (`puffin::now_ns`) won't work,
    /// so you'll want to call `puffin::ThreadProfiler::initialize(my_timing_function, puffin::global_reporter);`.
    pub fn initialize(now_ns: NsSource, reporter: ThreadReporter) {
        ThreadProfiler::call(|tp| {
            tp.now_ns = now_ns;
            tp.reporter = reporter;
        });
    }

    /// Returns position where to write scope size once the scope is closed.
    #[must_use]
    pub fn begin_scope(&mut self, id: &str, location: &str, data: &str) -> usize {
        let now_ns = (self.now_ns)();
        self.start_time_ns = Some(self.start_time_ns.unwrap_or(now_ns));

        self.depth += 1;
        self.stream.begin_scope(now_ns, id, location, data)
    }

    pub fn end_scope(&mut self, start_offset: usize) {
        if self.depth > 0 {
            self.depth -= 1;
        } else {
            eprintln!("puffin ERROR: Mismatched scope begin/end calls");
        }
        self.stream.end_scope(start_offset, (self.now_ns)());

        if self.depth == 0 {
            // We have no open scopes.
            // This is a good time to report our profiling stream to the global profiler:
            let info = ThreadInfo {
                start_time_ns: self.start_time_ns,
                name: std::thread::current().name().unwrap_or_default().to_owned(),
            };
            let stream = std::mem::take(&mut self.stream);
            (self.reporter)(info, stream);
        }
    }

    /// Do something with the thread local `ThreadProfiler`
    pub fn call<R>(f: impl Fn(&mut Self) -> R) -> R {
        thread_local! {
            pub static THREAD_PROFILER: std::cell::RefCell<ThreadProfiler> = Default::default();
        }
        THREAD_PROFILER.with(|p| f(&mut p.borrow_mut()))
    }
}

// ----------------------------------------------------------------------------

struct OrderedData(Arc<FrameData>);

impl PartialEq for OrderedData {
    fn eq(&self, other: &Self) -> bool {
        self.0.duration_ns().eq(&other.0.duration_ns())
    }
}
impl Eq for OrderedData {}

impl PartialOrd for OrderedData {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedData {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.duration_ns().cmp(&other.0.duration_ns()).reverse()
    }
}

/// Singleton. Collects profiling data from multiple threads.
pub struct GlobalProfiler {
    current_frame_index: FrameIndex,
    current_frame: BTreeMap<ThreadInfo, Stream>,

    /// newest first
    recent_frames: std::collections::VecDeque<Arc<FrameData>>,
    max_recent: usize,

    slowest_frames: std::collections::BinaryHeap<OrderedData>,
    max_slow: usize,
}

impl Default for GlobalProfiler {
    fn default() -> Self {
        let max_recent = 128;
        let max_slow = 128;

        Self {
            current_frame_index: 0,
            current_frame: Default::default(),
            recent_frames: std::collections::VecDeque::with_capacity(max_recent),
            max_recent,
            slowest_frames: std::collections::BinaryHeap::with_capacity(max_slow),
            max_slow,
        }
    }
}

impl GlobalProfiler {
    /// Access to the global profiler singleton.
    pub fn lock() -> std::sync::MutexGuard<'static, Self> {
        use once_cell::sync::Lazy;
        static GLOBAL_PROFILER: Lazy<Mutex<GlobalProfiler>> = Lazy::new(Default::default);
        GLOBAL_PROFILER.lock().unwrap() // panic on mutex poisoning
    }

    /// You need to call this once at the start of every frame.
    ///
    /// It is fine to call this from within a profile scope.
    pub fn new_frame(&mut self) {
        let current_frame_index = self.current_frame_index;
        self.current_frame_index += 1;
        let new_frame =
            match FrameData::new(current_frame_index, std::mem::take(&mut self.current_frame)) {
                Ok(new_frame) => Arc::new(new_frame),
                Err(Error::Empty) => {
                    return; // don't warn about empty frames, just ignore them
                }
                Err(err) => {
                    eprintln!("puffin ERROR: Bad frame: {:?}", err);
                    return;
                }
            };

        self.add_frame(new_frame);
    }

    /// Manually add frame data.
    pub fn add_frame(&mut self, new_frame: Arc<FrameData>) {
        let add_to_slowest = if self.slowest_frames.len() < self.max_slow {
            true
        } else if let Some(fastest_of_the_slow) = self.slowest_frames.peek() {
            new_frame.duration_ns() > fastest_of_the_slow.0.duration_ns()
        } else {
            false
        };

        if add_to_slowest {
            self.slowest_frames.push(OrderedData(new_frame.clone()));
            while self.slowest_frames.len() > self.max_slow {
                self.slowest_frames.pop();
            }
        }

        self.recent_frames.push_back(new_frame);
        while self.recent_frames.len() > self.max_recent {
            self.recent_frames.pop_front();
        }
    }

    /// Report some profiling data. Called from `ThreadProfiler`.
    pub fn report(&mut self, info: ThreadInfo, stream: Stream) {
        self.current_frame.entry(info).or_default().append(stream);
    }

    /// The latest fully captured frame of data.
    pub fn latest_frame(&self) -> Option<Arc<FrameData>> {
        self.recent_frames.back().cloned()
    }

    /// Oldest first
    pub fn recent_frames(&self) -> impl Iterator<Item = &Arc<FrameData>> {
        self.recent_frames.iter()
    }

    /// The slowest frames so far (or since last call to [`Self::clear_slowest`])
    /// in chronological order.
    pub fn slowest_frames_chronological(&self) -> Vec<Arc<FrameData>> {
        let mut frames: Vec<_> = self.slowest_frames.iter().map(|f| f.0.clone()).collect();
        frames.sort_by_key(|frame| frame.frame_index);
        frames
    }

    /// Clean history of the slowest frames.
    pub fn clear_slowest(&mut self) {
        self.slowest_frames.clear();
    }

    /// How many frames of recent history to store.
    pub fn max_recent(&self) -> usize {
        self.max_recent
    }

    /// How many frames of recent history to store.
    pub fn set_max_history(&mut self, max_recent: usize) {
        self.max_recent = max_recent
    }

    /// How many slow "spike" frames to store.
    pub fn max_slow(&self) -> usize {
        self.max_slow
    }

    /// How many slow "spike" frames to store.
    pub fn set_max_slow(&mut self, max_slow: usize) {
        self.max_slow = max_slow
    }
}

// ----------------------------------------------------------------------------

/// Returns monotonically increasing nanosecond count.
/// It is undefined when `now_ns()=0` is.
pub fn now_ns() -> NanoSecond {
    // This can maybe be optimized
    use once_cell::sync::Lazy;
    use std::time::Instant;
    static START_TIME: Lazy<Instant> = Lazy::new(Instant::now);
    START_TIME.elapsed().as_nanos() as NanoSecond
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
    pub fn new(id: &'static str, location: &str, data: impl AsRef<str>) -> Self {
        Self {
            start_stream_offset: ThreadProfiler::call(|tp| {
                tp.begin_scope(id, location, data.as_ref())
            }),
            _dont_send_me: Default::default(),
        }
    }
}

impl Drop for ProfilerScope {
    fn drop(&mut self) {
        ThreadProfiler::call(|tp| tp.end_scope(self.start_stream_offset))
    }
}

#[doc(hidden)]
pub fn type_name_of<T>(_: T) -> &'static str {
    std::any::type_name::<T>()
}

/// Returns the name of the calling function without a long module path prefix.
#[macro_export]
macro_rules! current_function_name {
    () => {{
        fn f() {}
        let name = $crate::type_name_of(f);
        // Remove "::f" from the name:
        let name = &name.get(..name.len() - 3).unwrap();
        $crate::clean_function_name(name)
    }};
}

#[doc(hidden)]
pub fn clean_function_name(name: &str) -> &str {
    if let Some(colon) = name.rfind("::") {
        if let Some(colon) = name[..colon].rfind("::") {
            // "foo::bar::baz::function_name" -> "baz::function_name"
            &name[colon + 2..]
        } else {
            // "foo::function_name" -> "foo::function_name"
            name
        }
    } else {
        name
    }
}

#[test]
fn test_clean_function_name() {
    assert_eq!(clean_function_name(""), "");
    assert_eq!(clean_function_name("foo"), "foo");
    assert_eq!(clean_function_name("foo::bar"), "foo::bar");
    assert_eq!(clean_function_name("foo::bar::baz"), "bar::baz");
}

/// Returns a shortened path to the current file.
#[macro_export]
macro_rules! current_file_name {
    () => {
        $crate::short_file_name(file!())
    };
}

/// Removes long path prefix to focus on the last parts of the path (and the file name).
#[doc(hidden)]
pub fn short_file_name(name: &str) -> &str {
    // TODO: "foo/bar/src/lib.rs" -> "bar/src/lib.rs"

    if let Some(separator) = name.rfind(&['/', '\\'][..]) {
        // "foo/bar/baz.rs" -> "baz.rs"
        &name[separator + 1..]
    } else {
        name
    }
}

#[test]
fn test_short_file_name() {
    assert_eq!(short_file_name(""), "");
    assert_eq!(short_file_name("foo.rs"), "foo.rs");
    assert_eq!(short_file_name("foo/bar.rs"), "bar.rs");
    assert_eq!(short_file_name("foo/bar/baz.rs"), "baz.rs");
    assert_eq!(short_file_name(r"C:\\windows\is\weird\src.rs"), "src.rs");
}

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
/// Overhead: around 210 ns on 2020 MacBook Pro
#[macro_export]
macro_rules! profile_function {
    () => {
        $crate::profile_function!("");
    };
    ($data:expr) => {
        let _profiler_scope = if $crate::are_scopes_on() {
            Some($crate::ProfilerScope::new(
                $crate::current_function_name!(),
                $crate::current_file_name!(),
                $data,
            ))
        } else {
            None
        };
    };
}

#[deprecated = "Use puffin::profile_function!(data); instead"]
#[macro_export]
macro_rules! profile_function_data {
    ($data:expr) => {
        $crate::profile_function($data);
    };
}

/// Profile the current scope with the given name (unique in the parent scope).
///
/// Names should be descriptive, ASCII and without spaces.
///
/// Example: `profile_scope!("load_mesh");`.
///
/// An optional second argument can be a string (e.g. a mesh name) to help diagnose what was slow.
/// Example: `profile_scope!("load_mesh", mesh_name);`
///
/// Overhead: around 140 ns on 2020 MacBook Pro
#[macro_export]
macro_rules! profile_scope {
    ($id:expr) => {
        $crate::profile_scope!($id, "");
    };
    ($id:expr, $data:expr) => {
        let _profiler_scope = if $crate::are_scopes_on() {
            Some($crate::ProfilerScope::new(
                $id,
                $crate::current_file_name!(),
                $data,
            ))
        } else {
            None
        };
    };
}

#[deprecated = "Use puffin::profile_scope!(id, data) instead"]
#[macro_export]
macro_rules! profile_scope_data {
    ($id:expr, $data:expr) => {
        $crate::profile_scope_function($id, $data);
    };
}
