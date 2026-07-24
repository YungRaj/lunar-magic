use crate::{CompactExAnimation, ExAnimationRecord};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExAnimationEditError {
    RecordIndexOutOfRange { index: usize, len: usize },
    TooManyRecords { actual: usize, maximum: usize },
    TriggerOutOfRange(usize),
}

impl fmt::Display for ExAnimationEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid ExAnimation edit: {self:?}")
    }
}

impl std::error::Error for ExAnimationEditError {}

impl CompactExAnimation {
    /// Inserts a record before `index`; `len` appends.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationEditError`] for bounds or when the configured record limit is reached.
    pub fn insert_record(
        &mut self,
        index: usize,
        record: ExAnimationRecord,
        maximum_records: usize,
    ) -> Result<(), ExAnimationEditError> {
        if index > self.records.len() {
            return Err(ExAnimationEditError::RecordIndexOutOfRange {
                index,
                len: self.records.len(),
            });
        }
        let maximum = maximum_records.min(usize::from(u8::MAX));
        let actual = self.records.len().saturating_add(1);
        if actual > maximum {
            return Err(ExAnimationEditError::TooManyRecords { actual, maximum });
        }
        self.records.insert(index, record);
        Ok(())
    }

    /// Removes and returns one animation record.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationEditError::RecordIndexOutOfRange`] for an invalid slot.
    pub fn remove_record(
        &mut self,
        index: usize,
    ) -> Result<ExAnimationRecord, ExAnimationEditError> {
        if index >= self.records.len() {
            return Err(ExAnimationEditError::RecordIndexOutOfRange {
                index,
                len: self.records.len(),
            });
        }
        Ok(self.records.remove(index))
    }

    /// Moves a record before a slot in the pre-move ordering; `len` means the end.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationEditError::RecordIndexOutOfRange`] for invalid indexes.
    pub fn move_record_before(
        &mut self,
        from: usize,
        before: usize,
    ) -> Result<(), ExAnimationEditError> {
        let len = self.records.len();
        if from >= len {
            return Err(ExAnimationEditError::RecordIndexOutOfRange { index: from, len });
        }
        if before > len {
            return Err(ExAnimationEditError::RecordIndexOutOfRange { index: before, len });
        }
        if from == before || from.checked_add(1) == Some(before) {
            return Ok(());
        }
        let record = self.records.remove(from);
        self.records
            .insert(if before > from { before - 1 } else { before }, record);
        Ok(())
    }

    /// Enables/updates a trigger value, or removes it when `value` is `None`.
    ///
    /// # Errors
    ///
    /// Returns [`ExAnimationEditError::TriggerOutOfRange`] above trigger 15.
    pub fn set_trigger(
        &mut self,
        trigger: usize,
        value: Option<u8>,
    ) -> Result<(), ExAnimationEditError> {
        if trigger >= self.trigger_values.len() {
            return Err(ExAnimationEditError::TriggerOutOfRange(trigger));
        }
        let mask = 1_u16 << trigger;
        if let Some(value) = value {
            self.trigger_mask |= mask;
            self.trigger_values[trigger] = value;
        } else {
            self.trigger_mask &= !mask;
            self.trigger_values[trigger] = 0;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(kind: u8) -> ExAnimationRecord {
        ExAnimationRecord::new(kind, 0, 0, 0, false, &[0, 0], false).unwrap()
    }

    fn animation() -> CompactExAnimation {
        CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![record(1), record(2), record(3)],
        }
    }

    #[test]
    fn records_insert_remove_and_reorder_with_limits() {
        let mut animation = animation();
        animation.move_record_before(0, 3).unwrap();
        assert_eq!(
            animation
                .records
                .iter()
                .map(ExAnimationRecord::kind)
                .collect::<Vec<_>>(),
            [2, 3, 1]
        );
        animation.insert_record(1, record(4), 4).unwrap();
        assert_eq!(animation.remove_record(2).unwrap().kind(), 3);
        let original = animation.clone();
        assert!(matches!(
            animation.insert_record(0, record(5), 3),
            Err(ExAnimationEditError::TooManyRecords { .. })
        ));
        assert_eq!(animation, original);
    }

    #[test]
    fn trigger_mask_and_values_change_together() {
        let mut animation = animation();
        animation.set_trigger(15, Some(9)).unwrap();
        assert_eq!(animation.trigger_mask, 0x8000);
        assert_eq!(animation.trigger_values[15], 9);
        animation.set_trigger(15, None).unwrap();
        assert_eq!(animation.trigger_mask, 0);
        assert_eq!(animation.trigger_values[15], 0);
        assert!(matches!(
            animation.set_trigger(16, Some(1)),
            Err(ExAnimationEditError::TriggerOutOfRange(16))
        ));
    }
}
