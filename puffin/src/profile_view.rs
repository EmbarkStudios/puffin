use std::sync::{Arc, Mutex};

use crate::{FrameData, FrameSinkId};

/// A view of recent and slowest frames, used by GUI:s.
#[derive(Clone)]
pub struct FrameView {
    /// newest first
    recent_frames: std::collections::VecDeque<Arc<FrameData>>,
    max_recent: usize,

    slowest_frames: std::collections::BinaryHeap<OrderedByDuration>,
    max_slow: usize,
}

impl Default for FrameView {
    fn default() -> Self {
        let max_recent = 60 * 60 * 15;
        let max_slow = 256;

        Self {
            recent_frames: std::collections::VecDeque::with_capacity(max_recent),
            max_recent,
            slowest_frames: std::collections::BinaryHeap::with_capacity(max_slow),
            max_slow,
        }
    }
}

impl FrameView {
    pub fn is_empty(&self) -> bool {
        self.recent_frames.is_empty() && self.slowest_frames.is_empty()
    }

    pub fn add_frame(&mut self, new_frame: Arc<FrameData>) {
        if let Some(last) = self.recent_frames.back() {
            if new_frame.frame_index <= last.frame_index {
                // A frame from the past!?
                // Likely we are `puffin_viewer`, and the server restarted.
                // The safe choice is to clear everything:
                self.recent_frames.clear();
                self.slowest_frames.clear();
            }
        }

        let add_to_slowest = if self.slowest_frames.len() < self.max_slow {
            true
        } else if let Some(fastest_of_the_slow) = self.slowest_frames.peek() {
            new_frame.duration_ns() > fastest_of_the_slow.0.duration_ns()
        } else {
            false
        };

        if add_to_slowest {
            self.slowest_frames
                .push(OrderedByDuration(new_frame.clone()));
            while self.slowest_frames.len() > self.max_slow {
                self.slowest_frames.pop();
            }
        }

        self.recent_frames.push_back(new_frame);
        while self.recent_frames.len() > self.max_recent {
            self.recent_frames.pop_front();
        }
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

    /// Export profile data as a `.puffin` file.
    #[cfg(feature = "serialization")]
    pub fn save_to_path(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let mut file = std::fs::File::create(path)?;
        self.save_to_writer(&mut file)
    }

    /// Export profile data as a `.puffin` file.
    #[cfg(feature = "serialization")]
    pub fn save_to_writer(&self, write: &mut impl std::io::Write) -> anyhow::Result<()> {
        write.write_all(b"PUF0")?;

        let slowest_frames = self.slowest_frames.iter().map(|f| &f.0);
        let mut frames: Vec<_> = slowest_frames.chain(self.recent_frames.iter()).collect();
        frames.sort_by_key(|frame| frame.frame_index);
        frames.dedup_by_key(|frame| frame.frame_index);

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
    slowest.sort_by_key(|frame| frame.frame_index);
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
