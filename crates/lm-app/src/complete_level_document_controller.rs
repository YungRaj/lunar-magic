use crate::portable_value_history::PortableValueHistory;
use lm_level::{
    CompleteLevelFile, CompleteLevelFileError, LevelAuxiliaryEdit, LevelAuxiliaryEditError,
};
use std::fmt;
use std::path::PathBuf;

mod edit;
mod save;

pub use edit::{CompleteLevelDocumentEdit, CompleteLevelDocumentEditError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteLevelDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: CompleteLevelFile,
}

/// Revisioned, toolkit-neutral owner of one portable `LMLEVEL2` document.
#[derive(Clone, Debug)]
pub struct CompleteLevelDocumentController {
    path: PathBuf,
    value: CompleteLevelFile,
    saved: CompleteLevelFile,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<CompleteLevelFile>,
}

impl CompleteLevelDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one exact, bounded complete-level artifact.
    ///
    /// # Errors
    ///
    /// Returns a file error when the bytes are malformed, excessive, or noncanonical.
    pub fn decode(
        path: PathBuf,
        bytes: &[u8],
    ) -> Result<Self, CompleteLevelDocumentControllerError> {
        let value =
            CompleteLevelFile::decode(bytes).map_err(CompleteLevelDocumentControllerError::File)?;
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
    pub const fn value(&self) -> &CompleteLevelFile {
        &self.value
    }

    #[must_use]
    pub const fn path(&self) -> &PathBuf {
        &self.path
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

    /// Applies a cross-domain auxiliary edit batch against one exact revision.
    ///
    /// The staged value must encode and reopen with identical semantics before it is committed.
    ///
    /// # Errors
    ///
    /// Returns stale-revision, invalid-edit, encoding, or revision-overflow errors atomically.
    pub fn apply_auxiliary_edits(
        &mut self,
        expected_revision: u64,
        edits: &[LevelAuxiliaryEdit],
    ) -> Result<(), CompleteLevelDocumentControllerError> {
        let edits = edits
            .iter()
            .cloned()
            .map(CompleteLevelDocumentEdit::Auxiliary)
            .collect::<Vec<_>>();
        self.apply_edits(expected_revision, &edits)
            .map_err(|error| match error {
                CompleteLevelDocumentControllerError::Domain {
                    error: CompleteLevelDocumentEditError::Auxiliary(error),
                    ..
                } => CompleteLevelDocumentControllerError::Edit(error),
                error => error,
            })
    }

    /// Applies one ordered batch spanning properties, both object layers, sprites, Layer 3, and
    /// auxiliary records, then encodes and canonically reopens the entire level atomically.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, invalid domain edits, noncanonical encoding, and overflow without
    /// changing the document or its history.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[CompleteLevelDocumentEdit],
    ) -> Result<(), CompleteLevelDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(CompleteLevelDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        for (command, edit) in edits.iter().enumerate() {
            edit::apply_edit(&mut staged, edit)
                .map_err(|error| CompleteLevelDocumentControllerError::Domain { command, error })?;
        }
        self.commit_staged(&staged)
    }

    fn commit_staged(
        &mut self,
        staged: &CompleteLevelFile,
    ) -> Result<(), CompleteLevelDocumentControllerError> {
        if *staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(CompleteLevelDocumentControllerError::RevisionOverflow)?;
        let bytes = staged
            .encode()
            .map_err(CompleteLevelDocumentControllerError::File)?;
        let reopened = CompleteLevelFile::decode(&bytes)
            .map_err(CompleteLevelDocumentControllerError::File)?;
        if reopened != *staged {
            return Err(CompleteLevelDocumentControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical complete-level value as a new document revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, CompleteLevelDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical complete-level value as a new document revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, CompleteLevelDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, CompleteLevelDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(CompleteLevelDocumentControllerError::StaleRevision {
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
            .ok_or(CompleteLevelDocumentControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }
}

#[derive(Debug)]
pub enum CompleteLevelDocumentControllerError {
    File(CompleteLevelFileError),
    Edit(LevelAuxiliaryEditError),
    Domain {
        command: usize,
        error: CompleteLevelDocumentEditError,
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

impl fmt::Display for CompleteLevelDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "complete level document controller failed: {self:?}"
        )
    }
}

impl std::error::Error for CompleteLevelDocumentControllerError {}

#[cfg(test)]
mod tests;
