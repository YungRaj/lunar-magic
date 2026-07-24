use crate::exanimation_controller::apply_animation_edits;
use crate::portable_value_history::PortableValueHistory;
use crate::{ExAnimationControllerEdit, ExAnimationControllerEditFailure};
use lm_graphics::{CompactExAnimationFile, CompactExAnimationFileError, ExAnimationFrame};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExAnimationDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: CompactExAnimationFile,
}

#[derive(Clone, Debug)]
pub struct ExAnimationDocumentController {
    path: PathBuf,
    value: CompactExAnimationFile,
    saved: CompactExAnimationFile,
    maximum_records: usize,
    double_size_modes: [bool; 256],
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<CompactExAnimationFile>,
}

impl ExAnimationDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one compact document with an exact transfer-size interpretation.
    ///
    /// # Errors
    ///
    /// Rejects non-256-entry modes and malformed or excessive compact payloads.
    pub fn decode(
        path: PathBuf,
        bytes: &[u8],
        maximum_records: usize,
        double_size_modes: &[bool],
    ) -> Result<Self, ExAnimationDocumentControllerError> {
        let modes: [bool; 256] = double_size_modes.try_into().map_err(|_| {
            ExAnimationDocumentControllerError::SizeModeCount(double_size_modes.len())
        })?;
        let value = CompactExAnimationFile::decode(bytes, maximum_records, &modes)
            .map_err(ExAnimationDocumentControllerError::File)?;
        Ok(Self {
            path,
            saved: value.clone(),
            value,
            maximum_records,
            double_size_modes: modes,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn value(&self) -> &CompactExAnimationFile {
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

    /// Returns ordinary source-word frames under the document's retained size-mode table.
    ///
    /// # Errors
    ///
    /// Rejects absent records and transfer kinds without ordinary frame payloads.
    pub fn record_frames(
        &self,
        record: usize,
    ) -> Result<Vec<ExAnimationFrame>, ExAnimationDocumentControllerError> {
        let len = self.value.animation.records.len();
        let value = self.value.animation.records.get(record).ok_or(
            ExAnimationDocumentControllerError::Edit {
                command: 0,
                error: ExAnimationControllerEditFailure::Animation(
                    lm_graphics::ExAnimationEditError::RecordIndexOutOfRange { index: record, len },
                ),
            },
        )?;
        lm_graphics::exanimation_frames(
            value,
            self.double_size_modes[usize::from(value.size_mode())],
        )
        .map_err(|error| ExAnimationDocumentControllerError::Edit {
            command: 0,
            error: ExAnimationControllerEditFailure::Frames { record, error },
        })
    }

    /// Applies a mixed compact edit batch at one exact revision and canonically reopens it.
    ///
    /// # Errors
    ///
    /// Stale revisions, invalid records/frames, capacity, encoding, and overflow failures are atomic.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[ExAnimationControllerEdit],
    ) -> Result<(), ExAnimationDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(ExAnimationDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        apply_animation_edits(
            &mut staged.animation,
            edits,
            self.maximum_records,
            &self.double_size_modes,
        )
        .map_err(|(command, error)| ExAnimationDocumentControllerError::Edit { command, error })?;
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ExAnimationDocumentControllerError::RevisionOverflow)?;
        let bytes = staged
            .encode(&self.double_size_modes)
            .map_err(ExAnimationDocumentControllerError::File)?;
        let reopened =
            CompactExAnimationFile::decode(&bytes, self.maximum_records, &self.double_size_modes)
                .map_err(ExAnimationDocumentControllerError::File)?;
        if reopened != staged {
            return Err(ExAnimationDocumentControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical animation value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, ExAnimationDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical animation value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, ExAnimationDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, ExAnimationDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(ExAnimationDocumentControllerError::StaleRevision {
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
            .ok_or(ExAnimationDocumentControllerError::RevisionOverflow)?;
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
    ) -> Result<ExAnimationDocumentSaveSnapshot, ExAnimationDocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(ExAnimationDocumentControllerError::SavePending);
        }
        let bytes = self
            .value
            .encode(&self.double_size_modes)
            .map_err(ExAnimationDocumentControllerError::File)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(ExAnimationDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(ExAnimationDocumentSaveSnapshot {
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
    ) -> Result<(), ExAnimationDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(ExAnimationDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(ExAnimationDocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases the exact failed save without changing the baseline.
    ///
    /// # Errors
    ///
    /// Missing or mismatched requests preserve the pending save.
    pub fn cancel_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), ExAnimationDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(ExAnimationDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(ExAnimationDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Debug)]
pub enum ExAnimationDocumentControllerError {
    SizeModeCount(usize),
    File(CompactExAnimationFileError),
    Edit {
        command: usize,
        error: ExAnimationControllerEditFailure,
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

impl fmt::Display for ExAnimationDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "ExAnimation document controller failed: {self:?}"
        )
    }
}

impl std::error::Error for ExAnimationDocumentControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{CompactExAnimation, ExAnimationRecord};

    const MODES: [bool; 256] = [false; 256];

    fn controller() -> ExAnimationDocumentController {
        let file = CompactExAnimationFile {
            source_slot: 2,
            animation: CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: vec![
                    ExAnimationRecord::new(1, 0, 0, 0x1234, false, &[1, 0], false).unwrap(),
                ],
            },
        };
        ExAnimationDocumentController::decode(
            "animation.lmexan".into(),
            &file.encode(&MODES).unwrap(),
            32,
            &MODES,
        )
        .unwrap()
    }

    #[test]
    fn edits_frames_and_save_snapshots_use_retained_modes() {
        let mut controller = controller();
        assert_eq!(controller.record_frames(0).unwrap()[0].source_words, [1]);
        controller
            .apply_edits(0, &[ExAnimationControllerEdit::SetSetting(3)])
            .unwrap();
        let save = controller.begin_save().unwrap();
        controller
            .apply_edits(1, &[ExAnimationControllerEdit::SetSetting(4)])
            .unwrap();
        controller.acknowledge_save(save.request_id).unwrap();
        assert!(controller.is_modified());
        assert_eq!(
            CompactExAnimationFile::decode(&save.bytes, 32, &MODES)
                .unwrap()
                .animation
                .setting,
            3
        );
    }

    #[test]
    fn history_restores_saved_animation_and_clears_divergent_redo() {
        let mut controller = controller();
        controller
            .apply_edits(0, &[ExAnimationControllerEdit::SetSetting(1)])
            .unwrap();
        let snapshot = controller.begin_save().unwrap();
        controller.acknowledge_save(snapshot.request_id).unwrap();
        controller
            .apply_edits(1, &[ExAnimationControllerEdit::SetSetting(2)])
            .unwrap();
        assert!(controller.undo(2).unwrap());
        assert!(!controller.is_modified());
        assert!(controller.redo(3).unwrap());
        assert!(controller.undo(4).unwrap());
        controller
            .apply_edits(5, &[ExAnimationControllerEdit::SetSetting(3)])
            .unwrap();
        assert!(!controller.can_redo());
        assert!(matches!(
            controller.undo(5),
            Err(ExAnimationDocumentControllerError::StaleRevision { .. })
        ));
    }
}
