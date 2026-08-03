use std::fmt;

#[path = "clipboard_domains.rs"]
mod domains;
pub use domains::{NativeMap16Clipboard, NativeMap16ClipboardError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ClipboardKind {
    LevelObjects = 1,
    LevelSprites = 2,
    Map16Tiles = 3,
    GraphicsTiles = 4,
    PaletteColors = 5,
    ExAnimationRecords = 6,
    OverworldMessages = 7,
    OverworldSprites = 8,
    Layer3TilemapBytes = 9,
    Layer3RemapBytes = 10,
    ExAnimationFrames = 11,
    Layer2TilemapSelection = 12,
    OverworldAppearanceParts = 13,
}

impl ClipboardKind {
    const fn decode(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::LevelObjects),
            2 => Some(Self::LevelSprites),
            3 => Some(Self::Map16Tiles),
            4 => Some(Self::GraphicsTiles),
            5 => Some(Self::PaletteColors),
            6 => Some(Self::ExAnimationRecords),
            7 => Some(Self::OverworldMessages),
            8 => Some(Self::OverworldSprites),
            9 => Some(Self::Layer3TilemapBytes),
            10 => Some(Self::Layer3RemapBytes),
            11 => Some(Self::ExAnimationFrames),
            12 => Some(Self::Layer2TilemapSelection),
            13 => Some(Self::OverworldAppearanceParts),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardPayload {
    pub kind: ClipboardKind,
    records: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    UnknownKind(u8),
    UnknownFlags(u8),
    TooManyRecords(usize),
    RecordTooLarge(usize),
    PayloadTooLarge(usize),
    TrailingBytes(usize),
    WrongKind {
        expected: ClipboardKind,
        actual: ClipboardKind,
    },
    InvalidRecord {
        index: usize,
        length: usize,
    },
    InvalidPixel {
        record: usize,
        pixel: usize,
        value: u8,
    },
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Lunar Magic clipboard payload: {self:?}")
    }
}

impl std::error::Error for ClipboardError {}

impl ClipboardPayload {
    pub const MIME_TYPE: &'static str = "application/x-lm-editor-clipboard";
    const MAGIC: &'static [u8; 6] = b"LMCLIP";
    const VERSION: u16 = 1;
    pub const MAX_RECORDS: usize = 0x1_0000;
    pub const MAX_RECORD_LEN: usize = 0x10_0000;
    pub const MAX_ENCODED_LEN: usize = 64 * 1024 * 1024;

    /// Constructs a bounded collection of losslessly framed records.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] if a count or record exceeds the portable format limits.
    pub fn new(kind: ClipboardKind, records: Vec<Vec<u8>>) -> Result<Self, ClipboardError> {
        validate_records(&records, Self::MAX_ENCODED_LEN)?;
        Ok(Self { kind, records })
    }

    #[must_use]
    pub fn records(&self) -> &[Vec<u8>] {
        &self.records
    }

    /// Encodes a versioned, endian-stable payload for a platform clipboard MIME type.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] if a typed payload exceeds the format's count or size limits.
    pub fn encode(&self) -> Result<Vec<u8>, ClipboardError> {
        let capacity = validate_records(&self.records, Self::MAX_ENCODED_LEN)?;
        let mut bytes = Vec::with_capacity(capacity);
        bytes.extend_from_slice(Self::MAGIC);
        bytes.extend_from_slice(&Self::VERSION.to_le_bytes());
        bytes.push(self.kind as u8);
        bytes.push(0);
        let count = u32::try_from(self.records.len())
            .map_err(|_| ClipboardError::TooManyRecords(self.records.len()))?;
        bytes.extend_from_slice(&count.to_le_bytes());
        for record in &self.records {
            if record.len() > Self::MAX_RECORD_LEN {
                return Err(ClipboardError::RecordTooLarge(record.len()));
            }
            let length = u32::try_from(record.len())
                .map_err(|_| ClipboardError::RecordTooLarge(record.len()))?;
            bytes.extend_from_slice(&length.to_le_bytes());
            bytes.extend_from_slice(record);
        }
        Ok(bytes)
    }

    /// Decodes an exact versioned payload and rejects trailing or oversized data.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for malformed framing, unsupported versions, or invalid limits.
    pub fn decode(bytes: &[u8]) -> Result<Self, ClipboardError> {
        Self::decode_with_limit(bytes, Self::MAX_ENCODED_LEN)
    }

    fn decode_with_limit(bytes: &[u8], maximum_encoded_len: usize) -> Result<Self, ClipboardError> {
        if bytes.len() > maximum_encoded_len {
            return Err(ClipboardError::PayloadTooLarge(bytes.len()));
        }
        let header = bytes.get(..14).ok_or(ClipboardError::Truncated)?;
        if &header[..6] != Self::MAGIC {
            return Err(ClipboardError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[6], header[7]]);
        if version != Self::VERSION {
            return Err(ClipboardError::UnsupportedVersion(version));
        }
        let kind =
            ClipboardKind::decode(header[8]).ok_or(ClipboardError::UnknownKind(header[8]))?;
        if header[9] != 0 {
            return Err(ClipboardError::UnknownFlags(header[9]));
        }
        let count = usize::try_from(u32::from_le_bytes([
            header[10], header[11], header[12], header[13],
        ]))
        .map_err(|_| ClipboardError::TooManyRecords(usize::MAX))?;
        if count > Self::MAX_RECORDS {
            return Err(ClipboardError::TooManyRecords(count));
        }
        let mut offset = 14;
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            let length_bytes = bytes
                .get(offset..offset + 4)
                .ok_or(ClipboardError::Truncated)?;
            let length = usize::try_from(u32::from_le_bytes([
                length_bytes[0],
                length_bytes[1],
                length_bytes[2],
                length_bytes[3],
            ]))
            .map_err(|_| ClipboardError::RecordTooLarge(usize::MAX))?;
            if length > Self::MAX_RECORD_LEN {
                return Err(ClipboardError::RecordTooLarge(length));
            }
            offset = offset.checked_add(4).ok_or(ClipboardError::Truncated)?;
            let end = offset
                .checked_add(length)
                .ok_or(ClipboardError::Truncated)?;
            records.push(
                bytes
                    .get(offset..end)
                    .ok_or(ClipboardError::Truncated)?
                    .to_vec(),
            );
            offset = end;
        }
        if offset != bytes.len() {
            return Err(ClipboardError::TrailingBytes(bytes.len() - offset));
        }
        Self::new(kind, records)
    }
}

fn validate_records(
    records: &[Vec<u8>],
    maximum_encoded_len: usize,
) -> Result<usize, ClipboardError> {
    if records.len() > ClipboardPayload::MAX_RECORDS {
        return Err(ClipboardError::TooManyRecords(records.len()));
    }
    let mut encoded_len = 14_usize;
    for record in records {
        if record.len() > ClipboardPayload::MAX_RECORD_LEN {
            return Err(ClipboardError::RecordTooLarge(record.len()));
        }
        encoded_len = encoded_len
            .checked_add(4)
            .and_then(|length| length.checked_add(record.len()))
            .ok_or(ClipboardError::PayloadTooLarge(usize::MAX))?;
        if encoded_len > maximum_encoded_len {
            return Err(ClipboardError::PayloadTooLarge(encoded_len));
        }
    }
    Ok(encoded_len)
}

#[cfg(test)]
#[path = "clipboard_tests.rs"]
mod tests;
