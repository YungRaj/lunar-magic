//! Transactional I/O for seven native expanded overworld settings records.

use crate::{ExpandedLevelSettingsLayout, Project, RomWrite, TransactionError};
use lm_level::{
    ExpandedLevelSettingsError, ExpandedLevelSettingsRecord, ExpandedOverworldSettings,
};
use lm_rom::{RomError, compute_snes_checksum, mapper_supports_image_len};

#[derive(Debug)]
pub enum ExpandedOverworldSettingsIoError {
    InvalidLayout,
    SlotRange { first: usize, entries: usize },
    OffsetOverflow,
    ChecksumOverlap,
    MapperImageShape,
    Record(ExpandedLevelSettingsError),
    Rom(RomError),
    Transaction(TransactionError),
}

impl std::fmt::Display for ExpandedOverworldSettingsIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded overworld settings I/O failed: {self:?}"
        )
    }
}

impl std::error::Error for ExpandedOverworldSettingsIoError {}

impl From<ExpandedLevelSettingsError> for ExpandedOverworldSettingsIoError {
    fn from(value: ExpandedLevelSettingsError) -> Self {
        Self::Record(value)
    }
}

impl From<RomError> for ExpandedOverworldSettingsIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for ExpandedOverworldSettingsIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads physical expanded-settings slots `$200..$206` (or another explicit seven-slot range).
    ///
    /// # Errors
    ///
    /// Rejects invalid strides, slot ranges, mapper/image shapes, overflow, bounds, or records.
    pub fn load_expanded_overworld_settings(
        &self,
        first_slot: usize,
        layout: ExpandedLevelSettingsLayout,
    ) -> Result<ExpandedOverworldSettings, ExpandedOverworldSettingsIoError> {
        validate(self, first_slot, layout)?;
        let records = (0..ExpandedOverworldSettings::SUBMAP_COUNT)
            .map(|index| {
                let offset = record_offset(first_slot + index, layout)?;
                Ok(ExpandedLevelSettingsRecord::decode(
                    self.rom
                        .read(offset, ExpandedLevelSettingsRecord::ENCODED_LEN)?,
                )?)
            })
            .collect::<Result<Vec<_>, ExpandedOverworldSettingsIoError>>()?
            .try_into()
            .map_err(|_: Vec<_>| ExpandedOverworldSettingsIoError::InvalidLayout)?;
        Ok(ExpandedOverworldSettings { records })
    }

    /// Saves all seven records and checksum as one undoable application transaction.
    ///
    /// # Errors
    ///
    /// Rejects invalid layout, mapper, bounds, checksum overlap, or transaction failures before
    /// mutation.
    pub fn save_expanded_overworld_settings(
        &mut self,
        first_slot: usize,
        settings: &ExpandedOverworldSettings,
        layout: ExpandedLevelSettingsLayout,
        checksum_field: usize,
    ) -> Result<bool, ExpandedOverworldSettingsIoError> {
        validate(self, first_slot, layout)?;
        let mut writes = Vec::with_capacity(ExpandedOverworldSettings::SUBMAP_COUNT + 1);
        for (index, record) in settings.records.iter().enumerate() {
            let offset = record_offset(first_slot + index, layout)?;
            let end = offset
                .checked_add(ExpandedLevelSettingsRecord::ENCODED_LEN)
                .ok_or(ExpandedOverworldSettingsIoError::OffsetOverflow)?;
            let checksum_end = checksum_field
                .checked_add(4)
                .ok_or(ExpandedOverworldSettingsIoError::OffsetOverflow)?;
            if offset < checksum_end && checksum_field < end {
                return Err(ExpandedOverworldSettingsIoError::ChecksumOverlap);
            }
            writes.push(RomWrite {
                offset,
                bytes: record.encoded().to_vec(),
            });
        }
        if !self.writes_would_change(&writes)? {
            return Ok(false);
        }
        let mut staged = self.rom.clone();
        for write in &writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        Ok(self.apply_writes("save expanded overworld settings", &writes)?)
    }
}

fn validate(
    project: &Project,
    first_slot: usize,
    layout: ExpandedLevelSettingsLayout,
) -> Result<(), ExpandedOverworldSettingsIoError> {
    if layout.stride < ExpandedLevelSettingsRecord::ENCODED_LEN || layout.entries == 0 {
        return Err(ExpandedOverworldSettingsIoError::InvalidLayout);
    }
    let end = first_slot
        .checked_add(ExpandedOverworldSettings::SUBMAP_COUNT)
        .ok_or(ExpandedOverworldSettingsIoError::OffsetOverflow)?;
    if end > layout.entries {
        return Err(ExpandedOverworldSettingsIoError::SlotRange {
            first: first_slot,
            entries: layout.entries,
        });
    }
    if !mapper_supports_image_len(layout.mapper, project.rom.logical_len()) {
        return Err(ExpandedOverworldSettingsIoError::MapperImageShape);
    }
    Ok(())
}

fn record_offset(
    slot: usize,
    layout: ExpandedLevelSettingsLayout,
) -> Result<usize, ExpandedOverworldSettingsIoError> {
    slot.checked_mul(layout.stride)
        .and_then(|relative| layout.table_offset.checked_add(relative))
        .ok_or(ExpandedOverworldSettingsIoError::OffsetOverflow)
}
