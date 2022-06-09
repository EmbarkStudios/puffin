//! The profiler records all events into a byte stream.
//! The profiler UI parses this byte stream as needed, on the fly.
//! The data format is as such:
//!
//! Each scope start consists of:
//!
//!    '('          byte       Sentinel
//!    time_ns      i64        Time stamp of when scope started
//!    id           str        Scope name. Human readable, e.g. a function name. Never the empty string.
//!    location     str        File name or similar. Could be the empty string.
//!    data         str        Resource that is being processed, e.g. name of image being loaded. Could be the empty string.
//!    scope_size   u64        Number of bytes of child scope
//!
//! This is followed by `scope_size` number of bytes of data
//! containing any child scopes. The scope is then closed by:
//!
//!    ')'          byte       Sentinel
//!    time_ns      i64        Time stamp of when scope finished
//!
//! Integers are encoded in little endian.
//! Strings are encoded as a single u8 length + that many bytes of UTF8.
//! At the moment strings may be at most 127 bytes long.

use super::*;
use byteorder::{LittleEndian as LE, ReadBytesExt, WriteBytesExt};
use std::mem::size_of;

const SCOPE_BEGIN: u8 = b'(';
const SCOPE_END: u8 = b')';

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
    PrematureEnd,
    InvalidStream,
    ScopeNeverEnded,
    InvalidOffset,
    Empty,
}

pub type Result<T> = std::result::Result<T, Error>;

// ----------------------------------------------------------------------------

impl Stream {
    /// Returns position where to write scope size once the scope is closed
    #[inline]
    pub fn begin_scope(
        &mut self,
        start_ns: NanoSecond,
        id: &str,
        location: &str,
        data: &str,
    ) -> usize {
        self.0.push(SCOPE_BEGIN);
        self.0.write_i64::<LE>(start_ns).expect("can't fail");
        self.write_str(id);
        self.write_str(location);
        self.write_str(data);

        // Put place-holder value for total scope size.
        let offset = self.0.len();
        self.write_scope_size(ScopeSize::unfinished());
        offset
    }

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
    fn write_str(&mut self, s: &str) {
        // Future-proof: we may want to use VLQs later.
        const MAX_STRING_LENGTH: usize = 127;
        let len = s.len().min(MAX_STRING_LENGTH);
        self.0.write_u8(len as u8).expect("can't fail");
        self.0.extend(s[0..len].as_bytes()); // This may split a character in two. The parser should handle that.
    }
}

// ----------------------------------------------------------------------------

/// Parses a [`Stream`] of profiler data.
pub struct Reader<'s>(std::io::Cursor<&'s [u8]>);

impl<'s> Reader<'s> {
    pub fn from_start(stream: &'s Stream) -> Self {
        Self(std::io::Cursor::new(&stream.0[..]))
    }

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
            Some(_) | None => {
                return Ok(None);
            }
        }

        let start_ns = self.parse_nanos()?;
        let id = self.parse_string()?;
        let location = self.parse_string()?;
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
            record: Record {
                start_ns,
                duration_ns: stop_ns - start_ns,
                id,
                location,
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

// ----------------------------------------------------------------------------

/// Read each top-level sibling scopes
impl<'s> Iterator for Reader<'s> {
    type Item = Result<Scope<'s>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.parse_scope().transpose()
    }
}

// ----------------------------------------------------------------------------

#[test]
fn test_profile_data() {
    let stream = {
        let mut stream = Stream::default();
        let t0 = stream.begin_scope(100, "top", "top.rs", "data_top");
        let m1 = stream.begin_scope(200, "middle_0", "middle.rs", "data_middle_0");
        stream.end_scope(m1, 300);
        let m1 = stream.begin_scope(300, "middle_1", "middle.rs:42", "data_middle_1");
        stream.end_scope(m1, 400);
        stream.end_scope(t0, 400);
        stream
    };

    let top_scopes = Reader::from_start(&stream).read_top_scopes().unwrap();
    assert_eq!(top_scopes.len(), 1);
    let middle_scopes = Reader::with_offset(&stream, top_scopes[0].child_begin_position)
        .unwrap()
        .read_top_scopes()
        .unwrap();

    assert_eq!(
        top_scopes[0].record,
        Record {
            start_ns: 100,
            duration_ns: 300,
            id: "top",
            location: "top.rs",
            data: "data_top",
        }
    );
    assert_eq!(
        top_scopes[0].next_sibling_position,
        stream.len() as u64,
        "Top scope has no siblings"
    );

    assert_eq!(middle_scopes.len(), 2);
    assert_eq!(
        middle_scopes[0].record,
        Record {
            start_ns: 200,
            duration_ns: 100,
            id: "middle_0",
            location: "middle.rs",
            data: "data_middle_0",
        }
    );
    assert_eq!(
        middle_scopes[1].record,
        Record {
            start_ns: 300,
            duration_ns: 100,
            id: "middle_1",
            location: "middle.rs:42",
            data: "data_middle_1",
        }
    );
}
