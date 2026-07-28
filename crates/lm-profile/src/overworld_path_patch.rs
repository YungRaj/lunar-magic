//! Lunar Magic-compatible expanded special-path runtime for SMW US revision 0.

use crate::{SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_path_link_layout};
use lm_overworld::{OverworldPathLinkTable, OverworldPathLinkTableError};
use lm_project::{
    OverworldPathPatchLocator, PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite,
    RelocatablePatchPlan,
};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_OVERWORLD_PATH_HOOK_OFFSET: usize = 0x02_1a35;
pub const SMW_US_V1_OVERWORLD_PATH_PATCH_SEARCH_START: usize = 0x08_0000;
pub const SMW_US_V1_OVERWORLD_PATH_PATCH_SEARCH_END: usize = 0x09_0000;

const HOOK_EXPECTED: [u8; 5] = [0xa9, 0x1a, 0x00, 0x85, 0x02];
const RUNTIME_TEMPLATE: [u8; 112] = [
    0xac, 0xd6, 0x0d, 0xc2, 0x30, 0xa9, 0x0d, 0x00, 0x85, 0x02, 0xa2, 0x41, 0x00, 0xb9, 0x19, 0x1f,
    0xdf, 0x00, 0x80, 0x00, 0xd0, 0x47, 0xb9, 0x17, 0x1f, 0xdf, 0x02, 0x80, 0x00, 0xd0, 0x3e, 0xbf,
    0x04, 0x80, 0x00, 0x29, 0xff, 0x00, 0xcd, 0xc3, 0x13, 0xd0, 0x32, 0xbf, 0x00, 0x90, 0x00, 0x99,
    0x19, 0x1f, 0xbf, 0x02, 0x90, 0x00, 0x99, 0x17, 0x1f, 0xbf, 0x04, 0x90, 0x00, 0x29, 0xff, 0x00,
    0x8d, 0xc3, 0x13, 0xa5, 0x02, 0x0a, 0xaa, 0xbf, 0x00, 0xa0, 0x00, 0x29, 0xff, 0x00, 0x99, 0x21,
    0x1f, 0xbf, 0x01, 0xa0, 0x00, 0x29, 0xff, 0x00, 0x99, 0x1f, 0x1f, 0x80, 0x09, 0xc6, 0x02, 0xca,
    0xca, 0xca, 0xca, 0xca, 0x10, 0xa7, 0xe2, 0x30, 0x6b, 0x4c, 0x4d, 0x00, 0x01, 0xff, 0xff, 0xff,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldPathPatchBuildError {
    FixedCapacity(usize),
    EmptyTable,
    Table(OverworldPathLinkTableError),
}

impl std::fmt::Display for OverworldPathPatchBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded overworld path patch build failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldPathPatchBuildError {}

impl From<OverworldPathLinkTableError> for OverworldPathPatchBuildError {
    fn from(value: OverworldPathLinkTableError) -> Self {
        Self::Table(value)
    }
}

#[must_use]
pub const fn smw_us_v1_overworld_path_patch_locator() -> OverworldPathPatchLocator {
    OverworldPathPatchLocator {
        mapper: Mapper::LoRom,
        hook_offset: SMW_US_V1_OVERWORLD_PATH_HOOK_OFFSET,
        fixed: smw_us_v1_overworld_path_link_layout(),
    }
}

#[must_use]
pub fn smw_us_v1_overworld_path_allocation_policy() -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_OVERWORLD_PATH_PATCH_SEARCH_START..SMW_US_V1_OVERWORLD_PATH_PATCH_SEARCH_END,
    )
}

#[must_use]
pub fn smw_us_v1_overworld_path_update_policy(image_len: usize) -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_OVERWORLD_PATH_PATCH_SEARCH_START
            ..image_len.saturating_add(0x8000).min(0x40_0000),
    )
}

/// Builds the exact recovered current runtime and contiguous `5N + 5N + 2N` allocation.
///
/// # Errors
///
/// Requires 15–128 links and rejects encoding/count overflow.
pub fn smw_us_v1_overworld_path_installation_plan(
    table: &OverworldPathLinkTable,
) -> Result<RelocatablePatchPlan, OverworldPathPatchBuildError> {
    if table.links.is_empty() {
        return Err(OverworldPathPatchBuildError::EmptyTable);
    }
    if table.links.len() <= 14 {
        return Err(OverworldPathPatchBuildError::FixedCapacity(
            table.links.len(),
        ));
    }
    let planes = table.encode_planes()?;
    let source_len = planes.sources.len();
    let encoded_count = u16::try_from(table.links.len() - 1).map_err(|_| {
        OverworldPathPatchBuildError::Table(OverworldPathLinkTableError::TooManyLinks(
            table.links.len(),
        ))
    })?;
    let stride = u16::try_from((table.links.len() - 1) * 5).map_err(|_| {
        OverworldPathPatchBuildError::Table(OverworldPathLinkTableError::TooManyLinks(
            table.links.len(),
        ))
    })?;
    let mut runtime = RUNTIME_TEMPLATE.to_vec();
    runtime[6..8].copy_from_slice(&encoded_count.to_le_bytes());
    runtime[0x0b..0x0d].copy_from_slice(&stride.to_le_bytes());
    let mut payload = planes.sources;
    payload.extend_from_slice(&planes.destinations);
    payload.extend_from_slice(&planes.targets);
    let fixup = |offset, target_addend| PatchFixup {
        offset,
        target_payload: 1,
        target_addend,
        encoding: PatchFixupEncoding::Long24,
    };
    Ok(RelocatablePatchPlan {
        description: "install expanded native overworld path links".into(),
        mapper: Mapper::LoRom,
        allocation: smw_us_v1_overworld_path_allocation_policy(),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![
            PatchPayload {
                bytes: runtime,
                fixups: vec![
                    fixup(0x11, 0),
                    fixup(0x1a, 2),
                    fixup(0x20, 4),
                    fixup(0x2c, source_len),
                    fixup(0x33, source_len + 2),
                    fixup(0x3a, source_len + 4),
                    fixup(0x48, source_len * 2),
                    fixup(0x52, source_len * 2 + 1),
                ],
            },
            PatchPayload {
                bytes: payload,
                fixups: Vec::new(),
            },
        ],
        writes: vec![PatchWrite {
            offset: SMW_US_V1_OVERWORLD_PATH_HOOK_OFFSET,
            expected: HOOK_EXPECTED.to_vec(),
            replacement: vec![0x22, 0, 0, 0, 0x60],
            fixups: vec![PatchFixup {
                offset: 1,
                target_payload: 0,
                target_addend: 0,
                encoding: PatchFixupEncoding::Long24,
            }],
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::{OverworldEndpoint, OverworldPathLink, OverworldPathTarget};
    use lm_project::{OverworldPathLinkStorage, Project};
    use lm_rom::RomImage;
    use std::path::PathBuf;

    fn table(count: u16) -> OverworldPathLinkTable {
        OverworldPathLinkTable {
            links: (0..count)
                .map(|value| OverworldPathLink {
                    source: OverworldEndpoint {
                        x: value,
                        y: value + 1,
                        submap: u8::try_from(value % 7).unwrap(),
                    },
                    destination: OverworldEndpoint {
                        x: value + 2,
                        y: value + 3,
                        submap: u8::try_from((value + 1) % 7).unwrap(),
                    },
                    target: OverworldPathTarget {
                        y_tile: u8::try_from(value).unwrap(),
                        x_tile: u8::try_from(value + 1).unwrap(),
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn pristine_install_reopens_as_current_runtime_and_undoes() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        project
            .install_relocatable_patch(
                &smw_us_v1_overworld_path_installation_plan(&table(20)).unwrap(),
            )
            .unwrap();
        let loaded = project
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap();
        assert_eq!(loaded.table, table(20));
        assert!(matches!(
            loaded.storage,
            OverworldPathLinkStorage::CurrentPatch { .. }
        ));
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn installed_table_growth_republishes_all_eight_operands_and_undoes() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project = Project::open_supported(RomImage::from_bytes(original).unwrap()).unwrap();
        project
            .install_relocatable_patch(
                &smw_us_v1_overworld_path_installation_plan(&table(20)).unwrap(),
            )
            .unwrap();
        let installed = project
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap();
        let before_growth = project.save_snapshot();
        project
            .save_installed_overworld_path_links(
                &table(30),
                installed.storage,
                smw_us_v1_overworld_path_patch_locator(),
                &smw_us_v1_overworld_path_update_policy(project.rom.logical_len()),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert_eq!(
            project
                .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
                .unwrap()
                .table,
            table(30)
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), before_growth);
    }
}
