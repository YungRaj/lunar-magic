use crate::portable_value_history::PortableValueHistory;
use lm_level::{CustomObjectLibrary, CustomObjectLibraryError};
use std::fmt;
use std::path::PathBuf;

mod edit;
mod save;

pub use edit::CustomObjectLibraryEdit;

/// Immutable paired bytes handed to a native frontend for all-or-nothing persistence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomObjectSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub data_path: PathBuf,
    pub descriptions_path: PathBuf,
    pub data: Vec<u8>,
    pub descriptions: Vec<u8>,
}

/// Toolkit-neutral document controller for Lunar Magic's paired custom-object sidecars.
#[derive(Clone, Debug)]
pub struct CustomObjectLibraryController {
    data_path: PathBuf,
    descriptions_path: PathBuf,
    library: CustomObjectLibrary,
    saved: CustomObjectLibrary,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingCustomObjectSave>,
    history: PortableValueHistory<CustomObjectLibrary>,
}

#[derive(Clone, Debug)]
struct PendingCustomObjectSave {
    request_id: u64,
    library: CustomObjectLibrary,
}

impl CustomObjectLibraryController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes a pair supplied by a frontend after its file-selection/read workflow.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectControllerError`] for aliased paths or malformed sidecar bytes.
    pub fn decode(
        data_path: PathBuf,
        descriptions_path: PathBuf,
        data: &[u8],
        descriptions: &[u8],
    ) -> Result<Self, CustomObjectControllerError> {
        if data_path == descriptions_path {
            return Err(CustomObjectControllerError::AliasedPaths);
        }
        let library = CustomObjectLibrary::decode(data, descriptions)
            .map_err(CustomObjectControllerError::Library)?;
        Ok(Self {
            data_path,
            descriptions_path,
            saved: library.clone(),
            library,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn library(&self) -> &CustomObjectLibrary {
        &self.library
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.library != self.saved
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

    /// Applies an ordered edit batch to a staged clone and advances one logical revision.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectControllerError`] for a stale caller revision, revision exhaustion,
    /// or the indexed failing library edit. No partial edit is retained.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[CustomObjectLibraryEdit],
    ) -> Result<(), CustomObjectControllerError> {
        self.require_revision(expected_revision)?;
        let mut staged = self.library.clone();
        for (command, edit) in edits.iter().enumerate() {
            let result = edit::apply_edit(&mut staged, edit);
            result.map_err(|error| CustomObjectControllerError::Edit { command, error })?;
        }
        if staged == self.library {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(CustomObjectControllerError::RevisionOverflow)?;
        // Encoding is the final cross-sidecar invariant and size check before publication in state.
        let (data, descriptions) =
            staged
                .encode()
                .map_err(|error| CustomObjectControllerError::Edit {
                    command: edits.len(),
                    error,
                })?;
        let reopened = CustomObjectLibrary::decode(&data, &descriptions)
            .map_err(CustomObjectControllerError::Library)?;
        if reopened != staged {
            return Err(CustomObjectControllerError::NonCanonicalEncoding);
        }
        self.history.record(self.library.clone());
        self.library = reopened;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous canonical paired library as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(&mut self, expected_revision: u64) -> Result<bool, CustomObjectControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted canonical paired library as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(&mut self, expected_revision: u64) -> Result<bool, CustomObjectControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, CustomObjectControllerError> {
        self.require_revision(expected_revision)?;
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
            .ok_or(CustomObjectControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.library)
        } else {
            self.history.redo(&mut self.library)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    fn require_revision(&self, expected: u64) -> Result<(), CustomObjectControllerError> {
        if expected == self.revision {
            Ok(())
        } else {
            Err(CustomObjectControllerError::StaleRevision {
                expected,
                actual: self.revision,
            })
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomObjectControllerError {
    AliasedPaths,
    Library(CustomObjectLibraryError),
    NonCanonicalEncoding,
    Edit {
        command: usize,
        error: CustomObjectLibraryError,
    },
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

impl fmt::Display for CustomObjectControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "custom-object library controller failed: {self:?}"
        )
    }
}

impl std::error::Error for CustomObjectControllerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{CustomObjectEntry, DescriptionFormat, ObjectRecord};

    fn controller() -> CustomObjectLibraryController {
        CustomObjectLibraryController::decode(
            "objects.mw0".into(),
            "objects.mw0t".into(),
            &[0, 0, 0, 0, 0, 1, 0, 3, 0xff],
            b"Original\n",
        )
        .unwrap()
    }

    fn entry(bytes: &[u8], description: &str) -> CustomObjectEntry {
        CustomObjectEntry::new(
            ObjectRecord::new(bytes.to_vec()).unwrap(),
            description.into(),
        )
        .unwrap()
    }

    #[test]
    fn mixed_edit_batch_is_atomic_revisioned_and_saveable() {
        let mut controller = controller();
        controller
            .apply_edits(
                0,
                &[
                    CustomObjectLibraryEdit::Insert {
                        index: 1,
                        entry: entry(&[2, 8, 4], "Second"),
                    },
                    CustomObjectLibraryEdit::Move { from: 1, to: 0 },
                ],
            )
            .unwrap();
        assert_eq!(controller.revision(), 1);
        assert!(controller.is_modified());
        assert_eq!(controller.library().entries()[0].description, "Second");
        let save = controller.begin_save().unwrap();
        assert_eq!(save.revision, 1);
        assert_eq!(save.data, [0, 0, 0, 0, 0, 2, 8, 4, 0x81, 0, 3, 0xff]);
        controller.acknowledge_save(save.request_id).unwrap();
        assert!(!controller.is_modified());
    }

    #[test]
    fn late_failure_and_stale_revision_preserve_everything() {
        let mut controller = controller();
        let before = controller.library().clone();
        assert!(matches!(
            controller.apply_edits(
                0,
                &[
                    CustomObjectLibraryEdit::Insert {
                        index: 1,
                        entry: entry(&[2, 8, 4], "Second"),
                    },
                    CustomObjectLibraryEdit::Remove { index: 9 },
                ]
            ),
            Err(CustomObjectControllerError::Edit { command: 1, .. })
        ));
        assert_eq!(controller.library(), &before);
        assert_eq!(controller.revision(), 0);
        assert!(matches!(
            controller.apply_edits(4, &[]),
            Err(CustomObjectControllerError::StaleRevision { .. })
        ));
    }

    #[test]
    fn pending_snapshot_survives_new_edits_and_bad_acknowledgement_is_retryable() {
        let mut controller = controller();
        let snapshot = controller.begin_save().unwrap();
        controller
            .apply_edits(
                0,
                &[CustomObjectLibraryEdit::Replace {
                    index: 0,
                    entry: entry(&[2, 8, 4], "Changed"),
                }],
            )
            .unwrap();
        assert!(matches!(
            controller.acknowledge_save(snapshot.request_id + 1),
            Err(CustomObjectControllerError::StaleSave { .. })
        ));
        assert!(controller.save_pending());
        controller.acknowledge_save(snapshot.request_id).unwrap();
        assert!(controller.is_modified());
    }

    #[test]
    fn cancel_releases_save_slot_without_changing_dirty_baseline() {
        let mut controller = controller();
        controller
            .apply_edits(
                0,
                &[CustomObjectLibraryEdit::Replace {
                    index: 0,
                    entry: entry(&[2, 8, 4], "Changed"),
                }],
            )
            .unwrap();
        let snapshot = controller.begin_save().unwrap();
        controller.cancel_save(snapshot.request_id).unwrap();
        assert!(controller.is_modified());
        assert!(!controller.save_pending());
        assert_eq!(
            controller.cancel_save(snapshot.request_id),
            Err(CustomObjectControllerError::NoPendingSave)
        );
        let newer = controller.begin_save().unwrap();
        assert_ne!(newer.request_id, snapshot.request_id);
        assert!(controller.cancel_save(snapshot.request_id).is_err());
        assert!(controller.save_pending());
        controller.acknowledge_save(newer.request_id).unwrap();
        controller.next_save_request = u64::MAX;
        assert_eq!(
            controller.begin_save(),
            Err(CustomObjectControllerError::SaveRequestOverflow)
        );
    }

    #[test]
    fn aliases_no_ops_and_revision_exhaustion_are_safe() {
        assert!(matches!(
            CustomObjectLibraryController::decode("same".into(), "same".into(), &[0xff], b""),
            Err(CustomObjectControllerError::AliasedPaths)
        ));
        let mut controller = controller();
        controller.apply_edits(0, &[]).unwrap();
        assert_eq!(controller.revision(), 0);
        let before = controller.library().clone();
        controller.revision = u64::MAX;
        assert_eq!(
            controller.apply_edits(
                u64::MAX,
                &[CustomObjectLibraryEdit::Replace {
                    index: 0,
                    entry: entry(&[2, 8, 4], "Changed"),
                }]
            ),
            Err(CustomObjectControllerError::RevisionOverflow)
        );
        assert_eq!(controller.library(), &before);
    }

    #[test]
    fn paired_history_restores_records_descriptions_and_text_framing() {
        let mut controller = controller();
        let original = controller.library().clone();
        controller
            .apply_edits(
                0,
                &[
                    CustomObjectLibraryEdit::Replace {
                        index: 0,
                        entry: entry(&[2, 8, 4], "Changed"),
                    },
                    CustomObjectLibraryEdit::SetDescriptionFormat(DescriptionFormat {
                        utf8_bom: true,
                        line_ending: lm_level::LineEnding::CrLf,
                        trailing_line_ending: true,
                    }),
                ],
            )
            .unwrap();
        assert!(controller.undo(1).unwrap());
        assert_eq!(controller.library(), &original);
        assert!(!controller.is_modified());
        assert!(controller.redo(2).unwrap());
        assert_eq!(controller.library().entries()[0].description, "Changed");
        assert!(controller.library().description_format().utf8_bom);
        assert!(controller.undo(3).unwrap());
        controller
            .apply_edits(
                4,
                &[CustomObjectLibraryEdit::Insert {
                    index: 1,
                    entry: entry(&[2, 8, 4], "Second"),
                }],
            )
            .unwrap();
        assert!(!controller.can_redo());
        assert!(matches!(
            controller.undo(4),
            Err(CustomObjectControllerError::StaleRevision { .. })
        ));
    }
}
