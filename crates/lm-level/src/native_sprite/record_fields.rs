use super::SpriteLengthTable;
use crate::SpriteRecord;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeSpriteRecordFields {
    pub y_low: u8,
    pub extra_bits: u8,
    pub screen: u8,
    pub x: u8,
    pub sprite_number: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeSpriteFieldError {
    RecordTooShort(usize),
    YOutOfRange(u8),
    ExtraBitsOutOfRange(u8),
    ScreenOutOfRange(u8),
    XOutOfRange(u8),
    UnknownRecordLength,
    RecordLengthMismatch { expected: usize, actual: usize },
}

impl fmt::Display for NativeSpriteFieldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native sprite field edit: {self:?}")
    }
}

impl std::error::Error for NativeSpriteFieldError {}

impl SpriteRecord {
    /// Decodes the proven `yyyyEESY / XXXXssss / NNNNNNNN` base record.
    ///
    /// Extension bytes are deliberately outside this view and remain lossless.
    ///
    /// # Errors
    ///
    /// Rejects records shorter than the three-byte base shape.
    pub fn native_fields(&self) -> Result<NativeSpriteRecordFields, NativeSpriteFieldError> {
        let [first, second, sprite_number, ..] = self.encoded.as_slice() else {
            return Err(NativeSpriteFieldError::RecordTooShort(self.encoded.len()));
        };
        Ok(NativeSpriteRecordFields {
            y_low: (first >> 4) | ((first & 1) << 4),
            extra_bits: (first >> 2) & 3,
            screen: (second & 0x0f) | ((first & 2) << 3),
            x: second >> 4,
            sprite_number: *sprite_number,
        })
    }

    /// Replaces all proven base fields while retaining every extension byte.
    ///
    /// # Errors
    ///
    /// Rejects out-of-range packed fields or any revision-table interpretation that would change
    /// this record's encoded width.
    pub fn set_native_fields(
        &mut self,
        fields: NativeSpriteRecordFields,
        lengths: &SpriteLengthTable,
    ) -> Result<(), NativeSpriteFieldError> {
        if self.encoded.len() < 3 {
            return Err(NativeSpriteFieldError::RecordTooShort(self.encoded.len()));
        }
        if fields.y_low > 0x1f {
            return Err(NativeSpriteFieldError::YOutOfRange(fields.y_low));
        }
        if fields.extra_bits > 3 {
            return Err(NativeSpriteFieldError::ExtraBitsOutOfRange(
                fields.extra_bits,
            ));
        }
        if fields.screen > 0x1f {
            return Err(NativeSpriteFieldError::ScreenOutOfRange(fields.screen));
        }
        if fields.x > 0x0f {
            return Err(NativeSpriteFieldError::XOutOfRange(fields.x));
        }
        let mut candidate = self.encoded.clone();
        candidate[0] = (fields.y_low & 0x0f) << 4
            | (fields.extra_bits & 3) << 2
            | (fields.screen >> 4) << 1
            | fields.y_low >> 4;
        candidate[1] = fields.x << 4 | fields.screen & 0x0f;
        candidate[2] = fields.sprite_number;
        validate_length(&candidate, lengths)?;
        self.encoded = candidate;
        Ok(())
    }
}

fn validate_length(
    candidate: &[u8],
    lengths: &SpriteLengthTable,
) -> Result<(), NativeSpriteFieldError> {
    let expected = lengths
        .record_len(candidate)
        .ok_or(NativeSpriteFieldError::UnknownRecordLength)?;
    if expected != candidate.len() {
        return Err(NativeSpriteFieldError::RecordLengthMismatch {
            expected,
            actual: candidate.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fields_round_trip_and_extension_bytes_are_preserved() {
        let mut lengths = SpriteLengthTable::standard();
        lengths.set(2, 0x42, 5).unwrap();
        let mut record = SpriteRecord {
            encoded: vec![0x9a, 0xc7, 0x42, 0xaa, 0xbb],
        };
        assert_eq!(
            record.native_fields().unwrap(),
            NativeSpriteRecordFields {
                y_low: 9,
                extra_bits: 2,
                screen: 23,
                x: 12,
                sprite_number: 0x42,
            }
        );
        record
            .set_native_fields(
                NativeSpriteRecordFields {
                    y_low: 0x1d,
                    extra_bits: 2,
                    screen: 0x1e,
                    x: 3,
                    sprite_number: 0x42,
                },
                &lengths,
            )
            .unwrap();
        assert_eq!(record.native_fields().unwrap().y_low, 0x1d);
        assert_eq!(record.native_fields().unwrap().screen, 0x1e);
        assert_eq!(&record.encoded[3..], [0xaa, 0xbb]);
    }

    #[test]
    fn range_and_shape_changing_edits_are_atomic() {
        let mut lengths = SpriteLengthTable::standard();
        lengths.set(1, 0x55, 4).unwrap();
        let mut record = SpriteRecord {
            encoded: vec![0, 0, 1],
        };
        let original = record.clone();
        for fields in [
            NativeSpriteRecordFields {
                y_low: 0x20,
                extra_bits: 0,
                screen: 0,
                x: 0,
                sprite_number: 1,
            },
            NativeSpriteRecordFields {
                y_low: 0,
                extra_bits: 1,
                screen: 0,
                x: 0,
                sprite_number: 0x55,
            },
        ] {
            assert!(record.set_native_fields(fields, &lengths).is_err());
            assert_eq!(record, original);
        }
    }
}
