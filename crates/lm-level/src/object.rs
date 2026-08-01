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

/// Lunar Magic's custom level timer stored in object-stream control command `$28`.
///
/// The ordinary five-byte level header can only select a preset timer. This control record
/// carries the complete 12-bit value used by Lunar Magic's bypass dialog. A value of zero means
/// infinite time and therefore requires `force_reset`; without that bit Lunar Magic treats the
/// all-zero control value as a disabled bypass and omits it when the level is next serialized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CustomTimeSettings {
    value: u16,
    force_reset: bool,
}

impl CustomTimeSettings {
    pub const MAX_VALUE: u16 = 0x0fff;

    /// Constructs a persistable Lunar Magic custom-time setting.
    ///
    /// # Errors
    ///
    /// Rejects values above `$FFF` and the non-persistable zero-without-force representation.
    pub const fn new(value: u16, force_reset: bool) -> Result<Self, CustomTimeError> {
        if value > Self::MAX_VALUE {
            Err(CustomTimeError::ValueOutOfRange(value))
        } else if value == 0 && !force_reset {
            Err(CustomTimeError::DisabledEncoding)
        } else {
            Ok(Self { value, force_reset })
        }
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.value
    }

    #[must_use]
    pub const fn force_reset(self) -> bool {
        self.force_reset
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomTimeError {
    ValueOutOfRange(u16),
    DisabledEncoding,
    BankLimitExceeded,
}

impl fmt::Display for CustomTimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid Lunar Magic custom-time setting: {self:?}"
        )
    }
}

impl std::error::Error for CustomTimeError {}

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

    /// Returns the last custom-time control value, matching Lunar Magic's decoder behavior.
    ///
    /// `vertical` selects the alternate nibble ordering used for vertical level modes. Reserved
    /// bits from noncanonical third-party streams are ignored while the raw record remains
    /// lossless until this setting is explicitly changed.
    #[must_use]
    pub fn custom_time(&self, vertical: bool) -> Option<CustomTimeSettings> {
        self.records.iter().rev().find_map(|record| {
            (record.command_id() == 0x28).then(|| {
                let first = u16::from(record.encoded[0] & 0x0f);
                let second = u16::from(record.encoded[1] & 0x0f);
                let low = if vertical {
                    (second << 4) | first
                } else {
                    (first << 4) | second
                };
                let raw = low | (u16::from(record.encoded[2]) << 8);
                CustomTimeSettings {
                    value: raw & CustomTimeSettings::MAX_VALUE,
                    force_reset: raw & 0x8000 != 0,
                }
            })
        })
    }

    /// Replaces Lunar Magic custom-time controls with one canonical trailing command `$28`.
    ///
    /// `None` disables the bypass. Duplicate or non-trailing command `$28` records are collapsed
    /// exactly as Lunar Magic does when it decodes and reserializes a level. Failure is atomic.
    ///
    /// # Errors
    ///
    /// Returns [`CustomTimeError::BankLimitExceeded`] if adding the control would cross Lunar
    /// Magic's single-bank object-stream limit.
    pub fn set_custom_time(
        &mut self,
        vertical: bool,
        settings: Option<CustomTimeSettings>,
    ) -> Result<(), CustomTimeError> {
        let mut staged = self.clone();
        staged.records.retain(|record| record.command_id() != 0x28);
        if let Some(settings) = settings {
            let low = settings.value.to_le_bytes()[0];
            let high_nibble = low >> 4;
            let low_nibble = low & 0x0f;
            let (first, second) = if vertical {
                (low_nibble | 0x40, high_nibble | 0x80)
            } else {
                (high_nibble | 0x40, low_nibble | 0x80)
            };
            let third = ((settings.value >> 8) as u8) | (u8::from(settings.force_reset) << 7);
            staged.records.push(ObjectRecord {
                encoded: vec![first, second, third],
            });
        }
        staged
            .encode_banked()
            .map_err(|_| CustomTimeError::BankLimitExceeded)?;
        *self = staged;
        Ok(())
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

    #[test]
    fn custom_time_matches_lunar_magic_horizontal_and_vertical_command_28() {
        let settings = CustomTimeSettings::new(0xabc, true).unwrap();
        let mut horizontal = ObjectStream::default();
        horizontal.set_custom_time(false, Some(settings)).unwrap();
        assert_eq!(horizontal.encode().unwrap(), [0x4b, 0x8c, 0x8a, 0xff]);
        assert_eq!(horizontal.custom_time(false), Some(settings));

        let mut vertical = ObjectStream::default();
        vertical.set_custom_time(true, Some(settings)).unwrap();
        assert_eq!(vertical.encode().unwrap(), [0x4c, 0x8b, 0x8a, 0xff]);
        assert_eq!(vertical.custom_time(true), Some(settings));
    }

    #[test]
    fn custom_time_edit_collapses_controls_and_preserves_ordinary_records() {
        let mut stream = ObjectStream::parse(&[
            0x11, 0x22, 0x33, 0x41, 0x82, 0x03, 0x55, 0x66, 0x77, 0x44, 0x85, 0x06, 0xff,
        ])
        .unwrap();
        let ordinary = [stream.records[0].clone(), stream.records[2].clone()];
        let settings = CustomTimeSettings::new(0x789, false).unwrap();
        stream.set_custom_time(false, Some(settings)).unwrap();
        assert_eq!(stream.records.len(), 3);
        assert_eq!(stream.records[..2], ordinary);
        assert_eq!(
            stream.encode().unwrap(),
            [0x11, 0x22, 0x33, 0x55, 0x66, 0x77, 0x48, 0x89, 0x07, 0xff,]
        );
        stream.set_custom_time(false, None).unwrap();
        assert_eq!(stream.records, ordinary);
    }

    #[test]
    fn custom_time_rejects_nonpersistable_values() {
        assert_eq!(
            CustomTimeSettings::new(0x1000, false),
            Err(CustomTimeError::ValueOutOfRange(0x1000))
        );
        assert_eq!(
            CustomTimeSettings::new(0, false),
            Err(CustomTimeError::DisabledEncoding)
        );
        assert_eq!(CustomTimeSettings::new(0, true).unwrap().value(), 0);
    }
}
