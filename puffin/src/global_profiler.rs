use std::{collections::BTreeMap, sync::Arc};

use once_cell::sync::Lazy;

use crate::{
    fetch_add_scope_id, Error, FrameData, FrameIndex, FrameSinkId, ScopeCollection, ScopeDetails,
    ScopeId, StreamInfo, StreamInfoRef, ThreadInfo,
};

/// Add these to [`GlobalProfiler`] with [`GlobalProfiler::add_sink()`].
pub type FrameSink = Box<dyn Fn(Arc<FrameData>) + Send>;

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
    pub fn report(
        &mut self,
        info: ThreadInfo,
        scope_details: &[ScopeDetails],
        stream_scope_times: &StreamInfoRef<'_>,
    ) {
        if !scope_details.is_empty() {
            // Here we can run slightly heavy logic as its only ran once for each scope.
            self.new_scopes
                .extend(scope_details.iter().map(|x| Arc::new(x.clone())));
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
