//! Failure-atomic migration of Lunar Magic's legacy overworld warp runtime.

use crate::{
    OverworldWarpLinkStorage, OverworldWarpPatchLocator, Project, payload::staging::commit_staged,
};
use lm_overworld::{OverworldWarpLinkTable, OverworldWarpLinkTableError};
use lm_rats::{
    AllocationError, AllocationPolicy, FreeSpaceAllocator, HEADER_LEN, ProtectedRange, parse_at,
};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes, snes_to_pc};

#[derive(Clone, Copy, Debug)]
pub struct OverworldWarpPatchMigrationOptions<'a> {
    pub locator: OverworldWarpPatchLocator,
    pub current_runtime: &'a [u8],
    pub allocation: &'a AllocationPolicy,
    pub checksum_field: usize,
    pub fill: u8,
}

#[derive(Debug)]
pub enum OverworldWarpPatchMigrationError {
    UnsupportedStorage,
    VanillaSizedTable(usize),
    RuntimeTemplateLength(usize),
    NonContiguousPlanes,
    MissingRuntimeAllocation,
    MissingPlaneAllocation,
    OverlappingAllocations,
    HookRange,
    HookMismatch,
    LegacyMarkerMismatch,
    LegacyLayoutMismatch,
    LengthOverflow,
    Table(OverworldWarpLinkTableError),
    Allocation(AllocationError),
    Rom(RomError),
    Commit(crate::PayloadSaveError),
}

impl std::fmt::Display for OverworldWarpPatchMigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "legacy overworld warp patch migration failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldWarpPatchMigrationError {}

impl From<OverworldWarpLinkTableError> for OverworldWarpPatchMigrationError {
    fn from(value: OverworldWarpLinkTableError) -> Self {
        Self::Table(value)
    }
}

impl From<AllocationError> for OverworldWarpPatchMigrationError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<RomError> for OverworldWarpPatchMigrationError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<crate::PayloadSaveError> for OverworldWarpPatchMigrationError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Replaces a recognized legacy runtime and table with the current Lunar Magic representation.
    ///
    /// The legacy runtime and four contiguous planes must each occupy one exact RATS allocation.
    /// Both old blocks are reclaimed only in a staging image; the current runtime, table, hooks,
    /// pointers, count, and checksum become one project history operation.
    ///
    /// # Errors
    ///
    /// Rejects non-legacy storage, small tables, malformed ownership, overlapping allocations,
    /// invalid templates, mapping/allocation failures, or commit errors without mutating the ROM.
    pub fn migrate_legacy_overworld_warp_patch(
        &mut self,
        table: &OverworldWarpLinkTable,
        storage: OverworldWarpLinkStorage,
        options: OverworldWarpPatchMigrationOptions<'_>,
    ) -> Result<bool, OverworldWarpPatchMigrationError> {
        let OverworldWarpPatchMigrationOptions {
            locator,
            current_runtime,
            allocation,
            checksum_field,
            fill,
        } = options;
        let OverworldWarpLinkStorage::LegacyPatch {
            patch_offset,
            planes,
        } = storage
        else {
            return Err(OverworldWarpPatchMigrationError::UnsupportedStorage);
        };
        if planes.mapper != locator.mapper {
            return Err(OverworldWarpPatchMigrationError::UnsupportedStorage);
        }
        if table.links.len() <= 27 {
            return Err(OverworldWarpPatchMigrationError::VanillaSizedTable(
                table.links.len(),
            ));
        }
        if current_runtime.len() != 0x80 {
            return Err(OverworldWarpPatchMigrationError::RuntimeTemplateLength(
                current_runtime.len(),
            ));
        }
        let table_payload = encode_payload(table)?;
        let original = self.rom.logical_bytes().to_vec();
        validate_legacy_hooks(&original, locator, patch_offset)?;
        let runtime = exact_block(&original, patch_offset, current_runtime.len(), true)?;
        if original.get(patch_offset + 0x3c..patch_offset + 0x40) != Some(&[0xff; 4]) {
            return Err(OverworldWarpPatchMigrationError::LegacyMarkerMismatch);
        }
        validate_legacy_layout(&original[runtime.payload.clone()], planes)?;
        let plane_len = planes
            .entries
            .checked_mul(2)
            .ok_or(OverworldWarpPatchMigrationError::LengthOverflow)?;
        if [
            planes.source_horizontal_offset,
            planes.destination_vertical_offset,
            planes.destination_horizontal_offset,
        ] != [
            planes.source_vertical_offset + plane_len,
            planes.source_vertical_offset + plane_len * 2,
            planes.source_vertical_offset + plane_len * 3,
        ] {
            return Err(OverworldWarpPatchMigrationError::NonContiguousPlanes);
        }
        let old_table_len = plane_len
            .checked_mul(4)
            .ok_or(OverworldWarpPatchMigrationError::LengthOverflow)?;
        let old_table = exact_block(
            &original,
            planes.source_vertical_offset,
            old_table_len,
            false,
        )?;
        if overlaps(&runtime.full_range(), &old_table.full_range()) {
            return Err(OverworldWarpPatchMigrationError::OverlappingAllocations);
        }
        let entry_hook = checked_range(locator.entry_hook_offset, 5, original.len())?;
        let return_hook = checked_range(locator.return_hook_offset, 4, original.len())?;
        let checksum_range = checked_range(checksum_field, 4, original.len())?;
        let protected = [
            ProtectedRange(checksum_range),
            ProtectedRange(entry_hook),
            ProtectedRange(return_hook),
        ];
        let (mut staged, runtime_block, table_block) = allocate_replacements(
            &original,
            ReplacementRequest {
                mapper: planes.mapper,
                allocation,
                old: [&runtime, &old_table],
                runtime: current_runtime,
                table: &table_payload,
                protected: &protected,
                fill,
            },
        )?;
        let mut runtime_bytes = current_runtime.to_vec();
        publish_runtime(
            &mut runtime_bytes,
            planes.mapper,
            table_block.payload.start,
            table.links.len(),
        )?;
        staged[runtime_block.payload.clone()].copy_from_slice(&runtime_bytes);
        publish_hooks(&mut staged, locator, runtime_block.payload.start)?;
        let checksum = compute_snes_checksum(&staged, checksum_field)?;
        staged[checksum_field..checksum_field + 4].copy_from_slice(&checksum.encoded());
        commit_migration(self, &original, &staged)
    }
}

fn commit_migration(
    project: &mut Project,
    original: &[u8],
    staged: &[u8],
) -> Result<bool, OverworldWarpPatchMigrationError> {
    if staged == original {
        return Ok(false);
    }
    commit_staged(
        project,
        "migrate legacy native overworld warp patch".into(),
        original,
        staged,
    )?;
    Ok(true)
}

fn exact_block(
    bytes: &[u8],
    payload_offset: usize,
    payload_len: usize,
    runtime: bool,
) -> Result<lm_rats::RatsBlock, OverworldWarpPatchMigrationError> {
    let error = || {
        if runtime {
            OverworldWarpPatchMigrationError::MissingRuntimeAllocation
        } else {
            OverworldWarpPatchMigrationError::MissingPlaneAllocation
        }
    };
    let header = payload_offset.checked_sub(HEADER_LEN).ok_or_else(error)?;
    let block = parse_at(bytes, header).map_err(|_| error())?;
    if block.payload.start != payload_offset || block.payload.len() != payload_len {
        return Err(error());
    }
    Ok(block)
}

fn encode_payload(
    table: &OverworldWarpLinkTable,
) -> Result<Vec<u8>, OverworldWarpPatchMigrationError> {
    let planes = table.encode_planes()?;
    let mut payload = planes.source_vertical;
    payload.extend_from_slice(&planes.source_horizontal);
    payload.extend_from_slice(&planes.destination_vertical);
    payload.extend_from_slice(&planes.destination_horizontal);
    Ok(payload)
}

fn expanded_image(
    original: &[u8],
    mapper: Mapper,
    allocation: &AllocationPolicy,
    fill: u8,
) -> Result<Vec<u8>, OverworldWarpPatchMigrationError> {
    if allocation.search.end <= original.len() {
        return Ok(original.to_vec());
    }
    if !allocation.fill_bytes.contains(&fill) {
        return Err(AllocationError::InvalidPolicy.into());
    }
    let mut image = RomImage::from_bytes(original.to_vec())?;
    image.expand(mapper, allocation.search.end, fill)?;
    Ok(image.logical_bytes().to_vec())
}

#[derive(Clone, Copy)]
struct ReplacementRequest<'a> {
    mapper: Mapper,
    allocation: &'a AllocationPolicy,
    old: [&'a lm_rats::RatsBlock; 2],
    runtime: &'a [u8],
    table: &'a [u8],
    protected: &'a [ProtectedRange],
    fill: u8,
}

fn allocate_replacements(
    original: &[u8],
    request: ReplacementRequest<'_>,
) -> Result<(Vec<u8>, lm_rats::RatsBlock, lm_rats::RatsBlock), OverworldWarpPatchMigrationError> {
    let ReplacementRequest {
        mapper,
        allocation,
        old,
        runtime,
        table,
        protected,
        fill,
    } = request;
    let attempt = |mut staged: Vec<u8>, mut policy: AllocationPolicy| {
        for block in old {
            staged[block.full_range()].fill(fill);
        }
        policy.protected.extend_from_slice(protected);
        let runtime_block =
            FreeSpaceAllocator::new(&mut staged, policy.clone()).allocate(runtime)?;
        let table_block = FreeSpaceAllocator::new(&mut staged, policy).allocate(table)?;
        Ok((staged, runtime_block, table_block))
    };
    let mut in_place_policy = allocation.clone();
    in_place_policy.search.end = in_place_policy.search.end.min(original.len());
    match attempt(original.to_vec(), in_place_policy) {
        Ok(result) => Ok(result),
        Err(AllocationError::NoSpace { .. }) if allocation.search.end > original.len() => {
            Ok(attempt(
                expanded_image(original, mapper, allocation, fill)?,
                allocation.clone(),
            )?)
        }
        Err(error) => Err(error.into()),
    }
}

fn publish_runtime(
    runtime: &mut [u8],
    mapper: Mapper,
    table_offset: usize,
    entries: usize,
) -> Result<(), OverworldWarpPatchMigrationError> {
    let count = entries
        .checked_mul(2)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(OverworldWarpPatchMigrationError::LengthOverflow)?;
    runtime[0x10..0x12].copy_from_slice(&count.to_le_bytes());
    let plane_len = entries
        .checked_mul(2)
        .ok_or(OverworldWarpPatchMigrationError::LengthOverflow)?;
    for (operand, addend) in
        [0x17, 0x27, 0x4c, 0x5e]
            .into_iter()
            .zip([0, plane_len, plane_len * 2, plane_len * 3])
    {
        let target = table_offset
            .checked_add(addend)
            .ok_or(OverworldWarpPatchMigrationError::LengthOverflow)?;
        runtime[operand..operand + 3]
            .copy_from_slice(&pc_to_snes(mapper, target)?.to_le_bytes()[..3]);
    }
    Ok(())
}

fn publish_hooks(
    bytes: &mut [u8],
    locator: OverworldWarpPatchLocator,
    runtime_offset: usize,
) -> Result<(), OverworldWarpPatchMigrationError> {
    let runtime = pc_to_snes(locator.mapper, runtime_offset)?.to_le_bytes();
    let continuation = pc_to_snes(locator.mapper, runtime_offset + 0x40)?.to_le_bytes();
    let entry = checked_range(locator.entry_hook_offset, 5, bytes.len())?;
    let return_hook = checked_range(locator.return_hook_offset, 4, bytes.len())?;
    bytes[entry].copy_from_slice(&[0x22, runtime[0], runtime[1], runtime[2], 0x60]);
    bytes[return_hook].copy_from_slice(&[0x22, continuation[0], continuation[1], continuation[2]]);
    Ok(())
}

fn checked_range(
    offset: usize,
    len: usize,
    image_len: usize,
) -> Result<std::ops::Range<usize>, OverworldWarpPatchMigrationError> {
    let end = offset
        .checked_add(len)
        .ok_or(OverworldWarpPatchMigrationError::HookRange)?;
    if end > image_len {
        return Err(OverworldWarpPatchMigrationError::HookRange);
    }
    Ok(offset..end)
}

fn validate_legacy_hooks(
    bytes: &[u8],
    locator: OverworldWarpPatchLocator,
    patch_offset: usize,
) -> Result<(), OverworldWarpPatchMigrationError> {
    let entry = bytes
        .get(checked_range(locator.entry_hook_offset, 4, bytes.len())?)
        .ok_or(OverworldWarpPatchMigrationError::HookRange)?;
    let return_hook = bytes
        .get(checked_range(locator.return_hook_offset, 4, bytes.len())?)
        .ok_or(OverworldWarpPatchMigrationError::HookRange)?;
    if entry[0] != 0x22
        || return_hook[0] != 0x22
        || decode_hook_pointer(locator.mapper, entry)? != patch_offset
        || decode_hook_pointer(locator.mapper, return_hook)? != patch_offset + 0x40
    {
        return Err(OverworldWarpPatchMigrationError::HookMismatch);
    }
    Ok(())
}

fn decode_hook_pointer(
    mapper: Mapper,
    hook: &[u8],
) -> Result<usize, OverworldWarpPatchMigrationError> {
    let address = u32::from(hook[1]) | (u32::from(hook[2]) << 8) | (u32::from(hook[3]) << 16);
    Ok(snes_to_pc(mapper, address)?)
}

fn validate_legacy_layout(
    runtime: &[u8],
    planes: crate::OverworldWarpLinkRomLayout,
) -> Result<(), OverworldWarpPatchMigrationError> {
    let encoded_count = usize::from(runtime[0x10]);
    let count = if encoded_count == 0 {
        OverworldWarpLinkTable::MAX_LINKS
    } else {
        encoded_count
    };
    let expected = [
        planes.source_vertical_offset,
        planes.source_horizontal_offset,
        planes.destination_vertical_offset,
        planes.destination_horizontal_offset,
    ];
    let mut actual = [0; 4];
    for (index, operand) in [0x14, 0x24, 0x47, 0x59].into_iter().enumerate() {
        let address = u32::from(runtime[operand])
            | (u32::from(runtime[operand + 1]) << 8)
            | (u32::from(runtime[operand + 2]) << 16);
        actual[index] = snes_to_pc(planes.mapper, address)?;
    }
    if count != planes.entries || actual != expected {
        return Err(OverworldWarpPatchMigrationError::LegacyLayoutMismatch);
    }
    Ok(())
}

fn overlaps(left: &std::ops::Range<usize>, right: &std::ops::Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
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

    fn legacy_project() -> (
        Project,
        OverworldWarpPatchLocator,
        AllocationPolicy,
        Vec<u8>,
    ) {
        let policy = AllocationPolicy::lorom(0x1000..0x8000);
        let mut bytes = vec![0xff; 0x8000];
        let legacy_runtime = FreeSpaceAllocator::new(&mut bytes, policy.clone())
            .allocate(&[0xff; 0x80])
            .unwrap();
        let payload = encode_payload(&table(30)).unwrap();
        let legacy_table = FreeSpaceAllocator::new(&mut bytes, policy.clone())
            .allocate(&payload)
            .unwrap();
        let patch = legacy_runtime.payload.start;
        bytes[patch + 0x10] = 30;
        let plane_len = 60;
        for (operand, addend) in
            [0x14, 0x24, 0x47, 0x59]
                .into_iter()
                .zip([0, plane_len, plane_len * 2, plane_len * 3])
        {
            let target = legacy_table.payload.start + addend;
            bytes[patch + operand..patch + operand + 3]
                .copy_from_slice(&pc_to_snes(Mapper::LoRom, target).unwrap().to_le_bytes()[..3]);
        }
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
        let entry = pc_to_snes(Mapper::LoRom, patch).unwrap().to_le_bytes();
        let return_target = pc_to_snes(Mapper::LoRom, patch + 0x40)
            .unwrap()
            .to_le_bytes();
        bytes[0x100..0x105].copy_from_slice(&[0x22, entry[0], entry[1], entry[2], 0x60]);
        bytes[0x110..0x114].copy_from_slice(&[
            0x22,
            return_target[0],
            return_target[1],
            return_target[2],
        ]);
        let original = bytes.clone();
        (
            Project::new(RomImage::from_bytes(bytes).unwrap()),
            locator,
            policy,
            original,
        )
    }

    #[test]
    fn legacy_blocks_migrate_reopen_checksum_and_undo_as_one_operation() {
        let (mut project, locator, policy, original) = legacy_project();
        let loaded = project.load_overworld_warp_links_detected(locator).unwrap();
        assert!(matches!(
            loaded.storage,
            OverworldWarpLinkStorage::LegacyPatch { .. }
        ));
        let mut runtime = [0xff; 0x80];
        runtime[0x3c..0x40].copy_from_slice(&[b'L', b'M', 0x10, 0x01]);
        project
            .migrate_legacy_overworld_warp_patch(
                &loaded.table,
                loaded.storage,
                OverworldWarpPatchMigrationOptions {
                    locator,
                    current_runtime: &runtime,
                    allocation: &policy,
                    checksum_field: 0x7fdc,
                    fill: 0xff,
                },
            )
            .unwrap();
        let reopened = project.load_overworld_warp_links_detected(locator).unwrap();
        assert_eq!(reopened.table, loaded.table);
        assert!(matches!(
            reopened.storage,
            OverworldWarpLinkStorage::CurrentPatch { .. }
        ));
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn missing_runtime_ownership_fails_without_mutation() {
        let (mut project, locator, policy, _) = legacy_project();
        let loaded = project.load_overworld_warp_links_detected(locator).unwrap();
        project
            .rom
            .write(
                match loaded.storage {
                    OverworldWarpLinkStorage::LegacyPatch { patch_offset, .. } => {
                        patch_offset - HEADER_LEN
                    }
                    _ => unreachable!(),
                },
                b"NOPE",
            )
            .unwrap();
        let before = project.save_snapshot();
        let mut runtime = [0xff; 0x80];
        runtime[0x3c..0x40].copy_from_slice(&[b'L', b'M', 0x10, 0x01]);
        assert!(
            project
                .migrate_legacy_overworld_warp_patch(
                    &loaded.table,
                    loaded.storage,
                    OverworldWarpPatchMigrationOptions {
                        locator,
                        current_runtime: &runtime,
                        allocation: &policy,
                        checksum_field: 0x7fdc,
                        fill: 0xff,
                    },
                )
                .is_err()
        );
        assert_eq!(project.save_snapshot(), before);
    }
}
