use crate::portable_value_history::PortableValueHistory;
use lm_level::{OscSidecar, OscSidecarError};
use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OscSidecarSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: OscSidecar,
}

#[derive(Clone, Debug)]
pub struct OscSidecarController {
    path: PathBuf,
    value: OscSidecar,
    saved: OscSidecar,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<OscSidecar>,
}

impl OscSidecarController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one bounded `.osc` document.
    ///
    /// # Errors
    ///
    /// Returns [`OscSidecarError`] when the source exceeds its safety bound.
    pub fn decode(path: PathBuf, bytes: &[u8]) -> Result<Self, OscSidecarControllerError> {
        let value = OscSidecar::decode(bytes).map_err(OscSidecarControllerError::Sidecar)?;
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
    pub const fn value(&self) -> &OscSidecar {
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
    /// Rejects stale revisions, oversized source, and revision exhaustion.
    pub fn replace_source(
        &mut self,
        expected_revision: u64,
        bytes: &[u8],
    ) -> Result<(), OscSidecarControllerError> {
        if expected_revision != self.revision {
            return Err(OscSidecarControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let staged = OscSidecar::decode(bytes).map_err(OscSidecarControllerError::Sidecar)?;
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(OscSidecarControllerError::RevisionOverflow)?;
        self.history.record(self.value.clone());
        self.value = staged;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous source as a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision exhaustion.
    pub fn undo(&mut self, expected_revision: u64) -> Result<bool, OscSidecarControllerError> {
        self.navigate(expected_revision, true)
    }

    /// Reapplies the next reverted source as a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision exhaustion.
    pub fn redo(&mut self, expected_revision: u64) -> Result<bool, OscSidecarControllerError> {
        self.navigate(expected_revision, false)
    }

    fn navigate(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, OscSidecarControllerError> {
        if expected_revision != self.revision {
            return Err(OscSidecarControllerError::StaleRevision {
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
            .ok_or(OscSidecarControllerError::RevisionOverflow)?;
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
    pub fn begin_save(&mut self) -> Result<OscSidecarSaveSnapshot, OscSidecarControllerError> {
        if self.pending_save.is_some() {
            return Err(OscSidecarControllerError::SavePending);
        }
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(OscSidecarControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(OscSidecarSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes: self.value.encode_lossless(),
        })
    }

    /// Marks exactly the pending snapshot as saved.
    ///
    /// # Errors
    ///
    /// Rejects missing or stale request IDs.
    pub fn acknowledge_save(&mut self, request_id: u64) -> Result<(), OscSidecarControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(OscSidecarControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(OscSidecarControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases exactly one failed pending save.
    ///
    /// # Errors
    ///
    /// Rejects missing or stale request IDs without losing the pending snapshot.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), OscSidecarControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(OscSidecarControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(OscSidecarControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OscSidecarControllerError {
    Sidecar(OscSidecarError),
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}

impl fmt::Display for OscSidecarControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "OSC sidecar controller failed: {self:?}")
    }
}

impl std::error::Error for OscSidecarControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::MAX_OSC_SOURCE_LEN;

    #[test]
    fn replacement_history_and_limits_are_atomic() {
        let mut value =
            OscSidecarController::decode("test.osc".into(), b"10\t2\t0\told\n").unwrap();
        value
            .replace_source(0, b"10\t2\t0\tnew\n10\t2\t2\t0,0,10\n")
            .unwrap();
        assert_eq!(value.revision(), 1);
        assert_eq!(value.value().entries().len(), 2);
        let before = value.value().clone();
        assert!(matches!(
            value.replace_source(0, b"1\t2\t0\tstale\n"),
            Err(OscSidecarControllerError::StaleRevision { .. })
        ));
        assert_eq!(value.value(), &before);
        assert!(
            value
                .replace_source(1, &vec![0; MAX_OSC_SOURCE_LEN + 1])
                .is_err()
        );
        assert!(value.undo(1).unwrap());
        assert_eq!(value.revision(), 2);
        assert!(!value.is_modified());
        assert!(value.redo(2).unwrap());
    }

    #[test]
    fn immutable_save_retains_later_edits() {
        let mut value =
            OscSidecarController::decode("test.osc".into(), b"10\t2\t0\tone\n").unwrap();
        value.replace_source(0, b"10\t2\t0\ttwo\n").unwrap();
        let save = value.begin_save().unwrap();
        value.replace_source(1, b"10\t2\t0\tthree\n").unwrap();
        assert_eq!(save.bytes, b"10\t2\t0\ttwo\n");
        assert!(value.acknowledge_save(save.request_id + 1).is_err());
        assert!(value.save_pending());
        value.acknowledge_save(save.request_id).unwrap();
        assert!(value.is_modified());
    }
}
