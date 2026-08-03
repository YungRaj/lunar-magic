//! Transactional I/O for Lunar Magic's four additional Lfix3 per-level fields.

use crate::{Project, RomWrite, TransactionError};
use lm_level::SpriteSpawnSettings;
use lm_rom::{Mapper, RomError, compute_snes_checksum, mapper_supports_image_len};

/// The four MWL level-header bytes stored outside vanilla SMW's entrance planes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Lfix3LevelFields {
    pub flags: u8,
    pub high_position: u8,
    pub additional_flags: u8,
    pub runtime_flags: u8,
}

impl Lfix3LevelFields {
    /// Returns the typed spawn-range/Smart Spawn view of the shared flags plane.
    #[must_use]
    pub const fn sprite_spawn_settings(self) -> SpriteSpawnSettings {
        SpriteSpawnSettings::from_raw(self.flags)
    }

    /// Replaces the shared flags byte with a losslessly prepared spawn-settings value.
    pub fn set_sprite_spawn_settings(&mut self, settings: SpriteSpawnSettings) {
        self.flags = settings.raw();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lfix3LevelFieldsRomLayout {
    pub mapper: Mapper,
    pub flags_offset: usize,
    pub high_position_offset: usize,
    pub additional_flags_offset: usize,
    pub runtime_flags_offset: usize,
    pub entries: usize,
}

#[derive(Debug)]
pub enum Lfix3LevelFieldsIoError {
    MapperImageShape,
    SlotOutOfRange { slot: usize, entries: usize },
    OffsetOverflow,
    Rom(RomError),
    Transaction(TransactionError),
}

impl std::fmt::Display for Lfix3LevelFieldsIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Lfix3 level-fields I/O failed: {self:?}")
    }
}

impl std::error::Error for Lfix3LevelFieldsIoError {}

impl From<RomError> for Lfix3LevelFieldsIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for Lfix3LevelFieldsIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads the four additional per-level fields used by Lfix3 generation 3.
    ///
    /// # Errors
    ///
    /// Rejects incompatible mapper/image shapes, invalid slots, overflow, or ROM bounds.
    pub fn load_lfix3_level_fields(
        &self,
        slot: usize,
        layout: Lfix3LevelFieldsRomLayout,
    ) -> Result<Lfix3LevelFields, Lfix3LevelFieldsIoError> {
        validate(self, slot, layout)?;
        Ok(Lfix3LevelFields {
            flags: self.rom.read(layout.flags_offset + slot, 1)?[0],
            high_position: self.rom.read(layout.high_position_offset + slot, 1)?[0],
            additional_flags: self.rom.read(layout.additional_flags_offset + slot, 1)?[0],
            runtime_flags: self.rom.read(layout.runtime_flags_offset + slot, 1)?[0],
        })
    }

    /// Saves all four fields and the SNES checksum in one undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects invalid layouts, ROM bounds, checksum failures, or transaction failures without
    /// changing ROM bytes or history.
    pub fn save_lfix3_level_fields(
        &mut self,
        slot: usize,
        fields: Lfix3LevelFields,
        layout: Lfix3LevelFieldsRomLayout,
        checksum_field: usize,
    ) -> Result<bool, Lfix3LevelFieldsIoError> {
        validate(self, slot, layout)?;
        let writes = [
            RomWrite {
                offset: layout.flags_offset + slot,
                bytes: vec![fields.flags],
            },
            RomWrite {
                offset: layout.high_position_offset + slot,
                bytes: vec![fields.high_position],
            },
            RomWrite {
                offset: layout.additional_flags_offset + slot,
                bytes: vec![fields.additional_flags],
            },
            RomWrite {
                offset: layout.runtime_flags_offset + slot,
                bytes: vec![fields.runtime_flags],
            },
        ];
        if !self.writes_would_change(&writes)? {
            return Ok(false);
        }
        let mut staged = self.rom.clone();
        for write in &writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        let mut all = writes.to_vec();
        all.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        Ok(self.apply_writes("save Lfix3 level fields", &all)?)
    }
}

fn validate(
    project: &Project,
    slot: usize,
    layout: Lfix3LevelFieldsRomLayout,
) -> Result<(), Lfix3LevelFieldsIoError> {
    if !mapper_supports_image_len(layout.mapper, project.rom.logical_len()) {
        return Err(Lfix3LevelFieldsIoError::MapperImageShape);
    }
    if slot >= layout.entries {
        return Err(Lfix3LevelFieldsIoError::SlotOutOfRange {
            slot,
            entries: layout.entries,
        });
    }
    for offset in [
        layout.flags_offset,
        layout.high_position_offset,
        layout.additional_flags_offset,
        layout.runtime_flags_offset,
    ] {
        offset
            .checked_add(slot)
            .ok_or(Lfix3LevelFieldsIoError::OffsetOverflow)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::RomImage;

    #[test]
    fn four_planes_save_reopen_and_undo_atomically() {
        let mut bytes = vec![0; 0x8000];
        let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        let mut project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let layout = Lfix3LevelFieldsRomLayout {
            mapper: Mapper::LoRom,
            flags_offset: 0x100,
            high_position_offset: 0x200,
            additional_flags_offset: 0x300,
            runtime_flags_offset: 0x400,
            entries: 0x20,
        };
        let expected = Lfix3LevelFields {
            flags: 1,
            high_position: 2,
            additional_flags: 3,
            runtime_flags: 4,
        };
        assert!(
            project
                .save_lfix3_level_fields(5, expected, layout, 0x7fdc)
                .unwrap()
        );
        assert_eq!(
            project.load_lfix3_level_fields(5, layout).unwrap(),
            expected
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), bytes);
    }

    #[test]
    fn spawn_settings_preserve_shared_lfix3_flags_through_rom_reopen_and_undo() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x105] = 0xe1;
        let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        let original = bytes.clone();
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let layout = Lfix3LevelFieldsRomLayout {
            mapper: Mapper::LoRom,
            flags_offset: 0x100,
            high_position_offset: 0x200,
            additional_flags_offset: 0x300,
            runtime_flags_offset: 0x400,
            entries: 0x20,
        };
        let mut fields = project.load_lfix3_level_fields(5, layout).unwrap();
        let settings = fields
            .sprite_spawn_settings()
            .with_properties(3, true)
            .unwrap();
        fields.set_sprite_spawn_settings(settings);
        project
            .save_lfix3_level_fields(5, fields, layout, 0x7fdc)
            .unwrap();

        let reopened = project.load_lfix3_level_fields(5, layout).unwrap();
        assert_eq!(reopened.flags, 0xe7);
        assert_eq!(reopened.sprite_spawn_settings().vertical_range(), 3);
        assert!(reopened.sprite_spawn_settings().smart_spawn());
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.logical_bytes(), original);
    }

    #[test]
    fn late_bounds_failure_does_not_mutate_or_record_history() {
        let bytes = vec![0; 0x8000];
        let mut project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let layout = Lfix3LevelFieldsRomLayout {
            mapper: Mapper::LoRom,
            flags_offset: 0x100,
            high_position_offset: 0x200,
            additional_flags_offset: 0x300,
            runtime_flags_offset: usize::MAX,
            entries: 2,
        };
        assert!(matches!(
            project.save_lfix3_level_fields(1, Lfix3LevelFields::default(), layout, 0x7fdc),
            Err(Lfix3LevelFieldsIoError::OffsetOverflow)
        ));
        assert_eq!(project.save_snapshot(), bytes);
        assert_eq!(project.history.undo_len(), 0);
    }
}
