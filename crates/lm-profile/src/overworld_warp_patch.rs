//! Lunar Magic-compatible expanded overworld warp/exit runtime for SMW US revision 0.

use lm_overworld::{OverworldWarpLinkTable, OverworldWarpLinkTableError};
use lm_project::{
    OverworldWarpPatchLocator, PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite,
    RelocatablePatchPlan,
};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

use crate::{SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_warp_link_layout};

pub const SMW_US_V1_OVERWORLD_WARP_ENTRY_HOOK_OFFSET: usize = 0x02_0509;
pub const SMW_US_V1_OVERWORLD_WARP_RETURN_HOOK_OFFSET: usize = 0x02_0566;
pub const SMW_US_V1_OVERWORLD_WARP_PATCH_SEARCH_START: usize = 0x08_0000;
pub const SMW_US_V1_OVERWORLD_WARP_PATCH_SEARCH_END: usize = 0x09_0000;

const ENTRY_HOOK_EXPECTED: [u8; 5] = [0xac, 0xb3, 0x0d, 0xb9, 0x11];
const RETURN_HOOK_EXPECTED: [u8; 4] = [0xeb, 0x29, 0x0f, 0x00];
const PATCH_TEMPLATE: [u8; 128] = [
    0xac, 0xb3, 0x0d, 0xb9, 0x11, 0x1f, 0x85, 0x01, 0x64, 0x00, 0xac, 0xd6, 0x0d, 0xc2, 0x30, 0xa2,
    0x00, 0x00, 0xca, 0xca, 0x30, 0x21, 0xbf, 0x00, 0x80, 0x00, 0x45, 0x00, 0xc9, 0x00, 0x02, 0xb0,
    0xf1, 0xd9, 0x1f, 0x1f, 0xd0, 0xec, 0xbf, 0x00, 0x80, 0x00, 0xd9, 0x21, 0x1f, 0xd0, 0xe3, 0x8a,
    0x4a, 0xe2, 0x32, 0x8d, 0xf6, 0x1d, 0x6b, 0xe2, 0x30, 0x6b, 0xff, 0xff, 0x4c, 0x4d, 0x10, 0x01,
    0xac, 0xd6, 0x0d, 0xae, 0xf6, 0x1d, 0xc2, 0x30, 0x8a, 0x0a, 0xaa, 0xbf, 0x00, 0x80, 0x00, 0x48,
    0x29, 0xff, 0x01, 0x99, 0x17, 0x1f, 0x4a, 0x4a, 0x4a, 0x4a, 0x99, 0x1f, 0x1f, 0xbf, 0x00, 0x80,
    0x00, 0x99, 0x19, 0x1f, 0x4a, 0x4a, 0x4a, 0x4a, 0x99, 0x21, 0x1f, 0x68, 0x4a, 0xeb, 0x29, 0x0f,
    0x00, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4c, 0x4d, 0x10, 0x01,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldWarpPatchBuildError {
    VanillaCapacity(usize),
    Table(OverworldWarpLinkTableError),
}

impl std::fmt::Display for OverworldWarpPatchBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded overworld warp patch build failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldWarpPatchBuildError {}

impl From<OverworldWarpLinkTableError> for OverworldWarpPatchBuildError {
    fn from(value: OverworldWarpLinkTableError) -> Self {
        Self::Table(value)
    }
}

#[must_use]
pub const fn smw_us_v1_overworld_warp_patch_locator() -> OverworldWarpPatchLocator {
    OverworldWarpPatchLocator {
        mapper: Mapper::LoRom,
        entry_hook_offset: SMW_US_V1_OVERWORLD_WARP_ENTRY_HOOK_OFFSET,
        return_hook_offset: SMW_US_V1_OVERWORLD_WARP_RETURN_HOOK_OFFSET,
        fixed: smw_us_v1_overworld_warp_link_layout(),
    }
}

#[must_use]
pub fn smw_us_v1_overworld_warp_allocation_policy() -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_OVERWORLD_WARP_PATCH_SEARCH_START..SMW_US_V1_OVERWORLD_WARP_PATCH_SEARCH_END,
    )
}

#[must_use]
pub fn smw_us_v1_overworld_warp_update_policy(image_len: usize) -> AllocationPolicy {
    let search_end = image_len.saturating_add(0x8000).min(0x40_0000);
    AllocationPolicy::lorom(SMW_US_V1_OVERWORLD_WARP_PATCH_SEARCH_START..search_end)
}

/// Returns the exact recovered current Lunar Magic 3.63 runtime before relocation fixups.
#[must_use]
pub const fn smw_us_v1_overworld_warp_runtime_template() -> [u8; 128] {
    PATCH_TEMPLATE
}

/// Builds the exact current Lunar Magic runtime and four-plane allocation.
///
/// # Errors
///
/// Requires 28–256 links and rejects table encoding overflow.
pub fn smw_us_v1_overworld_warp_installation_plan(
    table: &OverworldWarpLinkTable,
) -> Result<RelocatablePatchPlan, OverworldWarpPatchBuildError> {
    if table.links.len() <= 27 {
        return Err(OverworldWarpPatchBuildError::VanillaCapacity(
            table.links.len(),
        ));
    }
    let planes = table.encode_planes()?;
    let plane_len = planes.source_vertical.len();
    let mut patch = PATCH_TEMPLATE.to_vec();
    let count = u16::try_from(plane_len).map_err(|_| {
        OverworldWarpPatchBuildError::Table(OverworldWarpLinkTableError::TooManyLinks(
            table.links.len(),
        ))
    })?;
    patch[0x10..0x12].copy_from_slice(&count.to_le_bytes());
    let mut table_payload = planes.source_vertical;
    table_payload.extend_from_slice(&planes.source_horizontal);
    table_payload.extend_from_slice(&planes.destination_vertical);
    table_payload.extend_from_slice(&planes.destination_horizontal);
    let table_fixup = |offset, target_addend| PatchFixup {
        offset,
        target_payload: 1,
        target_addend,
        encoding: PatchFixupEncoding::Long24,
    };
    Ok(RelocatablePatchPlan {
        description: "install expanded native overworld warp links".into(),
        mapper: Mapper::LoRom,
        allocation: smw_us_v1_overworld_warp_allocation_policy(),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![
            PatchPayload {
                bytes: patch,
                fixups: vec![
                    table_fixup(0x17, 0),
                    table_fixup(0x27, plane_len),
                    table_fixup(0x4c, plane_len * 2),
                    table_fixup(0x5e, plane_len * 3),
                ],
            },
            PatchPayload {
                bytes: table_payload,
                fixups: Vec::new(),
            },
        ],
        writes: vec![
            PatchWrite {
                offset: SMW_US_V1_OVERWORLD_WARP_ENTRY_HOOK_OFFSET,
                expected: ENTRY_HOOK_EXPECTED.to_vec(),
                replacement: vec![0x22, 0, 0, 0, 0x60],
                fixups: vec![PatchFixup {
                    offset: 1,
                    target_payload: 0,
                    target_addend: 0,
                    encoding: PatchFixupEncoding::Long24,
                }],
            },
            PatchWrite {
                offset: SMW_US_V1_OVERWORLD_WARP_RETURN_HOOK_OFFSET,
                expected: RETURN_HOOK_EXPECTED.to_vec(),
                replacement: vec![0x22, 0, 0, 0],
                fixups: vec![PatchFixup {
                    offset: 1,
                    target_payload: 0,
                    target_addend: 0x40,
                    encoding: PatchFixupEncoding::Long24,
                }],
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::{OverworldWarpEndpoint, OverworldWarpLink};
    use lm_project::{OverworldWarpLinkStorage, Project};
    use lm_rom::RomImage;
    use std::path::PathBuf;

    fn expanded_table() -> OverworldWarpLinkTable {
        OverworldWarpLinkTable {
            links: (0_u16..30)
                .map(|index| OverworldWarpLink {
                    source: OverworldWarpEndpoint {
                        packed_vertical: index,
                        horizontal_tile: index + 0x100,
                    },
                    destination: OverworldWarpEndpoint {
                        packed_vertical: index + 0x200,
                        horizontal_tile: index + 0x300,
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn pristine_install_reopens_through_current_lunar_magic_contract_and_undoes() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            Project::open_supported(RomImage::from_bytes(bytes.clone()).unwrap()).unwrap();
        project
            .install_relocatable_patch(
                &smw_us_v1_overworld_warp_installation_plan(&expanded_table()).unwrap(),
            )
            .unwrap();
        let loaded = project
            .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())
            .unwrap();
        assert_eq!(loaded.table, expanded_table());
        assert!(matches!(
            loaded.storage,
            OverworldWarpLinkStorage::CurrentPatch { .. }
        ));
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), bytes);
    }
}
