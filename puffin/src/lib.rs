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
#![warn(
    clippy::all,
    clippy::doc_markdown,
    clippy::dbg_macro,
    clippy::todo,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::pub_enum_variant_names,
    clippy::mem_forget,
    //clippy::use_self,   // disabled for now due to https://github.com/rust-lang/rust-clippy/issues/3410
    clippy::filter_map_next,
    clippy::needless_continue,
    clippy::needless_borrow,
    clippy::match_wildcard_for_single_variants,
    clippy::if_let_mutex,
    clippy::mismatched_target_os,
    clippy::await_holding_lock,
    clippy::match_on_vec_items,
    clippy::imprecise_flops,
    //clippy::suboptimal_flops,
    clippy::lossy_float_literal,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::fn_params_excessive_bools,
    clippy::exit,
    clippy::inefficient_to_string,
    clippy::linkedlist,
    clippy::macro_use_imports,
    clippy::option_option,
    clippy::verbose_file_reads,
    rust_2018_idioms,
    future_incompatible,
    nonstandard_style
)]

mod data;
mod merge;

pub use data::*;
pub use merge::*;

use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

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
pub struct ThreadInfo {
    /// Useful for ordering threads.
    pub start_time_ns: Option<NanoSecond>,
    /// Name of the thread
    pub name: String,
}

// ----------------------------------------------------------------------------

/// Collected profile data from many sources.
#[derive(Clone, Default)]
pub struct FullProfileData(pub BTreeMap<ThreadInfo, Stream>);

impl FullProfileData {
    /// Returns the min/max nanosecond in the profile data, or None if empty.
    pub fn range_ns(&self) -> Result<Option<(NanoSecond, NanoSecond)>> {
        let mut min_ns = NanoSecond::MAX;
        let mut max_ns = NanoSecond::MIN;
        for stream in self.0.values() {
            let top_scopes = Reader::from_start(stream).read_top_scopes()?;
            if !top_scopes.is_empty() {
                min_ns = min_ns.min(top_scopes.first().unwrap().record.start_ns);
                max_ns = max_ns.max(top_scopes.last().unwrap().record.stop_ns());
            }
        }

        Ok(if min_ns < max_ns {
            Some((min_ns, max_ns))
        } else {
            None
        })
    }

    pub fn duration_ns(&self) -> Result<NanoSecond> {
        if let Some((min, max)) = self.range_ns()? {
            Ok(max - min)
        } else {
            Ok(0)
        }
    }

    pub fn insert(&mut self, info: ThreadInfo, stream: Stream) {
        self.0.entry(info).or_default().append(stream);
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
            eprintln!("ERROR: Mismatched scope begin/end calls");
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

/// Singleton. Collects profiling data from multiple threads.
#[derive(Default)]
pub struct GlobalProfiler {
    spike_frame: FullProfileData,
    past_frame: FullProfileData,
    current_frame: FullProfileData,
}

impl GlobalProfiler {
    /// Access to the global profiler singleton.
    pub fn lock() -> parking_lot::MutexGuard<'static, Self> {
        use once_cell::sync::Lazy;
        static GLOBAL_PROFILER: Lazy<Mutex<GlobalProfiler>> = Lazy::new(Default::default);
        GLOBAL_PROFILER.lock()
    }

    /// You need to call once every frame.
    pub fn new_frame(&mut self) {
        self.past_frame = std::mem::take(&mut self.current_frame);
        if self.past_frame.duration_ns().unwrap_or_default()
            > self.spike_frame.duration_ns().unwrap_or_default()
        {
            self.spike_frame = self.past_frame.clone();
        }
    }

    /// Report some profiling data. Called from `ThreadProfiler`.
    pub fn report(&mut self, info: ThreadInfo, stream: Stream) {
        self.current_frame.insert(info, stream);
    }

    /// Latest full frame
    pub fn past_frame(&self) -> &FullProfileData {
        &self.past_frame
    }

    /// The slowest frame so far (or since last call to `clear_spike_frame()`)
    pub fn spike_frame(&self) -> &FullProfileData {
        &self.spike_frame
    }

    /// Clean current spike frame.
    pub fn clear_spike_frame(&mut self) {
        self.spike_frame = Default::default();
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
    if let Some(slash) = name.rfind('/') {
        // "foo/bar/baz.rs" -> "baz.rs"
        &name[slash + 1..]
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
}

/// Automatically name the profiling scope based on function name.
/// Example: `profile_function!();`
/// Overhead: around 210 ns on 2020 MacBook Pro
#[macro_export]
macro_rules! profile_function {
    () => {
        let _profiler_scope = if $crate::are_scopes_on() {
            Some($crate::ProfilerScope::new(
                $crate::current_function_name!(),
                $crate::current_file_name!(),
                "",
            ))
        } else {
            None
        };
    };
}

/// Automatically name the profiling scope based on function name.
/// Additionally provide some data (e.g. a mesh name) to help diagnose what was slow.
/// Example: `profile_function_data!(mesh_name);`
/// Overhead: around 210 ns on 2020 MacBook Pro
#[macro_export]
macro_rules! profile_function_data {
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

/// Profile the current scope with the given name (unique in the parent scope).
/// Names should be descriptive, ASCII and without spaces.
/// Example: `profile_scope!("load_mesh");`
/// Overhead: around 140 ns on 2020 MacBook Pro
#[macro_export]
macro_rules! profile_scope {
    ($id:expr) => {
        let _profiler_scope = if $crate::are_scopes_on() {
            Some($crate::ProfilerScope::new(
                $id,
                $crate::current_file_name!(),
                "",
            ))
        } else {
            None
        };
    };
}

/// Profile the current scope with the given name (unique in the parent scope).
/// Names should be descriptive, ASCII and without spaces.
/// Additionally provide some data (e.g. a mesh name) to help diagnose what was slow.
/// Example: `profile_scope_data!("load_mesh", mesh_name);`
/// Overhead: around 150 ns on 2020 MacBook Pro
#[macro_export]
macro_rules! profile_scope_data {
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
