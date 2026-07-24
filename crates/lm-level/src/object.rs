use crate::LegacyLevelHeader;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectRecord {
    pub(crate) encoded: Vec<u8>,
}

impl ObjectRecord {
    /// Constructs a conservatively bounded encoded object record.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectStreamError::InvalidRecord`] for invalid lengths or embedded terminators.
    pub fn new(encoded: Vec<u8>) -> Result<Self, ObjectStreamError> {
        if (3..=8).contains(&encoded.len()) && encoded.first() != Some(&0xff) {
            Ok(Self { encoded })
        } else {
            Err(ObjectStreamError::InvalidRecord)
        }
    }

    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ObjectStream {
    pub records: Vec<ObjectRecord>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LevelObjectData {
    pub header: LegacyLevelHeader,
    pub objects: ObjectStream,
}

impl LevelObjectData {
    /// Parses the five-byte level header followed by a terminated object stream.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectStreamError`] for a truncated header or malformed object stream.
    pub fn parse(bytes: &[u8]) -> Result<Self, ObjectStreamError> {
        let header = bytes
            .get(..LegacyLevelHeader::ENCODED_LEN)
            .ok_or(ObjectStreamError::Truncated { offset: 0 })?;
        Ok(Self {
            header: LegacyLevelHeader::decode(header)
                .map_err(|_| ObjectStreamError::Truncated { offset: 0 })?,
            objects: ObjectStream::parse(&bytes[LegacyLevelHeader::ENCODED_LEN..])?,
        })
    }

    /// Encodes the header and object stream after exact aggregate-size preflight.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectStreamError::SizeOverflow`] if the aggregate length is not representable.
    pub fn encode(&self) -> Result<Vec<u8>, ObjectStreamError> {
        let stream_len = self.objects.encoded_len()?;
        let encoded_len = LegacyLevelHeader::ENCODED_LEN
            .checked_add(stream_len)
            .ok_or(ObjectStreamError::SizeOverflow)?;
        let mut bytes = Vec::with_capacity(encoded_len);
        bytes.extend_from_slice(&self.header.encoded());
        self.objects.append_encoded(&mut bytes);
        Ok(bytes)
    }

    /// Encodes header and objects within Lunar Magic's single-bank payload limit.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectStreamError::BankLimitExceeded`] above 0x8000 bytes.
    pub fn encode_banked(&self) -> Result<Vec<u8>, ObjectStreamError> {
        let stream_len = self.objects.encoded_len()?;
        let encoded_len = LegacyLevelHeader::ENCODED_LEN
            .checked_add(stream_len)
            .ok_or(ObjectStreamError::SizeOverflow)?;
        if encoded_len > 0x8000 {
            return Err(ObjectStreamError::BankLimitExceeded);
        }
        self.encode()
    }
}

impl ObjectStream {
    /// Parses the native Lunar Magic/SMW variable-length object stream.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectStreamError`] for malformed records, truncation, or no terminator.
    pub fn parse(bytes: &[u8]) -> Result<Self, ObjectStreamError> {
        Self::parse_with(bytes, encoded_record_length)
    }

    /// Parses a lossless object stream using a caller-provided record-size function.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectStreamError`] for invalid sizes, truncation, or a missing terminator.
    pub fn parse_with(
        bytes: &[u8],
        mut record_len: impl FnMut(&[u8]) -> Option<usize>,
    ) -> Result<Self, ObjectStreamError> {
        let mut offset = 0;
        let mut records = Vec::new();
        loop {
            let Some(first) = bytes.get(offset) else {
                return Err(ObjectStreamError::MissingTerminator);
            };
            if *first == 0xff {
                return Ok(Self { records });
            }
            let len = record_len(&bytes[offset..])
                .ok_or(ObjectStreamError::UnknownRecordLength { offset })?;
            let end = offset
                .checked_add(len)
                .ok_or(ObjectStreamError::Truncated { offset })?;
            let encoded = bytes
                .get(offset..end)
                .ok_or(ObjectStreamError::Truncated { offset })?
                .to_vec();
            records.push(ObjectRecord::new(encoded)?);
            offset = end;
        }
    }

    /// Encodes all records and the terminator after exact aggregate-size preflight.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectStreamError::SizeOverflow`] if record lengths plus the terminator overflow.
    pub fn encode(&self) -> Result<Vec<u8>, ObjectStreamError> {
        let mut result = Vec::with_capacity(self.encoded_len()?);
        self.append_encoded(&mut result);
        Ok(result)
    }

    fn encoded_len(&self) -> Result<usize, ObjectStreamError> {
        checked_stream_len(self.records.iter().map(|record| record.encoded().len()))
    }

    fn append_encoded(&self, result: &mut Vec<u8>) {
        for record in &self.records {
            result.extend_from_slice(record.encoded());
        }
        result.push(0xff);
    }

    /// Encodes a stream subject to the single-bank limit used by Lunar Magic.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectStreamError::BankLimitExceeded`] if the terminator would cross 0x8000 bytes.
    pub fn encode_banked(&self) -> Result<Vec<u8>, ObjectStreamError> {
        if self.encoded_len()? > 0x8000 {
            return Err(ObjectStreamError::BankLimitExceeded);
        }
        self.encode()
    }
}

fn checked_stream_len(
    record_lengths: impl IntoIterator<Item = usize>,
) -> Result<usize, ObjectStreamError> {
    record_lengths.into_iter().try_fold(1_usize, |total, len| {
        total
            .checked_add(len)
            .ok_or(ObjectStreamError::SizeOverflow)
    })
}

/// Returns the encoded length recovered from `GetEncodedLevelObjectRecordLength`.
#[must_use]
pub fn encoded_record_length(bytes: &[u8]) -> Option<usize> {
    let first = *bytes.first()?;
    let second = *bytes.get(1)?;
    let third = *bytes.get(2)?;
    let command = ((u16::from(second >> 3) | u16::from(first & 0x60)) >> 1).to_le_bytes()[0];
    Some(match command {
        0 if third == 0 => 4,
        0 if third == 2 => 5,
        0x22 | 0x23 => 4,
        0x27 | 0x29 => {
            let mode = bytes.get(3)? >> 6;
            match mode {
                2 => 6,
                3 => 7 + usize::from(third & 0x80 != 0),
                _ => 5,
            }
        }
        0x2d => 5,
        _ => 3,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObjectStreamError {
    InvalidRecord,
    MissingTerminator,
    UnknownRecordLength { offset: usize },
    Truncated { offset: usize },
    SizeOverflow,
    BankLimitExceeded,
}

impl fmt::Display for ObjectStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid level-object stream: {self:?}")
    }
}

impl std::error::Error for ObjectStreamError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossless_round_trip() {
        let bytes = [1, 2, 3, 4, 5, 6, 0xff];
        let stream = ObjectStream::parse_with(&bytes, |_| Some(3)).unwrap();
        assert_eq!(stream.encode().unwrap(), bytes);
    }

    #[test]
    fn recovered_length_rules() {
        assert_eq!(encoded_record_length(&[0, 0, 0, 0]), Some(4));
        assert_eq!(encoded_record_length(&[0, 0, 2, 0, 0]), Some(5));
        assert_eq!(
            encoded_record_length(&[0x40, 0x70, 0, 0xc0, 0, 0, 0]),
            Some(7)
        );
    }

    #[test]
    fn level_header_and_objects_round_trip_together() {
        let bytes = [1, 2, 3, 4, 5, 9, 8, 7, 0xff];
        let data = LevelObjectData {
            header: LegacyLevelHeader::decode(&bytes[..5]).unwrap(),
            objects: ObjectStream::parse_with(&bytes[5..], |_| Some(3)).unwrap(),
        };
        assert_eq!(data.encode().unwrap(), bytes);
    }

    #[test]
    fn aggregate_length_overflow_is_typed_without_allocating() {
        assert_eq!(checked_stream_len([3, 5, 8]).unwrap(), 17);
        assert_eq!(
            checked_stream_len([usize::MAX]),
            Err(ObjectStreamError::SizeOverflow)
        );
    }
}
