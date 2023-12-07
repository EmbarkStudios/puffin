use std::sync::{Arc, Mutex};

use crate::{FrameData, FrameSinkId, ScopeCollection};

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
}

impl Default for FrameView {
    fn default() -> Self {
        let max_recent = 60 * 60 * 5;
        let max_slow = 256;

        Self {
            recent: std::collections::VecDeque::with_capacity(max_recent),
            max_recent,
            slowest: std::collections::BinaryHeap::with_capacity(max_slow),
            max_slow,
            pack_frames: true,
        }
    }
}

impl FrameView {
    pub fn is_empty(&self) -> bool {
        self.recent.is_empty() && self.slowest.is_empty()
    }

    pub fn add_frame(&mut self, new_frame: Arc<FrameData>) {
        let mut scope_collection = ScopeCollection::instance_mut();

        // Register all scopes from the new frame into the scope collection.
        for new_scope in &new_frame.scope_delta {
            scope_collection.insert(new_scope.as_ref().clone());
        }

        if let Some(last) = self.recent.back() {
            if new_frame.frame_index() <= last.frame_index() {
                // A frame from the past!?
                // Likely we are `puffin_viewer`, and the server restarted.
                // The safe choice is to clear everything:
                self.recent.clear();
                self.slowest.clear();
            }
        }

        let add_to_slowest = if self.slowest.len() < self.max_slow {
            true
        } else if let Some(fastest_of_the_slow) = self.slowest.peek() {
            new_frame.duration_ns() > fastest_of_the_slow.0.duration_ns()
        } else {
            false
        };

        if add_to_slowest {
            self.slowest.push(OrderedByDuration(new_frame.clone()));
            while self.slowest.len() > self.max_slow {
                self.slowest.pop();
            }
        }

        if let Some(last) = self.recent.back() {
            // Assume there is a viewer viewing the newest frame,
            // and compress the previously newest frame to save RAM:
            if self.pack_frames {
                last.pack();
            }
        }

        self.recent.push_back(new_frame);
        while self.recent.len() > self.max_recent {
            self.recent.pop_front();
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
        let mut all: Vec<_> = self.slowest.iter().map(|f| f.0.clone()).collect();
        all.extend(self.recent.iter().cloned());
        all.sort_by_key(|frame| frame.frame_index());
        all.dedup_by_key(|frame| frame.frame_index());
        all
    }

    /// Clean history of the slowest frames.
    pub fn clear_slowest(&mut self) {
        self.slowest.clear();
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

    /// Export profile data as a `.puffin` file/stream.
    #[cfg(feature = "serialization")]
    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    pub fn write(&self, write: &mut impl std::io::Write) -> anyhow::Result<()> {
        write.write_all(b"PUF0")?;

        let slowest_frames = self.slowest.iter().map(|f| &f.0);
        let mut frames: Vec<_> = slowest_frames.chain(self.recent.iter()).collect();
        frames.sort_by_key(|frame| frame.frame_index());
        frames.dedup_by_key(|frame| frame.frame_index());

        for frame in frames {
            frame.write_into(false, write)?;
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
