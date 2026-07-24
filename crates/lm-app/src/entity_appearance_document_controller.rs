use crate::portable_value_history::PortableValueHistory;
use lm_level::{EntityAppearanceFile, EntityAppearanceFileError, EntityAppearanceRecord};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntityAppearanceDocumentEdit {
    Insert {
        index: usize,
        value: EntityAppearanceRecord,
    },
    Replace {
        index: usize,
        value: EntityAppearanceRecord,
    },
    Remove {
        index: usize,
    },
    MoveBefore {
        from: usize,
        before: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityAppearanceDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: EntityAppearanceFile,
}

/// Revisioned owner for provider-resolved, painter-ordered level entity appearances.
#[derive(Clone, Debug)]
pub struct EntityAppearanceDocumentController {
    path: PathBuf,
    value: EntityAppearanceFile,
    saved: EntityAppearanceFile,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<EntityAppearanceFile>,
}

impl EntityAppearanceDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one exact bounded `LMENTAPP` document.
    ///
    /// # Errors
    ///
    /// Returns a file error for malformed framing, flags, source kinds, palettes, or counts.
    pub fn decode(
        path: PathBuf,
        bytes: &[u8],
    ) -> Result<Self, EntityAppearanceDocumentControllerError> {
        let value = EntityAppearanceFile::decode(bytes)
            .map_err(EntityAppearanceDocumentControllerError::File)?;
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
    pub const fn value(&self) -> &EntityAppearanceFile {
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

    #[must_use]
    pub const fn save_pending(&self) -> bool {
        self.pending_save.is_some()
    }

    /// Applies a painter-order edit batch to a staged clone and canonically reopens the result.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, invalid sequence indexes, file invariants, and overflow atomically.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[EntityAppearanceDocumentEdit],
    ) -> Result<(), EntityAppearanceDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(EntityAppearanceDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        for (command, edit) in edits.iter().enumerate() {
            apply_edit(&mut staged.appearances, edit).map_err(|error| {
                EntityAppearanceDocumentControllerError::Edit { command, error }
            })?;
        }
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(EntityAppearanceDocumentControllerError::RevisionOverflow)?;
        let reopened = canonical_reopen(&staged)?;
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical painter-ordered appearance file as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, EntityAppearanceDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical appearance file as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, EntityAppearanceDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, EntityAppearanceDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(EntityAppearanceDocumentControllerError::StaleRevision {
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
            .ok_or(EntityAppearanceDocumentControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Reserves an immutable canonical save snapshot.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves, invalid public state, and request-counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<EntityAppearanceDocumentSaveSnapshot, EntityAppearanceDocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(EntityAppearanceDocumentControllerError::SavePending);
        }
        let bytes = canonical_bytes(&self.value)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(EntityAppearanceDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(EntityAppearanceDocumentSaveSnapshot {
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
    ) -> Result<(), EntityAppearanceDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(EntityAppearanceDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(EntityAppearanceDocumentControllerError::StaleSave {
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
    /// Rejects missing or mismatched requests.
    pub fn cancel_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), EntityAppearanceDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(EntityAppearanceDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(EntityAppearanceDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

fn apply_edit(
    values: &mut Vec<EntityAppearanceRecord>,
    edit: &EntityAppearanceDocumentEdit,
) -> Result<(), EntityAppearanceSequenceError> {
    match edit {
        EntityAppearanceDocumentEdit::Insert { index, value } => {
            if *index > values.len() {
                return Err(out_of_bounds(*index, values.len()));
            }
            values.insert(*index, *value);
        }
        EntityAppearanceDocumentEdit::Replace { index, value } => {
            let len = values.len();
            *values
                .get_mut(*index)
                .ok_or_else(|| out_of_bounds(*index, len))? = *value;
        }
        EntityAppearanceDocumentEdit::Remove { index } => {
            if *index >= values.len() {
                return Err(out_of_bounds(*index, values.len()));
            }
            values.remove(*index);
        }
        EntityAppearanceDocumentEdit::MoveBefore { from, before } => {
            if *from >= values.len() {
                return Err(out_of_bounds(*from, values.len()));
            }
            if *before > values.len() {
                return Err(out_of_bounds(*before, values.len()));
            }
            if *from == *before || from.checked_add(1) == Some(*before) {
                return Ok(());
            }
            let value = values.remove(*from);
            let destination = if from < before { before - 1 } else { *before };
            values.insert(destination, value);
        }
    }
    Ok(())
}

const fn out_of_bounds(index: usize, len: usize) -> EntityAppearanceSequenceError {
    EntityAppearanceSequenceError::IndexOutOfBounds { index, len }
}

fn canonical_bytes(
    value: &EntityAppearanceFile,
) -> Result<Vec<u8>, EntityAppearanceDocumentControllerError> {
    let bytes = value
        .encode()
        .map_err(EntityAppearanceDocumentControllerError::File)?;
    if EntityAppearanceFile::decode(&bytes)
        .map_err(EntityAppearanceDocumentControllerError::File)?
        != *value
    {
        return Err(EntityAppearanceDocumentControllerError::NonCanonicalEncoding);
    }
    Ok(bytes)
}

fn canonical_reopen(
    value: &EntityAppearanceFile,
) -> Result<EntityAppearanceFile, EntityAppearanceDocumentControllerError> {
    EntityAppearanceFile::decode(&canonical_bytes(value)?)
        .map_err(EntityAppearanceDocumentControllerError::File)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityAppearanceSequenceError {
    IndexOutOfBounds { index: usize, len: usize },
}

#[derive(Debug)]
pub enum EntityAppearanceDocumentControllerError {
    File(EntityAppearanceFileError),
    Edit {
        command: usize,
        error: EntityAppearanceSequenceError,
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

impl fmt::Display for EntityAppearanceDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "entity appearance document controller failed: {self:?}"
        )
    }
}

impl std::error::Error for EntityAppearanceDocumentControllerError {}

#[cfg(test)]
#[path = "entity_appearance_document_controller_tests.rs"]
mod tests;
