use crate::{ControllerSnapshot, EditorMode, PreparedRomCommit};
use lm_level::{ExpandedLevelSettingsError, ExpandedLevelSettingsRecord};
use lm_project::{
    ExpandedLevelSettingsIoError, ExpandedLevelSettingsLayout, Project, RomMutation,
    TransactionError,
};
use lm_rom::{Mapper, RomError, RomImage};
use std::fmt;

#[derive(Debug)]
pub enum ExpandedSettingsControllerError {
    WrongMode(EditorMode),
    MapperMismatch { snapshot: Mapper, layout: Mapper },
    Record(ExpandedLevelSettingsError),
    Io(ExpandedLevelSettingsIoError),
    Rom(RomError),
    Mutation(TransactionError),
    DuplicateWord(usize),
}

impl fmt::Display for ExpandedSettingsControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "expanded-settings controller failed: {self:?}")
    }
}
impl std::error::Error for ExpandedSettingsControllerError {}

/// One installed expanded-settings record decoded from an immutable application snapshot.
#[derive(Clone, Debug)]
pub struct ExpandedSettingsController {
    revision: u64,
    slot: usize,
    layout: ExpandedLevelSettingsLayout,
    checksum_field: usize,
    source_file_bytes: Vec<u8>,
    baseline: ExpandedLevelSettingsRecord,
    record: ExpandedLevelSettingsRecord,
}

impl ExpandedSettingsController {
    /// Decodes the selected level's exact installed record.
    ///
    /// # Errors
    ///
    /// Rejects non-level modes, mapper disagreement, malformed layouts, or inaccessible records.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: ExpandedLevelSettingsLayout,
    ) -> Result<Self, ExpandedSettingsControllerError> {
        let EditorMode::Level(slot) = snapshot.mode else {
            return Err(ExpandedSettingsControllerError::WrongMode(snapshot.mode));
        };
        if snapshot.identity.mapper != layout.mapper {
            return Err(ExpandedSettingsControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.mapper,
            });
        }
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(ExpandedSettingsControllerError::Rom)?;
        let record = Project::new(image)
            .load_expanded_level_settings(usize::from(slot), layout)
            .map_err(ExpandedSettingsControllerError::Io)?;
        Ok(Self {
            revision: snapshot.revision,
            slot: usize::from(slot),
            layout,
            checksum_field: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: record.clone(),
            record,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub const fn record(&self) -> &ExpandedLevelSettingsRecord {
        &self.record
    }
    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.record != self.baseline
    }

    /// Replaces one lossless native word.
    ///
    /// # Errors
    ///
    /// Rejects indexes outside the exact sixteen-word record.
    pub fn set_word(
        &mut self,
        index: usize,
        value: u16,
    ) -> Result<(), ExpandedSettingsControllerError> {
        self.record
            .set_word(index, value)
            .map_err(ExpandedSettingsControllerError::Record)
    }

    /// Applies a duplicate-free batch atomically to a staged record.
    ///
    /// # Errors
    ///
    /// Rejects duplicate or out-of-range indexes without changing any word.
    pub fn apply_word_edits(
        &mut self,
        edits: &[(usize, u16)],
    ) -> Result<(), ExpandedSettingsControllerError> {
        let mut staged = self.record.clone();
        let mut seen = [false; ExpandedLevelSettingsRecord::WORD_COUNT];
        for &(index, value) in edits {
            if index >= seen.len() {
                return Err(ExpandedSettingsControllerError::Record(
                    ExpandedLevelSettingsError::WordOutOfRange(index),
                ));
            }
            if std::mem::replace(&mut seen[index], true) {
                return Err(ExpandedSettingsControllerError::DuplicateWord(index));
            }
            staged
                .set_word(index, value)
                .map_err(ExpandedSettingsControllerError::Record)?;
        }
        self.record = staged;
        Ok(())
    }

    /// Prepares one checksum-inclusive, revision-bound application mutation.
    ///
    /// # Errors
    ///
    /// Returns native I/O, image, or mutation construction failures without changing the source.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
    ) -> Result<PreparedRomCommit, ExpandedSettingsControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(ExpandedSettingsControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let description = description.into();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(self.layout.mapper, before.len()),
            });
        }
        let mut project = Project::new(image);
        project
            .save_expanded_level_settings(self.slot, &self.record, self.layout, self.checksum_field)
            .map_err(ExpandedSettingsControllerError::Io)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(ExpandedSettingsControllerError::Mutation)?;
        Ok(PreparedRomCommit {
            expected_revision: self.revision,
            description,
            mutation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, Command};

    #[test]
    fn exact_word_commit_is_checksum_valid_undoable_and_stale_safe() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        let layout = ExpandedLevelSettingsLayout {
            mapper: Mapper::LoRom,
            table_offset: 0x2000,
            entries: 0x200,
            stride: 0x20,
        };
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let mut controller =
            ExpandedSettingsController::decode(&app.controller_snapshot().unwrap(), layout)
                .unwrap();
        controller.set_word(7, 0xa55a).unwrap();
        app.dispatch(
            controller
                .prepare_commit("Edit expanded settings")
                .unwrap()
                .into_command(),
        )
        .unwrap();
        assert_eq!(
            app.project()
                .unwrap()
                .load_expanded_level_settings(0x105, layout)
                .unwrap()
                .word(7)
                .unwrap(),
            0xa55a
        );
        assert!(
            app.project()
                .unwrap()
                .identity
                .as_ref()
                .unwrap()
                .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project()
                .unwrap()
                .load_expanded_level_settings(0x105, layout)
                .unwrap()
                .word(7)
                .unwrap(),
            0
        );
    }

    #[test]
    fn duplicate_and_late_invalid_batches_are_atomic() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        let layout = ExpandedLevelSettingsLayout {
            mapper: Mapper::LoRom,
            table_offset: 0x2000,
            entries: 0x200,
            stride: 0x20,
        };
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let mut controller =
            ExpandedSettingsController::decode(&app.controller_snapshot().unwrap(), layout)
                .unwrap();
        assert!(controller.apply_word_edits(&[(1, 7), (1, 8)]).is_err());
        assert_eq!(controller.record().word(1).unwrap(), 0);
        assert!(controller.apply_word_edits(&[(2, 9), (16, 1)]).is_err());
        assert_eq!(controller.record().word(2).unwrap(), 0);
    }
}
