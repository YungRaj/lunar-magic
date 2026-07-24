use crate::OverworldControllerEdit;
use crate::overworld_edit_batch::{
    OverworldEditBatchError, OverworldEditContext, apply_overworld_edit_batch,
};
use crate::portable_value_history::PortableValueHistory;
use lm_graphics::PaletteOwnership;
use lm_project::{CompleteOverworldFile, CompleteOverworldFileError};
use std::fmt;
use std::path::PathBuf;

mod save;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: CompleteOverworldFile,
}

/// Revisioned owner of one complete portable `LMOWFULL` overworld document.
#[derive(Clone, Debug)]
pub struct OverworldDocumentController {
    path: PathBuf,
    value: CompleteOverworldFile,
    saved: CompleteOverworldFile,
    maximum_animation_records: usize,
    double_size_modes: [bool; 256],
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<CompleteOverworldFile>,
}

impl OverworldDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes a bounded aggregate using one exact `ExAnimation` size-mode interpretation.
    ///
    /// # Errors
    ///
    /// Rejects a non-256-entry mode table or malformed complete-overworld bytes.
    pub fn decode(
        path: PathBuf,
        bytes: &[u8],
        maximum_animation_records: usize,
        double_size_modes: &[bool],
    ) -> Result<Self, OverworldDocumentControllerError> {
        let modes: [bool; 256] = double_size_modes.try_into().map_err(|_| {
            OverworldDocumentControllerError::SizeModeCount(double_size_modes.len())
        })?;
        let value = CompleteOverworldFile::decode(bytes, maximum_animation_records, &modes)
            .map_err(OverworldDocumentControllerError::File)?;
        Ok(Self {
            path,
            saved: value.clone(),
            value,
            maximum_animation_records,
            double_size_modes: modes,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn value(&self) -> &CompleteOverworldFile {
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

    /// Applies all nine-domain edits to a staged aggregate at one exact revision.
    ///
    /// The script slot must match the artifact's source slot. Palette ownership is validated even
    /// when the batch contains no palette command. The result must encode and reopen identically.
    ///
    /// # Errors
    ///
    /// Any stale revision, slot mismatch, invalid edit, encoding failure, or overflow is atomic.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        source_slot: usize,
        palette_ownership: &PaletteOwnership,
        edits: &[OverworldControllerEdit],
    ) -> Result<(), OverworldDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(OverworldDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        if source_slot != usize::from(self.value.source_slot) {
            return Err(OverworldDocumentControllerError::SourceSlotMismatch {
                expected: usize::from(self.value.source_slot),
                actual: source_slot,
            });
        }
        let mut staged = self.value.clone();
        apply_overworld_edit_batch(
            &mut staged.data,
            edits,
            &OverworldEditContext {
                sprite_record_len: staged.shape.sprite_record_len,
                maximum_animation_records: self.maximum_animation_records,
                double_size_modes: &self.double_size_modes,
                palette_ownership,
            },
        )
        .map_err(OverworldDocumentControllerError::Edit)?;
        if staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(OverworldDocumentControllerError::RevisionOverflow)?;
        let bytes = staged
            .encode(&self.double_size_modes)
            .map_err(OverworldDocumentControllerError::File)?;
        let reopened = CompleteOverworldFile::decode(
            &bytes,
            self.maximum_animation_records,
            &self.double_size_modes,
        )
        .map_err(OverworldDocumentControllerError::File)?;
        if reopened != staged {
            return Err(OverworldDocumentControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.value.clone());
        self.value = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical overworld as a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, OverworldDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical overworld as a new monotonic revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(
        &mut self,
        expected_revision: u64,
    ) -> Result<bool, OverworldDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, OverworldDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(OverworldDocumentControllerError::StaleRevision {
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
            .ok_or(OverworldDocumentControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }
}

#[derive(Debug)]
pub enum OverworldDocumentControllerError {
    SizeModeCount(usize),
    File(CompleteOverworldFileError),
    Edit(OverworldEditBatchError),
    SourceSlotMismatch { expected: usize, actual: usize },
    NonCanonicalEncoding,
    StaleRevision { expected: u64, actual: u64 },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave { expected: u64, actual: u64 },
}

impl fmt::Display for OverworldDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "complete overworld document controller failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldDocumentControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OverworldLayerId;
    use lm_graphics::{Bgr555, CompactExAnimation, Palette};
    use lm_overworld::{EventRevealTable, OverworldLayer};
    use lm_project::{CompleteOverworldData, CompleteOverworldShape, OverworldLayers};

    const MODES: [bool; 256] = [false; 256];

    fn file() -> CompleteOverworldFile {
        CompleteOverworldFile {
            source_slot: 2,
            shape: CompleteOverworldShape {
                width: 1,
                height: 1,
                event_reveals: 0,
                endpoints: 0,
                messages: 0,
                sprites: 0,
                sprite_record_len: 7,
                palette_colors: 16,
            },
            data: CompleteOverworldData {
                layers: OverworldLayers {
                    layer1: OverworldLayer::new(1, 1, vec![0]).unwrap(),
                    layer2: OverworldLayer::new(1, 1, vec![0]).unwrap(),
                },
                event_reveals: EventRevealTable { entries: vec![] },
                endpoints: vec![],
                messages: vec![],
                sprites: vec![],
                palette: Palette {
                    colors: vec![Bgr555(0); 16],
                },
                animation: CompactExAnimation {
                    setting: 0,
                    header_value: 0,
                    trigger_mask: 0,
                    trigger_values: [0; 16],
                    records: vec![],
                },
            },
        }
    }

    fn controller() -> OverworldDocumentController {
        let file = file();
        OverworldDocumentController::decode(
            "world.lmow".into(),
            &file.encode(&MODES).unwrap(),
            32,
            &MODES,
        )
        .unwrap()
    }

    fn tile(value: u16) -> OverworldControllerEdit {
        OverworldControllerEdit::SetLayerTile {
            layer: OverworldLayerId::Layer1,
            x: 0,
            y: 0,
            tile: value,
        }
    }

    #[test]
    fn mixed_batch_is_revisioned_atomic_and_bound_to_source_slot() {
        let ownership = PaletteOwnership::editable(16);
        let mut controller = controller();
        assert!(
            controller
                .apply_edits(0, 1, &ownership, &[tile(1)])
                .is_err()
        );
        assert!(
            controller
                .apply_edits(
                    0,
                    2,
                    &ownership,
                    &[
                        tile(1),
                        OverworldControllerEdit::SetLayerTile {
                            layer: OverworldLayerId::Layer2,
                            x: 9,
                            y: 0,
                            tile: 2,
                        },
                    ],
                )
                .is_err()
        );
        assert_eq!(controller.value().data.layers.layer1.tiles, [0]);
        controller
            .apply_edits(0, 2, &ownership, &[tile(1)])
            .unwrap();
        assert_eq!(controller.revision(), 1);
        assert_eq!(
            CompleteOverworldFile::decode(&controller.value().encode(&MODES).unwrap(), 32, &MODES)
                .unwrap(),
            *controller.value()
        );
    }

    #[test]
    fn immutable_save_acknowledgement_retains_newer_edits() {
        let ownership = PaletteOwnership::editable(16);
        let mut controller = controller();
        controller
            .apply_edits(0, 2, &ownership, &[tile(1)])
            .unwrap();
        let save = controller.begin_save().unwrap();
        controller
            .apply_edits(1, 2, &ownership, &[tile(2)])
            .unwrap();
        assert!(controller.acknowledge_save(save.request_id + 1).is_err());
        controller.acknowledge_save(save.request_id).unwrap();
        assert!(controller.is_modified());
        assert_eq!(
            CompleteOverworldFile::decode(&save.bytes, 32, &MODES)
                .unwrap()
                .data
                .layers
                .layer1
                .tiles,
            [1]
        );
    }

    #[test]
    fn history_tracks_saved_baseline_and_divergent_branches() {
        let ownership = PaletteOwnership::editable(16);
        let mut controller = controller();
        controller
            .apply_edits(0, 2, &ownership, &[tile(1)])
            .unwrap();
        let saved = controller.value().clone();
        let snapshot = controller.begin_save().unwrap();
        controller.acknowledge_save(snapshot.request_id).unwrap();
        controller
            .apply_edits(1, 2, &ownership, &[tile(2)])
            .unwrap();
        assert!(controller.undo(2).unwrap());
        assert_eq!(controller.revision(), 3);
        assert_eq!(controller.value(), &saved);
        assert!(!controller.is_modified());
        assert!(controller.redo(3).unwrap());
        assert_eq!(controller.value().data.layers.layer1.tiles, [2]);
        assert!(controller.undo(4).unwrap());
        controller
            .apply_edits(5, 2, &ownership, &[tile(3)])
            .unwrap();
        assert!(!controller.can_redo());
        assert!(controller.can_undo());
        assert!(matches!(
            controller.undo(5),
            Err(OverworldDocumentControllerError::StaleRevision { .. })
        ));
    }
}
