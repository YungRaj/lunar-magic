use crate::TransactionError;
use lm_rom::Mapper;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RomWrite {
    pub offset: usize,
    pub bytes: Vec<u8>,
}

/// A revision-prepared logical ROM mutation that can also append a validated new tail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RomMutation {
    pub mapper: Mapper,
    /// Logical length of the exact snapshot from which this mutation was prepared.
    pub expected_len: usize,
    /// Bytes appended at `expected_len` before applying `writes`.
    pub appended: Vec<u8>,
    pub writes: Vec<RomWrite>,
}

impl RomMutation {
    /// Constructs an explicitly unchanged mutation for one exact logical snapshot length.
    #[must_use]
    pub const fn unchanged(mapper: Mapper, expected_len: usize) -> Self {
        Self {
            mapper,
            expected_len,
            appended: Vec::new(),
            writes: Vec::new(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.appended.is_empty() && self.writes.iter().all(|write| write.bytes.is_empty())
    }

    /// Builds a compact mutation from exact logical before/after images.
    ///
    /// Changed runs in the original extent become disjoint writes; a larger suffix becomes the
    /// appended tail. Shrinking is intentionally excluded from editor-controller results because
    /// allocation workflows only grow or retain the active ROM.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError::CannotPrepareShrink`] when `after` is shorter than `before`.
    pub fn between(mapper: Mapper, before: &[u8], after: &[u8]) -> Result<Self, TransactionError> {
        if after.len() < before.len() {
            return Err(TransactionError::CannotPrepareShrink {
                before: before.len(),
                after: after.len(),
            });
        }
        let mut writes = Vec::new();
        let mut start = None;
        for (index, (old, new)) in before.iter().zip(after).enumerate() {
            if old != new {
                start.get_or_insert(index);
            } else if let Some(begin) = start.take() {
                writes.push(RomWrite {
                    offset: begin,
                    bytes: after[begin..index].to_vec(),
                });
            }
        }
        if let Some(begin) = start {
            writes.push(RomWrite {
                offset: begin,
                bytes: after[begin..before.len()].to_vec(),
            });
        }
        Ok(Self {
            mapper,
            expected_len: before.len(),
            appended: after[before.len()..].to_vec(),
            writes,
        })
    }
}

pub(super) fn validate_write_shapes(
    writes: &[RomWrite],
    image_len: usize,
) -> Result<(), TransactionError> {
    for (index, write) in writes.iter().enumerate() {
        let end = write
            .offset
            .checked_add(write.bytes.len())
            .ok_or(TransactionError::WriteRangeOverflow { index })?;
        if end > image_len {
            return Err(TransactionError::WriteOutsideMutation {
                index,
                offset: write.offset,
                len: write.bytes.len(),
                image_len,
            });
        }
        if write.bytes.is_empty() {
            continue;
        }
        for (other_index, other) in writes[..index].iter().enumerate() {
            let other_end = other
                .offset
                .checked_add(other.bytes.len())
                .ok_or(TransactionError::WriteRangeOverflow { index: other_index })?;
            if !other.bytes.is_empty() && write.offset < other_end && other.offset < end {
                return Err(TransactionError::OverlappingWrites {
                    first: other_index,
                    second: index,
                });
            }
        }
    }
    Ok(())
}
