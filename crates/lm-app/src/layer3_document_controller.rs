use crate::portable_value_history::PortableValueHistory;
use lm_level::{Layer3Edit, Layer3EditError, Layer3Error, Layer3File, Level};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer3DocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: Layer3File,
}

/// Revisioned toolkit-neutral controller for one portable `LMLAY3V1` document.
#[derive(Clone, Debug)]
pub struct Layer3DocumentController {
    path: PathBuf,
    value: Layer3File,
    saved: Layer3File,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<Layer3File>,
}

impl Layer3DocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one exact bounded Layer 3 artifact.
    ///
    /// # Errors
    ///
    /// Returns [`Layer3DocumentControllerError`] for malformed or excessive input.
    pub fn decode(path: PathBuf, bytes: &[u8]) -> Result<Self, Layer3DocumentControllerError> {
        let value = Layer3File::decode(bytes).map_err(Layer3DocumentControllerError::File)?;
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
    pub const fn value(&self) -> &Layer3File {
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

    /// Applies one exact-revision Layer 3 edit batch without permitting document enable/disable.
    ///
    /// # Errors
    ///
    /// Returns stale-revision, unsupported transition, edit, or overflow errors atomically.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[Layer3Edit],
    ) -> Result<(), Layer3DocumentControllerError> {
        if expected_revision != self.revision {
            return Err(Layer3DocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        if edits
            .iter()
            .any(|edit| matches!(edit, Layer3Edit::Enable(_) | Layer3Edit::Disable))
        {
            return Err(Layer3DocumentControllerError::StateTransition);
        }
        let mut level = Level {
            layer3: Some(self.value.0.clone()),
            ..Level::default()
        };
        level
            .apply_layer3_edits(edits)
            .map_err(Layer3DocumentControllerError::Edit)?;
        let staged = Layer3File(
            level
                .layer3
                .ok_or(Layer3DocumentControllerError::StateTransition)?,
        );
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(Layer3DocumentControllerError::RevisionOverflow)?;
        let bytes = staged
            .encode()
            .map_err(Layer3DocumentControllerError::File)?;
        let reopened = Layer3File::decode(&bytes).map_err(Layer3DocumentControllerError::File)?;
        if reopened != staged {
            return Err(Layer3DocumentControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical Layer 3 value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(&mut self, expected_revision: u64) -> Result<bool, Layer3DocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical Layer 3 value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(&mut self, expected_revision: u64) -> Result<bool, Layer3DocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, Layer3DocumentControllerError> {
        if expected_revision != self.revision {
            return Err(Layer3DocumentControllerError::StaleRevision {
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
            .ok_or(Layer3DocumentControllerError::RevisionOverflow)?;
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
    /// Rejects overlapping saves or invalid programmatic data.
    pub fn begin_save(
        &mut self,
    ) -> Result<Layer3DocumentSaveSnapshot, Layer3DocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(Layer3DocumentControllerError::SavePending);
        }
        let bytes = self
            .value
            .encode()
            .map_err(Layer3DocumentControllerError::File)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(Layer3DocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(Layer3DocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Acknowledges the exact pending snapshot while retaining newer edits as dirty.
    ///
    /// # Errors
    ///
    /// Rejects missing or stale acknowledgements without discarding a mismatched snapshot.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), Layer3DocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(Layer3DocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(Layer3DocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases a failed save attempt without changing the dirty baseline.
    ///
    /// # Errors
    ///
    /// Returns [`Layer3DocumentControllerError::NoPendingSave`] when idle.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), Layer3DocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(Layer3DocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(Layer3DocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer3DocumentControllerError {
    File(Layer3Error),
    Edit(Layer3EditError),
    StateTransition,
    NonCanonicalEncoding,
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}

impl fmt::Display for Layer3DocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Layer 3 document controller failed: {self:?}")
    }
}

impl std::error::Error for Layer3DocumentControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{Layer3Data, Layer3Settings};

    fn controller() -> Layer3DocumentController {
        let file = Layer3File(Layer3Data {
            settings: Layer3Settings::default(),
            tilemap: vec![0; 16],
            remap_commands: vec![1, 2],
        });
        Layer3DocumentController::decode("layer3.lmlayer3".into(), &file.encode().unwrap()).unwrap()
    }

    #[test]
    fn exact_revision_edits_and_older_save_leave_newer_changes_dirty() {
        let mut controller = controller();
        controller
            .apply_edits(0, &[Layer3Edit::SetFlags(0x80)])
            .unwrap();
        let snapshot = controller.begin_save().unwrap();
        controller
            .apply_edits(1, &[Layer3Edit::SetStartPosition(3)])
            .unwrap();
        controller.acknowledge_save(snapshot.request_id).unwrap();
        assert!(controller.is_modified());
    }

    #[test]
    fn state_transitions_stale_tokens_and_cancel_are_safe() {
        let mut controller = controller();
        assert_eq!(
            controller.apply_edits(0, &[Layer3Edit::Disable]),
            Err(Layer3DocumentControllerError::StateTransition)
        );
        assert!(matches!(
            controller.apply_edits(1, &[]),
            Err(Layer3DocumentControllerError::StaleRevision { .. })
        ));
        let snapshot = controller.begin_save().unwrap();
        assert!(
            controller
                .acknowledge_save(snapshot.request_id + 1)
                .is_err()
        );
        controller.cancel_save(snapshot.request_id).unwrap();
        assert!(!controller.save_pending());
        let newer = controller.begin_save().unwrap();
        assert_ne!(newer.request_id, snapshot.request_id);
        assert!(controller.acknowledge_save(snapshot.request_id).is_err());
        assert!(controller.save_pending());
        controller.acknowledge_save(newer.request_id).unwrap();
        controller.next_save_request = u64::MAX;
        assert_eq!(
            controller.begin_save(),
            Err(Layer3DocumentControllerError::SaveRequestOverflow)
        );
    }

    #[test]
    fn history_restores_saved_baseline_and_invalidates_divergent_redo() {
        let mut controller = controller();
        controller
            .apply_edits(0, &[Layer3Edit::SetFlags(0x80)])
            .unwrap();
        assert!(controller.can_undo());
        assert!(!controller.can_redo());
        assert!(controller.undo(1).unwrap());
        assert_eq!(controller.revision(), 2);
        assert!(!controller.is_modified());
        assert!(controller.can_redo());
        assert!(controller.redo(2).unwrap());
        assert!(controller.is_modified());
        assert!(controller.undo(3).unwrap());
        controller
            .apply_edits(4, &[Layer3Edit::SetStartPosition(9)])
            .unwrap();
        assert!(!controller.can_redo());
        assert!(!controller.redo(5).unwrap());
        assert_eq!(controller.revision(), 5);
    }

    #[test]
    fn stale_history_tokens_and_empty_navigation_are_atomic() {
        let mut controller = controller();
        assert!(!controller.undo(0).unwrap());
        assert_eq!(controller.revision(), 0);
        controller
            .apply_edits(0, &[Layer3Edit::SetFlags(0x80)])
            .unwrap();
        let before = controller.value().clone();
        assert!(matches!(
            controller.undo(0),
            Err(Layer3DocumentControllerError::StaleRevision { .. })
        ));
        assert_eq!(controller.value(), &before);
        assert_eq!(controller.revision(), 1);
    }
}
