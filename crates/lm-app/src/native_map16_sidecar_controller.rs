use crate::portable_value_history::PortableValueHistory;
use lm_level::{M16Sidecar, Map16Tile, NativeMap16SidecarError, S16Sidecar};
use std::{fmt, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeMap16SidecarDocumentKind {
    M16,
    S16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeMap16SidecarDocument {
    M16(M16Sidecar),
    S16(S16Sidecar),
}

impl NativeMap16SidecarDocument {
    #[must_use]
    pub const fn kind(&self) -> NativeMap16SidecarDocumentKind {
        match self {
            Self::M16(_) => NativeMap16SidecarDocumentKind::M16,
            Self::S16(_) => NativeMap16SidecarDocumentKind::S16,
        }
    }

    #[must_use]
    pub const fn entry_count(&self) -> usize {
        match self {
            Self::M16(_) => M16Sidecar::ENTRY_COUNT,
            Self::S16(_) => S16Sidecar::ENTRY_COUNT,
        }
    }

    #[must_use]
    pub fn entry(&self, index: usize) -> Option<u32> {
        match self {
            Self::M16(value) => value.entry(index),
            Self::S16(value) => value.entry(index),
        }
    }

    #[must_use]
    pub fn tile(&self, index: usize) -> Option<Map16Tile> {
        match self {
            Self::M16(value) => value.tile(index),
            Self::S16(value) => value.tile(index),
        }
    }

    #[must_use]
    pub const fn tile_count(&self) -> usize {
        match self {
            Self::M16(_) => M16Sidecar::TILE_COUNT,
            Self::S16(_) => S16Sidecar::TILE_COUNT,
        }
    }

    fn set_entry(&mut self, index: usize, value: u32) -> Result<(), NativeMap16SidecarError> {
        match self {
            Self::M16(sidecar) => sidecar.set_entry(index, value),
            Self::S16(sidecar) => sidecar.set_entry(index, value),
        }
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::M16(value) => value.encode(),
            Self::S16(value) => value.encode_canonical(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeMap16SidecarEdit {
    pub entry: usize,
    pub value: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeMap16SidecarSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: NativeMap16SidecarDocument,
}

#[derive(Clone, Debug)]
pub struct NativeMap16SidecarController {
    path: PathBuf,
    value: NativeMap16SidecarDocument,
    saved: NativeMap16SidecarDocument,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<NativeMap16SidecarDocument>,
}

impl NativeMap16SidecarController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one native sidecar under an explicit file-kind interpretation.
    ///
    /// # Errors
    ///
    /// Rejects wrong `.m16` length or `.s16` data beyond its recovered capacity.
    pub fn decode(
        path: PathBuf,
        kind: NativeMap16SidecarDocumentKind,
        bytes: &[u8],
    ) -> Result<Self, NativeMap16SidecarControllerError> {
        let value = match kind {
            NativeMap16SidecarDocumentKind::M16 => NativeMap16SidecarDocument::M16(
                M16Sidecar::decode(bytes).map_err(NativeMap16SidecarControllerError::Sidecar)?,
            ),
            NativeMap16SidecarDocumentKind::S16 => NativeMap16SidecarDocument::S16(
                S16Sidecar::decode(bytes).map_err(NativeMap16SidecarControllerError::Sidecar)?,
            ),
        };
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
    pub const fn value(&self) -> &NativeMap16SidecarDocument {
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

    /// Applies an ordered raw-entry batch to a staged clone as one revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, invalid entry indexes, or revision exhaustion without mutation.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[NativeMap16SidecarEdit],
    ) -> Result<(), NativeMap16SidecarControllerError> {
        if expected_revision != self.revision {
            return Err(NativeMap16SidecarControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        for (command, edit) in edits.iter().enumerate() {
            staged
                .set_entry(edit.entry, edit.value)
                .map_err(|error| NativeMap16SidecarControllerError::Edit { command, error })?;
        }
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(NativeMap16SidecarControllerError::RevisionOverflow)?;
        let kind = staged.kind();
        let reopened = match kind {
            NativeMap16SidecarDocumentKind::M16 => NativeMap16SidecarDocument::M16(
                M16Sidecar::decode(&staged.encode())
                    .map_err(NativeMap16SidecarControllerError::Sidecar)?,
            ),
            NativeMap16SidecarDocumentKind::S16 => NativeMap16SidecarDocument::S16(
                S16Sidecar::decode(&staged.encode())
                    .map_err(NativeMap16SidecarControllerError::Sidecar)?,
            ),
        };
        if reopened.encode() != staged.encode() {
            return Err(NativeMap16SidecarControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical sidecar value without changing its kind interpretation.
    ///
    /// # Errors
    /// Rejects stale revisions and revision overflow atomically.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, NativeMap16SidecarControllerError> {
        self.navigate_history(expected_revision, true)
    }
    /// Reapplies the next reverted canonical sidecar value as a new revision.
    ///
    /// # Errors
    /// Rejects stale revisions and revision overflow atomically.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, NativeMap16SidecarControllerError> {
        self.navigate_history(expected_revision, false)
    }
    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, NativeMap16SidecarControllerError> {
        if expected_revision != self.revision {
            return Err(NativeMap16SidecarControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
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
            .ok_or(NativeMap16SidecarControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Captures one immutable exact/canonical save snapshot.
    ///
    /// # Errors
    ///
    /// Rejects an overlapping save or request-counter exhaustion.
    pub fn begin_save(
        &mut self,
    ) -> Result<NativeMap16SidecarSaveSnapshot, NativeMap16SidecarControllerError> {
        if self.pending_save.is_some() {
            return Err(NativeMap16SidecarControllerError::SavePending);
        }
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(NativeMap16SidecarControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(NativeMap16SidecarSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes: self.value.encode(),
        })
    }

    /// Acknowledges the exact pending save snapshot.
    ///
    /// # Errors
    ///
    /// Rejects missing or stale tokens while retaining a mismatched pending save.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), NativeMap16SidecarControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(NativeMap16SidecarControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(NativeMap16SidecarControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases an exact failed/cancelled persistence request.
    ///
    /// # Errors
    ///
    /// Rejects missing or stale tokens without changing the pending request.
    pub fn cancel_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), NativeMap16SidecarControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(NativeMap16SidecarControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(NativeMap16SidecarControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeMap16SidecarControllerError {
    Sidecar(NativeMap16SidecarError),
    Edit {
        command: usize,
        error: NativeMap16SidecarError,
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

impl fmt::Display for NativeMap16SidecarControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native Map16 sidecar controller failed: {self:?}"
        )
    }
}

impl std::error::Error for NativeMap16SidecarControllerError {}

#[cfg(test)]
#[path = "native_map16_sidecar_controller_tests.rs"]
mod tests;
