use std::{fmt::Display, str::from_utf8};

/// Header of serialized data.
///
/// Used to differentiate data of `ScopeCollection` and `FrameData` by example.
#[derive(Debug, Clone, Copy)]
pub struct DataHeader([u8; 4]);

impl DataHeader {
    /// Tried to read header from reader.
    pub fn try_read(read: &mut impl std::io::Read) -> std::result::Result<Self, std::io::Error> {
        let mut header = [0_u8; 4];
        read.read_exact(&mut header)?;
        Ok(DataHeader(header))
    }

    /// Return a slice containing the entire header.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Return the header as array.
    pub fn bytes(&self) -> [u8; 4] {
        self.0
    }
}

impl Display for DataHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let header = from_utf8(&self.0).unwrap_or("????");
        write!(f, "{header}")
    }
}

impl From<DataHeader> for [u8; 4] {
    fn from(val: DataHeader) -> Self {
        val.0
    }
}

impl PartialEq<[u8; 4]> for &DataHeader {
    fn eq(&self, other: &[u8; 4]) -> bool {
        &self.0 == other
    }
}
impl PartialEq<&[u8]> for &DataHeader {
    fn eq(&self, other: &&[u8]) -> bool {
        &self.0 == other
    }
}
