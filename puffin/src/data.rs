//! The profiler records all events into a byte stream.
//! The profiler UI parses this byte stream as needed, on the fly.
//! The data format is as such:
//!
//! Each scope start consists of:
//!
//! ```ignore
//!    '('          byte       Sentinel
//!    scope id     u32        Unique monolithic identifier for a scope
//!    time_ns      i64        Time stamp of when scope started
//!    data         str        Resource that is being processed, e.g. name of image being loaded. Could be the empty string.
//!    scope_size   u64        Number of bytes of child scope
//! ```
//!
//! This is followed by `scope_size` number of bytes of data
//! containing any child scopes. The scope is then closed by:
//!
//! ```ignore
//!    ')'          byte       Sentinel
//!    time_ns      i64        Time stamp of when scope finished
//! ```
//!
//! Integers are encoded in little endian.
//! Strings are encoded as a single u8 length + that many bytes of UTF8.
//! At the moment strings may be at most 127 bytes long.

use super::*;
use anyhow::Context;
use byteorder::{LittleEndian as LE, ReadBytesExt, WriteBytesExt};
use std::mem::size_of;

const SCOPE_BEGIN: u8 = b'(';
const SCOPE_END: u8 = b')';

/// Used when parsing a Stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeRecord<'s> {
    /// The start of this scope in nanoseconds.
    pub start_ns: NanoSecond,

    /// The duration of this scope in nanoseconds.
    pub duration_ns: NanoSecond,

    /// e.g. function argument, like a mesh name. Optional.
    /// Example: "image.png".
    pub data: &'s str,
}

impl<'s> ScopeRecord<'s> {
    /// The end of this scope in nanoseconds.
    #[inline]
    pub fn stop_ns(&self) -> NanoSecond {
        self.start_ns + self.duration_ns
    }
}

/// Used when parsing a Stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope<'s> {
    /// Unique identifier for the profile scope.
    /// More detailed scope information can be requested via [`FrameView::scope_collection()`].
    pub id: ScopeId,
    /// Some dynamic data that is passed into the profiler scope.
    pub record: ScopeRecord<'s>,
    /// Stream offset for first child.
    pub child_begin_position: u64,
    /// Stream offset after last child.
    pub child_end_position: u64,
    /// Stream offset for next sibling (if any).
    pub next_sibling_position: u64,
}

/// Stream of profiling events from one thread.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct Stream(Vec<u8>);

impl Stream {
    /// Returns if stream is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the length in bytes of this steam.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns the bytes of this steam
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    /// Clears the steam of all bytes.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Extends the stream with the given bytes.
    fn extend(&mut self, bytes: &[u8]) {
        self.0.extend(bytes);
    }
}

impl From<Vec<u8>> for Stream {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl Stream {
    /// Marks the beginning of the scope.
    /// Returns position where to write scope size once the scope is closed
    #[inline]
    pub fn begin_scope<F: Fn() -> i64>(
        &mut self,
        now_ns: F,
        scope_id: ScopeId,
        data: &str,
    ) -> (usize, NanoSecond) {
        self.0.push(SCOPE_BEGIN);

        self.write_scope_id(scope_id);
        let time_stamp_offset = self.0.len();
        self.0
            .write_i64::<LE>(NanoSecond::default())
            .expect("can't fail");

        self.write_str(data);
        // Put place-holder value for total scope size.
        let offset = self.0.len();
        self.write_scope_size(ScopeSize::unfinished());

        // Do the timing last such that it doesn't include serialization
        let mut time_stamp_dest =
            &mut self.0[time_stamp_offset..time_stamp_offset + size_of::<NanoSecond>()];
        let start_ns = now_ns();
        time_stamp_dest
            .write_i64::<LE>(start_ns)
            .expect("can't fail");
        (offset, start_ns)
    }

    /// Marks the end of the scope.
    #[inline]
    pub fn end_scope(&mut self, start_offset: usize, stop_ns: NanoSecond) {
        // Write total scope size where scope was started:
        let scope_size = self.0.len() - (start_offset + size_of::<ScopeSize>());
        debug_assert!(start_offset + size_of::<ScopeSize>() <= self.0.len());
        let mut dest_range = &mut self.0[start_offset..start_offset + size_of::<ScopeSize>()];
        dest_range
            .write_u64::<LE>(scope_size as u64)
            .expect("can't fail");
        debug_assert!(dest_range.is_empty());

        // Write scope end:
        self.0.push(SCOPE_END);
        self.write_nanos(stop_ns);
    }

    #[inline]
    fn write_nanos(&mut self, nanos: NanoSecond) {
        self.0.write_i64::<LE>(nanos).expect("can't fail");
    }

    #[inline]
    fn write_scope_size(&mut self, nanos: ScopeSize) {
        self.0.write_u64::<LE>(nanos.0).expect("can't fail");
    }

    #[inline]
    fn write_scope_id(&mut self, scope_id: ScopeId) {
        // Could potentially use varint encoding.
        self.0
            .write_u32::<LE>(scope_id.0.get())
            .expect("can't fail");
    }

    #[inline]
    fn write_str(&mut self, s: &str) {
        // Future-proof: we may want to use VLQs later.
        const MAX_STRING_LENGTH: usize = 127;
        let len = s.len().min(MAX_STRING_LENGTH);
        self.0.write_u8(len as u8).expect("can't fail");
        self.0.extend(s[0..len].as_bytes()); // This may split a character in two. The parser should handle that.
    }
}

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
    /// The default value is ([`NanoSecond::MAX`], [`NanoSecond::MIN`]) which indicates an empty stream.
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
    /// Try to avoid calling this, and instead keep score while collecting a [`StreamInfo`].
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

    /// Extends this [`StreamInfo`] with another [`StreamInfo`].
    pub fn extend(&mut self, other: &StreamInfoRef<'_>) {
        self.stream.extend(other.stream);
        self.num_scopes += other.num_scopes;
        self.depth = self.depth.max(other.depth);
        self.range_ns.0 = self.range_ns.0.min(other.range_ns.0);
        self.range_ns.1 = self.range_ns.1.max(other.range_ns.1);
    }

    /// Clears the contents of this [`StreamInfo`].
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

    /// Returns a reference to the contents of this [`StreamInfo`].
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
    /// The default value is ([`NanoSecond::MAX`], [`NanoSecond::MIN`]) which indicates an empty stream.
    pub range_ns: (NanoSecond, NanoSecond),
}

/// Used to encode number of bytes covered by a scope.
#[derive(Clone, Copy, Eq, PartialEq)]
struct ScopeSize(u64);

impl ScopeSize {
    /// Special value to indicate that this profile scope was never closed
    pub fn unfinished() -> Self {
        Self(u64::MAX)
    }
}

/// Errors that can happen when parsing a [`Stream`] of profile data.
#[derive(Debug)]
pub enum Error {
    /// Could not read data from the stream because it ended prematurely.
    PrematureEnd,
    /// The stream is invalid.
    InvalidStream,
    /// The stream was not ended.
    ScopeNeverEnded,
    /// The offset into the stream is invalid.
    InvalidOffset,
    /// Empty stream.
    Empty,
}

/// Custom puffin result type.
pub type Result<T> = std::result::Result<T, Error>;
/// Parses a [`Stream`] of profiler data.
pub struct Reader<'s>(std::io::Cursor<&'s [u8]>);

impl<'s> Reader<'s> {
    /// Returns a reader that starts reading from the start of the stream.
    pub fn from_start(stream: &'s Stream) -> Self {
        Self(std::io::Cursor::new(&stream.0[..]))
    }

    /// Returns a reader that starts reading from an offset into the stream.
    pub fn with_offset(stream: &'s Stream, offset: u64) -> Result<Self> {
        if offset <= stream.len() as u64 {
            let mut cursor = std::io::Cursor::new(&stream.0[..]);
            cursor.set_position(offset);
            Ok(Self(cursor))
        } else {
            Err(Error::InvalidOffset)
        }
    }

    /// Parse the next scope in the stream, if any,
    /// and advance to the next sibling scope (if any).
    fn parse_scope(&mut self) -> Result<Option<Scope<'s>>> {
        match self.peek_u8() {
            Some(SCOPE_BEGIN) => {
                self.parse_u8()
                    .expect("swallowing already peeked SCOPE_BEGIN");
            }
            Some(_) | None => return Ok(None),
        }

        let scope_id = self.parse_scope_id()?;
        let start_ns = self.parse_nanos()?;
        let data = self.parse_string()?;
        let scope_size = self.parse_scope_size()?;
        if scope_size == ScopeSize::unfinished() {
            return Err(Error::ScopeNeverEnded);
        }
        let child_begin_position = self.0.position();
        self.0.set_position(child_begin_position + scope_size.0);
        let child_end_position = self.0.position();

        if self.parse_u8()? != SCOPE_END {
            return Err(Error::InvalidStream);
        }
        let stop_ns = self.parse_nanos()?;
        if stop_ns < start_ns {
            return Err(Error::InvalidStream);
        }

        Ok(Some(Scope {
            id: scope_id,
            record: ScopeRecord {
                start_ns,
                duration_ns: stop_ns - start_ns,
                data,
            },
            child_begin_position,
            child_end_position,
            next_sibling_position: self.0.position(),
        }))
    }

    /// Read all the top-level scopes (non-recursive) until the end of the stream.
    pub fn read_top_scopes(self) -> Result<Vec<Scope<'s>>> {
        let mut scopes = vec![];
        for scope in self {
            scopes.push(scope?);
        }
        Ok(scopes)
    }

    /// [`None`] if at end of stream
    fn peek_u8(&mut self) -> Option<u8> {
        let position = self.0.position();
        let value = self.0.read_u8().ok();
        self.0.set_position(position);
        value
    }

    fn parse_u8(&mut self) -> Result<u8> {
        self.0.read_u8().map_err(|_err| Error::PrematureEnd)
    }

    fn parse_scope_id(&mut self) -> Result<ScopeId> {
        self.0
            .read_u32::<LE>()
            .context("Can not parse scope id")
            .and_then(|x| NonZeroU32::new(x).context("Not a `NonZeroU32` scope id"))
            .map(ScopeId)
            .map_err(|_err| Error::PrematureEnd)
    }

    fn parse_nanos(&mut self) -> Result<NanoSecond> {
        self.0.read_i64::<LE>().map_err(|_err| Error::PrematureEnd)
    }

    fn parse_scope_size(&mut self) -> Result<ScopeSize> {
        self.0
            .read_u64::<LE>()
            .map_err(|_err| Error::PrematureEnd)
            .map(ScopeSize)
    }

    fn parse_string(&mut self) -> Result<&'s str> {
        let len = self.parse_u8().map_err(|_err| Error::PrematureEnd)? as usize;
        let data = self.0.get_ref();
        let begin = self.0.position() as usize;
        let end = begin + len;
        if end <= data.len() {
            let s = longest_valid_utf8_prefix(&data[begin..end]);
            self.0.set_position(end as u64);
            Ok(s)
        } else {
            Err(Error::PrematureEnd)
        }
    }

    /// Recursively count all profile scopes in a stream.
    /// Returns total number of scopes and maximum recursion depth.
    pub fn count_scope_and_depth(stream: &Stream) -> Result<(usize, usize)> {
        let mut max_depth = 0;
        let num_scopes = Self::count_all_scopes_at_offset(stream, 0, 0, &mut max_depth)?;
        Ok((num_scopes, max_depth))
    }

    fn count_all_scopes_at_offset(
        stream: &Stream,
        offset: u64,
        depth: usize,
        max_depth: &mut usize,
    ) -> Result<usize> {
        *max_depth = (*max_depth).max(depth);

        let mut num_scopes = 0;
        for child_scope in Reader::with_offset(stream, offset)? {
            num_scopes += 1 + Self::count_all_scopes_at_offset(
                stream,
                child_scope?.child_begin_position,
                depth + 1,
                max_depth,
            )?;
        }

        Ok(num_scopes)
    }
}

fn longest_valid_utf8_prefix(data: &[u8]) -> &str {
    match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(error) => {
            // The string may be been truncated to fit a max length of 255.
            // This truncation may have happened in the middle of a unicode character.
            std::str::from_utf8(&data[..error.valid_up_to()]).expect("We can trust valid_up_to")
        }
    }
}

/// Read each top-level sibling scopes
impl<'s> Iterator for Reader<'s> {
    type Item = Result<Scope<'s>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.parse_scope().transpose()
    }
}

#[test]
fn write_scope() {
    let mut stream: Stream = Stream::default();
    let start = stream.begin_scope(|| 100, ScopeId::new(1), "data");
    stream.end_scope(start.0, 300);

    let scopes = Reader::from_start(&stream).read_top_scopes().unwrap();
    assert_eq!(scopes.len(), 1);
    assert_eq!(
        scopes[0].record,
        ScopeRecord {
            start_ns: 100,
            duration_ns: 200,
            data: "data"
        }
    );
}

#[test]
fn test_profile_data() {
    let stream = {
        let mut stream = Stream::default();
        let (t0, _) = stream.begin_scope(|| 100, ScopeId::new(1), "data_top");
        let (m1, _) = stream.begin_scope(|| 200, ScopeId::new(2), "data_middle_0");
        stream.end_scope(m1, 300);
        let (m1, _) = stream.begin_scope(|| 300, ScopeId::new(3), "data_middle_1");
        stream.end_scope(m1, 400);
        stream.end_scope(t0, 400);
        stream
    };

    let top_scopes = Reader::from_start(&stream).read_top_scopes().unwrap();
    assert_eq!(top_scopes.len(), 1);
    assert_eq!(
        top_scopes[0].record,
        ScopeRecord {
            start_ns: 100,
            duration_ns: 300,
            data: "data_top"
        }
    );

    let middle_scopes = Reader::with_offset(&stream, top_scopes[0].child_begin_position)
        .unwrap()
        .read_top_scopes()
        .unwrap();

    assert_eq!(middle_scopes.len(), 2);

    assert_eq!(
        middle_scopes[0].record,
        ScopeRecord {
            start_ns: 200,
            duration_ns: 100,
            data: "data_middle_0"
        }
    );
    assert_eq!(
        middle_scopes[1].record,
        ScopeRecord {
            start_ns: 300,
            duration_ns: 100,
            data: "data_middle_1"
        }
    );
}
