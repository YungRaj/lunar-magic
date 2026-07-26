//! Transactional I/O for the four vanilla SMW main-entrance planes.

use crate::{Project, RomWrite, TransactionError};
use lm_rom::{Mapper, RomError, compute_snes_checksum, mapper_supports_image_len};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VanillaEntranceRomLayout {
    pub mapper: Mapper,
    pub position_offset: usize,
    pub vertical_settings_offset: usize,
    pub screen_and_method_offset: usize,
    pub level_mode_and_screen_offset: usize,
    pub entries: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VanillaMainEntrance {
    pub position: u8,
    pub vertical_settings: u8,
    pub screen_and_method: u8,
    pub level_mode_and_screen: u8,
}

#[derive(Debug)]
pub enum VanillaEntranceIoError {
    MapperImageShape,
    SlotOutOfRange { slot: usize, entries: usize },
    OffsetOverflow,
    Rom(RomError),
    Transaction(TransactionError),
}

impl std::fmt::Display for VanillaEntranceIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "vanilla entrance I/O failed: {self:?}")
    }
}

impl std::error::Error for VanillaEntranceIoError {}

impl From<RomError> for VanillaEntranceIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for VanillaEntranceIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads one entry from each of the four vanilla entrance planes.
    ///
    /// # Errors
    ///
    /// Rejects incompatible mapper/image shapes, out-of-range slots, overflow, or ROM bounds.
    pub fn load_vanilla_main_entrance(
        &self,
        slot: usize,
        layout: VanillaEntranceRomLayout,
    ) -> Result<VanillaMainEntrance, VanillaEntranceIoError> {
        validate(self, slot, layout)?;
        Ok(VanillaMainEntrance {
            position: self.rom.read(layout.position_offset + slot, 1)?[0],
            vertical_settings: self.rom.read(layout.vertical_settings_offset + slot, 1)?[0],
            screen_and_method: self.rom.read(layout.screen_and_method_offset + slot, 1)?[0],
            level_mode_and_screen: self
                .rom
                .read(layout.level_mode_and_screen_offset + slot, 1)?[0],
        })
    }

    /// Saves one entrance and the SNES checksum as a single undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects invalid layouts, ROM bounds, checksum failures, or transaction failures without
    /// mutation.
    pub fn save_vanilla_main_entrance(
        &mut self,
        slot: usize,
        entrance: VanillaMainEntrance,
        layout: VanillaEntranceRomLayout,
        checksum_field: usize,
    ) -> Result<bool, VanillaEntranceIoError> {
        validate(self, slot, layout)?;
        let writes = [
            RomWrite {
                offset: layout.position_offset + slot,
                bytes: vec![entrance.position],
            },
            RomWrite {
                offset: layout.vertical_settings_offset + slot,
                bytes: vec![entrance.vertical_settings],
            },
            RomWrite {
                offset: layout.screen_and_method_offset + slot,
                bytes: vec![entrance.screen_and_method],
            },
            RomWrite {
                offset: layout.level_mode_and_screen_offset + slot,
                bytes: vec![entrance.level_mode_and_screen],
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
        Ok(self.apply_writes("save vanilla main entrance", &all)?)
    }
}

fn validate(
    project: &Project,
    slot: usize,
    layout: VanillaEntranceRomLayout,
) -> Result<(), VanillaEntranceIoError> {
    if !mapper_supports_image_len(layout.mapper, project.rom.logical_len()) {
        return Err(VanillaEntranceIoError::MapperImageShape);
    }
    if slot >= layout.entries {
        return Err(VanillaEntranceIoError::SlotOutOfRange {
            slot,
            entries: layout.entries,
        });
    }
    for offset in [
        layout.position_offset,
        layout.vertical_settings_offset,
        layout.screen_and_method_offset,
        layout.level_mode_and_screen_offset,
    ] {
        offset
            .checked_add(slot)
            .ok_or(VanillaEntranceIoError::OffsetOverflow)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::RomImage;

    #[test]
    fn four_planes_save_atomically_and_undo() {
        let mut bytes = vec![0; 0x8000];
        let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let layout = VanillaEntranceRomLayout {
            mapper: Mapper::LoRom,
            position_offset: 0x100,
            vertical_settings_offset: 0x200,
            screen_and_method_offset: 0x300,
            level_mode_and_screen_offset: 0x400,
            entries: 0x20,
        };
        let value = VanillaMainEntrance {
            position: 1,
            vertical_settings: 2,
            screen_and_method: 3,
            level_mode_and_screen: 4,
        };
        assert!(
            project
                .save_vanilla_main_entrance(5, value, layout, 0x7fdc)
                .unwrap()
        );
        assert_eq!(
            project.load_vanilla_main_entrance(5, layout).unwrap(),
            value
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(
            project.load_vanilla_main_entrance(5, layout).unwrap(),
            VanillaMainEntrance {
                position: 0,
                vertical_settings: 0,
                screen_and_method: 0,
                level_mode_and_screen: 0,
            }
        );
    }
}
