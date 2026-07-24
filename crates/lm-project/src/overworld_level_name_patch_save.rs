//! Transactional updates for an installed Lunar Magic overworld level-name table.

use crate::{OverworldLevelNameStorage, Project, payload::staging::commit_staged};
use lm_overworld::{NativeOverworldLevelNameError, NativeOverworldLevelNameTable};
use lm_rats::{AllocationError, AllocationPolicy, FreeSpaceAllocator, parse_at};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes};

#[derive(Debug)]
pub enum OverworldLevelNamePatchSaveError {
    UnsupportedStorage,
    EmptyTable,
    MissingAllocation,
    Table(NativeOverworldLevelNameError),
    Allocation(AllocationError),
    Rom(RomError),
    Commit(crate::PayloadSaveError),
}

impl std::fmt::Display for OverworldLevelNamePatchSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded overworld level-name save failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldLevelNamePatchSaveError {}

impl From<NativeOverworldLevelNameError> for OverworldLevelNamePatchSaveError {
    fn from(value: NativeOverworldLevelNameError) -> Self {
        Self::Table(value)
    }
}

impl From<AllocationError> for OverworldLevelNamePatchSaveError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<RomError> for OverworldLevelNamePatchSaveError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<crate::PayloadSaveError> for OverworldLevelNamePatchSaveError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Replaces and republishes an installed direct name table as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Requires recognized expanded storage and exact RATS ownership. Empty or lossy native
    /// models, invalid allocation authority, mapping errors, and checksum failures are atomic.
    pub fn save_installed_overworld_level_names(
        &mut self,
        table: &NativeOverworldLevelNameTable,
        storage: OverworldLevelNameStorage,
        mapper: Mapper,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, OverworldLevelNamePatchSaveError> {
        let OverworldLevelNameStorage::Expanded {
            runtime_offset,
            table_offset,
            table_len,
        } = storage
        else {
            return Err(OverworldLevelNamePatchSaveError::UnsupportedStorage);
        };
        let payload = table.encode()?;
        if payload.is_empty() {
            return Err(OverworldLevelNamePatchSaveError::EmptyTable);
        }
        let original = self.rom.logical_bytes().to_vec();
        let header_offset = table_offset
            .checked_sub(8)
            .ok_or(OverworldLevelNamePatchSaveError::MissingAllocation)?;
        let old = parse_at(&original, header_offset)
            .map_err(|_| OverworldLevelNamePatchSaveError::MissingAllocation)?;
        if old.payload.start != table_offset || old.payload.len() != table_len {
            return Err(OverworldLevelNamePatchSaveError::MissingAllocation);
        }
        if original[old.payload.clone()] == payload {
            return Ok(false);
        }
        let (mut staged, replacement) =
            replace_with_optional_expansion(&original, mapper, allocation, &old, &payload, fill)?;
        let pointer = pc_to_snes(mapper, replacement.payload.start)?.to_le_bytes();
        staged
            .get_mut(runtime_offset + 0x37..runtime_offset + 0x3a)
            .ok_or(OverworldLevelNamePatchSaveError::MissingAllocation)?
            .copy_from_slice(&pointer[..3]);
        let checksum = compute_snes_checksum(&staged, checksum_field)?;
        staged[checksum_field..checksum_field + 4].copy_from_slice(&checksum.encoded());
        commit_staged(
            self,
            "save expanded native overworld level names".into(),
            &original,
            &staged,
        )?;
        Ok(true)
    }
}

fn replace_with_optional_expansion(
    original: &[u8],
    mapper: Mapper,
    policy: &AllocationPolicy,
    old: &lm_rats::RatsBlock,
    payload: &[u8],
    fill: u8,
) -> Result<(Vec<u8>, lm_rats::RatsBlock), OverworldLevelNamePatchSaveError> {
    let mut bounded = policy.clone();
    bounded.search.end = bounded.search.end.min(original.len());
    let mut staged = original.to_vec();
    match FreeSpaceAllocator::new(&mut staged, bounded).replace(old, payload, fill) {
        Ok(block) => Ok((staged, block)),
        Err(AllocationError::NoSpace { .. }) if policy.search.end > original.len() => {
            if !policy.fill_bytes.contains(&fill) {
                return Err(AllocationError::InvalidPolicy.into());
            }
            let mut image = RomImage::from_bytes(original.to_vec())?;
            image.expand(mapper, policy.search.end, fill)?;
            let mut expanded = image.logical_bytes().to_vec();
            let block = FreeSpaceAllocator::new(&mut expanded, policy.clone())
                .replace(old, payload, fill)?;
            Ok((expanded, block))
        }
        Err(error) => Err(error.into()),
    }
}
