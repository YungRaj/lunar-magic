//! Transactional updates for an installed current Lunar Magic special-path patch.

use crate::{
    OverworldPathLinkStorage, OverworldPathPatchError, OverworldPathPatchLocator, Project,
    payload::staging::commit_staged,
};
use lm_overworld::{OverworldPathLinkTable, OverworldPathLinkTableError};
use lm_rats::{AllocationError, AllocationPolicy, FreeSpaceAllocator, HEADER_LEN, parse_at};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes};

#[derive(Debug)]
pub enum OverworldPathPatchSaveError {
    UnsupportedStorage,
    EmptyTable,
    NonContiguousPlanes,
    MissingRuntimeAllocation,
    MissingTableAllocation,
    LengthOverflow,
    StorageMismatch,
    Detection(OverworldPathPatchError),
    Table(OverworldPathLinkTableError),
    Allocation(AllocationError),
    Rom(RomError),
    Commit(crate::PayloadSaveError),
}

impl std::fmt::Display for OverworldPathPatchSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded overworld path patch save failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldPathPatchSaveError {}

impl From<OverworldPathLinkTableError> for OverworldPathPatchSaveError {
    fn from(value: OverworldPathLinkTableError) -> Self {
        Self::Table(value)
    }
}

impl From<OverworldPathPatchError> for OverworldPathPatchSaveError {
    fn from(value: OverworldPathPatchError) -> Self {
        Self::Detection(value)
    }
}

impl From<AllocationError> for OverworldPathPatchSaveError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<RomError> for OverworldPathPatchSaveError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<crate::PayloadSaveError> for OverworldPathPatchSaveError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Replaces the exact contiguous table owned by an installed current special-path runtime.
    ///
    /// # Errors
    ///
    /// Requires recognized current storage, exact runtime/table RATS ownership, nonempty input,
    /// contiguous planes, and a valid allocation/checksum policy.
    pub fn save_installed_overworld_path_links(
        &mut self,
        table: &OverworldPathLinkTable,
        storage: OverworldPathLinkStorage,
        locator: OverworldPathPatchLocator,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, OverworldPathPatchSaveError> {
        let OverworldPathLinkStorage::CurrentPatch {
            patch_offset,
            planes,
        } = storage
        else {
            return Err(OverworldPathPatchSaveError::UnsupportedStorage);
        };
        if self.load_overworld_path_links_detected(locator)?.storage != storage {
            return Err(OverworldPathPatchSaveError::StorageMismatch);
        }
        if table.links.is_empty() {
            return Err(OverworldPathPatchSaveError::EmptyTable);
        }
        let payload = encode_payload(table)?;
        let source_len = planes
            .entries
            .checked_mul(5)
            .ok_or(OverworldPathPatchSaveError::LengthOverflow)?;
        if planes.destination_offset != planes.source_offset + source_len
            || planes.target_offset != planes.source_offset + source_len * 2
        {
            return Err(OverworldPathPatchSaveError::NonContiguousPlanes);
        }
        let original = self.rom.logical_bytes().to_vec();
        exact_block(&original, patch_offset, 0x70, true)?;
        let old_len = planes
            .entries
            .checked_mul(12)
            .ok_or(OverworldPathPatchSaveError::LengthOverflow)?;
        let old = exact_block(&original, planes.source_offset, old_len, false)?;
        if planes.entries == table.links.len() && original[old.payload.clone()] == payload {
            return Ok(false);
        }
        let (mut staged, replacement) = replace_with_optional_expansion(
            &original,
            planes.mapper,
            allocation,
            &old,
            &payload,
            fill,
        )?;
        publish_runtime(
            &mut staged,
            planes.mapper,
            patch_offset,
            replacement.payload.start,
            table.links.len(),
        )?;
        let checksum = compute_snes_checksum(&staged, checksum_field)?;
        staged[checksum_field..checksum_field + 4].copy_from_slice(&checksum.encoded());
        commit_staged(
            self,
            "save expanded native overworld path links".into(),
            &original,
            &staged,
        )?;
        Ok(true)
    }
}

fn exact_block(
    bytes: &[u8],
    payload_offset: usize,
    payload_len: usize,
    runtime: bool,
) -> Result<lm_rats::RatsBlock, OverworldPathPatchSaveError> {
    let error = || {
        if runtime {
            OverworldPathPatchSaveError::MissingRuntimeAllocation
        } else {
            OverworldPathPatchSaveError::MissingTableAllocation
        }
    };
    let header = payload_offset.checked_sub(HEADER_LEN).ok_or_else(error)?;
    let block = parse_at(bytes, header).map_err(|_| error())?;
    if block.payload.start != payload_offset || block.payload.len() != payload_len {
        return Err(error());
    }
    Ok(block)
}

fn encode_payload(table: &OverworldPathLinkTable) -> Result<Vec<u8>, OverworldPathPatchSaveError> {
    let planes = table.encode_planes()?;
    let mut payload = planes.sources;
    payload.extend_from_slice(&planes.destinations);
    payload.extend_from_slice(&planes.targets);
    Ok(payload)
}

fn replace_with_optional_expansion(
    original: &[u8],
    mapper: Mapper,
    policy: &AllocationPolicy,
    old: &lm_rats::RatsBlock,
    payload: &[u8],
    fill: u8,
) -> Result<(Vec<u8>, lm_rats::RatsBlock), OverworldPathPatchSaveError> {
    let mut current_policy = policy.clone();
    current_policy.search.end = current_policy.search.end.min(original.len());
    let mut staged = original.to_vec();
    match FreeSpaceAllocator::new(&mut staged, current_policy).replace(old, payload, fill) {
        Ok(block) => Ok((staged, block)),
        Err(AllocationError::NoSpace { .. }) if policy.search.end > original.len() => {
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

fn publish_runtime(
    bytes: &mut [u8],
    mapper: Mapper,
    patch_offset: usize,
    table_offset: usize,
    entries: usize,
) -> Result<(), OverworldPathPatchSaveError> {
    let encoded_count = entries
        .checked_sub(1)
        .ok_or(OverworldPathPatchSaveError::EmptyTable)?;
    let count =
        u16::try_from(encoded_count).map_err(|_| OverworldPathPatchSaveError::LengthOverflow)?;
    let stride = u16::try_from(
        encoded_count
            .checked_mul(5)
            .ok_or(OverworldPathPatchSaveError::LengthOverflow)?,
    )
    .map_err(|_| OverworldPathPatchSaveError::LengthOverflow)?;
    bytes
        .get_mut(patch_offset + 6..patch_offset + 8)
        .ok_or(OverworldPathPatchSaveError::LengthOverflow)?
        .copy_from_slice(&count.to_le_bytes());
    bytes
        .get_mut(patch_offset + 0x0b..patch_offset + 0x0d)
        .ok_or(OverworldPathPatchSaveError::LengthOverflow)?
        .copy_from_slice(&stride.to_le_bytes());
    let source_len = entries
        .checked_mul(5)
        .ok_or(OverworldPathPatchSaveError::LengthOverflow)?;
    for (operand, addend) in [0x11, 0x1a, 0x20, 0x2c, 0x33, 0x3a, 0x48, 0x52]
        .into_iter()
        .zip([
            0,
            2,
            4,
            source_len,
            source_len + 2,
            source_len + 4,
            source_len * 2,
            source_len * 2 + 1,
        ])
    {
        let target = table_offset
            .checked_add(addend)
            .ok_or(OverworldPathPatchSaveError::LengthOverflow)?;
        bytes
            .get_mut(patch_offset + operand..patch_offset + operand + 3)
            .ok_or(OverworldPathPatchSaveError::LengthOverflow)?
            .copy_from_slice(&pc_to_snes(mapper, target)?.to_le_bytes()[..3]);
    }
    Ok(())
}
