use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExAnimationRecord {
    bytes: [u8; Self::ENCODED_LEN],
}

impl ExAnimationRecord {
    pub const ENCODED_LEN: usize = 0x20a;

    /// Parses one exact-size record without assigning unproven field meanings.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationError::WrongRecordSize`] unless exactly 522 bytes are supplied.
    pub fn decode(bytes: &[u8]) -> Result<Self, ExAnimationError> {
        let bytes = bytes
            .try_into()
            .map_err(|_| ExAnimationError::WrongRecordSize(bytes.len()))?;
        Ok(Self { bytes })
    }

    #[must_use]
    pub const fn encoded(&self) -> &[u8; Self::ENCODED_LEN] {
        &self.bytes
    }

    #[must_use]
    pub const fn kind(&self) -> u8 {
        self.bytes[0]
    }

    #[must_use]
    pub const fn frame_count_minus_one(&self) -> u8 {
        self.bytes[1]
    }

    #[must_use]
    pub const fn size_mode(&self) -> u8 {
        self.bytes[2]
    }

    #[must_use]
    pub const fn destination(&self) -> u16 {
        u16::from_le_bytes([self.bytes[4], self.bytes[5]]) & 0x7fff
    }

    #[must_use]
    pub const fn destination_flag(&self) -> bool {
        self.bytes[6] != 0
    }

    #[must_use]
    pub fn frame_bytes(&self, double_size: bool) -> &[u8] {
        let len = compact_frame_len(self.kind(), self.frame_count_minus_one(), double_size);
        &self.bytes[8..8 + len]
    }

    pub(crate) fn with_frame_payload(
        &self,
        frame_count_minus_one: u8,
        frame_bytes: &[u8],
        double_size: bool,
    ) -> Result<Self, ExAnimationError> {
        let expected = checked_compact_frame_len(self.kind(), frame_count_minus_one, double_size)?;
        if frame_bytes.len() != expected {
            return Err(ExAnimationError::WrongFrameSize {
                expected,
                actual: frame_bytes.len(),
            });
        }
        let mut edited = self.clone();
        edited.bytes[1] = frame_count_minus_one;
        let previous = compact_frame_len(self.kind(), self.frame_count_minus_one(), double_size);
        edited.bytes[8..8 + previous.max(expected)].fill(0);
        edited.bytes[8..8 + expected].copy_from_slice(frame_bytes);
        Ok(edited)
    }

    #[must_use]
    pub const fn inactive() -> Self {
        Self {
            bytes: [0; Self::ENCODED_LEN],
        }
    }

    /// Constructs one validated in-memory record from semantic metadata and frame bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationError`] for destinations above `0x7FFF` or a frame payload whose
    /// length does not match the type/count/size-mode combination.
    pub fn new(
        kind: u8,
        frame_count_minus_one: u8,
        size_mode: u8,
        destination: u16,
        destination_flag: bool,
        frame_bytes: &[u8],
        double_size: bool,
    ) -> Result<Self, ExAnimationError> {
        if destination > 0x7fff {
            return Err(ExAnimationError::DestinationOutOfRange(destination));
        }
        let expected = checked_compact_frame_len(kind, frame_count_minus_one, double_size)?;
        if frame_bytes.len() != expected {
            return Err(ExAnimationError::WrongFrameSize {
                expected,
                actual: frame_bytes.len(),
            });
        }
        let mut record = Self::inactive();
        record.bytes[0] = kind;
        record.bytes[1] = frame_count_minus_one;
        record.bytes[2] = size_mode;
        record.bytes[4..6].copy_from_slice(&destination.to_le_bytes());
        record.bytes[6] = u8::from(destination_flag);
        record.bytes[8..8 + expected].copy_from_slice(frame_bytes);
        record.validate_compact(0, double_size)?;
        Ok(record)
    }

    fn validate_compact(&self, record: usize, double_size: bool) -> Result<(), ExAnimationError> {
        let frame_len =
            checked_compact_frame_len(self.kind(), self.frame_count_minus_one(), double_size)?;
        for (offset, value) in self.bytes.iter().copied().enumerate() {
            let represented =
                matches!(offset, 0..=2 | 4..=6) || (8..8 + frame_len).contains(&offset);
            if !represented && value != 0 {
                return Err(ExAnimationError::UnrepresentedRecordByte {
                    record,
                    offset,
                    value,
                });
            }
        }
        if self.kind() == 0 {
            if let Some((offset, value)) = self
                .bytes
                .iter()
                .copied()
                .enumerate()
                .find(|(_, value)| *value != 0)
            {
                return Err(ExAnimationError::UnrepresentedRecordByte {
                    record,
                    offset,
                    value,
                });
            }
        } else if self.bytes[5] & 0x80 != 0 || self.bytes[6] > 1 {
            let offset = if self.bytes[5] & 0x80 != 0 { 5 } else { 6 };
            return Err(ExAnimationError::UnrepresentedRecordByte {
                record,
                offset,
                value: self.bytes[offset],
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExAnimationSet {
    pub records: Vec<ExAnimationRecord>,
    pub visible_slots: usize,
}

#[path = "exanimation_compact.rs"]
mod compact;

pub use compact::CompactExAnimation;
#[cfg(test)]
use compact::declared_compact_frame_len;
use compact::{checked_compact_frame_len, compact_frame_len};

impl ExAnimationSet {
    /// Decodes a fixed number of recovered 0x20a-byte slot records.
    ///
    /// # Errors
    ///
    /// Returns an error for a mismatched byte count or impossible visible-slot count.
    pub fn decode(
        bytes: &[u8],
        record_count: usize,
        visible_slots: usize,
    ) -> Result<Self, ExAnimationError> {
        if visible_slots > record_count {
            return Err(ExAnimationError::TooManyVisibleSlots {
                visible_slots,
                record_count,
            });
        }
        let expected = record_count
            .checked_mul(ExAnimationRecord::ENCODED_LEN)
            .ok_or(ExAnimationError::WrongSetSize {
                expected: usize::MAX,
                actual: bytes.len(),
            })?;
        if bytes.len() != expected {
            return Err(ExAnimationError::WrongSetSize {
                expected,
                actual: bytes.len(),
            });
        }
        let records = bytes
            .chunks_exact(ExAnimationRecord::ENCODED_LEN)
            .map(ExAnimationRecord::decode)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            records,
            visible_slots,
        })
    }

    /// Encodes all fixed-size records after validating visible slots and aggregate byte length.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationError`] for an impossible visible-slot count or size overflow.
    pub fn encode(&self) -> Result<Vec<u8>, ExAnimationError> {
        let encoded_len = checked_set_len(self.records.len(), self.visible_slots)?;
        let mut encoded = Vec::with_capacity(encoded_len);
        for record in &self.records {
            encoded.extend_from_slice(record.encoded());
        }
        Ok(encoded)
    }
}

fn checked_set_len(record_count: usize, visible_slots: usize) -> Result<usize, ExAnimationError> {
    if visible_slots > record_count {
        return Err(ExAnimationError::TooManyVisibleSlots {
            visible_slots,
            record_count,
        });
    }
    record_count
        .checked_mul(ExAnimationRecord::ENCODED_LEN)
        .ok_or(ExAnimationError::SetSizeOverflow { record_count })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationError {
    WrongRecordSize(usize),
    WrongSetSize {
        expected: usize,
        actual: usize,
    },
    SetSizeOverflow {
        record_count: usize,
    },
    TooManyVisibleSlots {
        visible_slots: usize,
        record_count: usize,
    },
    Truncated {
        offset: usize,
        needed: usize,
    },
    TooManyRecords {
        actual: usize,
        maximum: usize,
    },
    MissingSizeMode(u8),
    InvalidOffset,
    DestinationOutOfRange(u16),
    WrongFrameSize {
        expected: usize,
        actual: usize,
    },
    FramePayloadTooLarge {
        actual: usize,
        maximum: usize,
    },
    DisabledTriggerValue {
        trigger: usize,
        value: u8,
    },
    UnrepresentedRecordByte {
        record: usize,
        offset: usize,
        value: u8,
    },
}

impl fmt::Display for ExAnimationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ExAnimation record has wrong size: {self:?}")
    }
}

impl std::error::Error for ExAnimationError {}

#[cfg(test)]
#[path = "exanimation_tests.rs"]
mod tests;
