use crate::{ControllerSnapshot, EditorMode, PreparedRomCommit};
use lm_graphics::{
    EXANIMATION_LEVEL_SLOT_COUNT, ExAnimationSlotOptionError, ExAnimationSlotOptionTable,
};
use lm_project::{
    ExAnimationSlotOptionIoError, ExAnimationSlotOptionRomLayout, ExAnimationSlotOptionSaveOptions,
    Project, RomMutation, TransactionError,
};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError, RomImage};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExAnimationSlotOptionEdit {
    SetLowNibble {
        slot: usize,
        value: u8,
    },
    SetEnabled {
        slot: usize,
        option: usize,
        enabled: bool,
    },
}

#[derive(Debug)]
pub enum ExAnimationSlotOptionsControllerError {
    WrongMode(EditorMode),
    MapperMismatch { snapshot: Mapper, layout: Mapper },
    SlotOutOfRange(usize),
    OptionOutOfRange(usize),
    DuplicateField { slot: usize, field: usize },
    Codec(ExAnimationSlotOptionError),
    Io(ExAnimationSlotOptionIoError),
    Rom(RomError),
    Mutation(TransactionError),
}

impl fmt::Display for ExAnimationSlotOptionsControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "ExAnimation slot-options controller failed: {self:?}"
        )
    }
}

impl std::error::Error for ExAnimationSlotOptionsControllerError {}

/// Revision-bound application controller for Lunar Magic's seven native slot-option bytes.
#[derive(Clone, Debug)]
pub struct ExAnimationSlotOptionsController {
    revision: u64,
    layout: ExAnimationSlotOptionRomLayout,
    checksum_field: usize,
    source_file_bytes: Vec<u8>,
    baseline: ExAnimationSlotOptionTable,
    table: ExAnimationSlotOptionTable,
    previous_block: RatsBlock,
}

impl ExAnimationSlotOptionsController {
    /// Loads the shared table from an immutable `ExAnimation` editor snapshot.
    ///
    /// # Errors
    ///
    /// Rejects other editor modes, mapper disagreement, invalid ownership, and malformed tables.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: ExAnimationSlotOptionRomLayout,
    ) -> Result<Self, ExAnimationSlotOptionsControllerError> {
        if !matches!(snapshot.mode, EditorMode::ExAnimation(_)) {
            return Err(ExAnimationSlotOptionsControllerError::WrongMode(
                snapshot.mode,
            ));
        }
        if snapshot.identity.mapper != layout.mapper {
            return Err(ExAnimationSlotOptionsControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.mapper,
            });
        }
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(ExAnimationSlotOptionsControllerError::Rom)?;
        let loaded = Project::new(image)
            .load_exanimation_slot_options(layout)
            .map_err(ExAnimationSlotOptionsControllerError::Io)?;
        Ok(Self {
            revision: snapshot.revision,
            layout,
            checksum_field: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            baseline: loaded.table.clone(),
            table: loaded.table,
            previous_block: loaded.block,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn table(&self) -> &ExAnimationSlotOptionTable {
        &self.table
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.table != self.baseline
    }

    /// Applies a duplicate-free edit batch to a staged table.
    ///
    /// Field zero is the preserved low nibble; fields one through four are option bits 4–7.
    ///
    /// # Errors
    ///
    /// Rejects invalid slots/options, duplicate fields, or non-nibble values atomically.
    pub fn apply_edits(
        &mut self,
        edits: &[ExAnimationSlotOptionEdit],
    ) -> Result<(), ExAnimationSlotOptionsControllerError> {
        let mut staged = self.table.clone();
        let mut seen = [[false; 5]; EXANIMATION_LEVEL_SLOT_COUNT];
        for edit in edits {
            let (slot, field) = match *edit {
                ExAnimationSlotOptionEdit::SetLowNibble { slot, .. } => (slot, 0),
                ExAnimationSlotOptionEdit::SetEnabled { slot, option, .. } => {
                    if option >= 4 {
                        return Err(ExAnimationSlotOptionsControllerError::OptionOutOfRange(
                            option,
                        ));
                    }
                    (slot, option + 1)
                }
            };
            if slot >= EXANIMATION_LEVEL_SLOT_COUNT {
                return Err(ExAnimationSlotOptionsControllerError::SlotOutOfRange(slot));
            }
            if std::mem::replace(&mut seen[slot][field], true) {
                return Err(ExAnimationSlotOptionsControllerError::DuplicateField { slot, field });
            }
            match *edit {
                ExAnimationSlotOptionEdit::SetLowNibble { value, .. } => {
                    staged.slots[slot].preserved_low_nibble = value;
                }
                ExAnimationSlotOptionEdit::SetEnabled {
                    option, enabled, ..
                } => staged.slots[slot].enabled[option] = enabled,
            }
        }
        let bytes = staged
            .encode()
            .map_err(ExAnimationSlotOptionsControllerError::Codec)?;
        self.table = ExAnimationSlotOptionTable::decode(&bytes)
            .map_err(ExAnimationSlotOptionsControllerError::Codec)?;
        Ok(())
    }

    /// Prepares a checksum-inclusive, revision-bound ROM mutation.
    ///
    /// # Errors
    ///
    /// Returns encoding, allocation, checksum, image, or mutation failures without changing the
    /// application snapshot.
    pub fn prepare_commit(
        &self,
        description: impl Into<String>,
        options: &ExAnimationSlotOptionSaveOptions,
    ) -> Result<PreparedRomCommit, ExAnimationSlotOptionsControllerError> {
        let image = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(ExAnimationSlotOptionsControllerError::Rom)?;
        let before = image.logical_bytes().to_vec();
        let description = description.into();
        if !self.is_modified() {
            return Ok(PreparedRomCommit {
                expected_revision: self.revision,
                description,
                mutation: RomMutation::unchanged(self.layout.mapper, before.len()),
            });
        }
        let mut options = options.clone();
        options.previous_block = Some(self.previous_block.clone());
        let mut project = Project::new(image);
        project
            .save_exanimation_slot_options(&self.table, self.layout, &options)
            .map_err(ExAnimationSlotOptionsControllerError::Io)?;
        project
            .rom
            .update_snes_checksum(self.checksum_field)
            .map_err(ExAnimationSlotOptionsControllerError::Rom)?;
        let mutation =
            RomMutation::between(self.layout.mapper, &before, project.rom.logical_bytes())
                .map_err(ExAnimationSlotOptionsControllerError::Mutation)?;
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
    use lm_graphics::ExAnimationSlotOptions;
    use lm_rats::{AllocationPolicy, ProtectedRange};

    const POINTER: usize = 0x2000;
    const CHECKSUM: usize = 0x7fdc;

    fn table() -> ExAnimationSlotOptionTable {
        ExAnimationSlotOptionTable {
            slots: [ExAnimationSlotOptions {
                preserved_low_nibble: 0,
                enabled: [true; 4],
            }; EXANIMATION_LEVEL_SLOT_COUNT],
        }
    }

    fn save_options() -> ExAnimationSlotOptionSaveOptions {
        ExAnimationSlotOptionSaveOptions {
            allocation: AllocationPolicy {
                search: 0x3000..0x7000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![
                    ProtectedRange(POINTER..POINTER + 3),
                    ProtectedRange(CHECKSUM..CHECKSUM + 4),
                ],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    fn application() -> AppState {
        let mut bytes = vec![0xff; 0x8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        bytes[0x7fdb] = 0;
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        project
            .save_exanimation_slot_options(
                &table(),
                ExAnimationSlotOptionRomLayout {
                    mapper: Mapper::LoRom,
                    pointer_offset: POINTER,
                },
                &save_options(),
            )
            .unwrap();
        project.rom.update_snes_checksum(CHECKSUM).unwrap();
        let mut app = AppState::default();
        app.load_rom(project.save_snapshot()).unwrap();
        app.dispatch(Command::ShowExAnimation(0)).unwrap();
        app
    }

    #[test]
    fn prepared_commit_is_checksum_valid_undoable_and_stale_safe() {
        let mut app = application();
        let layout = ExAnimationSlotOptionRomLayout {
            mapper: Mapper::LoRom,
            pointer_offset: POINTER,
        };
        let mut controller =
            ExAnimationSlotOptionsController::decode(&app.controller_snapshot().unwrap(), layout)
                .unwrap();
        controller
            .apply_edits(&[
                ExAnimationSlotOptionEdit::SetLowNibble { slot: 2, value: 9 },
                ExAnimationSlotOptionEdit::SetEnabled {
                    slot: 2,
                    option: 1,
                    enabled: false,
                },
            ])
            .unwrap();
        let commit = controller
            .prepare_commit("Edit ExAnimation slot options", &save_options())
            .unwrap();
        app.dispatch(commit.into_command()).unwrap();
        let loaded = app
            .project()
            .unwrap()
            .load_exanimation_slot_options(layout)
            .unwrap();
        assert_eq!(loaded.table.slots[2].preserved_low_nibble, 9);
        assert!(!loaded.table.slots[2].enabled[1]);
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
                .load_exanimation_slot_options(layout)
                .unwrap()
                .table,
            table()
        );
        assert!(
            app.dispatch(
                controller
                    .prepare_commit("stale", &save_options())
                    .unwrap()
                    .into_command()
            )
            .is_err()
        );
    }

    #[test]
    fn duplicate_and_late_invalid_batches_are_atomic() {
        let app = application();
        let mut controller = ExAnimationSlotOptionsController::decode(
            &app.controller_snapshot().unwrap(),
            ExAnimationSlotOptionRomLayout {
                mapper: Mapper::LoRom,
                pointer_offset: POINTER,
            },
        )
        .unwrap();
        let original = controller.table().clone();
        assert!(
            controller
                .apply_edits(&[
                    ExAnimationSlotOptionEdit::SetEnabled {
                        slot: 1,
                        option: 2,
                        enabled: false,
                    },
                    ExAnimationSlotOptionEdit::SetEnabled {
                        slot: 1,
                        option: 2,
                        enabled: true,
                    },
                ])
                .is_err()
        );
        assert_eq!(controller.table(), &original);
        assert!(
            controller
                .apply_edits(&[
                    ExAnimationSlotOptionEdit::SetLowNibble { slot: 0, value: 7 },
                    ExAnimationSlotOptionEdit::SetLowNibble {
                        slot: 6,
                        value: 0x80,
                    },
                ])
                .is_err()
        );
        assert_eq!(controller.table(), &original);
    }
}
