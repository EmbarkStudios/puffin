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
mod global_profiler;
mod merge;
mod profile_view;
mod scope_details;
mod thread_profiler;
mod utils;

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};

/// TODO: Improve encapsulation.
pub use data::{Error, Reader, Result, Scope, ScopeRecord, Stream, StreamInfo, StreamInfoRef};
pub use frame_data::{FrameData, FrameMeta, UnpackedFrameData};
pub use global_profiler::{FrameSink, GlobalProfiler};
pub use merge::{merge_scopes_for_thread, MergeScope};
pub use profile_view::{select_slowest, FrameStats, FrameView, GlobalFrameView};
pub use scope_details::{ScopeCollection, ScopeDetails, ScopeType};
pub use thread_profiler::{internal_profile_reporter, ThreadInfo, ThreadProfiler};
pub use utils::{clean_function_name, short_file_name, shorten_rust_function_name, type_name_of};

static MACROS_ON: AtomicBool = AtomicBool::new(false);

/// Turn on/off the profiler macros ([`profile_function`], [`profile_scope`] etc).
/// When off, these calls take only 1-2 ns to call (100x faster).
/// This is [`false`] by default.
pub fn set_scopes_on(on: bool) {
    MACROS_ON.store(on, Ordering::Relaxed);
}

/// Are the profiler scope macros turned on?
/// This is [`false`] by default.
///
/// Turn on with [`set_scopes_on`].
pub fn are_scopes_on() -> bool {
    MACROS_ON.load(Ordering::Relaxed)
}

/// All times are expressed as integer nanoseconds since some event.
pub type NanoSecond = i64;

/// An incremental monolithic counter to identify frames.
pub type FrameIndex = u64;

type NsSource = fn() -> NanoSecond;

/// Incremental monolithic counter to identify scopes.
static SCOPE_ID_TRACKER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn fetch_add_scope_id() -> ScopeId {
    let new_id = SCOPE_ID_TRACKER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ScopeId(
        NonZeroU32::new(new_id)
            .expect("safe because integer is retrieved from fetch-add atomic operation"),
    )
}

/// Identifies a specific [`FrameSink`] when added to [`GlobalProfiler`].
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct FrameSinkId(u64);

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

    static START_TIME: once_cell::sync::Lazy<(NanoSecond, Instant)> =
        once_cell::sync::Lazy::new(|| (nanos_since_epoch(), Instant::now()));
    START_TIME.0 + START_TIME.1.elapsed().as_nanos() as NanoSecond
}

/// Should not be used.
#[inline]
#[cfg(all(target_arch = "wasm32", not(feature = "web")))]
pub fn now_ns() -> NanoSecond {
    // This should be unused.
    panic!("Wasm without the `web` feature requires passing a custom source of time via `ThreadProfiler::initialize`");
}

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

/// Returns the name of the calling function without a long module path prefix.
#[macro_export]
macro_rules! current_function_name {
    () => {{
        fn f() {}
        $crate::type_name_of(f)
    }};
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
/// Overhead: around 54 ns on Macbook Pro with Apple M1 Max.
///
/// If the puffin profiler is turned off ([`crate::are_scopes_on`] is `false`),
/// the cost is only checking an `AtomicBool`, which is less than 1ns.
///
/// You can conditionally profile a function with [`profile_function_if`].
#[macro_export]
macro_rules! profile_function {
    () => {
        $crate::profile_function_if!(true, "");
    };
    ($data:expr) => {
        $crate::profile_function_if!(true, $data);
    };
}

/// Conditionally profile the current function.
///
/// This can be useful to avoid profiler overhead for functions that are sometimes fast and called often.
///
/// For instance:
///
/// ```rs
/// /// Very fast if given a small number,
/// /// and very slow if given a large number.
/// ///
/// /// This is sometimes called many, many times with small numbers,
/// /// and sometimes only a few times, but with a large number.
/// fn do_work(num_jobs: usize) {
///     puffin::profile_function_if!(num_jobs > 1000);
///     // …
/// }
///
/// fn caller() {
///     do_work(10_000_000); // will get profiled
///
///     for i in 0..10_000_000 {
///         do_work(1); // no proile scopes, meaning no profiler overhead.
///     }
/// }
/// ```
///
/// If [`crate::are_scopes_on`] is `false`, the condition is not evaluated.
#[macro_export]
macro_rules! profile_function_if {
    ($condition:expr) => {
        $crate::profile_function_if!($condition, "");
    };
    ($condition:expr, $data:expr) => {
        let _profiler_scope = if $crate::are_scopes_on() && ($condition) {
            static SCOPE_ID: std::sync::OnceLock<$crate::ScopeId> = std::sync::OnceLock::new();
            let scope_id = SCOPE_ID.get_or_init(|| {
                $crate::ThreadProfiler::call(|tp| {
                    let id = tp.register_function_scope(
                        $crate::clean_function_name($crate::current_function_name!()),
                        $crate::short_file_name(file!()),
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

/// Profile the current scope with the given name (unique in the parent scope).
///
/// This macro is identical to [profile_scope], except that it expands to the expression
/// containing the profiling scope, as opposed to [profile_scope] which expands to a
/// variable (which cannot be accessed due to macro hygiene).
///
/// This allows for profiling scopes to persist for a custom duration.
///
/// # Example
///
/// ```rust
/// # use std::iter::FromIterator as _;
/// #
/// # pub mod rayon { pub mod prelude {
/// #     pub fn for_each_init<T, I>(vec: &std::vec::Vec<T>, init: fn() -> I, body: fn ((I, T)) -> ()) {
/// #     }
/// # } }
/// #
/// let some_large_vec = Vec::from_iter(0..1000);
///
/// // Use rayon's parallel for loop over our large iterator
/// rayon::prelude::for_each_init(
///         &some_large_vec,
///         // This gets called to init each work segment, and is passed into the calls
///         // Rayon keeps the profiling scope stored for the entire duration of the work segment
///         // So we can track the entire segment as one, instead of each loop iteration
///         || puffin::profile_scope_custom!("rayon_work_segment"),
///         |((_profiler_scope), i)| {
///             // All calls here gets profiled on the same scope
///             println!("{i}")
///         },
/// );
/// ```
#[macro_export]
macro_rules! profile_scope_custom {
    ($name:expr) => {
        $crate::profile_scope_custom_if!(true, $name, "")
    };
    ($name:expr, $data:expr) => {{
        $crate::profile_scope_custom_if!(true, $name, $data)
    }};
}

/// Like [`profile_scope_custom`], but only conditionally profiles the scope.
///
/// This can be used to avoid profiling overhead for scopes that are sometimes fast and called often.
///
/// See [`profile_function_if`] for a motivating example.
#[macro_export]
macro_rules! profile_scope_custom_if {
    ($condition:expr, $name:expr) => {
        $crate::profile_scope_custom_if!($condition, $name, "")
    };
    ($condition:expr, $name:expr, $data:expr) => {{
        if $crate::are_scopes_on() && ($condition) {
            static SCOPE_ID: std::sync::OnceLock<$crate::ScopeId> = std::sync::OnceLock::new();
            let scope_id = SCOPE_ID.get_or_init(|| {
                $crate::ThreadProfiler::call(|tp| {
                    let id = tp.register_named_scope(
                        $name,
                        $crate::clean_function_name($crate::current_function_name!()),
                        $crate::short_file_name(file!()),
                        line!(),
                    );
                    id
                })
            });
            Some($crate::ProfilerScope::new(*scope_id, $data))
        } else {
            None
        }
    }};
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
/// Overhead: around 54 ns on Macbook Pro with Apple M1 Max.
///
/// If the puffin profiler is turned off ([`crate::are_scopes_on`] is `false`),
/// the cost is only checking an `AtomicBool`, which is less than 1ns.
///
/// You can conditionally profile a scope with [`profile_scope_if`].
#[macro_export]
macro_rules! profile_scope {
    ($name:expr) => {
        $crate::profile_scope_if!(true, $name, "");
    };
    ($name:expr, $data:expr) => {
        $crate::profile_scope_if!(true, $name, $data);
    };
}

/// Like [`profile_scope`], but only conditionally profiles the scope.
///
/// This can be used to avoid profiling overhead for scopes that are sometimes fast and called often.
///
/// See [`profile_function_if`] for a motivating example.
#[macro_export]
macro_rules! profile_scope_if {
    ($condition:expr, $name:expr) => {
        $crate::profile_scope_if!($condition, $name, "");
    };
    ($condition:expr, $name:expr, $data:expr) => {
        let _profiler_scope = $crate::profile_scope_custom_if!($condition, $name, $data);
    };
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::{set_scopes_on, GlobalFrameView, GlobalProfiler, ScopeId};

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
}
