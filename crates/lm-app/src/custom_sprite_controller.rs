use crate::portable_value_history::PortableValueHistory;
use lm_level::{
    CustomSpriteEntry, CustomSpriteLibrary, CustomSpriteLibraryError, DescriptionFormat,
    SpriteLengthTable,
};
use std::{fmt, path::PathBuf};

/// One application-level mutation of a paired custom-sprite placement library.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomSpriteLibraryEdit {
    Insert {
        index: usize,
        entry: CustomSpriteEntry,
    },
    Replace {
        index: usize,
        entry: CustomSpriteEntry,
    },
    Remove {
        index: usize,
    },
    Move {
        from: usize,
        to: usize,
    },
    SetHeader(u8),
    SetDescriptionFormat(DescriptionFormat),
}

/// Immutable paired `.mw2`/`.mwt` bytes handed to a frontend for atomic persistence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSpriteSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub data_path: PathBuf,
    pub descriptions_path: PathBuf,
    pub data: Vec<u8>,
    pub descriptions: Vec<u8>,
}

/// Toolkit-neutral revisioned controller for Lunar Magic custom-sprite sidecars.
#[derive(Clone, Debug)]
pub struct CustomSpriteLibraryController {
    data_path: PathBuf,
    descriptions_path: PathBuf,
    lengths: SpriteLengthTable,
    library: CustomSpriteLibrary,
    saved: CustomSpriteLibrary,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<CustomSpriteLibrary>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    library: CustomSpriteLibrary,
}

impl CustomSpriteLibraryController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes a frontend-supplied pair using one exact revision length table.
    ///
    /// # Errors
    ///
    /// Rejects aliased paths or malformed, unsynchronized sidecars.
    pub fn decode(
        data_path: PathBuf,
        descriptions_path: PathBuf,
        data: &[u8],
        descriptions: &[u8],
        lengths: SpriteLengthTable,
    ) -> Result<Self, CustomSpriteControllerError> {
        if data_path == descriptions_path {
            return Err(CustomSpriteControllerError::AliasedPaths);
        }
        let library = CustomSpriteLibrary::decode(data, descriptions, &lengths)
            .map_err(CustomSpriteControllerError::Library)?;
        library
            .encode_checked(&lengths)
            .map_err(CustomSpriteControllerError::Library)?;
        Ok(Self {
            data_path,
            descriptions_path,
            lengths,
            saved: library.clone(),
            library,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn library(&self) -> &CustomSpriteLibrary {
        &self.library
    }

    #[must_use]
    pub const fn sprite_lengths(&self) -> &SpriteLengthTable {
        &self.lengths
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.library != self.saved
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

    /// Applies an ordered batch to a staged clone and advances one revision on change.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, overflowing revisions, or the indexed failing edit. The complete
    /// staged result is checked against the controller's immutable sprite-length table.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[CustomSpriteLibraryEdit],
    ) -> Result<(), CustomSpriteControllerError> {
        self.require_revision(expected_revision)?;
        let mut staged = self.library.clone();
        for (command, edit) in edits.iter().enumerate() {
            let result = match edit {
                CustomSpriteLibraryEdit::Insert { index, entry } => {
                    staged.insert(*index, entry.clone())
                }
                CustomSpriteLibraryEdit::Replace { index, entry } => {
                    staged.replace(*index, entry.clone()).map(drop)
                }
                CustomSpriteLibraryEdit::Remove { index } => staged.remove(*index).map(drop),
                CustomSpriteLibraryEdit::Move { from, to } => staged.move_entry(*from, *to),
                CustomSpriteLibraryEdit::SetHeader(header) => {
                    staged.set_header(*header);
                    Ok(())
                }
                CustomSpriteLibraryEdit::SetDescriptionFormat(format) => {
                    staged.set_description_format(*format)
                }
            };
            result.map_err(|error| CustomSpriteControllerError::Edit { command, error })?;
        }
        if staged == self.library {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(CustomSpriteControllerError::RevisionOverflow)?;
        let (data, descriptions) = staged.encode_checked(&self.lengths).map_err(|error| {
            CustomSpriteControllerError::Edit {
                command: edits.len(),
                error,
            }
        })?;
        let reopened = CustomSpriteLibrary::decode(&data, &descriptions, &self.lengths)
            .map_err(CustomSpriteControllerError::Library)?;
        if reopened != staged {
            return Err(CustomSpriteControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.library.clone());
        self.library = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical paired library under the immutable length interpretation.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(&mut self, expected_revision: u64) -> Result<bool, CustomSpriteControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical paired library as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(&mut self, expected_revision: u64) -> Result<bool, CustomSpriteControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, CustomSpriteControllerError> {
        self.require_revision(expected_revision)?;
        if if undo {
            !self.can_undo()
        } else {
            !self.can_redo()
        } {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(CustomSpriteControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.library)
        } else {
            self.history.redo(&mut self.library)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Captures exact paired bytes and reserves the controller's save slot.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves, request exhaustion, or invalid programmatic state.
    pub fn begin_save(&mut self) -> Result<CustomSpriteSaveSnapshot, CustomSpriteControllerError> {
        if self.pending_save.is_some() {
            return Err(CustomSpriteControllerError::SavePending);
        }
        let (data, descriptions) = self
            .library
            .encode_checked(&self.lengths)
            .map_err(CustomSpriteControllerError::Library)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(CustomSpriteControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            library: self.library.clone(),
        });
        Ok(CustomSpriteSaveSnapshot {
            request_id,
            revision: self.revision,
            data_path: self.data_path.clone(),
            descriptions_path: self.descriptions_path.clone(),
            data,
            descriptions,
        })
    }

    /// Acknowledges persistence of the exact pending snapshot.
    ///
    /// # Errors
    ///
    /// Rejects a missing or mismatched request without discarding a retryable pending snapshot.
    pub fn acknowledge_save(&mut self, request_id: u64) -> Result<(), CustomSpriteControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(CustomSpriteControllerError::NoPendingSave)?;
        if request_id != pending.request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(CustomSpriteControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.library;
        Ok(())
    }

    /// Releases a failed or cancelled persistence attempt for immediate retry.
    ///
    /// # Errors
    ///
    /// Rejects a missing or mismatched request without changing controller state.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), CustomSpriteControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(CustomSpriteControllerError::NoPendingSave)?;
        if request_id != pending.request_id {
            return Err(CustomSpriteControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }

    fn require_revision(&self, expected: u64) -> Result<(), CustomSpriteControllerError> {
        if expected == self.revision {
            Ok(())
        } else {
            Err(CustomSpriteControllerError::StaleRevision {
                expected,
                actual: self.revision,
            })
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomSpriteControllerError {
    AliasedPaths,
    Library(CustomSpriteLibraryError),
    NonCanonicalEncoding,
    Edit {
        command: usize,
        error: CustomSpriteLibraryError,
    },
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

impl fmt::Display for CustomSpriteControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "custom-sprite library controller failed: {self:?}"
        )
    }
}

impl std::error::Error for CustomSpriteControllerError {}

#[cfg(test)]
#[path = "custom_sprite_controller_tests.rs"]
mod tests;
