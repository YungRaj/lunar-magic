use crate::{SpriteRecord, SpriteStream};
use std::fmt;

/// One ordered mutation in an atomic decoded-sprite batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpriteEdit {
    Insert {
        index: usize,
        record: SpriteRecord,
    },
    Replace {
        index: usize,
        record: SpriteRecord,
    },
    Remove {
        index: usize,
    },
    /// Moves one sprite before an index in the ordering that exists when this command runs.
    MoveBefore {
        from: usize,
        before: usize,
    },
}

/// Explicit limits for a decoded sprite edit boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpriteEditLimits {
    pub max_records: usize,
    pub max_record_len: usize,
    /// Maximum complete encoded stream length, including header and terminator.
    pub max_encoded_len: usize,
}

impl SpriteEditLimits {
    /// Native SMW/Lunar Magic streams occupy no more than one 32-KiB bank.
    pub const NATIVE_BANKED: Self = Self {
        max_records: 0x1_0000,
        max_record_len: 0x1_0000,
        max_encoded_len: 0x8000,
    };

    /// Bounded semantic interchange policy used by `LMLEVEL2`.
    pub const PORTABLE: Self = Self {
        max_records: 0x1_0000,
        max_record_len: 0x1_0000,
        max_encoded_len: 0x100_0000,
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpriteEditError {
    InvalidLimits,
    IndexOutOfBounds {
        command: usize,
        index: usize,
        len: usize,
    },
    InvalidRecordLength {
        record: usize,
        len: usize,
    },
    TooManyRecords(usize),
    EncodedLengthExceeded(usize),
    Overflow,
}

impl fmt::Display for SpriteEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid atomic sprite edit: {self:?}")
    }
}

impl std::error::Error for SpriteEditError {}

impl SpriteStream {
    /// Applies ordered mutations to a staged clone, then validates every record and the complete
    /// encoded stream against the caller-selected storage policy.
    ///
    /// Sprite record lengths are revision-dependent, so this boundary deliberately preserves raw
    /// records and enforces only the universal three-byte minimum plus explicit caller limits.
    /// Native serialization should subsequently use its recovered revision length table.
    ///
    /// # Errors
    ///
    /// Returns [`SpriteEditError`] for invalid limits, indexes, record sizes, counts, arithmetic,
    /// or total serialized length. Failure leaves the original stream unchanged.
    pub fn apply_edits(
        &mut self,
        edits: &[SpriteEdit],
        limits: SpriteEditLimits,
    ) -> Result<(), SpriteEditError> {
        validate(self, limits)?;
        if edits.is_empty() {
            return Ok(());
        }
        let mut staged = self.clone();
        for (command, edit) in edits.iter().enumerate() {
            match edit {
                SpriteEdit::Insert { index, record } => {
                    if *index > staged.records.len() {
                        return Err(index_error(command, *index, staged.records.len()));
                    }
                    staged.records.insert(*index, record.clone());
                }
                SpriteEdit::Replace { index, record } => {
                    let len = staged.records.len();
                    let Some(target) = staged.records.get_mut(*index) else {
                        return Err(index_error(command, *index, len));
                    };
                    *target = record.clone();
                }
                SpriteEdit::Remove { index } => {
                    if *index >= staged.records.len() {
                        return Err(index_error(command, *index, staged.records.len()));
                    }
                    staged.records.remove(*index);
                }
                SpriteEdit::MoveBefore { from, before } => {
                    move_before(&mut staged.records, command, *from, *before)?;
                }
            }
        }
        validate(&staged, limits)?;
        *self = staged;
        Ok(())
    }
}

fn validate(stream: &SpriteStream, limits: SpriteEditLimits) -> Result<(), SpriteEditError> {
    if limits.max_records == 0 || limits.max_record_len < 3 || limits.max_encoded_len < 2 {
        return Err(SpriteEditError::InvalidLimits);
    }
    if stream.records.len() > limits.max_records {
        return Err(SpriteEditError::TooManyRecords(stream.records.len()));
    }
    let mut encoded_len = 2usize;
    for (record, value) in stream.records.iter().enumerate() {
        if !(3..=limits.max_record_len).contains(&value.encoded.len()) {
            return Err(SpriteEditError::InvalidRecordLength {
                record,
                len: value.encoded.len(),
            });
        }
        encoded_len = encoded_len
            .checked_add(value.encoded.len())
            .ok_or(SpriteEditError::Overflow)?;
    }
    if encoded_len > limits.max_encoded_len {
        return Err(SpriteEditError::EncodedLengthExceeded(encoded_len));
    }
    Ok(())
}

fn index_error(command: usize, index: usize, len: usize) -> SpriteEditError {
    SpriteEditError::IndexOutOfBounds {
        command,
        index,
        len,
    }
}

fn move_before<T>(
    values: &mut Vec<T>,
    command: usize,
    from: usize,
    before: usize,
) -> Result<(), SpriteEditError> {
    let len = values.len();
    if from >= len {
        return Err(index_error(command, from, len));
    }
    if before > len {
        return Err(index_error(command, before, len));
    }
    if from == before || from.checked_add(1) == Some(before) {
        return Ok(());
    }
    let value = values.remove(from);
    values.insert(if before > from { before - 1 } else { before }, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u8) -> SpriteRecord {
        SpriteRecord {
            encoded: vec![0, 0, id],
        }
    }

    #[test]
    fn ordered_batch_round_trips_through_stream_parser() {
        let mut stream = SpriteStream {
            header: 0x12,
            records: vec![record(1), record(2), record(3)],
        };
        stream
            .apply_edits(
                &[
                    SpriteEdit::MoveBefore { from: 2, before: 0 },
                    SpriteEdit::Replace {
                        index: 1,
                        record: record(4),
                    },
                    SpriteEdit::Remove { index: 2 },
                    SpriteEdit::Insert {
                        index: 2,
                        record: record(5),
                    },
                ],
                SpriteEditLimits::NATIVE_BANKED,
            )
            .unwrap();
        let encoded = stream.encode().unwrap();
        assert_eq!(encoded, [0x12, 0, 0, 3, 0, 0, 4, 0, 0, 5, 0xff]);
        assert_eq!(
            SpriteStream::parse_with(&encoded, |_| Some(3)).unwrap(),
            stream
        );
    }

    #[test]
    fn late_index_failure_rolls_back_prior_commands() {
        let mut stream = SpriteStream {
            header: 0,
            records: vec![record(1), record(2)],
        };
        let original = stream.clone();
        let error = stream
            .apply_edits(
                &[
                    SpriteEdit::Remove { index: 0 },
                    SpriteEdit::Insert {
                        index: 4,
                        record: record(9),
                    },
                ],
                SpriteEditLimits::NATIVE_BANKED,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            SpriteEditError::IndexOutOfBounds { command: 1, .. }
        ));
        assert_eq!(stream, original);
    }

    #[test]
    fn invalid_record_and_final_size_fail_atomically() {
        let mut stream = SpriteStream {
            header: 0,
            records: vec![record(1)],
        };
        let original = stream.clone();
        assert!(matches!(
            stream.apply_edits(
                &[SpriteEdit::Insert {
                    index: 1,
                    record: SpriteRecord {
                        encoded: vec![1, 2]
                    },
                }],
                SpriteEditLimits::NATIVE_BANKED,
            ),
            Err(SpriteEditError::InvalidRecordLength { record: 1, len: 2 })
        ));
        assert_eq!(stream, original);

        let tiny = SpriteEditLimits {
            max_records: 2,
            max_record_len: 3,
            max_encoded_len: 7,
        };
        assert_eq!(
            stream.apply_edits(
                &[SpriteEdit::Insert {
                    index: 1,
                    record: record(2),
                }],
                tiny,
            ),
            Err(SpriteEditError::EncodedLengthExceeded(8))
        );
        assert_eq!(stream, original);
    }

    #[test]
    fn malformed_existing_stream_and_invalid_policy_are_rejected_without_edits() {
        let mut malformed = SpriteStream {
            header: 0,
            records: vec![SpriteRecord { encoded: vec![] }],
        };
        let original = malformed.clone();
        assert!(matches!(
            malformed.apply_edits(&[], SpriteEditLimits::PORTABLE),
            Err(SpriteEditError::InvalidRecordLength { .. })
        ));
        assert_eq!(malformed, original);

        let mut valid = SpriteStream::default();
        assert_eq!(
            valid.apply_edits(
                &[],
                SpriteEditLimits {
                    max_records: 0,
                    max_record_len: 3,
                    max_encoded_len: 2,
                },
            ),
            Err(SpriteEditError::InvalidLimits)
        );
    }
}
