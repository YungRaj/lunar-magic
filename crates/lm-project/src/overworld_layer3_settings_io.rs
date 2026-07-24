//! Transactional direct-table I/O for native overworld Layer 3 settings.

use crate::{Project, RomWrite, TransactionError};
use lm_overworld::{OverworldLayer3SettingsError, OverworldLayer3SettingsTable};
use lm_rom::{Mapper, RomError, compute_snes_checksum, mapper_supports_image_len};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldLayer3SettingsRomLayout {
    pub mapper: Mapper,
    pub table_offset: usize,
}

#[derive(Debug)]
pub enum OverworldLayer3SettingsIoError {
    MapperImageShape,
    OffsetOverflow,
    ChecksumOverlap,
    Codec(OverworldLayer3SettingsError),
    Rom(RomError),
    Transaction(TransactionError),
}

impl std::fmt::Display for OverworldLayer3SettingsIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "overworld Layer 3 settings I/O failed: {self:?}")
    }
}

impl std::error::Error for OverworldLayer3SettingsIoError {}

impl From<OverworldLayer3SettingsError> for OverworldLayer3SettingsIoError {
    fn from(value: OverworldLayer3SettingsError) -> Self {
        Self::Codec(value)
    }
}

impl From<RomError> for OverworldLayer3SettingsIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for OverworldLayer3SettingsIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads one exact contiguous seven-record table.
    ///
    /// # Errors
    ///
    /// Rejects an incompatible mapper/image shape, overflow, bounds, or malformed table.
    pub fn load_overworld_layer3_settings(
        &self,
        layout: OverworldLayer3SettingsRomLayout,
    ) -> Result<OverworldLayer3SettingsTable, OverworldLayer3SettingsIoError> {
        validate_mapper(self, layout.mapper)?;
        let bytes = self.rom.read(
            layout.table_offset,
            OverworldLayer3SettingsTable::ENCODED_LEN,
        )?;
        Ok(OverworldLayer3SettingsTable::decode(bytes)?)
    }

    /// Saves all seven records and the SNES checksum as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects mapper, bounds, overlap, checksum, or transaction failures without mutation.
    pub fn save_overworld_layer3_settings(
        &mut self,
        table: &OverworldLayer3SettingsTable,
        layout: OverworldLayer3SettingsRomLayout,
        checksum_field: usize,
    ) -> Result<bool, OverworldLayer3SettingsIoError> {
        validate_mapper(self, layout.mapper)?;
        let table_end = layout
            .table_offset
            .checked_add(OverworldLayer3SettingsTable::ENCODED_LEN)
            .ok_or(OverworldLayer3SettingsIoError::OffsetOverflow)?;
        let checksum_end = checksum_field
            .checked_add(4)
            .ok_or(OverworldLayer3SettingsIoError::OffsetOverflow)?;
        if layout.table_offset < checksum_end && checksum_field < table_end {
            return Err(OverworldLayer3SettingsIoError::ChecksumOverlap);
        }
        let table_write = RomWrite {
            offset: layout.table_offset,
            bytes: table.encode().to_vec(),
        };
        if !self.writes_would_change(std::slice::from_ref(&table_write))? {
            return Ok(false);
        }
        let mut staged = self.rom.clone();
        staged.write(table_write.offset, &table_write.bytes)?;
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        Ok(self.apply_writes(
            "save overworld Layer 3 settings",
            &[
                table_write,
                RomWrite {
                    offset: checksum_field,
                    bytes: checksum.encoded().to_vec(),
                },
            ],
        )?)
    }
}

fn validate_mapper(
    project: &Project,
    mapper: Mapper,
) -> Result<(), OverworldLayer3SettingsIoError> {
    if mapper_supports_image_len(mapper, project.rom.logical_len()) {
        Ok(())
    } else {
        Err(OverworldLayer3SettingsIoError::MapperImageShape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::RomImage;

    const TABLE: usize = 0x2000;
    const CHECKSUM: usize = 0x7fdc;

    fn layout() -> OverworldLayer3SettingsRomLayout {
        OverworldLayer3SettingsRomLayout {
            mapper: Mapper::LoRom,
            table_offset: TABLE,
        }
    }

    #[test]
    fn save_load_checksum_and_undo_are_atomic() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let before = project.save_snapshot();
        let mut table =
            OverworldLayer3SettingsTable::decode(&[0; OverworldLayer3SettingsTable::ENCODED_LEN])
                .unwrap();
        table.maps[6].set_uses_custom_graphics(true);
        table.maps[6].set_graphics_file(3, 0x777).unwrap();
        assert!(
            project
                .save_overworld_layer3_settings(&table, layout(), CHECKSUM)
                .unwrap()
        );
        assert_eq!(
            project.load_overworld_layer3_settings(layout()).unwrap(),
            table
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), before);
    }

    #[test]
    fn checksum_overlap_does_not_mutate() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let before = project.save_snapshot();
        let table =
            OverworldLayer3SettingsTable::decode(&[0; OverworldLayer3SettingsTable::ENCODED_LEN])
                .unwrap();
        assert!(matches!(
            project.save_overworld_layer3_settings(&table, layout(), TABLE + 4),
            Err(OverworldLayer3SettingsIoError::ChecksumOverlap)
        ));
        assert_eq!(project.save_snapshot(), before);
    }
}
