use itertools::Itertools;
use std::{
    cmp::Ordering,
    collections::{BTreeSet, VecDeque},
    sync::Arc,
};

use crate::{FrameData, FrameSinkId, ScopeCollection};

/// A view of recent and slowest frames, used by GUIs.
#[derive(Clone)]
pub struct FrameView {
    /// newest first
    recent: VecDeque<OrderedByIndex>,
    max_recent: usize,

    slowest_by_index: BTreeSet<OrderedByIndex>,
    slowest_by_duration: BTreeSet<OrderedByDuration>,
    max_slow: usize,

    /// Minimizes memory usage at the expense of CPU time.
    ///
    /// Only recommended if you set a large max_recent size.
    pack_frames: bool,

    /// Maintain stats as we add/remove frames
    stats: FrameStats,

    scope_collection: ScopeCollection,
}

impl Default for FrameView {
    fn default() -> Self {
        let max_recent = 1_000;
        let max_slow = 256;

        Self {
            recent: VecDeque::with_capacity(max_recent),
            max_recent,
            slowest_by_index: BTreeSet::new(),
            slowest_by_duration: BTreeSet::new(),
            max_slow,
            pack_frames: true,
            stats: Default::default(),
            scope_collection: Default::default(),
        }
    }
}

impl FrameView {
    /// Returns `true` if there are no recent or slowest frames.
    pub fn is_empty(&self) -> bool {
        self.recent.is_empty() && self.slowest_by_duration.is_empty()
    }

    /// Returns the collection of scope details.
    /// This can be used to fetch more information about a specific scope.
    pub fn scope_collection(&self) -> &ScopeCollection {
        &self.scope_collection
    }

    /// Adds a new frame to the view.
    pub fn add_frame(&mut self, new_frame: Arc<FrameData>) {
        // Register all scopes from the new frame into the scope collection.
        for new_scope in &new_frame.scope_delta {
            self.scope_collection.insert(new_scope.clone());
        }

        if let Some(last) = self.recent.iter().last() {
            if new_frame.frame_index() <= last.0.frame_index() {
                // A frame from the past!?
                // Likely we are `puffin_viewer`, and the server restarted.
                // The safe choice is to clear everything:
                self.stats.clear();
                self.recent.clear();
                self.slowest_by_index.clear();
                self.slowest_by_duration.clear();
            }
        }

        if let Some(last) = self.recent.iter().last() {
            // Assume there is a viewer viewing the newest frame,
            // and compress the previously newest frame to save RAM:
            if self.pack_frames {
                last.0.pack();
            }

            self.stats.add(&last.0);
        }

        let add_to_slowest = if self.slowest_by_duration.len() < self.max_slow {
            true
        } else if let Some(fastest_of_the_slow) = self.slowest_by_duration.iter().last() {
            new_frame.duration_ns() > fastest_of_the_slow.0.duration_ns()
        } else {
            false
        };

        if add_to_slowest {
            self.add_slow_frame(&new_frame);
        }

        self.add_recent_frame(&new_frame);
    }

    fn add_slow_frame(&mut self, new_frame: &Arc<FrameData>) {
        assert_eq!(self.slowest_by_duration.len(), self.slowest_by_index.len());

        self.slowest_by_duration
            .insert(OrderedByDuration(new_frame.clone()));
        self.slowest_by_index
            .insert(OrderedByIndex(new_frame.clone()));

        while self.slowest_by_duration.len() > self.max_slow {
            if let Some(removed_frame) = self.slowest_by_duration.pop_last() {
                let removed_by_index = OrderedByIndex(removed_frame.0.clone());
                self.slowest_by_index.remove(&removed_by_index);

                // Only remove from stats if the frame is not present in recent
                if self.recent.binary_search(&removed_by_index).is_err() {
                    self.stats.remove(&removed_frame.0);
                }
            }
        }
    }

    fn add_recent_frame(&mut self, new_frame: &Arc<FrameData>) {
        self.recent.push_back(OrderedByIndex(new_frame.clone()));

        while self.recent.len() > self.max_recent {
            if let Some(removed_frame) = self.recent.pop_front() {
                // Only remove from stats if the frame is not present in slowest
                if !self.slowest_by_index.contains(&removed_frame) {
                    self.stats.remove(&removed_frame.0);
                }
            }
        }
    }

    /// The latest fully captured frame of data.
    pub fn latest_frame(&self) -> Option<Arc<FrameData>> {
        self.recent.back().map(|f| f.0.clone())
    }

    /// Returns up to `n` latest fully captured frames of data.
    pub fn latest_frames(&self, n: usize) -> impl Iterator<Item = &Arc<FrameData>> {
        // Probably not the best way to do this, but since
        // [`self.recent`] is immutable in this context and
        // working with deque slices is complicated, we'll do
        // it this way for now.
        self.recent.iter().rev().take(n).rev().map(|f| &f.0)
    }

    /// Oldest first
    pub fn recent_frames(&self) -> impl Iterator<Item = &Arc<FrameData>> {
        self.recent.iter().map(|f| &f.0)
    }

    /// The slowest frames so far (or since last call to [`Self::clear_slowest()`])
    /// in chronological order.
    pub fn slowest_frames_chronological(&self) -> impl Iterator<Item = &Arc<FrameData>> {
        self.slowest_by_index.iter().map(|f| &f.0)
    }

    /// All frames sorted chronologically.
    pub fn all_uniq(&self) -> impl Iterator<Item = &Arc<FrameData>> {
        Itertools::merge(self.recent.iter(), self.slowest_by_index.iter())
            .dedup()
            .map(|f| &f.0)
    }

    /// Clean history of the slowest frames.
    pub fn clear_slowest(&mut self) {
        for frame in self.slowest_by_index.iter() {
            self.stats.remove(&frame.0);
        }

        self.slowest_by_duration.clear();
        self.slowest_by_index.clear();
    }

    /// How many frames of recent history to store.
    pub fn max_recent(&self) -> usize {
        self.max_recent
    }

    /// How many frames of recent history to store.
    pub fn set_max_recent(&mut self, max_recent: usize) {
        self.max_recent = max_recent;
    }

    /// How many slow "spike" frames to store.
    pub fn max_slow(&self) -> usize {
        self.max_slow
    }

    /// How many slow "spike" frames to store.
    pub fn set_max_slow(&mut self, max_slow: usize) {
        self.max_slow = max_slow;
    }

    /// Returns if frames are packed (compressed).
    pub fn pack_frames(&self) -> bool {
        self.pack_frames
    }

    /// Sets whether frames should be packed (compressed).
    /// Packing frames will increase CPU time and decrease memory usage.
    pub fn set_pack_frames(&mut self, pack_frames: bool) {
        self.pack_frames = pack_frames;
    }

    /// Retrieve statistics for added frames. This operation is efficient and suitable when
    /// frames have not been manipulated outside of `ProfileView`, such as being unpacked. For
    /// comprehensive statistics, refer to [`Self::stats_full()`]
    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    /// Retrieve detailed statistics by performing a full computation on all the added frames.
    pub fn stats_full(&self) -> FrameStats {
        FrameStats::from_frames(self.all_uniq().map(Arc::as_ref))
    }

    /// Export profile data as a `.puffin` file/stream.
    #[cfg(feature = "serialization")]
    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    pub fn write(&self, write: &mut impl std::io::Write) -> anyhow::Result<()> {
        write.write_all(b"PUF0")?;

        for frame in self.all_uniq() {
            frame.write_into(&self.scope_collection, false, write)?;
        }
        Ok(())
    }

    /// Import profile data from a `.puffin` file/stream.
    #[cfg(feature = "serialization")]
    pub fn read(read: &mut impl std::io::Read) -> anyhow::Result<Self> {
        let mut magic = [0_u8; 4];
        read.read_exact(&mut magic)?;
        if &magic != b"PUF0" {
            anyhow::bail!("Expected .puffin magic header of 'PUF0', found {:?}", magic);
        }

        let mut slf = Self {
            max_recent: usize::MAX,
            ..Default::default()
        };
        while let Some(frame) = FrameData::read_next(read)? {
            slf.add_frame(frame.into());
        }

        Ok(slf)
    }
}

// ----------------------------------------------------------------------------

/// Select the slowest frames, up to a certain count.
pub fn select_slowest(frames: &[Arc<FrameData>], max: usize) -> Vec<Arc<FrameData>> {
    let mut slowest: std::collections::BinaryHeap<OrderedByDuration> = Default::default();
    for frame in frames {
        slowest.push(OrderedByDuration(frame.clone()));
        while slowest.len() > max {
            slowest.pop();
        }
    }
    let mut slowest: Vec<_> = slowest.drain().map(|x| x.0).collect();
    slowest.sort_by_key(|frame| frame.frame_index());
    slowest
}

// ----------------------------------------------------------------------------

#[derive(Clone)]
struct OrderedByDuration(Arc<FrameData>);

impl Ord for OrderedByDuration {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.0.duration_ns().cmp(&other.0.duration_ns()).reverse() {
            Ordering::Equal => self.0.frame_index().cmp(&other.0.frame_index()),
            res => res,
        }
    }
}

impl PartialOrd for OrderedByDuration {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for OrderedByDuration {}

impl PartialEq for OrderedByDuration {
    fn eq(&self, other: &Self) -> bool {
        self.0.duration_ns() == other.0.duration_ns()
            && self.0.frame_index() == other.0.frame_index()
    }
}

// ----------------------------------------------------------------------------
#[derive(Clone)]
struct OrderedByIndex(Arc<FrameData>);

impl Ord for OrderedByIndex {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.0.frame_index().cmp(&other.0.frame_index()) {
            Ordering::Equal => self.0.duration_ns().cmp(&other.0.duration_ns()),
            res => res,
        }
    }
}

impl PartialOrd for OrderedByIndex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for OrderedByIndex {}

impl PartialEq for OrderedByIndex {
    fn eq(&self, other: &Self) -> bool {
        self.0.frame_index() == other.0.frame_index()
            && self.0.duration_ns() == other.0.duration_ns()
    }
}

// ----------------------------------------------------------------------------

/// Automatically connects to [`crate::GlobalProfiler`].
pub struct GlobalFrameView {
    sink_id: FrameSinkId,
    view: Arc<parking_lot::Mutex<FrameView>>,
}

impl Default for GlobalFrameView {
    fn default() -> Self {
        let view = Arc::new(parking_lot::Mutex::new(FrameView::default()));
        let view_clone = view.clone();
        let mut profiler = crate::GlobalProfiler::lock();
        let sink_id = profiler.add_sink(Box::new(move |frame| {
            view_clone.lock().add_frame(frame);
        }));
        // GlobalFrameView might be created after scope scopes were already created
        // and our registered sink won't see them without prior propagation.
        profiler.emit_scope_snapshot();

        Self { sink_id, view }
    }
}

impl Drop for GlobalFrameView {
    fn drop(&mut self) {
        crate::GlobalProfiler::lock().remove_sink(self.sink_id);
    }
}

impl GlobalFrameView {
    /// Sink ID
    pub fn sink_id(&self) -> FrameSinkId {
        self.sink_id
    }

    /// View the latest profiling data.
    pub fn lock(&self) -> parking_lot::MutexGuard<'_, FrameView> {
        self.view.lock()
    }
}

// ----------------------------------------------------------------------------

fn stats_entry(frame: &FrameData) -> (usize, usize) {
    let info = frame.packing_info();
    (
        info.packed_size.unwrap_or(0) + info.unpacked_size.unwrap_or(0),
        info.unpacked_size.is_some() as usize,
    )
}

/// Collect statistics for maintained frames
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    unique_frames: usize,
    total_ram_used: usize,
    unpacked_frames: usize,
}

impl FrameStats {
    /// Creates a `FrameStats` instance from an iterator of frames.
    pub fn from_frames<'a>(frames: impl Iterator<Item = &'a FrameData>) -> Self {
        let mut stats = FrameStats::default();

        for frame in frames {
            stats.add(frame);
        }

        stats
    }

    /// Adds a frame's statistics to the `FrameStats`.
    fn add(&mut self, frame: &FrameData) {
        let (total, unpacked) = stats_entry(frame);

        self.total_ram_used = self.total_ram_used.saturating_add(total);
        self.unpacked_frames = self.unpacked_frames.saturating_add(unpacked);
        self.unique_frames = self.unique_frames.saturating_add(1);
    }

    /// Removes a frame's statistics from the `FrameStats`.
    fn remove(&mut self, frame: &FrameData) {
        let (total, unpacked) = stats_entry(frame);

        self.total_ram_used = self.total_ram_used.saturating_sub(total);
        self.unpacked_frames = self.unpacked_frames.saturating_sub(unpacked);
        self.unique_frames = self.unique_frames.saturating_sub(1);
    }

    /// Returns the number of unique frames.
    pub fn frames(&self) -> usize {
        self.unique_frames
    }

    /// Returns the number of unpacked frames.
    pub fn unpacked_frames(&self) -> usize {
        self.unpacked_frames
    }

    /// Returns the total bytes of RAM used.
    pub fn bytes_of_ram_used(&self) -> usize {
        self.total_ram_used
    }

    /// Clears all statistics in `FrameStats`.
    pub fn clear(&mut self) {
        self.unique_frames = 0;
        self.unpacked_frames = 0;
        self.total_ram_used = 0;
    }
}
