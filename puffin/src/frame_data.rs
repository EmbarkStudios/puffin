use crate::ScopeDetails;
use crate::{Error, FrameIndex, NanoSecond, Result, StreamInfo, ThreadInfo};
#[cfg(feature = "packing")]
use parking_lot::RwLock;

use std::{collections::BTreeMap, sync::Arc};

// ----------------------------------------------------------------------------

/// The streams of profiling data for each thread.
pub type ThreadStreams = BTreeMap<ThreadInfo, Arc<StreamInfo>>;

/// Meta-information about a frame.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug)]
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
    /// Frame metadata.
    pub meta: FrameMeta,
    /// The streams of profiling data for each thread.
    pub thread_streams: ThreadStreams,
}

impl UnpackedFrameData {
    /// Create a new [`UnpackedFrameData`].
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

    /// The index of this frame.
    pub fn frame_index(&self) -> u64 {
        self.meta.frame_index
    }

    /// The range in nanoseconds of the entire profile frame.
    pub fn range_ns(&self) -> (NanoSecond, NanoSecond) {
        self.meta.range_ns
    }

    /// The duration in nanoseconds of the entire profile frame.
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
    /// Scopes that were registered during this frame.
    pub scope_delta: Vec<Arc<ScopeDetails>>,
    /// Does [`Self::scope_delta`] contain all the scopes up to this point?
    /// If `false`, it just contains the new scopes since last frame data.
    pub full_delta: bool,
}

#[cfg(not(feature = "packing"))]
pub enum Never {}

#[cfg(not(feature = "packing"))]
impl std::fmt::Display for Never {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

#[cfg(not(feature = "packing"))]
impl FrameData {
    /// Create a new [`FrameData`].
    pub fn new(
        frame_index: FrameIndex,
        thread_streams: BTreeMap<ThreadInfo, StreamInfo>,
        scope_delta: Vec<Arc<ScopeDetails>>,
        full_delta: bool,
    ) -> Result<Self> {
        Ok(Self::from_unpacked(
            Arc::new(UnpackedFrameData::new(frame_index, thread_streams)?),
            scope_delta,
            full_delta,
        ))
    }

    fn from_unpacked(
        unpacked_frame: Arc<UnpackedFrameData>,
        scope_delta: Vec<Arc<ScopeDetails>>,
        full_delta: bool,
    ) -> Self {
        Self {
            unpacked_frame,
            scope_delta,
            full_delta,
        }
    }

    /// Returns meta data from this frame.
    #[inline]
    pub fn meta(&self) -> &FrameMeta {
        &self.unpacked_frame.meta
    }

    /// Always returns `None`.
    pub fn packed_size(&self) -> Option<usize> {
        None
    }

    /// Number of bytes used when unpacked.
    pub fn unpacked_size(&self) -> Option<usize> {
        Some(self.unpacked_frame.meta.num_bytes)
    }

    /// Bytes currently used by the unpacked data.
    pub fn bytes_of_ram_used(&self) -> usize {
        self.unpacked_frame.meta.num_bytes
    }

    /// Returns the packing information for the frame.
    pub fn packing_info(&self) -> PackingInfo {
        PackingInfo {
            unpacked_size: Some(self.unpacked_frame.meta.num_bytes),
            packed_size: None,
        }
    }
    /// Always returns `false`.
    pub fn has_packed(&self) -> bool {
        false
    }

    /// Always returns `true`.
    pub fn has_unpacked(&self) -> bool {
        true
    }

    /// Return the unpacked data.
    pub fn unpacked(&self) -> std::result::Result<Arc<UnpackedFrameData>, Never> {
        Ok(self.unpacked_frame.clone())
    }

    /// Does nothing because this [`FrameData`] is unpacked by default.
    pub fn pack(&self) {}
}

#[cfg(all(feature = "serialization", not(feature = "packing")))]
compile_error!(
    "If the puffin feature 'serialization' is one, the 'packing' feature must also be enabled!"
);

// ----------------------------------------------------------------------------

/// See <https://github.com/EmbarkStudios/puffin/pull/130> for pros-and-cons of different compression algorithms.
#[cfg(feature = "packing")]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompressionKind {
    Uncompressed = 0,

    /// Very fast, and lightweight dependency
    #[allow(dead_code)] // with some feature sets
    Lz4 = 1,

    /// Big dependency, slow compression, but compresses better than lz4
    #[allow(dead_code)] // with some feature sets
    Zstd = 2,
}

#[cfg(feature = "packing")]
impl CompressionKind {
    #[cfg(feature = "serialization")]
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

    /// Encapsulates the frame data in its current state,  which can be
    /// uncompressed, compressed, or a combination of both
    data: RwLock<FrameDataState>,

    /// Scopes that were registered during this frame.
    pub scope_delta: Vec<Arc<ScopeDetails>>,

    /// Does [`Self::scope_delta`] contain all the scopes up to this point?
    /// If `false`, it just contains the new scopes since last frame data.
    pub full_delta: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PackingInfo {
    /// Number of bytes used when unpacked, if has unpacked.
    pub unpacked_size: Option<usize>,
    /// Number of bytes used by the packed data, if has packed.
    pub packed_size: Option<usize>,
}

#[cfg(feature = "packing")]
enum FrameDataState {
    /// Unpacked data.
    Unpacked(Arc<UnpackedFrameData>),

    /// [`UnpackedFrameData::thread_streams`], compressed.
    Packed(PackedStreams),

    /// Both compressed and uncompressed.
    Both(Arc<UnpackedFrameData>, PackedStreams),
}

#[cfg(feature = "packing")]
impl FrameDataState {
    fn unpacked_size(&self) -> Option<usize> {
        match self {
            FrameDataState::Packed(_) => None,
            FrameDataState::Unpacked(unpacked) | FrameDataState::Both(unpacked, _) => {
                Some(unpacked.meta.num_bytes)
            }
        }
    }

    fn unpacked(&self) -> Option<Arc<UnpackedFrameData>> {
        match self {
            FrameDataState::Packed(_) => None,
            FrameDataState::Unpacked(unpacked) | FrameDataState::Both(unpacked, _) => {
                Some(unpacked.clone())
            }
        }
    }

    fn unpack(&mut self, unpacked: Arc<UnpackedFrameData>) {
        let temp = std::mem::replace(
            self,
            FrameDataState::Packed(PackedStreams::new(CompressionKind::Uncompressed, vec![])),
        );

        if let FrameDataState::Packed(packed) = temp {
            // Transform only if we don't have unpacked already
            *self = FrameDataState::Both(unpacked, packed);
        } else {
            // Restore the original value if it was not Inner::Packed
            *self = temp;
        }
    }

    fn packed_size(&self) -> Option<usize> {
        match self {
            FrameDataState::Unpacked(_) => None,
            FrameDataState::Packed(packed) | FrameDataState::Both(_, packed) => {
                Some(packed.num_bytes())
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    fn packed(&self) -> Option<&PackedStreams> {
        match self {
            FrameDataState::Unpacked(_) => None,
            FrameDataState::Packed(packed) | FrameDataState::Both(_, packed) => Some(packed),
        }
    }

    fn pack_and_remove(&mut self) {
        if let FrameDataState::Unpacked(ref unpacked) | FrameDataState::Both(ref unpacked, _) =
            *self
        {
            let packed = PackedStreams::pack(&unpacked.thread_streams);
            *self = Self::Packed(packed);
        }
    }

    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    fn pack_and_keep(&mut self) {
        if let FrameDataState::Unpacked(ref unpacked) = *self {
            let packed = PackedStreams::pack(&unpacked.thread_streams);
            *self = Self::Packed(packed);
        }
    }

    fn bytes_of_ram_used(&self) -> usize {
        self.unpacked_size().unwrap_or(0) + self.packed_size().unwrap_or(0)
    }

    fn has_packed(&self) -> bool {
        matches!(self, FrameDataState::Packed(_) | FrameDataState::Both(..))
    }

    fn has_unpacked(&self) -> bool {
        matches!(self, FrameDataState::Unpacked(_) | FrameDataState::Both(..))
    }

    fn packing_info(&self) -> PackingInfo {
        PackingInfo {
            unpacked_size: self.unpacked_size(),
            packed_size: self.packed_size(),
        }
    }
}

#[cfg(feature = "packing")]
impl FrameData {
    /// Create a new [`FrameData`].
    pub fn new(
        frame_index: FrameIndex,
        thread_streams: BTreeMap<ThreadInfo, StreamInfo>,
        scope_delta: Vec<Arc<ScopeDetails>>,
        full_delta: bool,
    ) -> Result<Self> {
        Ok(Self::from_unpacked(
            Arc::new(UnpackedFrameData::new(frame_index, thread_streams)?),
            scope_delta,
            full_delta,
        ))
    }

    fn from_unpacked(
        unpacked_frame: Arc<UnpackedFrameData>,
        scope_delta: Vec<Arc<ScopeDetails>>,
        full_delta: bool,
    ) -> Self {
        Self {
            meta: unpacked_frame.meta,
            data: RwLock::new(FrameDataState::Unpacked(unpacked_frame)),
            scope_delta,
            full_delta,
        }
    }

    /// Returns meta data from this frame.
    #[inline]
    pub fn meta(&self) -> &FrameMeta {
        &self.meta
    }

    /// Number of bytes used by the packed data, if packed.
    pub fn packed_size(&self) -> Option<usize> {
        self.data.read().packed_size()
    }

    /// Number of bytes used when unpacked, if known.
    pub fn unpacked_size(&self) -> Option<usize> {
        self.data.read().unpacked_size()
    }

    /// bytes currently used by the unpacked and packed data.
    pub fn bytes_of_ram_used(&self) -> usize {
        self.data.read().bytes_of_ram_used()
    }

    /// Do we have a packed version stored internally?
    pub fn has_packed(&self) -> bool {
        self.data.read().has_packed()
    }

    /// Do we have a unpacked version stored internally?
    pub fn has_unpacked(&self) -> bool {
        self.data.read().has_unpacked()
    }

    /// Provides an overview of the frame's packing status.
    ///
    /// The function retrieves both the sizes of the unpacked and packed frames, as well as whether
    /// packed/unpacked versions of the frame exist internally. The goal of this function is to
    /// minimize the number of lock accesses by consolidating information retrieval into a single
    /// operation.
    pub fn packing_info(&self) -> PackingInfo {
        self.data.read().packing_info()
    }

    /// Return the unpacked data.
    ///
    /// This will lazily unpack if needed (and only once).
    ///
    /// Returns `Err` if failing to decode the packed data.
    pub fn unpacked(&self) -> anyhow::Result<Arc<UnpackedFrameData>> {
        let unpacked = {
            let inner_guard = self.data.read();
            let FrameDataState::Packed(ref packed) = *inner_guard else {
                // Safe to unwrap, variant has to contain unpacked if NOT `Packed`
                return Ok(self.data.read().unpacked().unwrap());
            };

            crate::profile_scope!("unpack_puffin_frame");

            Arc::new(UnpackedFrameData {
                meta: self.meta,
                thread_streams: packed.unpack()?,
            })
        };

        self.data.write().unpack(unpacked.clone());
        Ok(unpacked)
    }

    /// Make the [`FrameData`] use up less memory.
    /// Idempotent.
    pub fn pack(&self) {
        self.data.write().pack_and_remove();
    }

    /// Create a packed storage without freeing the unpacked storage.
    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    fn create_packed(&self) {
        self.data.write().pack_and_keep();
    }

    /// Writes one [`FrameData`] into a stream, prefixed by its length ([`u32`] le).
    #[cfg(not(target_arch = "wasm32"))] // compression not supported on wasm
    #[cfg(feature = "serialization")]
    pub fn write_into(
        &self,
        scope_collection: &crate::ScopeCollection,
        send_all_scopes: bool,
        write: &mut impl std::io::Write,
    ) -> anyhow::Result<()> {
        use bincode::Options as _;
        use byteorder::{WriteBytesExt as _, LE};

        let meta_serialized = bincode::options().serialize(&self.meta)?;

        write.write_all(b"PFD4")?;
        write.write_all(&(meta_serialized.len() as u32).to_le_bytes())?;
        write.write_all(&meta_serialized)?;

        self.create_packed();
        let packed_streams_lock = self.data.read();
        let packed_streams = packed_streams_lock.packed().unwrap(); // We just called create_packed

        write.write_all(&(packed_streams.num_bytes() as u32).to_le_bytes())?;
        write.write_u8(packed_streams.compression_kind as u8)?;
        write.write_all(&packed_streams.bytes)?;

        let to_serialize_scopes: Vec<_> = if send_all_scopes {
            scope_collection.scopes_by_id().values().cloned().collect()
        } else {
            self.scope_delta.clone()
        };

        let serialized_scopes = bincode::options().serialize(&to_serialize_scopes)?;
        write.write_u32::<LE>(serialized_scopes.len() as u32)?;
        write.write_all(&serialized_scopes)?;
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
        use byteorder::{ReadBytesExt, LE};

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
                FrameData::from_unpacked(
                    Arc::new(self.into_unpacked_frame_data()),
                    Default::default(),
                    false,
                )
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
                    data: RwLock::new(FrameDataState::Packed(packed_streams)),
                    scope_delta: Default::default(),
                    full_delta: false,
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
                    data: RwLock::new(FrameDataState::Packed(packed_streams)),
                    scope_delta: Default::default(),
                    full_delta: false,
                }))
            } else if &header == b"PFD4" {
                // Added 2024-01-08: Split up stream scope details from the record stream.
                let meta_length = read.read_u32::<LE>()? as usize;
                let meta = {
                    let mut meta = vec![0_u8; meta_length];
                    read.read_exact(&mut meta)?;
                    bincode::options()
                        .deserialize(&meta)
                        .context("bincode deserialize")?
                };

                let streams_compressed_length = read.read_u32::<LE>()? as usize;
                let compression_kind = CompressionKind::from_u8(read.read_u8()?)?;
                let streams_compressed = {
                    let mut streams_compressed = vec![0_u8; streams_compressed_length];
                    read.read_exact(&mut streams_compressed)?;
                    PackedStreams::new(compression_kind, streams_compressed)
                };

                let serialized_scope_len = read.read_u32::<LE>()?;
                let deserialized_scopes: Vec<crate::ScopeDetails> = {
                    let mut serialized_scopes = vec![0; serialized_scope_len as usize];
                    read.read_exact(&mut serialized_scopes)?;
                    bincode::options()
                        .deserialize_from(serialized_scopes.as_slice())
                        .context("Can not deserialize scope details")?
                };

                let new_scopes: Vec<_> = deserialized_scopes
                    .into_iter()
                    .map(|x| Arc::new(x.clone()))
                    .collect();

                Ok(Some(Self {
                    meta,
                    data: RwLock::new(FrameDataState::Packed(streams_compressed)),
                    scope_delta: new_scopes,
                    full_delta: false,
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
    /// The index of this frame.
    pub fn frame_index(&self) -> u64 {
        self.meta().frame_index
    }

    /// The range in nanoseconds of the entire profile frame.
    pub fn range_ns(&self) -> (NanoSecond, NanoSecond) {
        self.meta().range_ns
    }

    /// The duration in nanoseconds of the entire profile frame.
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
