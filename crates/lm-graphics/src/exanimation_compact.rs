use super::{ExAnimationError, ExAnimationRecord};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactExAnimation {
    pub setting: u8,
    pub header_value: u32,
    pub trigger_mask: u16,
    pub trigger_values: [u8; 16],
    pub records: Vec<ExAnimationRecord>,
}

impl CompactExAnimation {
    /// Decodes the compact variable-length ROM representation recovered from Lunar Magic.
    ///
    /// `double_size_modes` is the recovered 256-entry type table controlling whether a record's
    /// frame payload is doubled.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationError`] for truncation, invalid offsets, excessive record counts, or a
    /// missing type-table entry.
    pub fn decode(
        bytes: &[u8],
        maximum_records: usize,
        double_size_modes: &[bool],
    ) -> Result<(Self, usize), ExAnimationError> {
        let header = bytes.get(..8).ok_or(ExAnimationError::Truncated {
            offset: 0,
            needed: 8,
        })?;
        let record_count = usize::from(header[0]);
        if record_count > maximum_records {
            return Err(ExAnimationError::TooManyRecords {
                actual: record_count,
                maximum: maximum_records,
            });
        }
        let header_value = u32::from_le_bytes([header[2], header[3], header[4], header[5]]);
        let trigger_mask = u16::from_le_bytes([header[6], header[7]]);
        let mut cursor = 8;
        let mut trigger_values = [0; 16];
        for (trigger, value) in trigger_values.iter_mut().enumerate() {
            if trigger_mask & (1 << trigger) != 0 {
                *value = *bytes.get(cursor).ok_or(ExAnimationError::Truncated {
                    offset: cursor,
                    needed: 1,
                })?;
                cursor += 1;
            }
        }
        let offset_base = cursor;
        let table_len = record_count
            .checked_mul(2)
            .ok_or(ExAnimationError::InvalidOffset)?;
        let table_end = cursor
            .checked_add(table_len)
            .ok_or(ExAnimationError::InvalidOffset)?;
        let table = bytes
            .get(cursor..table_end)
            .ok_or(ExAnimationError::Truncated {
                offset: cursor,
                needed: table_len,
            })?;
        let mut consumed = table_end;
        let mut records = Vec::with_capacity(record_count);
        for pair in table.chunks_exact(2) {
            let relative = usize::from(u16::from_le_bytes([pair[0], pair[1]]));
            if relative == 0 {
                records.push(ExAnimationRecord::inactive());
                continue;
            }
            let start = offset_base
                .checked_add(relative)
                .ok_or(ExAnimationError::InvalidOffset)?;
            if start < table_end {
                return Err(ExAnimationError::InvalidOffset);
            }
            let metadata = bytes
                .get(start..start + 5)
                .ok_or(ExAnimationError::Truncated {
                    offset: start,
                    needed: 5,
                })?;
            let double_size = *double_size_modes
                .get(usize::from(metadata[1]))
                .ok_or(ExAnimationError::MissingSizeMode(metadata[1]))?;
            let frame_len = checked_compact_frame_len(metadata[0], metadata[2], double_size)?;
            let frame_start = start + 5;
            let frame_end = frame_start
                .checked_add(frame_len)
                .ok_or(ExAnimationError::InvalidOffset)?;
            let frames = bytes
                .get(frame_start..frame_end)
                .ok_or(ExAnimationError::Truncated {
                    offset: frame_start,
                    needed: frame_len,
                })?;
            let mut record = ExAnimationRecord::inactive();
            record.bytes[0] = metadata[0];
            record.bytes[2] = metadata[1];
            record.bytes[1] = metadata[2];
            record.bytes[4] = metadata[3];
            record.bytes[5] = metadata[4] & 0x7f;
            record.bytes[6] = metadata[4] >> 7;
            record.bytes[8..8 + frame_len].copy_from_slice(frames);
            records.push(record);
            consumed = consumed.max(frame_end);
        }
        Ok((
            Self {
                setting: header[1],
                header_value,
                trigger_mask,
                trigger_values,
                records,
            },
            consumed,
        ))
    }

    /// Encodes the compact ROM representation, trimming inactive trailing slots.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationError`] for more than 255 slots, oversized output offsets, or a missing
    /// size-mode table entry.
    pub fn encode(&self, double_size_modes: &[bool]) -> Result<Vec<u8>, ExAnimationError> {
        for (trigger, value) in self.trigger_values.iter().copied().enumerate() {
            if self.trigger_mask & (1 << trigger) == 0 && value != 0 {
                return Err(ExAnimationError::DisabledTriggerValue { trigger, value });
            }
        }
        for (record, entry) in self.records.iter().enumerate() {
            let double_size = *double_size_modes
                .get(usize::from(entry.size_mode()))
                .ok_or(ExAnimationError::MissingSizeMode(entry.size_mode()))?;
            entry.validate_compact(record, double_size)?;
        }
        let record_count = self
            .records
            .iter()
            .rposition(|record| record.kind() != 0)
            .map_or(0, |index| index + 1);
        let record_count_u8 =
            u8::try_from(record_count).map_err(|_| ExAnimationError::TooManyRecords {
                actual: record_count,
                maximum: usize::from(u8::MAX),
            })?;
        let mut output = vec![record_count_u8, self.setting];
        output.extend_from_slice(&self.header_value.to_le_bytes());
        output.extend_from_slice(&self.trigger_mask.to_le_bytes());
        for (trigger, value) in self.trigger_values.iter().enumerate() {
            if self.trigger_mask & (1 << trigger) != 0 {
                output.push(*value);
            }
        }
        let offset_base = output.len();
        output.resize(output.len() + record_count * 2, 0);
        for (index, record) in self.records[..record_count].iter().enumerate() {
            if record.kind() == 0 {
                continue;
            }
            let relative = output
                .len()
                .checked_sub(offset_base)
                .ok_or(ExAnimationError::InvalidOffset)?;
            let relative = u16::try_from(relative).map_err(|_| ExAnimationError::InvalidOffset)?;
            output[offset_base + index * 2..offset_base + index * 2 + 2]
                .copy_from_slice(&relative.to_le_bytes());
            output.push(record.kind());
            output.push(record.size_mode());
            output.push(record.frame_count_minus_one());
            let destination = record.destination().to_le_bytes();
            output.push(destination[0]);
            output.push(destination[1] & 0x7f | u8::from(record.destination_flag()) << 7);
            let double_size = *double_size_modes
                .get(usize::from(record.size_mode()))
                .ok_or(ExAnimationError::MissingSizeMode(record.size_mode()))?;
            output.extend_from_slice(record.frame_bytes(double_size));
        }
        Ok(output)
    }
}

pub(super) const fn compact_frame_len(
    kind: u8,
    frame_count_minus_one: u8,
    double_size: bool,
) -> usize {
    let length = declared_compact_frame_len(kind, frame_count_minus_one, double_size);
    if length > 0x200 { 0x200 } else { length }
}

pub(super) const fn declared_compact_frame_len(
    kind: u8,
    frame_count_minus_one: u8,
    double_size: bool,
) -> usize {
    if kind >= 0x18 && kind <= 0x1b || kind == 0 {
        return 0;
    }
    let base = (frame_count_minus_one as usize + 1) * 2;
    if double_size { base * 2 } else { base }
}

pub(super) fn checked_compact_frame_len(
    kind: u8,
    frame_count_minus_one: u8,
    double_size: bool,
) -> Result<usize, ExAnimationError> {
    let actual = declared_compact_frame_len(kind, frame_count_minus_one, double_size);
    if actual > 0x200 {
        Err(ExAnimationError::FramePayloadTooLarge {
            actual,
            maximum: 0x200,
        })
    } else {
        Ok(actual)
    }
}
