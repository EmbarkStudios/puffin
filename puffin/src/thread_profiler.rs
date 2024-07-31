use std::borrow::Cow;

use crate::GlobalProfiler;
use crate::NanoSecond;
use crate::NsSource;

use crate::fetch_add_scope_id;
use crate::ScopeDetails;
use crate::ScopeId;
use crate::StreamInfo;
use crate::StreamInfoRef;

/// Report a stream of profile data from a thread to the [`GlobalProfiler`] singleton.
/// This is used for internal purposes only
pub fn internal_profile_reporter(
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
    /// If not called, each thread will use the default nanosecond source ([`crate::now_ns`])
    /// and report scopes to the global profiler ([`internal_profile_reporter`]).
    ///
    /// For instance, when compiling for WASM the default timing function ([`crate::now_ns`]) won't work,
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
        self.depth += 1;

        let (offset, start_ns) = self
            .stream_info
            .stream
            .begin_scope(self.now_ns, scope_id, data);

        self.stream_info.range_ns.0 = self.stream_info.range_ns.0.min(start_ns);
        self.start_time_ns = Some(self.start_time_ns.unwrap_or(start_ns));

        offset
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

/// Used to identify one source of profiling data.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct ThreadInfo {
    /// Useful for ordering threads.
    pub start_time_ns: Option<NanoSecond>,
    /// Name of the thread
    pub name: String,
}

// Function interface for reporting thread local scope details.
// The scope details array will contain information about a scope the first time it is seen.
// The stream will always contain the scope timing details.
type ThreadReporter = fn(ThreadInfo, &[ScopeDetails], &StreamInfoRef<'_>);
