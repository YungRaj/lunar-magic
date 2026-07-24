//! Transactional updates for an installed current Lunar Magic warp-link patch.

use crate::{OverworldWarpLinkStorage, Project, payload::staging::commit_staged};
use lm_overworld::{OverworldWarpLinkTable, OverworldWarpLinkTableError};
use lm_rats::{AllocationError, AllocationPolicy, FreeSpaceAllocator, parse_at};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes};

#[derive(Debug)]
pub enum OverworldWarpPatchSaveError {
    UnsupportedStorage,
    VanillaSizedTable(usize),
    NonContiguousPlanes,
    MissingAllocation,
    LengthOverflow,
    Table(OverworldWarpLinkTableError),
    Allocation(AllocationError),
    Rom(RomError),
    Commit(crate::PayloadSaveError),
}

impl std::fmt::Display for OverworldWarpPatchSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded overworld warp patch save failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldWarpPatchSaveError {}

impl From<OverworldWarpLinkTableError> for OverworldWarpPatchSaveError {
    fn from(value: OverworldWarpLinkTableError) -> Self {
        Self::Table(value)
    }
}

impl From<AllocationError> for OverworldWarpPatchSaveError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<RomError> for OverworldWarpPatchSaveError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<crate::PayloadSaveError> for OverworldWarpPatchSaveError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Replaces an installed current-variant four-plane allocation as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Requires current patch storage and 28–256 entries. The old four planes must be one exact,
    /// contiguous RATS allocation authorized by `allocation`; malformed ownership fails closed.
    pub fn save_installed_overworld_warp_links(
        &mut self,
        table: &OverworldWarpLinkTable,
        storage: OverworldWarpLinkStorage,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, OverworldWarpPatchSaveError> {
        let OverworldWarpLinkStorage::CurrentPatch {
            patch_offset,
            planes,
        } = storage
        else {
            return Err(OverworldWarpPatchSaveError::UnsupportedStorage);
        };
        if table.links.len() <= 27 {
            return Err(OverworldWarpPatchSaveError::VanillaSizedTable(
                table.links.len(),
            ));
        }
        let payload = encode_payload(table)?;
        let old_plane_len = planes
            .entries
            .checked_mul(2)
            .ok_or(OverworldWarpPatchSaveError::LengthOverflow)?;
        if [
            planes.source_horizontal_offset,
            planes.destination_vertical_offset,
            planes.destination_horizontal_offset,
        ] != [
            planes.source_vertical_offset + old_plane_len,
            planes.source_vertical_offset + old_plane_len * 2,
            planes.source_vertical_offset + old_plane_len * 3,
        ] {
            return Err(OverworldWarpPatchSaveError::NonContiguousPlanes);
        }
        let header_offset = planes
            .source_vertical_offset
            .checked_sub(lm_rats::HEADER_LEN)
            .ok_or(OverworldWarpPatchSaveError::MissingAllocation)?;
        let original = self.rom.logical_bytes().to_vec();
        let old = parse_at(&original, header_offset)
            .map_err(|_| OverworldWarpPatchSaveError::MissingAllocation)?;
        let expected_old_len = old_plane_len
            .checked_mul(4)
            .ok_or(OverworldWarpPatchSaveError::LengthOverflow)?;
        if old.payload.start != planes.source_vertical_offset
            || old.payload.len() != expected_old_len
        {
            return Err(OverworldWarpPatchSaveError::MissingAllocation);
        }
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
        publish_current_patch(
            &mut staged,
            planes.mapper,
            patch_offset,
            replacement.payload.start,
            table.links.len(),
        )?;
        let checksum = compute_snes_checksum(&staged, checksum_field)?;
        staged[checksum_field..checksum_field + 4].copy_from_slice(&checksum.encoded());
        if staged == original {
            return Ok(false);
        }
        commit_staged(
            self,
            "save expanded native overworld warp links".into(),
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
) -> Result<(Vec<u8>, lm_rats::RatsBlock), OverworldWarpPatchSaveError> {
    let mut current_policy = policy.clone();
    current_policy.search.end = current_policy.search.end.min(original.len());
    let mut staged = original.to_vec();
    match FreeSpaceAllocator::new(&mut staged, current_policy).replace(old, payload, fill) {
        Ok(block) => Ok((staged, block)),
        Err(AllocationError::NoSpace { .. }) if policy.search.end > original.len() => {
            let mut expanded = expand_for_policy(original, mapper, policy, fill)?;
            let block = FreeSpaceAllocator::new(&mut expanded, policy.clone())
                .replace(old, payload, fill)?;
            Ok((expanded, block))
        }
        Err(error) => Err(error.into()),
    }
}

fn encode_payload(table: &OverworldWarpLinkTable) -> Result<Vec<u8>, OverworldWarpPatchSaveError> {
    let planes = table.encode_planes()?;
    let mut payload = planes.source_vertical;
    payload.extend_from_slice(&planes.source_horizontal);
    payload.extend_from_slice(&planes.destination_vertical);
    payload.extend_from_slice(&planes.destination_horizontal);
    Ok(payload)
}

fn expand_for_policy(
    original: &[u8],
    mapper: Mapper,
    policy: &AllocationPolicy,
    fill: u8,
) -> Result<Vec<u8>, OverworldWarpPatchSaveError> {
    if policy.search.end <= original.len() {
        return Ok(original.to_vec());
    }
    if !policy.fill_bytes.contains(&fill) {
        return Err(OverworldWarpPatchSaveError::Allocation(
            AllocationError::InvalidPolicy,
        ));
    }
    let mut image = RomImage::from_bytes(original.to_vec())?;
    image.expand(mapper, policy.search.end, fill)?;
    Ok(image.logical_bytes().to_vec())
}

fn publish_current_patch(
    bytes: &mut [u8],
    mapper: Mapper,
    patch_offset: usize,
    payload_offset: usize,
    entries: usize,
) -> Result<(), OverworldWarpPatchSaveError> {
    let count = u16::try_from(entries).map_err(|_| OverworldWarpPatchSaveError::LengthOverflow)?;
    bytes
        .get_mut(patch_offset + 0x10..patch_offset + 0x12)
        .ok_or(OverworldWarpPatchSaveError::LengthOverflow)?
        .copy_from_slice(&count.to_le_bytes());
    let plane_len = entries
        .checked_mul(2)
        .ok_or(OverworldWarpPatchSaveError::LengthOverflow)?;
    for (pointer_offset, addend) in
        [0x17, 0x27, 0x4c, 0x5e]
            .into_iter()
            .zip([0, plane_len, plane_len * 2, plane_len * 3])
    {
        let target = payload_offset
            .checked_add(addend)
            .ok_or(OverworldWarpPatchSaveError::LengthOverflow)?;
        let encoded = pc_to_snes(mapper, target)?.to_le_bytes();
        bytes
            .get_mut(patch_offset + pointer_offset..patch_offset + pointer_offset + 3)
            .ok_or(OverworldWarpPatchSaveError::LengthOverflow)?
            .copy_from_slice(&encoded[..3]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OverworldWarpLinkRomLayout, OverworldWarpPatchLocator};
    use lm_overworld::{OverworldWarpEndpoint, OverworldWarpLink};
    use lm_rats::FreeSpaceAllocator;
    use lm_rom::{RomImage, pc_to_snes};

    fn table(count: u16) -> OverworldWarpLinkTable {
        OverworldWarpLinkTable {
            links: (0..count)
                .map(|value| OverworldWarpLink {
                    source: OverworldWarpEndpoint {
                        packed_vertical: value,
                        horizontal_tile: value + 1,
                    },
                    destination: OverworldWarpEndpoint {
                        packed_vertical: value + 2,
                        horizontal_tile: value + 3,
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn growth_repoints_reopens_checksums_and_undoes_atomically() {
        let policy = AllocationPolicy::lorom(0x1000..0x8000);
        let mut bytes = vec![0xff; 0x8000];
        let old_payload = encode_payload(&table(30)).unwrap();
        let old = FreeSpaceAllocator::new(&mut bytes, policy.clone())
            .allocate(&old_payload)
            .unwrap();
        let patch = 0x300;
        let patch_pointer = pc_to_snes(Mapper::LoRom, patch).unwrap().to_le_bytes();
        bytes[0x100..0x104].copy_from_slice(&[
            0x22,
            patch_pointer[0],
            patch_pointer[1],
            patch_pointer[2],
        ]);
        bytes[patch + 0x3c..patch + 0x40].copy_from_slice(&[b'L', b'M', 0x10, 0x01]);
        publish_current_patch(&mut bytes, Mapper::LoRom, patch, old.payload.start, 30).unwrap();
        let locator = OverworldWarpPatchLocator {
            mapper: Mapper::LoRom,
            entry_hook_offset: 0x100,
            return_hook_offset: 0x110,
            fixed: OverworldWarpLinkRomLayout {
                mapper: Mapper::LoRom,
                source_vertical_offset: 0x400,
                source_horizontal_offset: 0x436,
                destination_vertical_offset: 0x46c,
                destination_horizontal_offset: 0x4a2,
                entries: 27,
            },
        };
        let return_pointer = pc_to_snes(Mapper::LoRom, patch + 0x40)
            .unwrap()
            .to_le_bytes();
        bytes[0x110..0x114].copy_from_slice(&[
            0x22,
            return_pointer[0],
            return_pointer[1],
            return_pointer[2],
        ]);
        let mut project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let loaded = project.load_overworld_warp_links_detected(locator).unwrap();
        project
            .save_installed_overworld_warp_links(&table(40), loaded.storage, &policy, 0x7fdc, 0xff)
            .unwrap();
        assert_eq!(
            project
                .load_overworld_warp_links_detected(locator)
                .unwrap()
                .table,
            table(40)
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), bytes);
    }
}
