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

// BEGIN - Embark standard lints v0.4
// do not change or add/remove here, but one can add exceptions after this section
// for more info see: <https://github.com/EmbarkStudios/rust-ecosystem/issues/59>
#![deny(unsafe_code)]
#![warn(
    clippy::all,
    clippy::await_holding_lock,
    clippy::char_lit_as_u8,
    clippy::checked_conversions,
    clippy::dbg_macro,
    clippy::debug_assert_with_mut_call,
    clippy::doc_markdown,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::exit,
    clippy::expl_impl_clone_on_copy,
    clippy::explicit_deref_methods,
    clippy::explicit_into_iter_loop,
    clippy::fallible_impl_from,
    clippy::filter_map_next,
    clippy::float_cmp_const,
    clippy::fn_params_excessive_bools,
    clippy::if_let_mutex,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::inefficient_to_string,
    clippy::invalid_upcast_comparisons,
    clippy::large_types_passed_by_value,
    clippy::let_unit_value,
    clippy::linkedlist,
    clippy::lossy_float_literal,
    clippy::macro_use_imports,
    clippy::manual_ok_or,
    clippy::map_err_ignore,
    clippy::map_flatten,
    clippy::map_unwrap_or,
    clippy::match_on_vec_items,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::mem_forget,
    clippy::mismatched_target_os,
    clippy::mut_mut,
    clippy::mutex_integer,
    clippy::needless_borrow,
    clippy::needless_continue,
    clippy::option_option,
    clippy::path_buf_push_overwrite,
    clippy::ptr_as_ptr,
    clippy::ref_option_ref,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::same_functions_in_if_condition,
    clippy::semicolon_if_nothing_returned,
    clippy::string_add_assign,
    clippy::string_add,
    clippy::string_lit_as_bytes,
    clippy::string_to_string,
    clippy::todo,
    clippy::trait_duplication_in_bounds,
    clippy::unimplemented,
    clippy::unnested_or_patterns,
    clippy::unused_self,
    clippy::useless_transmute,
    clippy::verbose_file_reads,
    clippy::zero_sized_map_values,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms
)]
// END - Embark standard lints v0.4
// crate-specific exceptions:

mod data;
mod merge;
mod profile_view;

pub use data::*;
pub use merge::*;
pub use profile_view::{select_slowest, FrameView, GlobalFrameView};

use parking_lot::{Mutex, RwLock};
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

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

// ----------------------------------------------------------------------------

/// Stream of profiling events from one thread.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
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

    pub fn clear(&mut self) {
        self.0.clear();
    }

    fn extend(&mut self, bytes: &[u8]) {
        self.0.extend(bytes);
    }
}

impl From<Vec<u8>> for Stream {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

// ----------------------------------------------------------------------------

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
    #[inline]
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
#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct ThreadInfo {
    /// Useful for ordering threads.
    pub start_time_ns: Option<NanoSecond>,
    /// Name of the thread
    pub name: String,
}

// ----------------------------------------------------------------------------

pub type FrameIndex = u64;

/// A [`Stream`] plus some info about it.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct StreamInfo {
    /// The raw profile data.
    pub stream: Stream,

    /// Total number of scopes in the stream.
    pub num_scopes: usize,

    /// The depth of the deepest scope.
    /// `0` mean no scopes, `1` some scopes without children, etc.
    pub depth: usize,

    /// The smallest and largest nanosecond value in the stream.
    ///
    /// The default value is `(NanoSecond::MAX, NanoSecond::MIN)` which indicates an empty stream.
    pub range_ns: (NanoSecond, NanoSecond),
}

impl Default for StreamInfo {
    fn default() -> Self {
        Self {
            stream: Default::default(),
            num_scopes: 0,
            depth: 0,
            range_ns: (NanoSecond::MAX, NanoSecond::MIN),
        }
    }
}

impl StreamInfo {
    /// Parse a stream to count the depth, number of scopes in it etc.
    ///
    /// Try to avoid calling this, and instead keep score while collecting a `StreamInfo`.
    pub fn parse(stream: Stream) -> Result<StreamInfo> {
        let top_scopes = Reader::from_start(&stream).read_top_scopes()?;
        if top_scopes.is_empty() {
            Ok(StreamInfo {
                stream,
                num_scopes: 0,
                depth: 0,
                range_ns: (NanoSecond::MAX, NanoSecond::MIN),
            })
        } else {
            let (num_scopes, depth) = Reader::count_scope_and_depth(&stream)?;
            let min_ns = top_scopes.first().unwrap().record.start_ns;
            let max_ns = top_scopes.last().unwrap().record.stop_ns();

            Ok(StreamInfo {
                stream,
                num_scopes,
                depth,
                range_ns: (min_ns, max_ns),
            })
        }
    }

    pub fn extend(&mut self, other: &StreamInfoRef<'_>) {
        self.stream.extend(other.stream);
        self.num_scopes += other.num_scopes;
        self.depth = self.depth.max(other.depth);
        self.range_ns.0 = self.range_ns.0.min(other.range_ns.0);
        self.range_ns.1 = self.range_ns.1.max(other.range_ns.1);
    }

    pub fn clear(&mut self) {
        let Self {
            stream,
            num_scopes,
            depth,
            range_ns,
        } = self;
        stream.clear();
        *num_scopes = 0;
        *depth = 0;
        *range_ns = (NanoSecond::MAX, NanoSecond::MIN);
    }

    pub fn as_stream_into_ref(&self) -> StreamInfoRef<'_> {
        StreamInfoRef {
            stream: self.stream.bytes(),
            num_scopes: self.num_scopes,
            depth: self.depth,
            range_ns: self.range_ns,
        }
    }
}

/// A reference to the contents of a [`StreamInfo`].
#[derive(Clone, Copy)]
pub struct StreamInfoRef<'a> {
    /// The raw profile data.
    pub stream: &'a [u8],

    /// Total number of scopes in the stream.
    pub num_scopes: usize,

    /// The depth of the deepest scope.
    /// `0` mean no scopes, `1` some scopes without children, etc.
    pub depth: usize,

    /// The smallest and largest nanosecond value in the stream.
    ///
    /// The default value is `(NanoSecond::MAX, NanoSecond::MIN)` which indicates an empty stream.
    pub range_ns: (NanoSecond, NanoSecond),
}

// ----------------------------------------------------------------------------

pub type ThreadStreams = BTreeMap<ThreadInfo, Arc<StreamInfo>>;

/// Meta-information about a frame.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug)]
pub struct FrameMeta {
    /// What frame this is (counting from 0 at application startup).
    pub frame_index: FrameIndex,
    /// The span we cover.
    pub range_ns: (NanoSecond, NanoSecond),
    /// The uncompressed size.
    pub num_bytes: usize,
    /// Total number of scopes.
    pub num_scopes: usize,
}

/// One frame worth of profile data, collected from many sources.
///
/// More often encoded as a [`CompressedFrameData`].
pub struct UnpackedFrameData {
    pub meta: FrameMeta,
    /// `None` if still compressed.
    pub thread_streams: ThreadStreams,
}

impl UnpackedFrameData {
    pub fn new(
        frame_index: FrameIndex,
        thread_streams: BTreeMap<ThreadInfo, StreamInfo>,
    ) -> Result<Self> {
        let thread_streams: BTreeMap<_, _> = thread_streams
            .into_iter()
            .map(|(info, stream_info)| (info, Arc::new(stream_info)))
            .collect();

        let mut num_bytes = 0;
        let mut num_scopes = 0;

        let mut min_ns = NanoSecond::MAX;
        let mut max_ns = NanoSecond::MIN;
        for stream_info in thread_streams.values() {
            num_bytes += stream_info.stream.len();
            num_scopes += stream_info.num_scopes;
            min_ns = min_ns.min(stream_info.range_ns.0);
            max_ns = max_ns.max(stream_info.range_ns.1);
        }

        if min_ns <= max_ns {
            Ok(Self {
                meta: FrameMeta {
                    frame_index,
                    range_ns: (min_ns, max_ns),
                    num_bytes,
                    num_scopes,
                },
                thread_streams,
            })
        } else {
            Err(Error::Empty)
        }
    }

    pub fn frame_index(&self) -> u64 {
        self.meta.frame_index
    }

    pub fn range_ns(&self) -> (NanoSecond, NanoSecond) {
        self.meta.range_ns
    }

    pub fn duration_ns(&self) -> NanoSecond {
        let (min, max) = self.meta.range_ns;
        max - min
    }
}

/// One frame worth of profile data, collected from many sources.
pub struct FrameData {
    pub meta: FrameMeta,
    /// * `None` if still compressed.
    /// * `Some(Err(…))` if there was a problem during unpacking.
    /// * `Some(Ok(…))` if unpacked.
    unpacked_frame: RwLock<Option<anyhow::Result<Arc<UnpackedFrameData>>>>,
    /// [`FrameMeta::thread_streams`], compressed with zstd.
    /// `None` if not yet compressed.
    zstd_streams: RwLock<Option<Vec<u8>>>,
}

impl FrameData {
    pub fn new(
        frame_index: FrameIndex,
        thread_streams: BTreeMap<ThreadInfo, StreamInfo>,
    ) -> Result<Self> {
        Ok(Self::from_unpacked(Arc::new(UnpackedFrameData::new(
            frame_index,
            thread_streams,
        )?)))
    }

    /// Will lazily compress.
    pub fn from_unpacked(frame: Arc<UnpackedFrameData>) -> Self {
        Self {
            meta: frame.meta.clone(),
            unpacked_frame: RwLock::new(Some(Ok(frame))),
            zstd_streams: RwLock::new(None),
        }
    }

    pub fn frame_index(&self) -> u64 {
        self.meta.frame_index
    }

    pub fn range_ns(&self) -> (NanoSecond, NanoSecond) {
        self.meta.range_ns
    }

    pub fn duration_ns(&self) -> NanoSecond {
        let (min, max) = self.meta.range_ns;
        max - min
    }

    /// Number of bytes used when compressed, if known.
    pub fn compressed_size(&self) -> Option<usize> {
        self.zstd_streams.read().as_ref().map(|c| c.len())
    }

    /// Lazily unpacks.
    pub fn unpack(&self) -> anyhow::Result<Arc<UnpackedFrameData>> {
        fn unpack_frame_data(
            meta: FrameMeta,
            compressed: &[u8],
        ) -> anyhow::Result<UnpackedFrameData> {
            use anyhow::Context as _;
            use bincode::Options as _;

            let streams_serialized = zstd::decode_all(compressed).context("zstd decompress")?;

            let thread_streams: ThreadStreams = bincode::options()
                .deserialize(&streams_serialized)
                .context("bincode deserialize")?;

            Ok(UnpackedFrameData {
                meta,
                thread_streams,
            })
        }

        let needs_unpack = self.unpacked_frame.read().is_none();
        if needs_unpack {
            let compressed_lock = self.zstd_streams.read();
            let compressed = compressed_lock
                .as_ref()
                .expect("FrameData is neither compressed or uncompressed");

            let frame_data_result = unpack_frame_data(self.meta.clone(), compressed);
            let frame_data_result = frame_data_result.map(Arc::new);
            *self.unpacked_frame.write() = Some(frame_data_result);
        }

        match self.unpacked_frame.read().as_ref().unwrap() {
            Ok(frame) => Ok(frame.clone()),
            Err(err) => Err(anyhow::format_err!("{}", err)), // can't clone `anyhow::Error`
        }
    }

    fn compress_if_needed(&self) {
        use bincode::Options as _;
        let needs_compress = self.zstd_streams.read().is_none();
        if needs_compress {
            let unpacked_frame = self
                .unpacked_frame
                .read()
                .as_ref()
                .expect("We should have an unpacked frame if we don't have a compressed one")
                .as_ref()
                .expect("The unpacked frame should be error free, since it doesn't come from compressed source")
                .clone();

            let streams_serialized = bincode::options()
                .serialize(&unpacked_frame.thread_streams)
                .expect("bincode failed to encode");

            // zstd cuts sizes in half compared to lz4_flex
            let level = 3;
            let streams_compressed =
                zstd::encode_all(std::io::Cursor::new(&streams_serialized), level)
                    .expect("zstd failed to compress");

            *self.zstd_streams.write() = Some(streams_compressed);
        }
    }

    /// Writes one [`FrameData`] into a stream, prefixed by it's length (u32 le).
    #[cfg(feature = "serialization")]
    pub fn write_into(&self, write: &mut impl std::io::Write) -> anyhow::Result<()> {
        use bincode::Options as _;
        let meta_serialized = bincode::options().serialize(&self.meta)?;

        write.write_all(b"PFD2")?;
        write.write_all(&(meta_serialized.len() as u32).to_le_bytes())?;
        write.write_all(&meta_serialized)?;

        self.compress_if_needed();
        let zstd_streams_lock = self.zstd_streams.read();
        let zstd_streams = zstd_streams_lock.as_ref().unwrap();

        write.write_all(&(zstd_streams.len() as u32).to_le_bytes())?;
        write.write_all(zstd_streams)?;

        Ok(())
    }

    /// Read the next [`FrameData`] from a stream.
    ///
    /// `None` is returned if the end of the stream is reached (EOF),
    /// or an end-of-stream sentinel of 0u32 is read.
    #[cfg(feature = "serialization")]
    pub fn read_next(read: &mut impl std::io::Read) -> anyhow::Result<Option<Self>> {
        use anyhow::Context as _;
        use bincode::Options as _;

        let mut header = [0_u8; 4];
        if let Err(err) = read.read_exact(&mut header) {
            if err.kind() == std::io::ErrorKind::UnexpectedEof {
                return Ok(None);
            } else {
                return Err(err.into());
            }
        }

        #[derive(Clone)]
        #[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
        pub struct LegacyFrameData {
            pub frame_index: FrameIndex,
            pub thread_streams: ThreadStreams,
            pub range_ns: (NanoSecond, NanoSecond),
            pub num_bytes: usize,
            pub num_scopes: usize,
        }

        impl LegacyFrameData {
            fn into_frame_data(self) -> UnpackedFrameData {
                let Self {
                    frame_index,
                    thread_streams,
                    range_ns,
                    num_bytes,
                    num_scopes,
                } = self;
                UnpackedFrameData {
                    meta: FrameMeta {
                        frame_index,
                        range_ns,
                        num_bytes,
                        num_scopes,
                    },
                    thread_streams,
                }
            }

            fn into_compressed_frame_data(self) -> FrameData {
                FrameData::from_unpacked(Arc::new(self.into_frame_data()))
            }
        }

        if header == [0_u8; 4] {
            Ok(None) // end-of-stream sentinel.
        } else if header.starts_with(b"PFD") {
            if &header == b"PFD0" {
                let mut compressed_length = [0_u8; 4];
                read.read_exact(&mut compressed_length)?;
                let compressed_length = u32::from_le_bytes(compressed_length) as usize;
                let mut compressed = vec![0_u8; compressed_length];
                read.read_exact(&mut compressed)?;

                let serialized =
                    lz4_flex::decompress_size_prepended(&compressed).context("lz4 decompress")?;

                let legacy: LegacyFrameData = bincode::options()
                    .deserialize(&serialized)
                    .context("bincode deserialize")?;
                Ok(Some(legacy.into_compressed_frame_data()))
            } else if &header == b"PFD1" {
                let mut compressed_length = [0_u8; 4];
                read.read_exact(&mut compressed_length)?;
                let compressed_length = u32::from_le_bytes(compressed_length) as usize;
                let mut compressed = vec![0_u8; compressed_length];
                read.read_exact(&mut compressed)?;

                let serialized = zstd::decode_all(&compressed[..]).context("zstd decompress")?;

                let legacy: LegacyFrameData = bincode::options()
                    .deserialize(&serialized)
                    .context("bincode deserialize")?;
                Ok(Some(legacy.into_compressed_frame_data()))
            } else if &header == b"PFD2" {
                let mut meta_length = [0_u8; 4];
                read.read_exact(&mut meta_length)?;
                let meta_length = u32::from_le_bytes(meta_length) as usize;
                let mut meta = vec![0_u8; meta_length];
                read.read_exact(&mut meta)?;

                let meta: FrameMeta = bincode::options()
                    .deserialize(&meta)
                    .context("bincode deserialize")?;

                let mut streams_compressed_length = [0_u8; 4];
                read.read_exact(&mut streams_compressed_length)?;
                let streams_compressed_length =
                    u32::from_le_bytes(streams_compressed_length) as usize;
                let mut streams_compressed = vec![0_u8; streams_compressed_length];
                read.read_exact(&mut streams_compressed)?;

                Ok(Some(Self {
                    meta,
                    unpacked_frame: RwLock::new(None),
                    zstd_streams: RwLock::new(Some(streams_compressed)),
                }))
            } else {
                anyhow::bail!("Failed to decode: this data is newer than this reader. Please update your puffin version!");
            }
        } else {
            // Old packet without magic header
            let mut bytes = vec![0_u8; u32::from_le_bytes(header) as usize];
            read.read_exact(&mut bytes)?;

            use bincode::Options as _;
            let legacy: LegacyFrameData = bincode::options()
                .deserialize(&bytes)
                .context("bincode deserialize")?;
            Ok(Some(legacy.into_compressed_frame_data()))
        }
    }
}

// ----------------------------------------------------------------------------

type NsSource = fn() -> NanoSecond;
type ThreadReporter = fn(ThreadInfo, &StreamInfoRef<'_>);

/// Report a stream of profile data from a thread to the [`GlobalProfiler`] singleton.
pub fn global_reporter(info: ThreadInfo, stream_info: &StreamInfoRef<'_>) {
    GlobalProfiler::lock().report(info, stream_info);
}

/// Collects profiling data for one thread
pub struct ThreadProfiler {
    stream_info: StreamInfo,
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
    /// If not called, each thread will use the default nanosecond source (`[now_ns]`)
    /// and report scopes to the global profiler ([`global_reporter`]).
    ///
    /// For instance, when compiling for WASM the default timing function (`[now_ns]`) won't work,
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

        self.stream_info.range_ns.0 = self.stream_info.range_ns.0.min(now_ns);
        self.stream_info
            .stream
            .begin_scope(now_ns, id, location, data)
    }

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
            (self.reporter)(info, &self.stream_info.as_stream_into_ref());
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

// ----------------------------------------------------------------------------

/// Add these to [`GlobalProfiler`] with [`GlobalProfiler::add_sink`].
pub type FrameSink = Box<dyn Fn(Arc<FrameData>) + Send>;

/// Identifies a specific [`FrameSink`] when added to [`GlobalProfiler`].
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct FrameSinkId(u64);

/// Singleton. Collects profiling data from multiple threads
/// and passes them on to different [`FrameSink`]:s.
pub struct GlobalProfiler {
    current_frame_index: FrameIndex,
    current_frame: BTreeMap<ThreadInfo, StreamInfo>,

    next_sink_id: FrameSinkId,
    sinks: std::collections::HashMap<FrameSinkId, FrameSink>,
}

impl Default for GlobalProfiler {
    fn default() -> Self {
        Self {
            current_frame_index: 0,
            current_frame: Default::default(),
            next_sink_id: FrameSinkId(1),
            sinks: Default::default(),
        }
    }
}

impl GlobalProfiler {
    /// Access to the global profiler singleton.
    pub fn lock() -> parking_lot::MutexGuard<'static, Self> {
        use once_cell::sync::Lazy;
        static GLOBAL_PROFILER: Lazy<Mutex<GlobalProfiler>> = Lazy::new(Default::default);
        GLOBAL_PROFILER.lock()
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
        for sink in self.sinks.values() {
            sink(new_frame.clone());
        }
    }

    /// Report some profiling data. Called from [`ThreadProfiler`].
    pub fn report(&mut self, info: ThreadInfo, stream_info: &StreamInfoRef<'_>) {
        self.current_frame
            .entry(info)
            .or_default()
            .extend(stream_info);
    }

    /// Tells [`GlobalProfiler`] to call this function with each new finished frame.
    ///
    /// The returned [`FrameSinkId`] can be used to remove the sink with [`Self::remove_sink`].
    pub fn add_sink(&mut self, sink: FrameSink) -> FrameSinkId {
        let id = self.next_sink_id;
        self.next_sink_id.0 += 1;
        self.sinks.insert(id, sink);
        id
    }

    pub fn remove_sink(&mut self, id: FrameSinkId) -> Option<FrameSink> {
        self.sinks.remove(&id)
    }
}

// ----------------------------------------------------------------------------

/// Returns a high-precision, monotonically increasing nanosecond count since unix epoch.
#[inline]
pub fn now_ns() -> NanoSecond {
    // This can maybe be optimized
    use once_cell::sync::Lazy;
    use std::time::Instant;

    fn epoch_offset_and_start() -> (NanoSecond, Instant) {
        if let Ok(duration_since_epoch) = std::time::UNIX_EPOCH.elapsed() {
            let nanos_since_epoch = duration_since_epoch.as_nanos() as NanoSecond;
            (nanos_since_epoch, Instant::now())
        } else {
            // system time is set before 1970. this should be quite rare.
            (0, Instant::now())
        }
    }

    static START_TIME: Lazy<(NanoSecond, Instant)> = Lazy::new(epoch_offset_and_start);
    START_TIME.0 + START_TIME.1.elapsed().as_nanos() as NanoSecond
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
    #[inline]
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
    #[inline]
    fn drop(&mut self) {
        ThreadProfiler::call(|tp| tp.end_scope(self.start_stream_offset));
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
        let name = $crate::type_name_of(f);
        // Remove "::f" from the name:
        let name = &name.get(..name.len() - 3).unwrap();
        $crate::clean_function_name(name)
    }};
}

#[doc(hidden)]
#[inline]
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
#[inline]
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
/// Overhead: around 210 ns on 2020 Intel MacBook Pro.
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
/// Overhead: around 140 ns on 2020 Intel MacBook Pro.
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
