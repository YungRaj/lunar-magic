use crate::GraphicsCompression;
use lm_rom::{COPIER_HEADER_LEN, RomError, RomImage};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EditKind {
    #[default]
    Ordinary,
    GraphicsCompressionMigration {
        source: GraphicsCompression,
        target: GraphicsCompression,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Edit {
    pub offset: usize,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub description: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EditBatch {
    pub description: String,
    pub edits: Vec<Edit>,
    pub kind: EditKind,
    pub copier_header: Option<CopierHeaderEdit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CopierHeaderEdit {
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}

impl CopierHeaderEdit {
    pub(crate) fn apply(&self, rom: &mut RomImage) -> Result<(), RomError> {
        self.validate()?;
        rom.replace_copier_header_exact(self.before.as_deref(), self.after.as_deref())
    }

    fn revert(&self, rom: &mut RomImage) -> Result<(), RomError> {
        self.validate()?;
        rom.replace_copier_header_exact(self.after.as_deref(), self.before.as_deref())
    }

    fn validate(&self) -> Result<(), RomError> {
        if self
            .before
            .as_ref()
            .is_some_and(|bytes| bytes.len() != COPIER_HEADER_LEN)
            || self
                .after
                .as_ref()
                .is_some_and(|bytes| bytes.len() != COPIER_HEADER_LEN)
        {
            return Err(RomError::BytesMismatch {
                offset: 0,
                len: COPIER_HEADER_LEN,
            });
        }
        Ok(())
    }
}

impl EditBatch {
    /// Applies every write in order, rolling back already-applied writes on failure.
    ///
    /// # Errors
    ///
    /// Returns the first ROM range error encountered.
    pub fn apply(&self, rom: &mut RomImage) -> Result<(), RomError> {
        if let Some(header) = &self.copier_header {
            header.apply(rom)?;
        }
        for (applied, edit) in self.edits.iter().enumerate() {
            if let Err(error) = edit.apply(rom) {
                for previous in self.edits[..applied].iter().rev() {
                    let _ = previous.revert(rom);
                }
                if let Some(header) = &self.copier_header {
                    let _ = header.revert(rom);
                }
                return Err(error);
            }
        }
        Ok(())
    }

    /// Reverts every write in reverse order, restoring reverted writes on failure.
    ///
    /// # Errors
    ///
    /// Returns the first ROM range error encountered.
    pub fn revert(&self, rom: &mut RomImage) -> Result<(), RomError> {
        let mut reverted: Vec<&Edit> = Vec::new();
        for edit in self.edits.iter().rev() {
            if let Err(error) = edit.revert(rom) {
                for previous in reverted.iter().rev() {
                    let _ = previous.apply(rom);
                }
                return Err(error);
            }
            reverted.push(edit);
        }
        if let Some(header) = &self.copier_header
            && let Err(error) = header.revert(rom)
        {
            for previous in reverted.iter().rev() {
                let _ = previous.apply(rom);
            }
            return Err(error);
        }
        Ok(())
    }
}

impl Edit {
    /// Applies the edit's new bytes.
    ///
    /// # Errors
    ///
    /// Returns [`RomError`] if the recorded range no longer fits the image.
    pub fn apply(&self, rom: &mut RomImage) -> Result<(), RomError> {
        if self.before.len() == self.after.len() {
            rom.replace_exact(self.offset, &self.before, &self.after)
        } else {
            rom.replace_logical_tail(self.offset, &self.before, &self.after)
        }
    }

    /// Restores the edit's previous bytes.
    ///
    /// # Errors
    ///
    /// Returns [`RomError`] if the recorded range no longer fits the image.
    pub fn revert(&self, rom: &mut RomImage) -> Result<(), RomError> {
        if self.before.len() == self.after.len() {
            rom.replace_exact(self.offset, &self.after, &self.before)
        } else {
            rom.replace_logical_tail(self.offset, &self.after, &self.before)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct History {
    undo: Vec<EditBatch>,
    redo: Vec<EditBatch>,
    limit: usize,
}

impl History {
    #[must_use]
    pub const fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
        }
    }

    pub fn push(&mut self, edit: Edit) {
        let description = edit.description.clone();
        self.push_batch(EditBatch {
            description,
            edits: vec![edit],
            kind: EditKind::Ordinary,
            copier_header: None,
        });
    }

    pub fn push_batch(&mut self, batch: EditBatch) {
        if batch.edits.is_empty() && batch.copier_header.is_none() {
            return;
        }
        self.redo.clear();
        self.undo.push(batch);
        self.trim_undo();
    }

    /// Changes the maximum number of retained undo operations.
    ///
    /// Oldest undo entries are discarded immediately. Redo entries remain
    /// valid because they describe states newer than the current ROM state.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.trim_undo();
    }

    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }

    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Describes the next operation that would be undone without changing history or ROM bytes.
    #[must_use]
    pub fn undo_kind(&self) -> Option<EditKind> {
        self.undo.last().map(|batch| batch.kind)
    }

    /// Describes the next operation that would be redone without changing history or ROM bytes.
    #[must_use]
    pub fn redo_kind(&self) -> Option<EditKind> {
        self.redo.last().map(|batch| batch.kind)
    }

    /// Discards undo and redo metadata without changing the ROM.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Undoes the newest edit.
    ///
    /// # Errors
    ///
    /// Returns [`RomError`] if its recorded range no longer fits.
    pub fn undo(&mut self, rom: &mut RomImage) -> Result<bool, RomError> {
        let Some(batch) = self.undo.last() else {
            return Ok(false);
        };
        batch.revert(rom)?;
        if let Some(batch) = self.undo.pop() {
            self.redo.push(batch);
        }
        Ok(true)
    }

    /// Redoes the newest reverted edit.
    ///
    /// # Errors
    ///
    /// Returns [`RomError`] if its recorded range no longer fits.
    pub fn redo(&mut self, rom: &mut RomImage) -> Result<bool, RomError> {
        let Some(batch) = self.redo.last() else {
            return Ok(false);
        };
        batch.apply(rom)?;
        if let Some(batch) = self.redo.pop() {
            self.undo.push(batch);
        }
        Ok(true)
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn trim_undo(&mut self) {
        let excess = self.undo.len().saturating_sub(self.limit);
        if excess != 0 {
            self.undo.drain(..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_is_one_undo_step() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let batch = EditBatch {
            description: "two writes".into(),
            edits: vec![
                Edit {
                    offset: 1,
                    before: vec![0],
                    after: vec![4],
                    description: "first".into(),
                },
                Edit {
                    offset: 9,
                    before: vec![0],
                    after: vec![7],
                    description: "second".into(),
                },
            ],
            kind: EditKind::Ordinary,
            copier_header: None,
        };
        batch.apply(&mut rom).unwrap();
        let mut history = History::with_limit(10);
        history.push_batch(batch);
        assert!(history.undo(&mut rom).unwrap());
        assert_eq!(rom.read(1, 1).unwrap(), &[0]);
        assert_eq!(rom.read(9, 1).unwrap(), &[0]);
        assert!(!history.can_undo());
        assert!(history.can_redo());
    }

    #[test]
    fn failed_undo_keeps_the_operation_available() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let mut history = History::with_limit(10);
        history.push(Edit {
            offset: 0x8000,
            before: vec![0],
            after: vec![1],
            description: "stale edit".into(),
        });

        assert!(history.undo(&mut rom).is_err());
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.redo_len(), 0);
        assert!(rom.logical_bytes().iter().all(|byte| *byte == 0));
    }

    #[test]
    fn failed_redo_keeps_the_operation_available() {
        let mut rom = RomImage::from_bytes(vec![0; 0x1_0000]).unwrap();
        rom.write(0xf000, &[1]).unwrap();
        let mut history = History::with_limit(10);
        history.push(Edit {
            offset: 0xf000,
            before: vec![0],
            after: vec![1],
            description: "high edit".into(),
        });
        assert!(history.undo(&mut rom).unwrap());
        let old_tail = rom.read(0x8000, 0x8000).unwrap().to_vec();
        rom.replace_logical_tail(0x8000, &old_tail, &[]).unwrap();

        assert!(history.redo(&mut rom).is_err());
        assert_eq!(history.undo_len(), 0);
        assert_eq!(history.redo_len(), 1);
        assert_eq!(rom.logical_len(), 0x8000);
    }

    #[test]
    fn stale_fixed_bytes_are_never_overwritten_by_undo_or_redo() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let edit = Edit {
            offset: 7,
            before: vec![0],
            after: vec![1],
            description: "guarded".into(),
        };
        edit.apply(&mut rom).unwrap();
        let mut history = History::with_limit(10);
        history.push(edit);

        rom.write(7, &[2]).unwrap();
        assert_eq!(
            history.undo(&mut rom),
            Err(RomError::BytesMismatch { offset: 7, len: 1 })
        );
        assert_eq!(rom.read(7, 1).unwrap(), [2]);
        assert_eq!((history.undo_len(), history.redo_len()), (1, 0));

        rom.write(7, &[1]).unwrap();
        history.undo(&mut rom).unwrap();
        rom.write(7, &[3]).unwrap();
        assert_eq!(
            history.redo(&mut rom),
            Err(RomError::BytesMismatch { offset: 7, len: 1 })
        );
        assert_eq!(rom.read(7, 1).unwrap(), [3]);
        assert_eq!((history.undo_len(), history.redo_len()), (0, 1));
    }

    #[test]
    fn failed_batch_undo_restores_already_reverted_edits() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let batch = EditBatch {
            description: "guarded batch".into(),
            edits: vec![
                Edit {
                    offset: 1,
                    before: vec![0],
                    after: vec![1],
                    description: "first".into(),
                },
                Edit {
                    offset: 2,
                    before: vec![0],
                    after: vec![2],
                    description: "second".into(),
                },
            ],
            kind: EditKind::Ordinary,
            copier_header: None,
        };
        batch.apply(&mut rom).unwrap();
        let mut history = History::with_limit(10);
        history.push_batch(batch);
        rom.write(1, &[9]).unwrap();

        assert!(matches!(
            history.undo(&mut rom),
            Err(RomError::BytesMismatch { offset: 1, len: 1 })
        ));
        assert_eq!(rom.read(1, 2).unwrap(), [9, 2]);
        assert_eq!((history.undo_len(), history.redo_len()), (1, 0));
    }

    #[test]
    fn lowering_limit_discards_oldest_entries_and_zero_disables_undo() {
        let mut history = History::with_limit(3);
        for offset in 0..3 {
            history.push(Edit {
                offset,
                before: vec![0],
                after: vec![1],
                description: offset.to_string(),
            });
        }
        history.set_limit(2);
        assert_eq!(history.limit(), 2);
        assert_eq!(history.undo_len(), 2);
        history.set_limit(0);
        assert!(!history.can_undo());

        history.push(Edit {
            offset: 0,
            before: vec![0],
            after: vec![1],
            description: "disabled".into(),
        });
        assert_eq!(history.undo_len(), 0);
    }

    #[test]
    fn new_edit_clears_redo_and_clear_drops_both_stacks() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let mut history = History::with_limit(10);
        history.push(Edit {
            offset: 0,
            before: vec![0],
            after: vec![1],
            description: "first".into(),
        });
        rom.write(0, &[1]).unwrap();
        assert!(history.undo(&mut rom).unwrap());
        assert!(history.can_redo());
        history.push(Edit {
            offset: 1,
            before: vec![0],
            after: vec![2],
            description: "branch".into(),
        });
        assert!(!history.can_redo());
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }
}
