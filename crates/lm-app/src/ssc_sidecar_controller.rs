use crate::portable_value_history::PortableValueHistory;
use lm_level::{SscSidecar, SscSidecarError};
use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SscSidecarSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: SscSidecar,
}

/// Revisioned ownership for Lunar Magic's ecosystem-authored `.ssc` source.
#[derive(Clone, Debug)]
pub struct SscSidecarController {
    path: PathBuf,
    value: SscSidecar,
    saved: SscSidecar,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<SscSidecar>,
}

impl SscSidecarController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes a bounded `.ssc` document.
    ///
    /// # Errors
    ///
    /// Returns [`SscSidecarError`] when the source exceeds its safety bound.
    pub fn decode(path: PathBuf, bytes: &[u8]) -> Result<Self, SscSidecarControllerError> {
        let value = SscSidecar::decode(bytes).map_err(SscSidecarControllerError::Sidecar)?;
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
    pub const fn value(&self) -> &SscSidecar {
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
    /// # Errors
    ///
    /// Rejects stale revisions, oversized input, and revision exhaustion without mutation.
    pub fn replace_source(
        &mut self,
        expected_revision: u64,
        bytes: &[u8],
    ) -> Result<(), SscSidecarControllerError> {
        if expected_revision != self.revision {
            return Err(SscSidecarControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let staged = SscSidecar::decode(bytes).map_err(SscSidecarControllerError::Sidecar)?;
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(SscSidecarControllerError::RevisionOverflow)?;
        self.history.record(self.value.clone());
        self.value = staged;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous source as a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow atomically.
    pub fn undo(&mut self, expected_revision: u64) -> Result<bool, SscSidecarControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted source as a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow atomically.
    pub fn redo(&mut self, expected_revision: u64) -> Result<bool, SscSidecarControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, SscSidecarControllerError> {
        if expected_revision != self.revision {
            return Err(SscSidecarControllerError::StaleRevision {
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
            .ok_or(SscSidecarControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Captures an immutable save request.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves and request-counter exhaustion.
    pub fn begin_save(&mut self) -> Result<SscSidecarSaveSnapshot, SscSidecarControllerError> {
        if self.pending_save.is_some() {
            return Err(SscSidecarControllerError::SavePending);
        }
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(SscSidecarControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(SscSidecarSaveSnapshot {
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
    /// Rejects missing and stale request IDs.
    pub fn acknowledge_save(&mut self, request_id: u64) -> Result<(), SscSidecarControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(SscSidecarControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(SscSidecarControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases exactly a failed persistence request.
    ///
    /// # Errors
    ///
    /// Rejects missing and stale request IDs without changing the pending request.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), SscSidecarControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(SscSidecarControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(SscSidecarControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SscSidecarControllerError {
    Sidecar(SscSidecarError),
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}

impl fmt::Display for SscSidecarControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SSC sidecar controller failed: {self:?}")
    }
}

impl std::error::Error for SscSidecarControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::MAX_SSC_SOURCE_LEN;

    #[test]
    fn revisions_reparse_atomically_and_history_is_monotonic() {
        let mut value = SscSidecarController::decode("test.ssc".into(), b"10\t0\told\n").unwrap();
        value
            .replace_source(0, b"10\t0\tnew\n11\t2\t0,0,10\n")
            .unwrap();
        assert_eq!(value.revision(), 1);
        assert_eq!(value.value().entries().len(), 2);
        assert!(value.is_modified());
        let before = value.value().clone();
        assert!(matches!(
            value.replace_source(0, b"20\t0\tstale\n"),
            Err(SscSidecarControllerError::StaleRevision { .. })
        ));
        assert_eq!(value.value(), &before);
        assert!(
            value
                .replace_source(1, &vec![0; MAX_SSC_SOURCE_LEN + 1])
                .is_err()
        );
        assert!(value.undo(1).unwrap());
        assert_eq!(value.revision(), 2);
        assert!(!value.is_modified());
        assert!(value.redo(2).unwrap());
        assert_eq!(value.revision(), 3);
    }

    #[test]
    fn immutable_save_acknowledges_only_its_snapshot() {
        let mut value = SscSidecarController::decode("test.ssc".into(), b"10\t0\tone\n").unwrap();
        value.replace_source(0, b"10\t0\ttwo\n").unwrap();
        let save = value.begin_save().unwrap();
        value.replace_source(1, b"10\t0\tthree\n").unwrap();
        assert_eq!(save.bytes, b"10\t0\ttwo\n");
        assert!(value.acknowledge_save(save.request_id + 1).is_err());
        assert!(value.save_pending());
        value.acknowledge_save(save.request_id).unwrap();
        assert!(value.is_modified());
        let retry = value.begin_save().unwrap();
        value.cancel_save(retry.request_id).unwrap();
        assert!(!value.save_pending());
    }
}
