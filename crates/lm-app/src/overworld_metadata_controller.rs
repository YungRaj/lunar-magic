use crate::portable_value_history::PortableValueHistory;
use lm_overworld::{MetadataEdit, MetadataEditError, MetadataFileError, OverworldMetadata};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldMetadataSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    metadata: OverworldMetadata,
}

/// Toolkit-neutral document controller for portable lossless overworld metadata.
#[derive(Clone, Debug)]
pub struct OverworldMetadataController {
    path: PathBuf,
    metadata: OverworldMetadata,
    saved: OverworldMetadata,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<OverworldMetadata>,
}

impl OverworldMetadataController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one complete bounded `LMOWMETA` document.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldMetadataControllerError`] for malformed framing or metadata.
    pub fn decode(path: PathBuf, bytes: &[u8]) -> Result<Self, OverworldMetadataControllerError> {
        let metadata = OverworldMetadata::decode_file(bytes)
            .map_err(OverworldMetadataControllerError::Metadata)?;
        Ok(Self {
            path,
            saved: metadata.clone(),
            metadata,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn metadata(&self) -> &OverworldMetadata {
        &self.metadata
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.metadata != self.saved
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

    /// Applies one failure-atomic stable-key batch against an exact document revision.
    ///
    /// # Errors
    ///
    /// Returns a stale-revision, edit, or revision-overflow error without partial mutation.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[MetadataEdit],
    ) -> Result<(), OverworldMetadataControllerError> {
        if expected_revision != self.revision {
            return Err(OverworldMetadataControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.metadata.clone();
        staged
            .apply_edits(edits)
            .map_err(OverworldMetadataControllerError::Edit)?;
        if staged == self.metadata {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(OverworldMetadataControllerError::RevisionOverflow)?;
        let bytes = staged
            .encode_file()
            .map_err(OverworldMetadataControllerError::Metadata)?;
        let reopened = OverworldMetadata::decode_file(&bytes)
            .map_err(OverworldMetadataControllerError::Metadata)?;
        if reopened != staged {
            return Err(OverworldMetadataControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.metadata.clone());
        self.metadata = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical metadata value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, OverworldMetadataControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical metadata value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, OverworldMetadataControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, OverworldMetadataControllerError> {
        if expected_revision != self.revision {
            return Err(OverworldMetadataControllerError::StaleRevision {
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
            .ok_or(OverworldMetadataControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.metadata)
        } else {
            self.history.redo(&mut self.metadata)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Reserves one immutable save snapshot.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves or invalid programmatic metadata.
    pub fn begin_save(
        &mut self,
    ) -> Result<OverworldMetadataSaveSnapshot, OverworldMetadataControllerError> {
        if self.pending_save.is_some() {
            return Err(OverworldMetadataControllerError::SavePending);
        }
        let bytes = self
            .metadata
            .encode_file()
            .map_err(OverworldMetadataControllerError::Metadata)?;
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(OverworldMetadataControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            metadata: self.metadata.clone(),
        });
        Ok(OverworldMetadataSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Marks the exact pending snapshot as persisted.
    ///
    /// # Errors
    ///
    /// Rejects missing or stale acknowledgements while retaining a mismatched pending snapshot.
    pub fn acknowledge_save(
        &mut self,
        request_id: u64,
    ) -> Result<(), OverworldMetadataControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(OverworldMetadataControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(OverworldMetadataControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.metadata;
        Ok(())
    }

    /// Releases a failed persistence attempt without changing the dirty baseline.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldMetadataControllerError::NoPendingSave`] if no save is active.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), OverworldMetadataControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(OverworldMetadataControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(OverworldMetadataControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldMetadataControllerError {
    Metadata(MetadataFileError),
    Edit(MetadataEditError),
    NonCanonicalEncoding,
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}

impl fmt::Display for OverworldMetadataControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "overworld metadata controller failed: {self:?}")
    }
}

impl std::error::Error for OverworldMetadataControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::{OverworldLevelName, PlayerStart, Submap};

    fn controller() -> OverworldMetadataController {
        let value = OverworldMetadata {
            level_names: vec![OverworldLevelName {
                level: 1,
                tiles: [2; OverworldLevelName::TILE_COUNT],
                raw_flags: 0x80,
            }],
            ..OverworldMetadata::default()
        };
        OverworldMetadataController::decode(
            "metadata.lmowmeta".into(),
            &value.encode_file().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn revisioned_edit_and_snapshot_acknowledgement_retain_newer_edits() {
        let mut controller = controller();
        controller
            .apply_edits(0, &[MetadataEdit::RemoveLevelName(1)])
            .unwrap();
        let snapshot = controller.begin_save().unwrap();
        controller
            .apply_edits(
                1,
                &[MetadataEdit::UpsertPlayerStart(PlayerStart {
                    player: 0,
                    x: 1,
                    y: 2,
                    submap: Submap::Main,
                    raw_flags: 0x80,
                })],
            )
            .unwrap();
        controller.acknowledge_save(snapshot.request_id).unwrap();
        assert!(controller.is_modified());
    }

    #[test]
    fn stale_edits_and_save_tokens_are_retryable() {
        let mut controller = controller();
        assert!(matches!(
            controller.apply_edits(1, &[]),
            Err(OverworldMetadataControllerError::StaleRevision { .. })
        ));
        let snapshot = controller.begin_save().unwrap();
        assert!(matches!(
            controller.acknowledge_save(snapshot.request_id + 1),
            Err(OverworldMetadataControllerError::StaleSave { .. })
        ));
        assert!(controller.save_pending());
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
            Err(OverworldMetadataControllerError::SaveRequestOverflow)
        );
    }

    #[test]
    fn history_restores_saved_metadata_and_rejects_stale_or_divergent_navigation() {
        let mut controller = controller();
        assert!(!controller.undo(0).unwrap());
        controller
            .apply_edits(0, &[MetadataEdit::RemoveLevelName(1)])
            .unwrap();
        assert!(controller.undo(1).unwrap());
        assert!(!controller.is_modified());
        assert!(controller.redo(2).unwrap());
        assert!(controller.metadata().level_names.is_empty());
        assert!(controller.undo(3).unwrap());
        controller
            .apply_edits(
                4,
                &[MetadataEdit::UpsertPlayerStart(PlayerStart {
                    player: 0,
                    x: 1,
                    y: 2,
                    submap: Submap::Main,
                    raw_flags: 0x80,
                })],
            )
            .unwrap();
        assert!(!controller.can_redo());
        assert!(matches!(
            controller.undo(4),
            Err(OverworldMetadataControllerError::StaleRevision { .. })
        ));
    }
}
