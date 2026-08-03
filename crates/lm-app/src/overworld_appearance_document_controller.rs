use crate::portable_value_history::PortableValueHistory;
use lm_overworld::{SpriteAppearanceFile, SpriteAppearanceFileError, SpriteAppearancePart};
use std::fmt;
use std::path::PathBuf;

mod canonical;
mod editing;

use canonical::{canonical_bytes, canonical_reopen};
use editing::apply_edit;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldAppearanceDocumentEdit {
    InsertDefinition {
        index: usize,
        sprite_id: u16,
    },
    RemoveDefinition {
        sprite_id: u16,
    },
    MoveDefinitionBefore {
        sprite_id: u16,
        before: Option<u16>,
    },
    InsertPart {
        sprite_id: u16,
        index: usize,
        value: SpriteAppearancePart,
    },
    ReplacePart {
        sprite_id: u16,
        index: usize,
        value: SpriteAppearancePart,
    },
    MovePartBefore {
        sprite_id: u16,
        index: usize,
        before: Option<usize>,
    },
    RemovePart {
        sprite_id: u16,
        index: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldAppearanceDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: SpriteAppearanceFile,
}

/// Revisioned owner for sprite-ID-keyed overworld appearance definitions and ordered parts.
#[derive(Clone, Debug)]
pub struct OverworldAppearanceDocumentController {
    path: PathBuf,
    value: SpriteAppearanceFile,
    saved: SpriteAppearanceFile,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<SpriteAppearanceFile>,
}

impl OverworldAppearanceDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one exact bounded `LMOWAPP1` file.
    ///
    /// # Errors
    ///
    /// Returns a file error for malformed definitions, part ranges, flags, palettes, or counts.
    pub fn decode(
        path: PathBuf,
        bytes: &[u8],
    ) -> Result<Self, OverworldAppearanceDocumentControllerError> {
        let value = SpriteAppearanceFile::decode(bytes)
            .map_err(OverworldAppearanceDocumentControllerError::File)?;
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
    pub const fn value(&self) -> &SpriteAppearanceFile {
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

    /// Applies stable-ID definition and ordered-part edits on a staged clone.
    ///
    /// # Errors
    ///
    /// Stale revisions, duplicate/missing IDs, invalid indexes, file limits, and canonical reopen
    /// failures preserve every definition and part.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[OverworldAppearanceDocumentEdit],
    ) -> Result<(), OverworldAppearanceDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(OverworldAppearanceDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        for (command, edit) in edits.iter().enumerate() {
            apply_edit(&mut staged.definitions, edit).map_err(|error| {
                OverworldAppearanceDocumentControllerError::Edit { command, error }
            })?;
        }
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(OverworldAppearanceDocumentControllerError::RevisionOverflow)?;
        let reopened = canonical_reopen(&staged)?;
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical definition and nested-part ordering as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, OverworldAppearanceDocumentControllerError> {
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
    ) -> Result<bool, OverworldAppearanceDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, OverworldAppearanceDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(OverworldAppearanceDocumentControllerError::StaleRevision {
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
            .ok_or(OverworldAppearanceDocumentControllerError::RevisionOverflow)?;
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
    /// Rejects overlapping saves, invalid public state, and request-counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<OverworldAppearanceDocumentSaveSnapshot, OverworldAppearanceDocumentControllerError>
    {
        if self.pending_save.is_some() {
            return Err(OverworldAppearanceDocumentControllerError::SavePending);
        }
        let bytes = canonical_bytes(&self.value)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(OverworldAppearanceDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(OverworldAppearanceDocumentSaveSnapshot {
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
    ) -> Result<(), OverworldAppearanceDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(OverworldAppearanceDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(OverworldAppearanceDocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases one failed persistence request without moving the saved baseline.
    ///
    /// # Errors
    ///
    /// Rejects missing or mismatched request IDs.
    pub fn cancel_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), OverworldAppearanceDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(OverworldAppearanceDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(OverworldAppearanceDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverworldAppearanceEditError {
    DuplicateSpriteId(u16),
    UnknownSpriteId(u16),
    IndexOutOfBounds { index: usize, len: usize },
}

#[derive(Debug)]
pub enum OverworldAppearanceDocumentControllerError {
    File(SpriteAppearanceFileError),
    Edit {
        command: usize,
        error: OverworldAppearanceEditError,
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

impl fmt::Display for OverworldAppearanceDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "overworld appearance document controller failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldAppearanceDocumentControllerError {}

#[cfg(test)]
#[path = "overworld_appearance_document_controller_tests.rs"]
mod tests;
