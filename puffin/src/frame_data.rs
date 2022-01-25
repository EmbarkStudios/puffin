use crate::{Error, FrameIndex, NanoSecond, Result, StreamInfo, ThreadInfo};

#[cfg(feature = "packing")]
use parking_lot::RwLock;

use std::{collections::BTreeMap, sync::Arc};

// ----------------------------------------------------------------------------

/// The streams of profiling data for each thread.
pub type ThreadStreams = BTreeMap<ThreadInfo, Arc<StreamInfo>>;

/// Meta-information about a frame.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug)]
pub struct FrameMeta {
    /// What frame this is (counting from 0 at application startup).
    pub frame_index: FrameIndex,
    /// The span we cover.
    pub range_ns: (NanoSecond, NanoSecond),
    /// The unpacked size of all streams.
    pub num_bytes: usize,
    /// Total number of scopes.
    pub num_scopes: usize,
}

/// One frame worth of profile data, collected from many sources.
///
/// More often encoded as a [`FrameData`].
pub struct UnpackedFrameData {
    pub meta: FrameMeta,
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

// ----------------------------------------------------------------------------

/// One frame worth of profile data, collected from many sources.
///
/// If you turn on the the "packing" feature, this will compress the
/// profiling data in order to save RAM.
#[cfg(not(feature = "packing"))]
pub struct FrameData {
    unpacked_frame: Arc<UnpackedFrameData>,
}

#[cfg(not(feature = "packing"))]
pub enum Never {}

#[cfg(not(feature = "packing"))]
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

    fn from_unpacked(unpacked_frame: Arc<UnpackedFrameData>) -> Self {
        Self { unpacked_frame }
    }

    #[inline]
    pub fn meta(&self) -> &FrameMeta {
        &self.unpacked_frame.meta
    }

    pub fn packed_size(&self) -> Option<usize> {
        None
    }
    pub fn unpacked_size(&self) -> Option<usize> {
        Some(self.unpacked_frame.meta.num_bytes)
    }
    pub fn bytes_of_ram_used(&self) -> usize {
        self.unpacked_frame.meta.num_bytes
    }
    pub fn has_packed(&self) -> bool {
        false
    }
    pub fn has_unpacked(&self) -> bool {
        true
    }
    pub fn unpacked(&self) -> std::result::Result<Arc<UnpackedFrameData>, Never> {
        Ok(self.unpacked_frame.clone())
    }
    pub fn pack(&self) {}
}

#[cfg(all(feature = "serialization", not(feature = "packing")))]
compile_error!(
    "If the puffin feature 'serialization' is one, the 'packing' feature must also be enabled!"
);

// ----------------------------------------------------------------------------

/// One frame worth of profile data, collected from many sources.
///
/// If you turn on the "packing" feature, then [`FrameData`] has interior mutability with double storage:
/// * Unpacked data ([`UnpackedFrameData`])
/// * Packed (compressed) data
///
/// One or both are always stored.
/// This allows RAM-efficient storage and viewing of many frames of profiling data.
/// Packing and unpacking is done lazily, on-demand.
#[cfg(feature = "packing")]
pub struct FrameData {
    meta: FrameMeta,
    /// * [`None`] if still compressed.
    /// * `Some(Err(…))` if there was a problem during unpacking.
    /// * `Some(Ok(…))` if unpacked.
    unpacked_frame: RwLock<Option<anyhow::Result<Arc<UnpackedFrameData>>>>,
    /// [`UnpackedFrameData::thread_streams`], compressed with zstd.
    /// [`None`] if not yet compressed.
    packed_zstd_streams: RwLock<Option<Vec<u8>>>,
}

#[cfg(feature = "packing")]
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

    fn from_unpacked(unpacked_frame: Arc<UnpackedFrameData>) -> Self {
        Self {
            meta: unpacked_frame.meta.clone(),
            unpacked_frame: RwLock::new(Some(Ok(unpacked_frame))),
            packed_zstd_streams: RwLock::new(None),
        }
    }

    #[inline]
    pub fn meta(&self) -> &FrameMeta {
        &self.meta
    }

    /// Number of bytes used by the packed data, if packed.
    pub fn packed_size(&self) -> Option<usize> {
        self.packed_zstd_streams.read().as_ref().map(|c| c.len())
    }

    /// Number of bytes used when unpacked, if known.
    pub fn unpacked_size(&self) -> Option<usize> {
        if self.has_unpacked() {
            Some(self.meta.num_bytes)
        } else {
            None
        }
    }

    /// bytes currently used by the unpacked and packed data.
    pub fn bytes_of_ram_used(&self) -> usize {
        self.unpacked_size().unwrap_or(0) + self.packed_size().unwrap_or(0)
    }

    /// Do we have a packed version stored internally?
    pub fn has_packed(&self) -> bool {
        self.packed_zstd_streams.read().is_some()
    }

    /// Do we have a unpacked version stored internally?
    pub fn has_unpacked(&self) -> bool {
        self.unpacked_frame.read().is_some()
    }

    /// Return the unpacked data.
    ///
    /// This will lazily unpack if needed (and only once).
    ///
    /// Returns `Err` if failing to decode the packed data.
    pub fn unpacked(&self) -> anyhow::Result<Arc<UnpackedFrameData>> {
        fn unpack_frame_data(
            meta: FrameMeta,
            compressed: &[u8],
        ) -> anyhow::Result<UnpackedFrameData> {
            use anyhow::Context as _;
            use bincode::Options as _;

            let streams_serialized = decode_zstd(compressed)?;

            let thread_streams: ThreadStreams = bincode::options()
                .deserialize(&streams_serialized)
                .context("bincode deserialize")?;

            Ok(UnpackedFrameData {
                meta,
                thread_streams,
            })
        }

        let has_unpacked = self.unpacked_frame.read().is_some();
        if !has_unpacked {
            crate::profile_scope!("unpack_puffin_frame");
            let packed_lock = self.packed_zstd_streams.read();
            let packed = packed_lock
                .as_ref()
                .expect("FrameData is neither packed or unpacked");

            let frame_data_result = unpack_frame_data(self.meta.clone(), packed);
            let frame_data_result = frame_data_result.map(Arc::new);
            *self.unpacked_frame.write() = Some(frame_data_result);
        }

        match self.unpacked_frame.read().as_ref().unwrap() {
            Ok(frame) => Ok(frame.clone()),
            Err(err) => Err(anyhow::format_err!("{}", err)), // can't clone `anyhow::Error`
        }
    }

    /// Make the [`FrameData`] use up less memory.
    /// Idempotent.
    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    pub fn pack(&self) {
        self.create_packed();
        *self.unpacked_frame.write() = None;
    }

    #[cfg(target_arch = "wasm32")]
    pub fn pack(&self) {
        // compression not supported on wasm, so this is a no-op
    }

    /// Create a packed storage without freeing the unpacked storage.
    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    fn create_packed(&self) {
        use bincode::Options as _;
        let has_packed = self.packed_zstd_streams.read().is_some();
        if !has_packed {
            // crate::profile_scope!("pack_puffin_frame"); // we get called from `GlobalProfiler::new_frame`, so avoid recursiveness!
            let unpacked_frame = self
                .unpacked_frame
                .read()
                .as_ref()
                .expect("We should have an unpacked frame if we don't have a packed one")
                .as_ref()
                .expect("The unpacked frame should be error free, since it doesn't come from packed source")
                .clone();

            let streams_serialized = bincode::options()
                .serialize(&unpacked_frame.thread_streams)
                .expect("bincode failed to encode");

            // zstd cuts sizes in half compared to lz4_flex
            let level = 3;
            let streams_compressed =
                zstd::encode_all(std::io::Cursor::new(&streams_serialized), level)
                    .expect("zstd failed to compress");

            *self.packed_zstd_streams.write() = Some(streams_compressed);
        }
    }

    /// Writes one [`FrameData`] into a stream, prefixed by its length ([`u32`] le).
    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    #[cfg(feature = "serialization")]
    pub fn write_into(&self, write: &mut impl std::io::Write) -> anyhow::Result<()> {
        use bincode::Options as _;
        let meta_serialized = bincode::options().serialize(&self.meta)?;

        write.write_all(b"PFD2")?;
        write.write_all(&(meta_serialized.len() as u32).to_le_bytes())?;
        write.write_all(&meta_serialized)?;

        self.create_packed();
        let zstd_streams_lock = self.packed_zstd_streams.read();
        let zstd_streams = zstd_streams_lock.as_ref().unwrap();

        write.write_all(&(zstd_streams.len() as u32).to_le_bytes())?;
        write.write_all(zstd_streams)?;

        Ok(())
    }

    /// Read the next [`FrameData`] from a stream.
    ///
    /// [`None`] is returned if the end of the stream is reached (EOF),
    /// or an end-of-stream sentinel of `0u32` is read.
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

        #[derive(Clone, serde::Deserialize, serde::Serialize)]
        pub struct LegacyFrameData {
            pub frame_index: FrameIndex,
            pub thread_streams: ThreadStreams,
            pub range_ns: (NanoSecond, NanoSecond),
            pub num_bytes: usize,
            pub num_scopes: usize,
        }

        impl LegacyFrameData {
            fn into_unpacked_frame_data(self) -> UnpackedFrameData {
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

            fn into_frame_data(self) -> FrameData {
                FrameData::from_unpacked(Arc::new(self.into_unpacked_frame_data()))
            }
        }

        if header == [0_u8; 4] {
            Ok(None) // end-of-stream sentinel.
        } else if header.starts_with(b"PFD") {
            if &header == b"PFD0" {
                // Like PDF1, but compressed with `lz4_flex`.
                // We stopped supporting this in 2021-11-16 in order to remove `lz4_flex` dependency.
                anyhow::bail!("Found legacy puffin data, which we can no longer decode")
            } else if &header == b"PFD1" {
                // Added 2021-09
                let mut compressed_length = [0_u8; 4];
                read.read_exact(&mut compressed_length)?;
                let compressed_length = u32::from_le_bytes(compressed_length) as usize;
                let mut compressed = vec![0_u8; compressed_length];
                read.read_exact(&mut compressed)?;

                let serialized = decode_zstd(&compressed[..])?;

                let legacy: LegacyFrameData = bincode::options()
                    .deserialize(&serialized)
                    .context("bincode deserialize")?;
                Ok(Some(legacy.into_frame_data()))
            } else if &header == b"PFD2" {
                // Added 2021-11-15
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

                // Don't unpack now - do it if/when needed!

                Ok(Some(Self {
                    meta,
                    unpacked_frame: RwLock::new(None),
                    packed_zstd_streams: RwLock::new(Some(streams_compressed)),
                }))
            } else {
                anyhow::bail!("Failed to decode: this data is newer than this reader. Please update your puffin version!");
            }
        } else {
            // Very old packet without magic header
            let mut bytes = vec![0_u8; u32::from_le_bytes(header) as usize];
            read.read_exact(&mut bytes)?;

            use bincode::Options as _;
            let legacy: LegacyFrameData = bincode::options()
                .deserialize(&bytes)
                .context("bincode deserialize")?;
            Ok(Some(legacy.into_frame_data()))
        }
    }
}

// ----------------------------------------------------------------------------

impl FrameData {
    pub fn frame_index(&self) -> u64 {
        self.meta().frame_index
    }

    pub fn range_ns(&self) -> (NanoSecond, NanoSecond) {
        self.meta().range_ns
    }

    pub fn duration_ns(&self) -> NanoSecond {
        let (min, max) = self.meta().range_ns;
        max - min
    }
}

// ----------------------------------------------------------------------------

#[cfg(feature = "packing")]
#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "zstd")]
fn decode_zstd(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use anyhow::Context as _;
    zstd::decode_all(bytes).context("zstd decompress")
}

#[cfg(feature = "packing")]
#[cfg(target_arch = "wasm32")]
#[cfg(feature = "ruzstd")]
fn decode_zstd(mut bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use anyhow::Context as _;
    use std::io::Read as _;
    let mut decoded = Vec::new();
    let mut decoder = ruzstd::StreamingDecoder::new(&mut bytes)
        .map_err(|err| anyhow::format_err!("zstd decompress: {}", err))?;
    decoder
        .read_to_end(&mut decoded)
        .context("zstd decompress")?;
    Ok(decoded)
}

#[cfg(all(not(feature = "zstd"), not(feature = "ruzstd")))]
compile_error!("Either feature zstd or ruzstd must be enabled");
