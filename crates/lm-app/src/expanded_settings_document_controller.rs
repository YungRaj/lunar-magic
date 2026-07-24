use crate::portable_value_history::PortableValueHistory;
use lm_level::{ExpandedLevelSettingsError, ExpandedLevelSettingsRecord};
use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedSettingsDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: [u8; ExpandedLevelSettingsRecord::ENCODED_LEN],
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: ExpandedLevelSettingsRecord,
}

/// Revisioned controller for one exact native 32-byte expanded-settings record.
#[derive(Clone, Debug)]
pub struct ExpandedSettingsDocumentController {
    path: PathBuf,
    value: ExpandedLevelSettingsRecord,
    saved: ExpandedLevelSettingsRecord,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<ExpandedLevelSettingsRecord>,
}

impl ExpandedSettingsDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Opens one exact 32-byte record.
    ///
    /// # Errors
    ///
    /// Returns a record error when the byte length is not exact.
    pub fn decode(
        path: PathBuf,
        bytes: &[u8],
    ) -> Result<Self, ExpandedSettingsDocumentControllerError> {
        let value = ExpandedLevelSettingsRecord::decode(bytes)
            .map_err(ExpandedSettingsDocumentControllerError::Record)?;
        Ok(Self {
            path,
            saved: value.clone(),
            value,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn value(&self) -> &ExpandedLevelSettingsRecord {
        &self.value
    }
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.value != self.saved
    }
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Applies an atomic, exact-revision batch of native word replacements.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, duplicate/out-of-range words, or revision overflow.
    pub fn apply_word_edits(
        &mut self,
        expected_revision: u64,
        edits: &[(usize, u16)],
    ) -> Result<(), ExpandedSettingsDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(ExpandedSettingsDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        let mut seen = [false; ExpandedLevelSettingsRecord::WORD_COUNT];
        for &(index, value) in edits {
            if index >= seen.len() {
                return Err(ExpandedSettingsDocumentControllerError::Record(
                    ExpandedLevelSettingsError::WordOutOfRange(index),
                ));
            }
            if std::mem::replace(&mut seen[index], true) {
                return Err(ExpandedSettingsDocumentControllerError::DuplicateWord(
                    index,
                ));
            }
            staged
                .set_word(index, value)
                .map_err(ExpandedSettingsDocumentControllerError::Record)?;
        }
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ExpandedSettingsDocumentControllerError::RevisionOverflow)?;
        self.history.record(self.value.clone());
        self.value = staged;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous exact record as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, ExpandedSettingsDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted exact record as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, ExpandedSettingsDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, ExpandedSettingsDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(ExpandedSettingsDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        if if undo {
            !self.history.can_undo()
        } else {
            !self.history.can_redo()
        } {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ExpandedSettingsDocumentControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Captures an immutable save request.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves or counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<ExpandedSettingsDocumentSaveSnapshot, ExpandedSettingsDocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(ExpandedSettingsDocumentControllerError::SavePending);
        }
        let request_id = self.next_save_request;
        self.next_save_request = request_id
            .checked_add(1)
            .ok_or(ExpandedSettingsDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(ExpandedSettingsDocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes: *self.value.encoded(),
        })
    }

    /// Marks the exact pending snapshot as persisted.
    ///
    /// # Errors
    ///
    /// Rejects absent or mismatched request identifiers.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), ExpandedSettingsDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(ExpandedSettingsDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(ExpandedSettingsDocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Cancels the exact pending snapshot without changing the saved baseline.
    ///
    /// # Errors
    ///
    /// Rejects absent or mismatched request identifiers.
    pub fn cancel_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), ExpandedSettingsDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(ExpandedSettingsDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(ExpandedSettingsDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsDocumentControllerError {
    Record(ExpandedLevelSettingsError),
    DuplicateWord(usize),
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}
impl fmt::Display for ExpandedSettingsDocumentControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expanded-settings document controller failed: {self:?}")
    }
}
impl std::error::Error for ExpandedSettingsDocumentControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batches_are_atomic_and_save_snapshots_are_immutable() {
        let mut controller =
            ExpandedSettingsDocumentController::decode("record.bin".into(), &[0; 32]).unwrap();
        controller.apply_word_edits(0, &[(2, 0x1234)]).unwrap();
        assert!(controller.apply_word_edits(1, &[(1, 1), (1, 2)]).is_err());
        assert_eq!(controller.value().word(1).unwrap(), 0);
        let snapshot = controller.begin_save().unwrap();
        controller.apply_word_edits(1, &[(3, 0xabcd)]).unwrap();
        controller.acknowledge_save(snapshot.request_id).unwrap();
        assert!(controller.is_modified());
        assert_eq!(&snapshot.bytes[4..6], &[0x34, 0x12]);
    }

    #[test]
    fn history_restores_saved_baseline_and_invalidates_divergent_redo() {
        let mut controller =
            ExpandedSettingsDocumentController::decode("record.bin".into(), &[0; 32]).unwrap();
        controller.apply_word_edits(0, &[(2, 0x1234)]).unwrap();
        assert!(controller.can_undo());
        assert!(controller.undo(1).unwrap());
        assert_eq!(controller.revision(), 2);
        assert!(!controller.is_modified());
        assert!(controller.can_redo());
        assert!(controller.redo(2).unwrap());
        assert!(controller.is_modified());
        assert!(controller.undo(3).unwrap());
        controller.apply_word_edits(4, &[(3, 0xabcd)]).unwrap();
        assert!(!controller.can_redo());
        assert!(!controller.redo(5).unwrap());
        assert_eq!(controller.revision(), 5);
    }

    #[test]
    fn stale_and_empty_history_navigation_are_atomic() {
        let mut controller =
            ExpandedSettingsDocumentController::decode("record.bin".into(), &[0; 32]).unwrap();
        assert!(!controller.undo(0).unwrap());
        controller.apply_word_edits(0, &[(1, 7)]).unwrap();
        let before = controller.value().clone();
        assert!(matches!(
            controller.undo(0),
            Err(ExpandedSettingsDocumentControllerError::StaleRevision { .. })
        ));
        assert_eq!(controller.value(), &before);
        assert_eq!(controller.revision(), 1);
    }
}
