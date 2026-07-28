//! SMW US revision-0 native overworld level-name tables and Lunar Magic runtime.

use lm_overworld::{NativeOverworldLevelNameError, NativeOverworldLevelNameTable};
use lm_project::{
    OverworldLevelNameLocator, PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite,
    RelocatablePatchPlan,
};
use lm_rats::AllocationPolicy;
use lm_rom::{Mapper, pc_to_snes};

use crate::SMW_US_V1_CHECKSUM_FIELD;

pub const SMW_US_V1_OVERWORLD_NAME_CODES_OFFSET: usize = 0x02_20fc;
pub const SMW_US_V1_OVERWORLD_NAME_SEGMENT_OFFSETS_OFFSET: usize = 0x02_1c91;
pub const SMW_US_V1_OVERWORLD_NAME_TEXT_OFFSET: usize = 0x02_1ac5;
pub const SMW_US_V1_OVERWORLD_NAME_TEXT_LEN: usize = 0x01cc;
pub const SMW_US_V1_OVERWORLD_NAME_RUNTIME_OFFSET: usize = 0x01_bb20;
pub const SMW_US_V1_OVERWORLD_NAME_PRIMARY_HOOK_OFFSET: usize = 0x02_1549;
pub const SMW_US_V1_OVERWORLD_NAME_SECONDARY_HOOK_OFFSET: usize = 0x02_0e81;
pub const SMW_US_V1_OVERWORLD_NAME_SEARCH_START: usize = 0x08_0000;
pub const SMW_US_V1_OVERWORLD_NAME_SEARCH_END: usize = 0x09_0000;

const PRIMARY_EXPECTED: [u8; 10] = [0x0a, 0xaa, 0xbd, 0xfc, 0xa0, 0x85, 0x00, 0x20, 0x07, 0x9d];
const SECONDARY_EXPECTED: [u8; 10] = [0x0a, 0xaa, 0xbd, 0xfc, 0xa0, 0x85, 0x00, 0x20, 0x07, 0x9d];
const RUNTIME_TEMPLATE: [u8; 0x60] = [
    0x85, 0x02, 0x0a, 0x0a, 0x0a, 0x0a, 0x85, 0x00, 0xa5, 0x02, 0x0a, 0x18, 0x65, 0x02, 0x65, 0x00,
    0xaa, 0x8b, 0xf4, 0x7f, 0x7f, 0xab, 0xab, 0xad, 0x7b, 0x83, 0xa8, 0x18, 0x69, 0x26, 0x00, 0x85,
    0x02, 0x18, 0x69, 0x04, 0x00, 0x8d, 0x7b, 0x83, 0xa9, 0x00, 0x25, 0x99, 0x7f, 0x83, 0xa9, 0x50,
    0x8b, 0x99, 0x7d, 0x83, 0xe2, 0x20, 0xbf, 0x00, 0x80, 0x10, 0x99, 0x81, 0x83, 0xa9, 0x39, 0x99,
    0x82, 0x83, 0xc8, 0xc8, 0xe8, 0xc4, 0x02, 0x90, 0xed, 0xa9, 0xff, 0x99, 0x81, 0x83, 0xc2, 0x20,
    0xab, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldLevelNamePatchBuildError {
    EmptyTable,
    Table(NativeOverworldLevelNameError),
    Mapping,
}

impl std::fmt::Display for OverworldLevelNamePatchBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded overworld level-name patch build failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldLevelNamePatchBuildError {}

impl From<NativeOverworldLevelNameError> for OverworldLevelNamePatchBuildError {
    fn from(value: NativeOverworldLevelNameError) -> Self {
        Self::Table(value)
    }
}

#[must_use]
pub const fn smw_us_v1_overworld_level_name_locator() -> OverworldLevelNameLocator {
    OverworldLevelNameLocator {
        mapper: Mapper::LoRom,
        primary_hook_offset: SMW_US_V1_OVERWORLD_NAME_PRIMARY_HOOK_OFFSET,
        secondary_hook_offset: SMW_US_V1_OVERWORLD_NAME_SECONDARY_HOOK_OFFSET,
        fixed_runtime_offset: SMW_US_V1_OVERWORLD_NAME_RUNTIME_OFFSET,
        vanilla_codes_offset: SMW_US_V1_OVERWORLD_NAME_CODES_OFFSET,
        vanilla_offsets_offset: SMW_US_V1_OVERWORLD_NAME_SEGMENT_OFFSETS_OFFSET,
        vanilla_text_offset: SMW_US_V1_OVERWORLD_NAME_TEXT_OFFSET,
        vanilla_text_len: SMW_US_V1_OVERWORLD_NAME_TEXT_LEN,
    }
}

#[must_use]
pub const fn smw_us_v1_overworld_level_name_runtime() -> &'static [u8; 0x60] {
    &RUNTIME_TEMPLATE
}

#[must_use]
pub fn smw_us_v1_overworld_level_name_allocation_policy() -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_OVERWORLD_NAME_SEARCH_START..SMW_US_V1_OVERWORLD_NAME_SEARCH_END,
    )
}

/// Builds the exact fixed Lunar Magic runtime with one relocatable direct-name table.
///
/// # Errors
///
/// Rejects empty, excessive, noncanonical, or lossy native tables and mapping failures.
pub fn smw_us_v1_overworld_level_name_installation_plan(
    table: &NativeOverworldLevelNameTable,
) -> Result<RelocatablePatchPlan, OverworldLevelNamePatchBuildError> {
    let payload = table.encode()?;
    if payload.is_empty() {
        return Err(OverworldLevelNamePatchBuildError::EmptyTable);
    }
    let runtime_address = pc_to_snes(Mapper::LoRom, SMW_US_V1_OVERWORLD_NAME_RUNTIME_OFFSET)
        .map_err(|_| OverworldLevelNamePatchBuildError::Mapping)?
        .to_le_bytes();
    let mut hook = vec![
        0x22,
        runtime_address[0],
        runtime_address[1],
        runtime_address[2],
    ];
    hook.extend_from_slice(&[0xea; 6]);
    Ok(RelocatablePatchPlan {
        description: "install expanded native overworld level names".into(),
        mapper: Mapper::LoRom,
        allocation: smw_us_v1_overworld_level_name_allocation_policy(),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![PatchPayload {
            bytes: payload,
            fixups: Vec::new(),
        }],
        writes: vec![
            PatchWrite {
                offset: SMW_US_V1_OVERWORLD_NAME_RUNTIME_OFFSET,
                expected: vec![0xff; RUNTIME_TEMPLATE.len()],
                replacement: RUNTIME_TEMPLATE.to_vec(),
                fixups: vec![PatchFixup {
                    offset: 0x37,
                    target_payload: 0,
                    target_addend: 0,
                    encoding: PatchFixupEncoding::Long24,
                }],
            },
            PatchWrite {
                offset: SMW_US_V1_OVERWORLD_NAME_PRIMARY_HOOK_OFFSET,
                expected: PRIMARY_EXPECTED.to_vec(),
                replacement: hook.clone(),
                fixups: Vec::new(),
            },
            PatchWrite {
                offset: SMW_US_V1_OVERWORLD_NAME_SECONDARY_HOOK_OFFSET,
                expected: SECONDARY_EXPECTED.to_vec(),
                replacement: hook,
                fixups: Vec::new(),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::OverworldLevelName;
    use lm_project::{OverworldLevelNameStorage, Project};
    use lm_rom::RomImage;
    use std::path::PathBuf;

    #[test]
    fn pristine_names_decode_and_expanded_install_reopens_and_undoes() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let vanilla = project
            .load_overworld_level_names_detected(
                smw_us_v1_overworld_level_name_locator(),
                smw_us_v1_overworld_level_name_runtime(),
            )
            .unwrap();
        assert_eq!(vanilla.table.names.len(), 93);
        assert!(matches!(
            vanilla.storage,
            OverworldLevelNameStorage::Vanilla
        ));
        let table = NativeOverworldLevelNameTable {
            names: (0..100)
                .map(|slot| OverworldLevelName {
                    level: NativeOverworldLevelNameTable::level_for_slot(slot).unwrap(),
                    tiles: [u8::try_from(slot).unwrap(); OverworldLevelName::TILE_COUNT],
                    raw_flags: 0,
                })
                .collect(),
        };
        project
            .install_relocatable_patch(
                &smw_us_v1_overworld_level_name_installation_plan(&table).unwrap(),
            )
            .unwrap();
        let loaded = project
            .load_overworld_level_names_detected(
                smw_us_v1_overworld_level_name_locator(),
                smw_us_v1_overworld_level_name_runtime(),
            )
            .unwrap();
        assert_eq!(loaded.table, table);
        assert!(matches!(
            loaded.storage,
            OverworldLevelNameStorage::Expanded { .. }
        ));
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
    }
}
