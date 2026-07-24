use crate::{LevelControllerError, NativeLevelEdit, portable_value_history::PortableValueHistory};
use lm_level::{NativeLevelFile, NativeLevelFileError, SpriteLengthTable};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLevelDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: NativeLevelFile,
}

/// Revisioned owner for one interpretation-bound `LMLVL1` native-level transfer file.
#[derive(Clone, Debug)]
pub struct NativeLevelDocumentController {
    path: PathBuf,
    sprite_lengths: SpriteLengthTable,
    value: NativeLevelFile,
    saved: NativeLevelFile,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<NativeLevelFile>,
}

impl NativeLevelDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes a transfer file with its exact four-table sprite-length interpretation.
    ///
    /// # Errors
    ///
    /// Returns [`NativeLevelDocumentControllerError::File`] for malformed or noncanonical input.
    pub fn decode(
        path: PathBuf,
        bytes: &[u8],
        sprite_lengths: SpriteLengthTable,
    ) -> Result<Self, NativeLevelDocumentControllerError> {
        let value = NativeLevelFile::decode(bytes, &sprite_lengths)
            .map_err(NativeLevelDocumentControllerError::File)?;
        Ok(Self {
            path,
            sprite_lengths,
            saved: value.clone(),
            value,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn value(&self) -> &NativeLevelFile {
        &self.value
    }

    #[must_use]
    pub const fn sprite_lengths(&self) -> &SpriteLengthTable {
        &self.sprite_lengths
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
    pub const fn save_pending(&self) -> bool {
        self.pending_save.is_some()
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Applies the same staged native object/sprite edits as the ROM-backed level controller.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, invalid edits, noncanonical results, and revision overflow without
    /// changing either stream.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[NativeLevelEdit],
    ) -> Result<(), NativeLevelDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(NativeLevelDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        crate::native_level_edit_batch::apply_native_level_edits(
            &mut staged.layer1,
            &mut staged.sprites,
            edits,
            &self.sprite_lengths,
        )
        .map_err(NativeLevelDocumentControllerError::Edit)?;
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(NativeLevelDocumentControllerError::RevisionOverflow)?;
        canonical_bytes(&staged, &self.sprite_lengths)?;
        self.history.record(self.value.clone());
        self.value = staged;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous interpretation-bound native level as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, NativeLevelDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted native level as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, NativeLevelDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, NativeLevelDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(NativeLevelDocumentControllerError::StaleRevision {
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
            .ok_or(NativeLevelDocumentControllerError::RevisionOverflow)?;
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
    /// Rejects overlapping saves, invalid public state, or request-counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<NativeLevelDocumentSaveSnapshot, NativeLevelDocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(NativeLevelDocumentControllerError::SavePending);
        }
        let bytes = canonical_bytes(&self.value, &self.sprite_lengths)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(NativeLevelDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(NativeLevelDocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Acknowledges exactly one persisted immutable snapshot.
    ///
    /// # Errors
    ///
    /// Missing or stale acknowledgements retain a mismatched pending request.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), NativeLevelDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(NativeLevelDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(NativeLevelDocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Cancels one failed persistence attempt without moving the saved baseline.
    ///
    /// # Errors
    ///
    /// Rejects missing or mismatched save requests.
    pub fn cancel_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), NativeLevelDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(NativeLevelDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(NativeLevelDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

fn canonical_bytes(
    value: &NativeLevelFile,
    sprite_lengths: &SpriteLengthTable,
) -> Result<Vec<u8>, NativeLevelDocumentControllerError> {
    let bytes = value
        .encode()
        .map_err(NativeLevelDocumentControllerError::File)?;
    let reopened = NativeLevelFile::decode(&bytes, sprite_lengths)
        .map_err(NativeLevelDocumentControllerError::File)?;
    if reopened != *value {
        return Err(NativeLevelDocumentControllerError::CanonicalMismatch);
    }
    Ok(bytes)
}

#[derive(Debug)]
pub enum NativeLevelDocumentControllerError {
    File(NativeLevelFileError),
    Edit(LevelControllerError),
    CanonicalMismatch,
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}

impl fmt::Display for NativeLevelDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native-level document controller failed: {self:?}"
        )
    }
}

impl std::error::Error for NativeLevelDocumentControllerError {}

#[cfg(test)]
#[path = "native_level_document_controller_tests.rs"]
mod tests;
