use crate::{Project, RomWrite, TransactionError};
use lm_level::{ExpandedLevelSettingsError, ExpandedLevelSettingsRecord};
use lm_rom::{Mapper, RomError, compute_snes_checksum, mapper_supports_image_len};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpandedLevelSettingsLayout {
    pub mapper: Mapper,
    pub table_offset: usize,
    pub entries: usize,
    pub stride: usize,
}

#[derive(Debug)]
pub enum ExpandedLevelSettingsIoError {
    InvalidLayout,
    SlotOutOfRange { slot: usize, entries: usize },
    OffsetOverflow,
    ChecksumOverlap,
    MapperImageShape { mapper: Mapper, image_len: usize },
    Record(ExpandedLevelSettingsError),
    Rom(RomError),
    Transaction(TransactionError),
}

impl fmt::Display for ExpandedLevelSettingsIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "expanded level settings I/O failed: {self:?}")
    }
}
impl std::error::Error for ExpandedLevelSettingsIoError {}
impl From<ExpandedLevelSettingsError> for ExpandedLevelSettingsIoError {
    fn from(value: ExpandedLevelSettingsError) -> Self {
        Self::Record(value)
    }
}
impl From<RomError> for ExpandedLevelSettingsIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}
impl From<TransactionError> for ExpandedLevelSettingsIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads one lossless record from an already-installed expanded settings table.
    ///
    /// # Errors
    ///
    /// Returns [`ExpandedLevelSettingsIoError`] for invalid layouts, slots, image shape, or bounds.
    pub fn load_expanded_level_settings(
        &self,
        slot: usize,
        layout: ExpandedLevelSettingsLayout,
    ) -> Result<ExpandedLevelSettingsRecord, ExpandedLevelSettingsIoError> {
        validate_layout(self, slot, layout)?;
        Ok(ExpandedLevelSettingsRecord::decode(self.rom.read(
            record_offset(slot, layout)?,
            ExpandedLevelSettingsRecord::ENCODED_LEN,
        )?)?)
    }

    /// Writes one installed expanded-settings record and checksum as one undoable operation.
    ///
    /// # Errors
    ///
    /// Returns [`ExpandedLevelSettingsIoError`] for invalid layout, image, checksum, or transaction.
    pub fn save_expanded_level_settings(
        &mut self,
        slot: usize,
        record: &ExpandedLevelSettingsRecord,
        layout: ExpandedLevelSettingsLayout,
        checksum_field: usize,
    ) -> Result<bool, ExpandedLevelSettingsIoError> {
        validate_layout(self, slot, layout)?;
        let offset = record_offset(slot, layout)?;
        let record_end = offset
            .checked_add(ExpandedLevelSettingsRecord::ENCODED_LEN)
            .ok_or(ExpandedLevelSettingsIoError::OffsetOverflow)?;
        let checksum_end = checksum_field
            .checked_add(4)
            .ok_or(ExpandedLevelSettingsIoError::OffsetOverflow)?;
        if offset < checksum_end && checksum_field < record_end {
            return Err(ExpandedLevelSettingsIoError::ChecksumOverlap);
        }
        let mut staged = self.rom.clone();
        staged.write(offset, record.encoded())?;
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        Ok(self.apply_writes(
            format!("save expanded level settings {slot:03x}"),
            &[
                RomWrite {
                    offset,
                    bytes: record.encoded().to_vec(),
                },
                RomWrite {
                    offset: checksum_field,
                    bytes: checksum.encoded().to_vec(),
                },
            ],
        )?)
    }
}

pub(crate) fn expanded_settings_write(
    project: &Project,
    slot: usize,
    record: &ExpandedLevelSettingsRecord,
    layout: ExpandedLevelSettingsLayout,
) -> Result<RomWrite, ExpandedLevelSettingsIoError> {
    validate_layout(project, slot, layout)?;
    Ok(RomWrite {
        offset: record_offset(slot, layout)?,
        bytes: record.encoded().to_vec(),
    })
}

fn validate_layout(
    project: &Project,
    slot: usize,
    layout: ExpandedLevelSettingsLayout,
) -> Result<(), ExpandedLevelSettingsIoError> {
    if layout.entries == 0 || layout.stride < ExpandedLevelSettingsRecord::ENCODED_LEN {
        return Err(ExpandedLevelSettingsIoError::InvalidLayout);
    }
    if slot >= layout.entries {
        return Err(ExpandedLevelSettingsIoError::SlotOutOfRange {
            slot,
            entries: layout.entries,
        });
    }
    if !mapper_supports_image_len(layout.mapper, project.rom.logical_len()) {
        return Err(ExpandedLevelSettingsIoError::MapperImageShape {
            mapper: layout.mapper,
            image_len: project.rom.logical_len(),
        });
    }
    Ok(())
}

fn record_offset(
    slot: usize,
    layout: ExpandedLevelSettingsLayout,
) -> Result<usize, ExpandedLevelSettingsIoError> {
    slot.checked_mul(layout.stride)
        .and_then(|relative| layout.table_offset.checked_add(relative))
        .ok_or(ExpandedLevelSettingsIoError::OffsetOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{RomImage, SnesChecksum};

    fn layout() -> ExpandedLevelSettingsLayout {
        ExpandedLevelSettingsLayout {
            mapper: Mapper::LoRom,
            table_offset: 0x100,
            entries: 0x200,
            stride: 0x20,
        }
    }

    #[test]
    fn record_and_checksum_commit_as_one_reversible_edit() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let before = project.rom.logical_bytes().to_vec();
        let record = ExpandedLevelSettingsRecord::decode(&[0x5a; 0x20]).unwrap();
        assert!(
            project
                .save_expanded_level_settings(7, &record, layout(), 0x7fdc)
                .unwrap()
        );
        assert_eq!(
            project.load_expanded_level_settings(7, layout()).unwrap(),
            record
        );
        assert_eq!(project.history.undo_len(), 1);
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc)
                .unwrap()
                .is_complementary()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.rom.logical_bytes(), before);
    }

    #[test]
    fn late_checksum_and_overlap_failures_leave_project_unchanged() {
        let record = ExpandedLevelSettingsRecord::decode(&[1; 0x20]).unwrap();
        for checksum in [0x100, usize::MAX] {
            let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
            let before = project.rom.logical_bytes().to_vec();
            assert!(
                project
                    .save_expanded_level_settings(0, &record, layout(), checksum)
                    .is_err()
            );
            assert_eq!(project.rom.logical_bytes(), before);
            assert_eq!(project.history.undo_len(), 0);
        }
    }
}
