//! Revisioned application ownership for portable aggregate native level assets.

use crate::{
    NativeLevelAssetsControllerEdit, NativeLevelAssetsControllerError,
    native_level_assets_controller::apply_native_level_assets_edits,
    portable_value_history::PortableValueHistory,
};
use lm_graphics::PaletteOwnership;
use lm_level::SpriteLengthTable;
use lm_project::{NativeLevelAssetsFile, NativeLevelAssetsFileError};
use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLevelAssetsDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: NativeLevelAssetsFile,
}

/// Toolkit-neutral owner of one portable `LMNATAS1` document.
#[derive(Clone, Debug)]
pub struct NativeLevelAssetsDocumentController {
    path: PathBuf,
    value: NativeLevelAssetsFile,
    saved: NativeLevelAssetsFile,
    sprite_lengths: SpriteLengthTable,
    maximum_animation_records: usize,
    double_size_modes: [bool; 256],
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<NativeLevelAssetsFile>,
}

impl NativeLevelAssetsDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes a bounded aggregate with explicit revision interpretation tables.
    ///
    /// # Errors
    ///
    /// Rejects a non-256-entry mode table or any malformed nested resource.
    pub fn decode(
        path: PathBuf,
        bytes: &[u8],
        sprite_lengths: SpriteLengthTable,
        maximum_animation_records: usize,
        double_size_modes: &[bool],
    ) -> Result<Self, NativeLevelAssetsDocumentControllerError> {
        let modes: [bool; 256] = double_size_modes.try_into().map_err(|_| {
            NativeLevelAssetsDocumentControllerError::SizeModeCount(double_size_modes.len())
        })?;
        let value = NativeLevelAssetsFile::decode(
            bytes,
            &sprite_lengths,
            maximum_animation_records,
            &modes,
        )
        .map_err(NativeLevelAssetsDocumentControllerError::File)?;
        Ok(Self {
            path,
            saved: value.clone(),
            value,
            sprite_lengths,
            maximum_animation_records,
            double_size_modes: modes,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn value(&self) -> &NativeLevelAssetsFile {
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

    /// Applies a mixed edit batch atomically against one exact revision.
    ///
    /// # Errors
    ///
    /// Returns stale-revision, domain-edit, canonical encoding, or revision-overflow errors.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[NativeLevelAssetsControllerEdit],
        palette_ownership: &PaletteOwnership,
    ) -> Result<(), NativeLevelAssetsDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(NativeLevelAssetsDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        let mut unavailable_layer2 = None;
        let mut unavailable_layer2_descriptor = None;
        let mut unavailable_features = None;
        apply_native_level_assets_edits(
            &mut staged.assets,
            (
                (&mut unavailable_layer2, &mut unavailable_layer2_descriptor),
                &mut unavailable_features,
            ),
            edits,
            &self.sprite_lengths,
            self.maximum_animation_records,
            &self.double_size_modes,
            palette_ownership,
        )
        .map_err(NativeLevelAssetsDocumentControllerError::Edit)?;
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(NativeLevelAssetsDocumentControllerError::RevisionOverflow)?;
        let bytes = staged
            .encode(&self.double_size_modes)
            .map_err(NativeLevelAssetsDocumentControllerError::File)?;
        let reopened = NativeLevelAssetsFile::decode(
            &bytes,
            &self.sprite_lengths,
            self.maximum_animation_records,
            &self.double_size_modes,
        )
        .map_err(NativeLevelAssetsDocumentControllerError::File)?;
        if reopened != staged {
            return Err(NativeLevelAssetsDocumentControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical aggregate as a new monotonic document revision.
    ///
    /// # Errors
    ///
    /// Rejects a stale expected revision or revision-counter overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, NativeLevelAssetsDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical aggregate as a new monotonic document revision.
    ///
    /// # Errors
    ///
    /// Rejects a stale expected revision or revision-counter overflow without changing history.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, NativeLevelAssetsDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, NativeLevelAssetsDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(NativeLevelAssetsDocumentControllerError::StaleRevision {
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
            .ok_or(NativeLevelAssetsDocumentControllerError::RevisionOverflow)?;
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
    /// Rejects overlapping saves, invalid programmatic data, and request-counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<NativeLevelAssetsDocumentSaveSnapshot, NativeLevelAssetsDocumentControllerError>
    {
        if self.pending_save.is_some() {
            return Err(NativeLevelAssetsDocumentControllerError::SavePending);
        }
        let bytes = self
            .value
            .encode(&self.double_size_modes)
            .map_err(NativeLevelAssetsDocumentControllerError::File)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(NativeLevelAssetsDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(NativeLevelAssetsDocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Acknowledges the exact pending snapshot, retaining newer edits as dirty.
    ///
    /// # Errors
    ///
    /// Rejects a missing or mismatched pending request without losing it.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), NativeLevelAssetsDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(NativeLevelAssetsDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(NativeLevelAssetsDocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Cancels the exact failed save without changing the saved baseline.
    ///
    /// # Errors
    ///
    /// Rejects a missing or mismatched pending request.
    pub fn cancel_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), NativeLevelAssetsDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(NativeLevelAssetsDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(NativeLevelAssetsDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Debug)]
pub enum NativeLevelAssetsDocumentControllerError {
    SizeModeCount(usize),
    File(NativeLevelAssetsFileError),
    Edit(NativeLevelAssetsControllerError),
    NonCanonicalEncoding,
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}

impl fmt::Display for NativeLevelAssetsDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native level-assets document controller failed: {self:?}"
        )
    }
}

impl std::error::Error for NativeLevelAssetsDocumentControllerError {}

#[cfg(test)]
#[path = "native_level_assets_document_controller_tests.rs"]
mod tests;
