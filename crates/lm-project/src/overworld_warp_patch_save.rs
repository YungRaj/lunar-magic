//! Transactional updates for an installed current Lunar Magic warp-link patch.

use crate::{OverworldWarpLinkStorage, Project, payload::staging::commit_staged};
use lm_overworld::{OverworldWarpLinkTable, OverworldWarpLinkTableError};
use lm_rats::{AllocationError, AllocationPolicy, FreeSpaceAllocator, RatsBlock, scan};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes};

#[derive(Debug)]
pub enum OverworldWarpPatchSaveError {
    UnsupportedStorage,
    InvalidEntryCount(usize),
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
    /// Requires current patch storage and 1–256 entries. The old four planes may share one exact
    /// RATS allocation, use Lunar Magic's separate exact per-plane owners, or reside in untagged
    /// fixed space. Every byte in each reclaimed owner must belong to those planes; non-exclusive
    /// and unowned source storage is preserved while the authenticated runtime is repointed.
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
        if table.links.is_empty() {
            return Err(OverworldWarpPatchSaveError::InvalidEntryCount(
                table.links.len(),
            ));
        }
        let encoded = table.encode_planes()?;
        let plane_payloads = [
            encoded.source_vertical.as_slice(),
            encoded.source_horizontal.as_slice(),
            encoded.destination_vertical.as_slice(),
            encoded.destination_horizontal.as_slice(),
        ];
        let payload = encode_payload(table)?;
        let old_plane_len = planes
            .entries
            .checked_mul(2)
            .ok_or(OverworldWarpPatchSaveError::LengthOverflow)?;
        let original = self.rom.logical_bytes().to_vec();
        let offsets = [
            planes.source_vertical_offset,
            planes.source_horizontal_offset,
            planes.destination_vertical_offset,
            planes.destination_horizontal_offset,
        ];
        let old = reclaimable_plane_owners(&original, offsets, old_plane_len)?;
        if planes.entries == table.links.len()
            && offsets
                .into_iter()
                .zip(plane_payloads)
                .all(|(offset, expected)| {
                    original.get(offset..offset + expected.len()) == Some(expected)
                })
        {
            return Ok(false);
        }
        let (mut staged, replacement) = replace_owners_with_optional_expansion(
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

fn replace_owners_with_optional_expansion(
    original: &[u8],
    mapper: Mapper,
    policy: &AllocationPolicy,
    old: &[RatsBlock],
    payload: &[u8],
    fill: u8,
) -> Result<(Vec<u8>, lm_rats::RatsBlock), OverworldWarpPatchSaveError> {
    let mut current_policy = policy.clone();
    current_policy.search.end = current_policy.search.end.min(original.len());
    let mut staged = original.to_vec();
    match erase_and_allocate(&mut staged, current_policy, old, payload, fill) {
        Ok(block) => Ok((staged, block)),
        Err(AllocationError::NoSpace { .. }) if policy.search.end > original.len() => {
            let mut expanded = expand_for_policy(original, mapper, policy, fill)?;
            let block = erase_and_allocate(&mut expanded, policy.clone(), old, payload, fill)?;
            Ok((expanded, block))
        }
        Err(error) => Err(error.into()),
    }
}

fn erase_and_allocate(
    bytes: &mut [u8],
    policy: AllocationPolicy,
    old: &[RatsBlock],
    payload: &[u8],
    fill: u8,
) -> Result<RatsBlock, AllocationError> {
    let mut allocator = FreeSpaceAllocator::new(bytes, policy);
    for block in old {
        allocator.erase(block, fill)?;
    }
    allocator.allocate(payload)
}

fn reclaimable_plane_owners(
    bytes: &[u8],
    offsets: [usize; 4],
    plane_len: usize,
) -> Result<Vec<RatsBlock>, OverworldWarpPatchSaveError> {
    let blocks = scan(bytes);
    let mut owners = Vec::<RatsBlock>::new();
    let mut ranges = Vec::with_capacity(4);
    for offset in offsets {
        let end = offset
            .checked_add(plane_len)
            .ok_or(OverworldWarpPatchSaveError::LengthOverflow)?;
        let containing = blocks
            .iter()
            .filter(|block| block.payload.start <= offset && end <= block.payload.end)
            .collect::<Vec<_>>();
        match containing.as_slice() {
            [] => {
                if blocks
                    .iter()
                    .any(|block| block.payload.start < end && offset < block.payload.end)
                {
                    return Err(OverworldWarpPatchSaveError::MissingAllocation);
                }
            }
            [owner] => {
                if !owners.iter().any(|known| known == *owner) {
                    owners.push((*owner).clone());
                }
            }
            _ => return Err(OverworldWarpPatchSaveError::MissingAllocation),
        }
        ranges.push(offset..end);
    }
    ranges.sort_by_key(|range| range.start);
    if ranges.windows(2).any(|pair| pair[0].end > pair[1].start) {
        return Err(OverworldWarpPatchSaveError::NonContiguousPlanes);
    }
    owners.retain(|owner| {
        let covered = ranges
            .iter()
            .filter(|range| owner.payload.start <= range.start && range.end <= owner.payload.end)
            .collect::<Vec<_>>();
        covered
            .first()
            .is_some_and(|first| first.start == owner.payload.start)
            && covered
                .last()
                .is_some_and(|last| last.end == owner.payload.end)
            && !covered.windows(2).any(|pair| pair[0].end != pair[1].start)
    });
    Ok(owners)
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
    let count = entries
        .checked_mul(2)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(OverworldWarpPatchSaveError::LengthOverflow)?;
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

    #[test]
    fn separate_exact_plane_owners_are_reclaimed_and_republished_atomically() {
        let policy = AllocationPolicy::lorom(0x1000..0x8000);
        let mut bytes = vec![0xff; 0x8000];
        let encoded = table(30).encode_planes().unwrap();
        let old = {
            let mut allocator = FreeSpaceAllocator::new(&mut bytes, policy.clone());
            [
                allocator.allocate(&encoded.source_vertical).unwrap(),
                allocator.allocate(&encoded.source_horizontal).unwrap(),
                allocator.allocate(&encoded.destination_vertical).unwrap(),
                allocator.allocate(&encoded.destination_horizontal).unwrap(),
            ]
        };
        let patch = 0x300;
        let patch_pointer = pc_to_snes(Mapper::LoRom, patch).unwrap().to_le_bytes();
        bytes[0x100..0x104].copy_from_slice(&[
            0x22,
            patch_pointer[0],
            patch_pointer[1],
            patch_pointer[2],
        ]);
        bytes[patch + 0x10..patch + 0x12].copy_from_slice(&60u16.to_le_bytes());
        bytes[patch + 0x3c..patch + 0x40].copy_from_slice(&[b'L', b'M', 0x10, 0x01]);
        for (pointer_offset, owner) in [0x17, 0x27, 0x4c, 0x5e].into_iter().zip(&old) {
            let pointer = pc_to_snes(Mapper::LoRom, owner.payload.start)
                .unwrap()
                .to_le_bytes();
            bytes[patch + pointer_offset..patch + pointer_offset + 3]
                .copy_from_slice(&pointer[..3]);
        }
        let return_pointer = pc_to_snes(Mapper::LoRom, patch + 0x40)
            .unwrap()
            .to_le_bytes();
        bytes[0x110..0x114].copy_from_slice(&[
            0x22,
            return_pointer[0],
            return_pointer[1],
            return_pointer[2],
        ]);
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
        let mut replacement = table(40);
        replacement.links[0].destination.horizontal_tile ^= 1;
        let mut project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let loaded = project.load_overworld_warp_links_detected(locator).unwrap();
        assert_eq!(loaded.table, table(30));
        project
            .save_installed_overworld_warp_links(
                &replacement,
                loaded.storage,
                &policy,
                0x7fdc,
                0xff,
            )
            .unwrap();
        assert_eq!(
            project
                .load_overworld_warp_links_detected(locator)
                .unwrap()
                .table,
            replacement
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), bytes);
        assert!(project.redo().unwrap());
        assert_eq!(
            project
                .load_overworld_warp_links_detected(locator)
                .unwrap()
                .table,
            replacement
        );
    }
}
