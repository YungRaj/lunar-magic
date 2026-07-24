use crate::portable_value_history::PortableValueHistory;
use lm_overworld::{OverworldPathGraph, PathFileError, PathGraphEdit, PathGraphEditError};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldPathSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    graph: OverworldPathGraph,
}

/// Revisioned toolkit-neutral document controller for portable overworld path graphs.
#[derive(Clone, Debug)]
pub struct OverworldPathController {
    path: PathBuf,
    graph: OverworldPathGraph,
    saved: OverworldPathGraph,
    revision: u64,
    next_save_request: u64,
    require_reciprocal: bool,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<OverworldPathGraph>,
}

impl OverworldPathController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one complete `LMOWPATH` document.
    ///
    /// # Errors
    ///
    /// Returns a structured file error for malformed or structurally invalid graphs.
    pub fn decode(
        path: PathBuf,
        bytes: &[u8],
        require_reciprocal: bool,
    ) -> Result<Self, OverworldPathControllerError> {
        let graph =
            OverworldPathGraph::decode_file(bytes).map_err(OverworldPathControllerError::File)?;
        Ok(Self {
            path,
            saved: graph.clone(),
            graph,
            revision: 0,
            next_save_request: 0,
            require_reciprocal,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn graph(&self) -> &OverworldPathGraph {
        &self.graph
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.graph != self.saved
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

    /// Applies one exact-revision graph batch and validates the configured reciprocity policy.
    ///
    /// # Errors
    ///
    /// Returns stale-revision, graph-edit, or revision-overflow errors atomically.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[PathGraphEdit],
    ) -> Result<(), OverworldPathControllerError> {
        if expected_revision != self.revision {
            return Err(OverworldPathControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.graph.clone();
        staged
            .apply_edits(edits, self.require_reciprocal)
            .map_err(OverworldPathControllerError::Edit)?;
        if staged == self.graph {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(OverworldPathControllerError::RevisionOverflow)?;
        let bytes = staged
            .encode_file()
            .map_err(OverworldPathControllerError::File)?;
        let reopened =
            OverworldPathGraph::decode_file(&bytes).map_err(OverworldPathControllerError::File)?;
        if reopened != staged {
            return Err(OverworldPathControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.graph.clone());
        self.graph = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical graph as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(&mut self, expected_revision: u64) -> Result<bool, OverworldPathControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical graph as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(&mut self, expected_revision: u64) -> Result<bool, OverworldPathControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, OverworldPathControllerError> {
        if expected_revision != self.revision {
            return Err(OverworldPathControllerError::StaleRevision {
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
            .ok_or(OverworldPathControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.graph)
        } else {
            self.history.redo(&mut self.graph)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Reserves immutable canonical bytes for frontend persistence.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves or a graph that violates the configured reciprocity policy.
    pub fn begin_save(
        &mut self,
    ) -> Result<OverworldPathSaveSnapshot, OverworldPathControllerError> {
        if self.pending_save.is_some() {
            return Err(OverworldPathControllerError::SavePending);
        }
        self.graph
            .apply_edits(&[], self.require_reciprocal)
            .map_err(OverworldPathControllerError::Edit)?;
        let bytes = self
            .graph
            .encode_file()
            .map_err(OverworldPathControllerError::File)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(OverworldPathControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            graph: self.graph.clone(),
        });
        Ok(OverworldPathSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Acknowledges the exact pending snapshot and retains newer in-memory edits as dirty.
    ///
    /// # Errors
    ///
    /// Rejects missing or stale acknowledgements without discarding a mismatched snapshot.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), OverworldPathControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(OverworldPathControllerError::NoPendingSave)?;
        if request_id != pending.request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(OverworldPathControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.graph;
        Ok(())
    }

    /// Releases one failed persistence attempt without changing the saved baseline.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldPathControllerError::NoPendingSave`] when idle.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), OverworldPathControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(OverworldPathControllerError::NoPendingSave)?;
        if request_id != pending.request_id {
            return Err(OverworldPathControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldPathControllerError {
    File(PathFileError),
    Edit(PathGraphEditError),
    NonCanonicalEncoding,
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}

impl fmt::Display for OverworldPathControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "overworld path controller failed: {self:?}")
    }
}

impl std::error::Error for OverworldPathControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::{PathDirection, PathEdge, PathNode, Submap};

    fn graph() -> OverworldPathGraph {
        let mut edge = PathEdge {
            from: 1,
            to: 2,
            direction: PathDirection::Right,
            exit_index: None,
            raw_flags: 0,
        };
        edge.set_one_way(true);
        OverworldPathGraph {
            nodes: vec![
                PathNode {
                    id: 1,
                    x: 1,
                    y: 2,
                    submap: Submap::Main,
                    level: Some(0x105),
                    raw_flags: 0x80,
                },
                PathNode {
                    id: 2,
                    x: 3,
                    y: 4,
                    submap: Submap::Main,
                    level: None,
                    raw_flags: 0x40,
                },
            ],
            edges: vec![edge],
        }
    }

    #[test]
    fn edits_snapshot_and_newer_revision_remain_dirty() {
        let source = graph().encode_file().unwrap();
        let mut controller =
            OverworldPathController::decode("paths.lmowpath".into(), &source, true).unwrap();
        controller
            .apply_edits(
                0,
                &[PathGraphEdit::UpsertNode(PathNode {
                    x: 9,
                    ..graph().nodes[0]
                })],
            )
            .unwrap();
        let snapshot = controller.begin_save().unwrap();
        controller
            .apply_edits(
                1,
                &[PathGraphEdit::UpsertNode(PathNode {
                    y: 10,
                    ..graph().nodes[1]
                })],
            )
            .unwrap();
        controller.acknowledge_save(snapshot.request_id).unwrap();
        assert!(controller.is_modified());
    }

    #[test]
    fn stale_tokens_and_cancel_are_retryable() {
        let source = graph().encode_file().unwrap();
        let mut controller =
            OverworldPathController::decode("paths.lmowpath".into(), &source, true).unwrap();
        assert!(matches!(
            controller.apply_edits(1, &[]),
            Err(OverworldPathControllerError::StaleRevision { .. })
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
        assert!(controller.cancel_save(snapshot.request_id).is_err());
        assert!(controller.save_pending());
        controller.acknowledge_save(newer.request_id).unwrap();
        controller.next_save_request = u64::MAX;
        assert_eq!(
            controller.begin_save(),
            Err(OverworldPathControllerError::SaveRequestOverflow)
        );
    }

    #[test]
    fn history_restores_saved_graph_and_rejects_stale_or_divergent_navigation() {
        let source = graph().encode_file().unwrap();
        let mut controller =
            OverworldPathController::decode("paths.lmowpath".into(), &source, true).unwrap();
        assert!(!controller.undo(0).unwrap());
        controller
            .apply_edits(
                0,
                &[PathGraphEdit::UpsertNode(PathNode {
                    x: 9,
                    ..graph().nodes[0]
                })],
            )
            .unwrap();
        assert!(controller.undo(1).unwrap());
        assert!(!controller.is_modified());
        assert!(controller.redo(2).unwrap());
        assert_eq!(controller.graph().nodes[0].x, 9);
        assert!(controller.undo(3).unwrap());
        controller
            .apply_edits(
                4,
                &[PathGraphEdit::UpsertNode(PathNode {
                    y: 10,
                    ..graph().nodes[1]
                })],
            )
            .unwrap();
        assert!(!controller.can_redo());
        assert!(matches!(
            controller.undo(4),
            Err(OverworldPathControllerError::StaleRevision { .. })
        ));
    }
}
