use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::{FrameData, FrameIndex, FrameSinkId};

/// A view of recent and slowest frames, used by GUIs.
#[derive(Clone)]
pub struct FrameView {
    /// newest first
    recent: std::collections::VecDeque<Arc<FrameData>>,
    max_recent: usize,

    slowest: std::collections::BinaryHeap<OrderedByDuration>,
    max_slow: usize,

    /// Minimizes memory usage at the expense of CPU time.
    ///
    /// Only recommended if you set a large max_recent size.
    pack_frames: bool,

    /// Maintain stats as we add/remove frames
    stats: FrameStats,
}

impl Default for FrameView {
    fn default() -> Self {
        let max_recent = 60 * 60 * 5;
        let max_slow = 256;
        let stats = Default::default();

        Self {
            recent: std::collections::VecDeque::with_capacity(max_recent),
            max_recent,
            slowest: std::collections::BinaryHeap::with_capacity(max_slow),
            max_slow,
            pack_frames: true,
            stats,
        }
    }
}

impl FrameView {
    pub fn is_empty(&self) -> bool {
        self.recent.is_empty() && self.slowest.is_empty()
    }

    pub fn add_frame(&mut self, new_frame: Arc<FrameData>) {
        if let Some(last) = self.recent.back() {
            if new_frame.frame_index() <= last.frame_index() {
                // A frame from the past!?
                // Likely we are `puffin_viewer`, and the server restarted.
                // The safe choice is to clear everything:
                self.recent.clear();
                self.slowest.clear();
                self.stats.clear();
            }
        }

        let add_to_slowest = if self.slowest.len() < self.max_slow {
            true
        } else if let Some(fastest_of_the_slow) = self.slowest.peek() {
            new_frame.duration_ns() > fastest_of_the_slow.0.duration_ns()
        } else {
            false
        };

        if let Some(last) = self.recent.back() {
            // Assume there is a viewer viewing the newest frame,
            // and compress the previously newest frame to save RAM:
            if self.pack_frames {
                last.pack();
            }

            self.stats.add(last);
        }

        if add_to_slowest {
            self.add_slow_frame(&new_frame);
        }

        self.add_recent_frame(&new_frame);
    }

    pub fn add_slow_frame(&mut self, new_frame: &Arc<FrameData>) {
        self.slowest.push(OrderedByDuration(new_frame.clone()));

        while self.slowest.len() > self.max_slow {
            if let Some(removed_frame) = self.slowest.pop() {
                // Only remove from stats if the frame is not present in recent
                if self
                    .recent
                    .binary_search_by_key(&removed_frame.0.frame_index(), |f| f.frame_index())
                    .is_err()
                {
                    self.stats.remove(&removed_frame.0);
                }
            }
        }
    }

    pub fn add_recent_frame(&mut self, new_frame: &Arc<FrameData>) {
        self.recent.push_back(new_frame.clone());

        while self.recent.len() > self.max_recent {
            if let Some(removed_frame) = self.recent.pop_front() {
                // Only remove from stats if the frame is not present in slowest
                if !self
                    .slowest
                    .iter()
                    .any(|f| removed_frame.frame_index() == f.0.frame_index())
                {
                    self.stats.remove(&removed_frame);
                }
            }
        }
    }

    /// The latest fully captured frame of data.
    pub fn latest_frame(&self) -> Option<Arc<FrameData>> {
        self.recent.back().cloned()
    }

    /// Returns up to `n` latest fully captured frames of data.
    pub fn latest_frames(&self, n: usize) -> Vec<Arc<FrameData>> {
        // Probably not the best way to do this, but since
        // [`self.recent`] is immutable in this context and
        // working with deque slices is complicated, we'll do
        // it this way for now.
        self.recent.iter().rev().take(n).rev().cloned().collect()
    }

    /// Oldest first
    pub fn recent_frames(&self) -> impl Iterator<Item = &Arc<FrameData>> {
        self.recent.iter()
    }

    /// The slowest frames so far (or since last call to [`Self::clear_slowest()`])
    /// in chronological order.
    pub fn slowest_frames_chronological(&self) -> Vec<Arc<FrameData>> {
        let mut frames: Vec<_> = self.slowest.iter().map(|f| f.0.clone()).collect();
        frames.sort_by_key(|frame| frame.frame_index());
        frames
    }

    /// All frames sorted chronologically.
    pub fn all_uniq(&self) -> Vec<Arc<FrameData>> {
        let mut all: Vec<_> = self
            .slowest
            .iter()
            .map(|f| f.0.clone())
            .chain(self.recent.iter().cloned())
            .collect();

        all.sort_by_key(|frame| frame.frame_index());
        all.dedup_by_key(|frame| frame.frame_index());
        all
    }

    /// Clean history of the slowest frames.
    pub fn clear_slowest(&mut self) {
        for frame in self.slowest.drain() {
            self.stats.remove(&frame.0);
        }
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

    pub fn pack_frames(&self) -> bool {
        self.pack_frames
    }

    pub fn set_pack_frames(&mut self, pack_frames: bool) {
        self.pack_frames = pack_frames;
    }

    /// Retrieve statistics for added frames. This operation is efficient and suitable when
    /// frames have not been manipulated outside of `ProfileView`, such as being unpacked. For
    /// comprehensive statistics, refer to [`Self::stats_full()`]
    pub fn stats(&self) -> &FrameStats {
        &self.stats
    }

    /// Retrieve detailed statistics by performing a full computation on all the added frames.
    pub fn stats_full(&self) -> FrameStats {
        FrameStats::from_frames(self.all_uniq().map(Arc::as_ref))
    }

    /// Export profile data as a `.puffin` file.
    #[cfg(feature = "serialization")]
    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    pub fn save_to_path(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let mut file = std::fs::File::create(path)?;
        self.save_to_writer(&mut file)
    }

    /// Export profile data as a `.puffin` file.
    #[cfg(feature = "serialization")]
    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    pub fn save_to_writer(&self, write: &mut impl std::io::Write) -> anyhow::Result<()> {
        write.write_all(b"PUF0")?;

        let slowest_frames = self.slowest.iter().map(|f| &f.0);
        let mut frames: Vec<_> = slowest_frames.chain(self.recent.iter()).collect();
        frames.sort_by_key(|frame| frame.frame_index());
        frames.dedup_by_key(|frame| frame.frame_index());

        for frame in frames {
            frame.write_into(write)?;
        }
        Ok(())
    }

    /// Import profile data from a `.puffin` file.
    #[cfg(feature = "serialization")]
    pub fn load_path(path: &std::path::Path) -> anyhow::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        Self::load_reader(&mut file)
    }

    /// Import profile data from a `.puffin` file.
    #[cfg(feature = "serialization")]
    pub fn load_reader(read: &mut impl std::io::Read) -> anyhow::Result<Self> {
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

impl PartialEq for OrderedByDuration {
    fn eq(&self, other: &Self) -> bool {
        self.0.duration_ns().eq(&other.0.duration_ns())
    }
}
impl Eq for OrderedByDuration {}

impl PartialOrd for OrderedByDuration {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedByDuration {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.duration_ns().cmp(&other.0.duration_ns()).reverse()
    }
}

// ----------------------------------------------------------------------------

/// Automatically connects to [`crate::GlobalProfiler`].
pub struct GlobalFrameView {
    sink_id: FrameSinkId,
    view: Arc<Mutex<FrameView>>,
}

impl Default for GlobalFrameView {
    fn default() -> Self {
        let view = Arc::new(Mutex::new(FrameView::default()));
        let view_clone = view.clone();
        let sink_id = crate::GlobalProfiler::lock().add_sink(Box::new(move |frame| {
            view_clone.lock().unwrap().add_frame(frame);
        }));
        Self { sink_id, view }
    }
}

impl Drop for GlobalFrameView {
    fn drop(&mut self) {
        crate::GlobalProfiler::lock().remove_sink(self.sink_id);
    }
}

impl GlobalFrameView {
    /// View the latest profiling data.
    pub fn lock(&self) -> std::sync::MutexGuard<'_, FrameView> {
        self.view.lock().unwrap()
    }
}

// ----------------------------------------------------------------------------

/// Collect statistics for maintained frames
#[derive(Clone, Debug, Default)]
pub struct FrameStats {
    unique_frames: HashSet<FrameIndex>,
    total_ram_used: usize,
    unpacked_frames: usize,
}

impl FrameStats {
    pub fn from_frames<'a>(frames: impl Iterator<Item = &'a FrameData>) -> Self {
        let mut stats = FrameStats::default();

        for frame in frames {
            stats.add(frame);
        }

        stats
    }

    fn add(&mut self, frame: &FrameData) {
        if self.unique_frames.insert(frame.frame_index()) {
            self.total_ram_used = self
                .total_ram_used
                .saturating_add(frame.bytes_of_ram_used());

            self.unpacked_frames = self
                .unpacked_frames
                .saturating_add(frame.has_unpacked() as usize);
        }
    }

    fn remove(&mut self, frame: &FrameData) {
        if self.unique_frames.remove(&frame.frame_index()) {
            self.total_ram_used = self
                .total_ram_used
                .saturating_sub(frame.bytes_of_ram_used());

            self.unpacked_frames = self
                .unpacked_frames
                .saturating_sub(frame.has_unpacked() as usize);
        }
    }

    pub fn frames(&self) -> usize {
        assert!(self.unique_frames.len() >= self.unpacked_frames);
        self.unique_frames.len()
    }

    pub fn unpacked_frames(&self) -> usize {
        assert!(self.unique_frames.len() >= self.unpacked_frames);
        self.unpacked_frames
    }

    pub fn bytes_of_ram_used(&self) -> usize {
        assert!(self.unique_frames.len() >= self.unpacked_frames);
        self.total_ram_used
    }

    pub fn clear(&mut self) {
        self.unique_frames.clear();

        self.unpacked_frames = 0;
        self.total_ram_used = 0;
    }
}
