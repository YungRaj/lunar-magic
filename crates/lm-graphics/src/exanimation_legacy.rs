use super::{ExAnimationError, ExAnimationRecord};
use std::fmt;

/// Size of one record consumed by Lunar Magic's legacy ExAnimation migration routine.
pub const LEGACY_EXANIMATION_RECORD_LEN: usize = 0x23;
/// The original migration clamps each legacy payload to 32 visible records.
pub const LEGACY_EXANIMATION_MAX_RECORDS: usize = 0x20;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyExAnimationError {
    TooManyRecords {
        actual: usize,
        maximum: usize,
    },
    SizeOverflow,
    WrongPayloadLength {
        expected: usize,
        actual: usize,
    },
    Record {
        index: usize,
        error: ExAnimationError,
    },
}

impl fmt::Display for LegacyExAnimationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "legacy ExAnimation conversion failed: {self:?}")
    }
}

impl std::error::Error for LegacyExAnimationError {}

/// Converts the exact legacy `$23`-byte record representation used by
/// `MigrateLegacyGlobalExAnimations` (`0045F980`) into canonical current records.
///
/// `ConvertLegacyExAnimationRecords` (`0045E9C0`) decrements the packed type/trigger byte, maps its
/// high nibble to a current transfer kind, maps adjusted low nibbles 1–3 to triggers 1–3, and
/// collapses repeated frames to the smallest 1/2/4/8/16-frame power-of-two period. Those three
/// trigger forms carry two source words per frame and therefore have at most eight legacy frames.
///
/// # Errors
///
/// Returns [`LegacyExAnimationError`] for more than 32 records, size overflow, an inexact payload,
/// or a converted record that cannot be represented canonically.
pub fn convert_legacy_exanimation_records(
    bytes: &[u8],
    record_count: usize,
) -> Result<Vec<ExAnimationRecord>, LegacyExAnimationError> {
    if record_count > LEGACY_EXANIMATION_MAX_RECORDS {
        return Err(LegacyExAnimationError::TooManyRecords {
            actual: record_count,
            maximum: LEGACY_EXANIMATION_MAX_RECORDS,
        });
    }
    let expected = record_count
        .checked_mul(LEGACY_EXANIMATION_RECORD_LEN)
        .ok_or(LegacyExAnimationError::SizeOverflow)?;
    if bytes.len() != expected {
        return Err(LegacyExAnimationError::WrongPayloadLength {
            expected,
            actual: bytes.len(),
        });
    }

    bytes
        .chunks_exact(LEGACY_EXANIMATION_RECORD_LEN)
        .enumerate()
        .map(|(index, record)| {
            convert_record(record).map_err(|error| LegacyExAnimationError::Record { index, error })
        })
        .collect()
}

fn convert_record(bytes: &[u8]) -> Result<ExAnimationRecord, ExAnimationError> {
    let control = bytes[0];
    if control & 0x0f == 0 {
        return Ok(ExAnimationRecord::inactive());
    }
    let adjusted = control.wrapping_sub(1);
    let kind = match adjusted >> 4 {
        0x0 => 0x13,
        0x1 => 0x0f,
        0x3 => 0x10,
        0x4 => 0x11,
        0x5 => 0x02,
        0x6 => 0x03,
        0x8 => 0x04,
        0xa => 0x05,
        0xc => 0x06,
        0xe => 0x07,
        _ => 0x01,
    };
    let trigger = match adjusted & 0x0f {
        trigger @ 1..=3 => trigger,
        _ => 0,
    };
    let double_size = trigger != 0;
    let words_per_frame = if double_size { 2 } else { 1 };
    let frame_capacity = 16 / words_per_frame;
    let words = bytes[3..]
        .chunks_exact(2)
        .map(|word| u16::from_le_bytes([word[0], word[1]]))
        .collect::<Vec<_>>();
    let period = [1, 2, 4, 8, 16]
        .into_iter()
        .filter(|candidate| *candidate <= frame_capacity)
        .find(|candidate| {
            (0..frame_capacity).all(|frame| {
                let repeated = frame % candidate;
                (0..words_per_frame).all(|word| {
                    words[frame * words_per_frame + word]
                        == words[repeated * words_per_frame + word]
                })
            })
        })
        .expect("the complete legacy frame capacity is always a valid period");
    let frame_bytes = words[..period * words_per_frame]
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();
    let mut destination = u16::from_le_bytes([bytes[1], bytes[2]]) & 0x7fff;
    if kind == 0x13 {
        destination &= 0x00ff;
    }
    ExAnimationRecord::new(
        kind,
        u8::try_from(period - 1).expect("legacy period fits u8"),
        trigger,
        destination,
        false,
        &frame_bytes,
        double_size,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_record(control: u8, destination: u16, frames: [u16; 16]) -> [u8; 0x23] {
        let mut bytes = [0; 0x23];
        bytes[0] = control;
        bytes[1..3].copy_from_slice(&destination.to_le_bytes());
        for (index, word) in frames.into_iter().enumerate() {
            bytes[3 + index * 2..5 + index * 2].copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn converts_every_legacy_type_nibble_to_the_recovered_current_kind() {
        let expected = [
            0x13, 0x0f, 0x01, 0x10, 0x11, 0x02, 0x03, 0x01, 0x04, 0x01, 0x05, 0x01, 0x06, 0x01,
            0x07, 0x01,
        ];
        for (high, expected_kind) in expected.into_iter().enumerate() {
            let control = u8::try_from((high << 4) + 1).unwrap();
            let converted = convert_legacy_exanimation_records(
                &legacy_record(control, 0x4321, [0x1234; 16]),
                1,
            )
            .unwrap();
            assert_eq!(converted[0].kind(), expected_kind, "high nibble {high:X}");
        }
    }

    #[test]
    fn collapses_single_word_periods_and_preserves_the_minimal_payload() {
        for period in [1, 2, 4, 8, 16] {
            let frames = std::array::from_fn(|index| {
                u16::try_from(index % period).unwrap().wrapping_add(0x1200)
            });
            let converted =
                convert_legacy_exanimation_records(&legacy_record(0x21, 0x4321, frames), 1)
                    .unwrap();
            let record = &converted[0];
            assert_eq!(usize::from(record.frame_count_minus_one()) + 1, period);
            assert_eq!(record.frame_bytes(false).len(), period * 2);
        }
    }

    #[test]
    fn adjusted_triggers_one_to_three_use_two_words_per_frame() {
        for trigger in 1..=3 {
            let frames = std::array::from_fn(|word| {
                let frame = word / 2;
                u16::try_from((frame % 4) * 2 + word % 2).unwrap()
            });
            let converted =
                convert_legacy_exanimation_records(&legacy_record(trigger + 1, 0x2222, frames), 1)
                    .unwrap();
            let record = &converted[0];
            assert_eq!(record.trigger(), trigger);
            assert_eq!(record.frame_count_minus_one(), 3);
            assert_eq!(record.frame_bytes(true).len(), 16);
        }
    }

    #[test]
    fn inactive_records_and_strict_bounds_are_preserved() {
        assert_eq!(
            convert_legacy_exanimation_records(&[0; 0x23], 1).unwrap(),
            [ExAnimationRecord::inactive()]
        );
        assert!(matches!(
            convert_legacy_exanimation_records(&[], 33),
            Err(LegacyExAnimationError::TooManyRecords { .. })
        ));
        assert!(matches!(
            convert_legacy_exanimation_records(&[0; 0x22], 1),
            Err(LegacyExAnimationError::WrongPayloadLength { .. })
        ));
    }
}
