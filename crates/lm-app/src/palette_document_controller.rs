use crate::PaletteControllerEdit;
use crate::palette_edit_batch::apply_palette_edit_batch;
use crate::portable_value_history::PortableValueHistory;
use lm_graphics::{
    PaletteBatchEditError, PaletteInterchangeError, PaletteInterchangeFile, PaletteOwnership,
};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaletteDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: PaletteInterchangeFile,
}

#[derive(Clone, Debug)]
pub struct PaletteDocumentController {
    path: PathBuf,
    value: PaletteInterchangeFile,
    saved: PaletteInterchangeFile,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<PaletteInterchangeFile>,
}

impl PaletteDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one exact bounded portable palette file.
    ///
    /// # Errors
    ///
    /// Returns a file error for malformed framing, colors, or limits.
    pub fn decode(path: PathBuf, bytes: &[u8]) -> Result<Self, PaletteDocumentControllerError> {
        let value =
            PaletteInterchangeFile::decode(bytes).map_err(PaletteDocumentControllerError::File)?;
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
    pub const fn value(&self) -> &PaletteInterchangeFile {
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

    /// Applies an ownership-aware batch against one exact revision and canonically reopens it.
    ///
    /// # Errors
    ///
    /// Stale revisions, ownership mismatches, protected colors, and encoding failures are atomic.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        ownership: &PaletteOwnership,
        edits: &[PaletteControllerEdit],
    ) -> Result<(), PaletteDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(PaletteDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        apply_palette_edit_batch(&mut staged.palette, ownership, edits)
            .map_err(|(command, error)| PaletteDocumentControllerError::Edit { command, error })?;
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(PaletteDocumentControllerError::RevisionOverflow)?;
        let bytes = staged
            .encode()
            .map_err(PaletteDocumentControllerError::File)?;
        let reopened =
            PaletteInterchangeFile::decode(&bytes).map_err(PaletteDocumentControllerError::File)?;
        if reopened != staged {
            return Err(PaletteDocumentControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical palette value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(&mut self, expected_revision: u64) -> Result<bool, PaletteDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical palette value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(&mut self, expected_revision: u64) -> Result<bool, PaletteDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, PaletteDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(PaletteDocumentControllerError::StaleRevision {
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
            .ok_or(PaletteDocumentControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Reserves one immutable canonical save snapshot.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves, invalid data, and request-counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<PaletteDocumentSaveSnapshot, PaletteDocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(PaletteDocumentControllerError::SavePending);
        }
        let bytes = self
            .value
            .encode()
            .map_err(PaletteDocumentControllerError::File)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(PaletteDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(PaletteDocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Acknowledges only the exact immutable pending snapshot.
    ///
    /// # Errors
    ///
    /// Missing or mismatched requests preserve retryable state.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), PaletteDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(PaletteDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(PaletteDocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases the exact failed save without changing the saved baseline.
    ///
    /// # Errors
    ///
    /// Missing or mismatched requests preserve the pending save.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), PaletteDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(PaletteDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(PaletteDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Debug)]
pub enum PaletteDocumentControllerError {
    File(PaletteInterchangeError),
    Edit {
        command: usize,
        error: PaletteBatchEditError,
    },
    NonCanonicalEncoding,
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave {
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for PaletteDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "palette document controller failed: {self:?}")
    }
}

impl std::error::Error for PaletteDocumentControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, Palette, PaletteChange};

    fn controller() -> PaletteDocumentController {
        let file = PaletteInterchangeFile {
            source_palette: 1,
            palette: Palette {
                colors: vec![Bgr555(0); 16],
            },
        };
        PaletteDocumentController::decode("colors.lmpal".into(), &file.encode().unwrap()).unwrap()
    }

    fn edit(value: u16) -> PaletteControllerEdit {
        PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
            index: 1,
            color: Bgr555(value),
        }])
    }

    #[test]
    fn revision_ownership_and_save_snapshot_are_exact() {
        let mut controller = controller();
        assert!(
            controller
                .apply_edits(0, &PaletteOwnership::editable(15), &[edit(1)])
                .is_err()
        );
        let ownership = PaletteOwnership::editable(16);
        controller.apply_edits(0, &ownership, &[edit(1)]).unwrap();
        let save = controller.begin_save().unwrap();
        controller.apply_edits(1, &ownership, &[edit(2)]).unwrap();
        controller.acknowledge_save(save.request_id).unwrap();
        assert!(controller.is_modified());
        assert_eq!(
            PaletteInterchangeFile::decode(&save.bytes)
                .unwrap()
                .palette
                .colors[1],
            Bgr555(1)
        );
    }

    #[test]
    fn history_restores_saved_palette_and_clears_divergent_redo() {
        let ownership = PaletteOwnership::editable(16);
        let mut controller = controller();
        controller.apply_edits(0, &ownership, &[edit(1)]).unwrap();
        let snapshot = controller.begin_save().unwrap();
        controller.acknowledge_save(snapshot.request_id).unwrap();
        controller.apply_edits(1, &ownership, &[edit(2)]).unwrap();
        assert!(controller.undo(2).unwrap());
        assert!(!controller.is_modified());
        assert!(controller.redo(3).unwrap());
        assert!(controller.undo(4).unwrap());
        controller.apply_edits(5, &ownership, &[edit(3)]).unwrap();
        assert!(!controller.can_redo());
        assert!(matches!(
            controller.undo(5),
            Err(PaletteDocumentControllerError::StaleRevision { .. })
        ));
    }
}
