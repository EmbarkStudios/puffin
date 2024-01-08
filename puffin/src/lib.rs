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

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};

/// TODO: Improve encapsulation.
pub use data::{Error, Reader, Result, Scope, ScopeRecord, Stream, StreamInfo, StreamInfoRef};
pub use frame_data::{FrameData, FrameMeta, UnpackedFrameData};
pub use global_profiler::{FrameSink, GlobalProfiler};
pub use merge::{merge_scopes_for_thread, MergeScope};
pub use profile_view::{select_slowest, FrameView, GlobalFrameView};
pub use scope_details::{ScopeCollection, ScopeDetails, ScopeType};
pub use thread_profiler::{ThreadInfo, ThreadProfiler};

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

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::{
        clean_function_name, set_scopes_on, short_file_name, GlobalFrameView, GlobalProfiler,
        ScopeId, USELESS_SCOPE_NAME_SUFFIX,
    };

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
