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

/// See <https://github.com/EmbarkStudios/puffin/pull/130> for pros-and-cons of different compression algorithms.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompressionKind {
    Uncompressed = 0,

    /// Very fast, and lightweight dependency
    Lz4 = 1,

    /// Big dependency, slow compression, but compresses better than lz4
    Zstd = 2,
}

impl CompressionKind {
    fn from_u8(value: u8) -> anyhow::Result<Self> {
        match value {
            0 => Ok(Self::Uncompressed),
            1 => Ok(Self::Lz4),
            2 => Ok(Self::Zstd),
            _ => Err(anyhow::anyhow!("Unknown compression kind: {value}")),
        }
    }
}

/// Packed with bincode and compressed.
#[cfg(feature = "packing")]
struct PackedStreams {
    compression_kind: CompressionKind,
    bytes: Vec<u8>,
}

#[cfg(feature = "packing")]
impl PackedStreams {
    pub fn new(compression_kind: CompressionKind, bytes: Vec<u8>) -> Self {
        Self {
            compression_kind,
            bytes,
        }
    }

    pub fn pack(streams: &ThreadStreams) -> Self {
        use bincode::Options as _;

        let serialized = bincode::options()
            .serialize(streams)
            .expect("bincode failed to encode");

        cfg_if::cfg_if! {
            if #[cfg(feature = "lz4")] {
                Self {
                    compression_kind: CompressionKind::Lz4,
                    bytes: lz4_flex::compress_prepend_size(&serialized),
                }
            } else if #[cfg(feature = "zstd")] {
                let level = 3;
                let bytes = zstd::encode_all(std::io::Cursor::new(&serialized), level)
                    .expect("zstd failed to compress");
                Self {
                    compression_kind: CompressionKind::Zstd,
                    bytes,
                }
            } else {
                Self {
                    compression_kind: CompressionKind::Uncompressed,
                    bytes: serialized,
                }
            }
        }
    }

    pub fn num_bytes(&self) -> usize {
        self.bytes.len()
    }

    pub fn unpack(&self) -> anyhow::Result<ThreadStreams> {
        crate::profile_function!();

        use anyhow::Context as _;
        use bincode::Options as _;

        fn deserialize(bytes: &[u8]) -> anyhow::Result<ThreadStreams> {
            crate::profile_scope!("bincode deserialize");
            bincode::options()
                .deserialize(bytes)
                .context("bincode deserialize")
        }

        match self.compression_kind {
            CompressionKind::Uncompressed => deserialize(&self.bytes),

            CompressionKind::Lz4 => {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "lz4")] {
                        let compressed = lz4_flex::decompress_size_prepended(&self.bytes)
                            .map_err(|err| anyhow::anyhow!("lz4: {err}"))?;
                        deserialize(&compressed)
                    } else {
                        anyhow::bail!("Data compressed with lz4, but the lz4 feature is not enabled")
                    }
                }
            }

            CompressionKind::Zstd => {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "zstd")] {
                        deserialize(&decode_zstd(&self.bytes)?)
                    } else {
                        anyhow::bail!("Data compressed with zstd, but the zstd feature is not enabled")
                    }
                }
            }
        }
    }
}

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

    /// [`UnpackedFrameData::thread_streams`], compressed.
    /// [`None`] if not yet compressed.
    packed_streams: RwLock<Option<PackedStreams>>,
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
            packed_streams: RwLock::new(None),
        }
    }

    #[inline]
    pub fn meta(&self) -> &FrameMeta {
        &self.meta
    }

    /// Number of bytes used by the packed data, if packed.
    pub fn packed_size(&self) -> Option<usize> {
        self.packed_streams.read().as_ref().map(|c| c.num_bytes())
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
        self.packed_streams.read().is_some()
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
            packed: &PackedStreams,
        ) -> anyhow::Result<UnpackedFrameData> {
            Ok(UnpackedFrameData {
                meta,
                thread_streams: packed.unpack()?,
            })
        }

        let has_unpacked = self.unpacked_frame.read().is_some();
        if !has_unpacked {
            crate::profile_scope!("unpack_puffin_frame");
            let packed_lock = self.packed_streams.read();
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
    pub fn pack(&self) {
        self.create_packed();
        *self.unpacked_frame.write() = None;
    }

    /// Create a packed storage without freeing the unpacked storage.
    fn create_packed(&self) {
        let has_packed = self.packed_streams.read().is_some();
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

            let packed = PackedStreams::pack(&unpacked_frame.thread_streams);

            *self.packed_streams.write() = Some(packed);
        }
    }

    /// Writes one [`FrameData`] into a stream, prefixed by its length ([`u32`] le).
    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    #[cfg(feature = "serialization")]
    pub fn write_into(&self, write: &mut impl std::io::Write) -> anyhow::Result<()> {
        use bincode::Options as _;
        use byteorder::WriteBytesExt as _;
        let meta_serialized = bincode::options().serialize(&self.meta)?;

        write.write_all(b"PFD3")?;
        write.write_all(&(meta_serialized.len() as u32).to_le_bytes())?;
        write.write_all(&meta_serialized)?;

        self.create_packed();
        let packed_streams_lock = self.packed_streams.read();
        let packed_streams = packed_streams_lock.as_ref().unwrap(); // We just called create_packed

        write.write_all(&(packed_streams.num_bytes() as u32).to_le_bytes())?;
        write.write_u8(packed_streams.compression_kind as u8)?;
        write.write_all(&packed_streams.bytes)?;

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
        use byteorder::ReadBytesExt;

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
                #[cfg(feature = "zstd")]
                {
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
                }
                #[cfg(not(feature = "zstd"))]
                {
                    anyhow::bail!("Cannot decode old puffin data without the `zstd` feature")
                }
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
                let compression_kind = CompressionKind::Zstd;
                let mut streams_compressed = vec![0_u8; streams_compressed_length];
                read.read_exact(&mut streams_compressed)?;

                let packed_streams = PackedStreams::new(compression_kind, streams_compressed);

                // Don't unpack now - do it if/when needed!

                Ok(Some(Self {
                    meta,
                    unpacked_frame: RwLock::new(None),
                    packed_streams: RwLock::new(Some(packed_streams)),
                }))
            } else if &header == b"PFD3" {
                // Added 2023-05-13: CompressionKind field
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
                let compression_kind = read.read_u8()?;
                let compression_kind = CompressionKind::from_u8(compression_kind)?;
                let mut streams_compressed = vec![0_u8; streams_compressed_length];
                read.read_exact(&mut streams_compressed)?;

                let packed_streams = PackedStreams::new(compression_kind, streams_compressed);

                // Don't unpack now - do it if/when needed!

                Ok(Some(Self {
                    meta,
                    unpacked_frame: RwLock::new(None),
                    packed_streams: RwLock::new(Some(packed_streams)),
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
    zstd::decode_all(bytes).context("zstd decompress failed")
}

#[cfg(feature = "packing")]
#[cfg(target_arch = "wasm32")]
#[cfg(feature = "zstd")]
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
