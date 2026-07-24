use crate::GraphicsControllerEdit;
use crate::graphics_edit_batch::apply_graphics_edit_batch;
use crate::portable_value_history::PortableValueHistory;
use lm_graphics::{
    GraphicsEditError, GraphicsInterchangeError, GraphicsInterchangeFile, GraphicsOwnership,
};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: GraphicsInterchangeFile,
}

#[derive(Clone, Debug)]
pub struct GraphicsDocumentController {
    path: PathBuf,
    value: GraphicsInterchangeFile,
    saved: GraphicsInterchangeFile,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<GraphicsInterchangeFile>,
}

impl GraphicsDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one exact bounded portable graphics file.
    ///
    /// # Errors
    ///
    /// Returns an interchange error for malformed framing, tile data, or limits.
    pub fn decode(path: PathBuf, bytes: &[u8]) -> Result<Self, GraphicsDocumentControllerError> {
        let value = GraphicsInterchangeFile::decode(bytes)
            .map_err(GraphicsDocumentControllerError::File)?;
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
    pub const fn value(&self) -> &GraphicsInterchangeFile {
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

    /// Applies an ownership-aware batch against one exact revision and canonically reopens it.
    ///
    /// # Errors
    ///
    /// Any stale revision, ownership mismatch, invalid tile, encoding failure, or overflow is
    /// atomic and preserves the controller.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        ownership: &GraphicsOwnership,
        edits: &[GraphicsControllerEdit],
    ) -> Result<(), GraphicsDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(GraphicsDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        apply_graphics_edit_batch(&mut staged.graphics, ownership, edits)
            .map_err(|(command, error)| GraphicsDocumentControllerError::Edit { command, error })?;
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(GraphicsDocumentControllerError::RevisionOverflow)?;
        let bytes = staged
            .encode()
            .map_err(GraphicsDocumentControllerError::File)?;
        let reopened = GraphicsInterchangeFile::decode(&bytes)
            .map_err(GraphicsDocumentControllerError::File)?;
        if reopened != staged {
            return Err(GraphicsDocumentControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical graphics value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, GraphicsDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical graphics value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, GraphicsDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, GraphicsDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(GraphicsDocumentControllerError::StaleRevision {
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
            .ok_or(GraphicsDocumentControllerError::RevisionOverflow)?;
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
    /// Rejects overlapping saves, invalid data, and request-counter overflow.
    pub fn begin_save(
        &mut self,
    ) -> Result<GraphicsDocumentSaveSnapshot, GraphicsDocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(GraphicsDocumentControllerError::SavePending);
        }
        let bytes = self
            .value
            .encode()
            .map_err(GraphicsDocumentControllerError::File)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(GraphicsDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(GraphicsDocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Acknowledges only the exact immutable pending snapshot.
    ///
    /// # Errors
    ///
    /// Missing or mismatched requests preserve retryable state.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), GraphicsDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(GraphicsDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(GraphicsDocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases the exact failed save without changing the saved baseline.
    ///
    /// # Errors
    ///
    /// Missing or mismatched requests preserve the pending save.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), GraphicsDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(GraphicsDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(GraphicsDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Debug)]
pub enum GraphicsDocumentControllerError {
    File(GraphicsInterchangeError),
    Edit {
        command: usize,
        error: GraphicsEditError,
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

impl fmt::Display for GraphicsDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "graphics document controller failed: {self:?}")
    }
}

impl std::error::Error for GraphicsDocumentControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{GraphicsFile4bpp, GraphicsTileChange, IndexedTile};

    fn controller() -> GraphicsDocumentController {
        let file = GraphicsInterchangeFile {
            source_slot: 2,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([0; 64]), IndexedTile::new([1; 64])],
            },
        };
        GraphicsDocumentController::decode("gfx.lmgfx".into(), &file.encode().unwrap()).unwrap()
    }

    fn edit(value: u8) -> GraphicsControllerEdit {
        GraphicsControllerEdit::ApplyChanges(vec![GraphicsTileChange {
            index: 0,
            tile: IndexedTile::new([value; 64]),
        }])
    }

    #[test]
    fn edits_are_revisioned_ownership_checked_and_atomic() {
        let mut controller = controller();
        assert!(
            controller
                .apply_edits(0, &GraphicsOwnership::editable(1), &[edit(2)])
                .is_err()
        );
        assert_eq!(controller.revision(), 0);
        controller
            .apply_edits(0, &GraphicsOwnership::editable(2), &[edit(2)])
            .unwrap();
        assert_eq!(controller.revision(), 1);
        assert!(
            controller
                .apply_edits(0, &GraphicsOwnership::editable(2), &[edit(3)])
                .is_err()
        );
    }

    #[test]
    fn immutable_save_snapshot_retains_newer_dirty_revision() {
        let ownership = GraphicsOwnership::editable(2);
        let mut controller = controller();
        controller.apply_edits(0, &ownership, &[edit(2)]).unwrap();
        let save = controller.begin_save().unwrap();
        controller.apply_edits(1, &ownership, &[edit(3)]).unwrap();
        controller.acknowledge_save(save.request_id).unwrap();
        assert!(controller.is_modified());
        assert_eq!(
            GraphicsInterchangeFile::decode(&save.bytes)
                .unwrap()
                .graphics
                .tiles[0],
            IndexedTile::new([2; 64])
        );
    }

    #[test]
    fn history_restores_saved_graphics_and_clears_divergent_redo() {
        let ownership = GraphicsOwnership::editable(2);
        let mut controller = controller();
        controller.apply_edits(0, &ownership, &[edit(2)]).unwrap();
        let snapshot = controller.begin_save().unwrap();
        controller.acknowledge_save(snapshot.request_id).unwrap();
        controller.apply_edits(1, &ownership, &[edit(3)]).unwrap();
        assert!(controller.undo(2).unwrap());
        assert!(!controller.is_modified());
        assert!(controller.redo(3).unwrap());
        assert!(controller.undo(4).unwrap());
        controller.apply_edits(5, &ownership, &[edit(4)]).unwrap();
        assert!(!controller.can_redo());
        assert!(matches!(
            controller.undo(5),
            Err(GraphicsDocumentControllerError::StaleRevision { .. })
        ));
    }
}
