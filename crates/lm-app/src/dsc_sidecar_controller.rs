use crate::portable_value_history::PortableValueHistory;
use lm_level::{DscSidecar, DscSidecarError};
use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DscSidecarSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: DscSidecar,
}

/// Revisioned application ownership for Lunar Magic's read-only `.dsc` sidecar format.
#[derive(Clone, Debug)]
pub struct DscSidecarController {
    path: PathBuf,
    value: DscSidecar,
    saved: DscSidecar,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<DscSidecar>,
}

impl DscSidecarController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes a bounded `.dsc` document.
    ///
    /// # Errors
    ///
    /// Returns [`DscSidecarError`] when the source exceeds the format safety bound.
    pub fn decode(path: PathBuf, bytes: &[u8]) -> Result<Self, DscSidecarControllerError> {
        let value = DscSidecar::decode(bytes).map_err(DscSidecarControllerError::Sidecar)?;
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
    pub const fn value(&self) -> &DscSidecar {
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

    /// Replaces the complete lossless source as one atomic revision.
    ///
    /// This deliberately does not synthesize records: Lunar Magic has a recovered reader but no
    /// recovered writer for this ecosystem-owned text sidecar.
    ///
    /// # Errors
    ///
    /// Rejects a stale revision, oversized replacement, or revision exhaustion without mutation.
    pub fn replace_source(
        &mut self,
        expected_revision: u64,
        bytes: &[u8],
    ) -> Result<(), DscSidecarControllerError> {
        if expected_revision != self.revision {
            return Err(DscSidecarControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let staged = DscSidecar::decode(bytes).map_err(DscSidecarControllerError::Sidecar)?;
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DscSidecarControllerError::RevisionOverflow)?;
        self.history.record(self.value.clone());
        self.value = staged;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous lossless source as a new revision.
    ///
    /// # Errors
    /// Rejects stale revisions and revision overflow atomically.
    pub fn undo(&mut self, expected_revision: u64) -> Result<bool, DscSidecarControllerError> {
        self.navigate_history(expected_revision, true)
    }
    /// Reapplies the next reverted lossless source as a new revision.
    ///
    /// # Errors
    /// Rejects stale revisions and revision overflow atomically.
    pub fn redo(&mut self, expected_revision: u64) -> Result<bool, DscSidecarControllerError> {
        self.navigate_history(expected_revision, false)
    }
    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, DscSidecarControllerError> {
        if expected_revision != self.revision {
            return Err(DscSidecarControllerError::StaleRevision {
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
            .ok_or(DscSidecarControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Captures an immutable lossless save request.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves or request-counter exhaustion.
    pub fn begin_save(&mut self) -> Result<DscSidecarSaveSnapshot, DscSidecarControllerError> {
        if self.pending_save.is_some() {
            return Err(DscSidecarControllerError::SavePending);
        }
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(DscSidecarControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(DscSidecarSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes: self.value.encode_lossless(),
        })
    }

    /// Marks exactly the pending immutable snapshot as saved.
    ///
    /// # Errors
    ///
    /// Rejects missing or stale request IDs while preserving a mismatched pending request.
    pub fn acknowledge_save(&mut self, request_id: u64) -> Result<(), DscSidecarControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(DscSidecarControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(DscSidecarControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases exactly a failed or cancelled persistence request.
    ///
    /// # Errors
    ///
    /// Rejects missing or stale request IDs without changing the pending request.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), DscSidecarControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(DscSidecarControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(DscSidecarControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DscSidecarControllerError {
    Sidecar(DscSidecarError),
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}

impl fmt::Display for DscSidecarControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "DSC sidecar controller failed: {self:?}")
    }
}

impl std::error::Error for DscSidecarControllerError {}

#[cfg(test)]
#[path = "dsc_sidecar_controller_tests.rs"]
mod tests;
