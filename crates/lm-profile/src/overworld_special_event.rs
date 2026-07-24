//! Lunar Magic-compatible special-event reveal patch for SMW US revision 0.

use crate::SMW_US_V1_CHECKSUM_FIELD;
use lm_overworld::{SpecialEventRevealError, SpecialEventRevealTable};
use lm_project::{
    PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan,
    SpecialEventRevealPatchLocator,
};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_SPECIAL_EVENT_SOURCE_OPERAND: usize = 0x02_669c;
pub const SMW_US_V1_SPECIAL_EVENT_DESTINATION_OPERAND: usize = 0x02_6ec9;
pub const SMW_US_V1_SPECIAL_EVENT_DIRECTION_OPERAND: usize = 0x02_667c;
pub const SMW_US_V1_SPECIAL_EVENT_FIXED_SOURCE: usize = 0x02_65b6;
pub const SMW_US_V1_SPECIAL_EVENT_FIXED_DESTINATION: usize = 0x02_6587;
pub const SMW_US_V1_SPECIAL_EVENT_FIXED_DIRECTIONS: usize = 0x02_65d6;
pub const SMW_US_V1_SPECIAL_EVENT_SEARCH_START: usize = 0x08_0000;
pub const SMW_US_V1_SPECIAL_EVENT_SEARCH_END: usize = 0x09_0000;

const FULL_HOOK: usize = 0x02_6ddd;
const SECONDARY_HOOK: usize = 0x02_6ec3;
const OPCODE_PATCH: usize = 0x02_6edd;
const NOP_PATCH: usize = 0x02_6ee1;
const INLINE_PATCH: usize = 0x02_6f27;
const POINTER_HOOKS: [usize; 2] = [0x02_66c5, 0x02_6ef1];
const HELPER_OFFSET: usize = 0x03_7540;

const FULL_RUNTIME: [u8; 64] = [
    0xe2, 0x20, 0xbf, 0x00, 0xc8, 0x7f, 0xeb, 0xbf, 0x00, 0xc8, 0x7e, 0xc2, 0x20, 0x0a, 0x22, 0x40,
    0xf5, 0x06, 0x85, 0x0a, 0xa0, 0x00, 0x00, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0x29, 0xff, 0x00, 0x0a, 0xaa, 0xbf, 0xb6, 0xe5, 0x04, 0x85, 0x02, 0xda, 0xaa, 0x22, 0x00, 0x80,
    0x00, 0xfa, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4c, 0x4d, 0x00, 0x01,
];
const POINTER_RUNTIME: [u8; 48] = [
    0xe2, 0x20, 0x18, 0x69, 0x10, 0xc2, 0x20, 0x90, 0x03, 0x69, 0xff, 0x01, 0x6b, 0xff, 0xff, 0xff,
    0x48, 0x29, 0xe0, 0x03, 0xc9, 0xe0, 0x03, 0x68, 0xb0, 0x04, 0x69, 0x20, 0x00, 0x6b, 0x69, 0x1f,
    0x04, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4c, 0x4d, 0x00, 0x01,
];
const HELPER: [u8; 16] = [
    0xa0, 0x00, 0x05, 0x84, 0x0b, 0xa8, 0xb9, 0xbe, 0x0f, 0x6b, 0xff, 0xff, 0x4c, 0x4d, 0x00, 0x01,
];
const INLINE: [u8; 20] = [
    0xa5, 0x02, 0xe2, 0x20, 0x69, 0x10, 0xc2, 0x20, 0x90, 0x03, 0x69, 0xff, 0x01, 0xda, 0xaa, 0x22,
    0x00, 0x80, 0x00, 0xfa,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpecialEventRevealPatchBuildError {
    Table(SpecialEventRevealError),
}

impl std::fmt::Display for SpecialEventRevealPatchBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "special-event reveal patch build failed: {self:?}"
        )
    }
}

impl std::error::Error for SpecialEventRevealPatchBuildError {}

impl From<SpecialEventRevealError> for SpecialEventRevealPatchBuildError {
    fn from(value: SpecialEventRevealError) -> Self {
        Self::Table(value)
    }
}

#[must_use]
pub const fn smw_us_v1_special_event_reveal_locator() -> SpecialEventRevealPatchLocator {
    SpecialEventRevealPatchLocator {
        mapper: Mapper::LoRom,
        source_operand: SMW_US_V1_SPECIAL_EVENT_SOURCE_OPERAND,
        destination_operand: SMW_US_V1_SPECIAL_EVENT_DESTINATION_OPERAND,
        direction_operand: SMW_US_V1_SPECIAL_EVENT_DIRECTION_OPERAND,
        fixed_source: SMW_US_V1_SPECIAL_EVENT_FIXED_SOURCE,
        fixed_destination: SMW_US_V1_SPECIAL_EVENT_FIXED_DESTINATION,
        fixed_directions: SMW_US_V1_SPECIAL_EVENT_FIXED_DIRECTIONS,
        full_hook: FULL_HOOK,
        secondary_hook: SECONDARY_HOOK,
        opcode_patch: OPCODE_PATCH,
        nop_patch: NOP_PATCH,
        inline_patch: INLINE_PATCH,
        pointer_hooks: POINTER_HOOKS,
        helper_offset: HELPER_OFFSET,
        full_runtime_template: FULL_RUNTIME,
        pointer_runtime_template: POINTER_RUNTIME,
        helper_template: HELPER,
        inline_template: INLINE,
        pointer_bank_mask: 0x7f,
    }
}

#[must_use]
pub fn smw_us_v1_special_event_allocation_policy() -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_SPECIAL_EVENT_SEARCH_START..SMW_US_V1_SPECIAL_EVENT_SEARCH_END,
    )
}

#[must_use]
pub fn smw_us_v1_special_event_update_policy(image_len: usize) -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_SPECIAL_EVENT_SEARCH_START..image_len.saturating_add(0x8000).min(0x40_0000),
    )
}

/// Builds the exact two-runtime Lunar Magic patch and all three special-event planes.
///
/// # Errors
///
/// Rejects a source tile that would normalize when Lunar Magic reopens it.
pub fn smw_us_v1_special_event_reveal_installation_plan(
    table: &SpecialEventRevealTable,
) -> Result<RelocatablePatchPlan, SpecialEventRevealPatchBuildError> {
    let planes = table.encode()?;
    let fixup = |offset, target_payload, target_addend| PatchFixup {
        offset,
        target_payload,
        target_addend,
        encoding: PatchFixupEncoding::Long24LowBank,
    };
    Ok(RelocatablePatchPlan {
        description: "install native special-event reveals".into(),
        mapper: Mapper::LoRom,
        allocation: smw_us_v1_special_event_allocation_policy(),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![
            PatchPayload {
                bytes: planes.sources,
                fixups: Vec::new(),
            },
            PatchPayload {
                bytes: planes.destinations,
                fixups: Vec::new(),
            },
            PatchPayload {
                bytes: planes.directions,
                fixups: Vec::new(),
            },
            PatchPayload {
                bytes: FULL_RUNTIME.to_vec(),
                fixups: vec![fixup(0x26, 0, 0), fixup(0x2e, 3, 0)],
            },
            PatchPayload {
                bytes: POINTER_RUNTIME.to_vec(),
                fixups: Vec::new(),
            },
        ],
        writes: vec![
            patched_write(
                SMW_US_V1_SPECIAL_EVENT_SOURCE_OPERAND,
                &[0xb6, 0xe5, 0x04],
                0,
                0,
            ),
            patched_write(
                SMW_US_V1_SPECIAL_EVENT_DESTINATION_OPERAND,
                &[0x87, 0xe5, 0x04],
                1,
                0,
            ),
            patched_write(
                SMW_US_V1_SPECIAL_EVENT_DIRECTION_OPERAND,
                &[0xd6, 0xe5, 0x04],
                2,
                0,
            ),
            jsl_write(FULL_HOOK, &[0x0a, 0x0a, 0x0a, 0xa8], 3, 0, false),
            jsl_write(
                SECONDARY_HOOK,
                &[0x29, 0xff, 0x00, 0x0a, 0xaa],
                3,
                0x20,
                true,
            ),
            PatchWrite {
                offset: OPCODE_PATCH,
                expected: vec![0x49],
                replacement: vec![0x5d],
                fixups: Vec::new(),
            },
            PatchWrite {
                offset: NOP_PATCH,
                expected: vec![0xa8],
                replacement: vec![0xea],
                fixups: Vec::new(),
            },
            PatchWrite {
                offset: INLINE_PATCH,
                expected: vec![
                    0xad, 0xd0, 0x13, 0x29, 0xff, 0x00, 0xc9, 0x02, 0x00, 0x10, 0x06, 0x0a, 0x0a,
                    0x0a, 0xa8, 0x80, 0x03, 0xa0, 0x28, 0x00,
                ],
                replacement: INLINE.to_vec(),
                fixups: vec![fixup(16, 3, 0)],
            },
            jsl_write(POINTER_HOOKS[0], &[0x18, 0x69, 0x10, 0x00], 4, 0, false),
            jsl_write(POINTER_HOOKS[1], &[0x18, 0x69, 0x20, 0x00], 4, 0x10, false),
            PatchWrite {
                offset: HELPER_OFFSET,
                expected: vec![0xff; HELPER.len()],
                replacement: HELPER.to_vec(),
                fixups: Vec::new(),
            },
        ],
    })
}

fn patched_write(
    offset: usize,
    expected: &[u8],
    target_payload: usize,
    target_addend: usize,
) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: vec![0; 3],
        fixups: vec![PatchFixup {
            offset: 0,
            target_payload,
            target_addend,
            encoding: PatchFixupEncoding::Long24LowBank,
        }],
    }
}

fn jsl_write(
    offset: usize,
    expected: &[u8],
    target_payload: usize,
    target_addend: usize,
    trailing_nop: bool,
) -> PatchWrite {
    let mut replacement = vec![0x22, 0, 0, 0];
    if trailing_nop {
        replacement.push(0xea);
    }
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement,
        fixups: vec![PatchFixup {
            offset: 1,
            target_payload,
            target_addend,
            encoding: PatchFixupEncoding::Long24LowBank,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::EventReveal;
    use lm_project::{Project, SpecialEventRevealStorage};
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    fn changed_table() -> SpecialEventRevealTable {
        let mut table = SpecialEventRevealTable::default();
        for index in 0_u16..24 {
            table.reveals[usize::from(index)] = EventReveal {
                source_tile: index + 0x100,
                destination_tile: index + 0x300,
            };
            table.directions[usize::from(index)] = index.to_le_bytes()[0];
        }
        table
    }

    #[test]
    fn pristine_load_install_reopen_checksum_and_undo_are_exact() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let locator = smw_us_v1_special_event_reveal_locator();
        assert!(matches!(
            project
                .load_special_event_reveals_detected(locator)
                .unwrap()
                .storage,
            SpecialEventRevealStorage::Fixed
        ));
        let first = changed_table();
        project
            .save_special_event_reveals_detected(
                &first,
                locator,
                &smw_us_v1_special_event_reveal_installation_plan(&first).unwrap(),
                &smw_us_v1_special_event_update_policy(project.rom.logical_len()),
                crate::SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        let reopened = project
            .load_special_event_reveals_detected(locator)
            .unwrap();
        assert_eq!(reopened.table, first);
        assert!(matches!(
            reopened.storage,
            SpecialEventRevealStorage::Expanded { .. }
        ));
        assert!(project.identity.as_ref().unwrap().checksum_matches());
        let mut second = first;
        second.directions[23] ^= 0x80;
        project
            .save_special_event_reveals_detected(
                &second,
                locator,
                &smw_us_v1_special_event_reveal_installation_plan(&second).unwrap(),
                &smw_us_v1_special_event_update_policy(project.rom.logical_len()),
                crate::SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert_eq!(
            project
                .load_special_event_reveals_detected(locator)
                .unwrap()
                .table,
            second
        );
        assert!(project.undo().unwrap());
        assert_eq!(
            project
                .load_special_event_reveals_detected(locator)
                .unwrap()
                .table,
            changed_table()
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn wine_transfer_overworld_special_events_are_detected() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive");
        let before = fs::read(fixture.join("before.smc")).unwrap();
        let after = fs::read(fixture.join("after.smc")).unwrap();
        let pristine = Project::open_supported(RomImage::from_bytes(before).unwrap()).unwrap();
        let pristine_table = pristine
            .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
            .unwrap()
            .table;
        let mut project =
            Project::open_supported(RomImage::from_bytes(after.clone()).unwrap()).unwrap();
        let loaded = project
            .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
            .unwrap();
        assert_eq!(loaded.table, pristine_table);
        assert!(matches!(
            loaded.storage,
            SpecialEventRevealStorage::Expanded { .. }
        ));

        let mut edited = loaded.table;
        edited.directions[23] ^= 0x80;
        project
            .save_special_event_reveals_detected(
                &edited,
                smw_us_v1_special_event_reveal_locator(),
                &smw_us_v1_special_event_reveal_installation_plan(&edited).unwrap(),
                &smw_us_v1_special_event_update_policy(project.rom.logical_len()),
                crate::SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert_eq!(
            project
                .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator(),)
                .unwrap()
                .table,
            edited
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), after);

        project.rom.write(FULL_HOOK + 3, &[0x90]).unwrap();
        assert!(
            project
                .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator(),)
                .is_err()
        );
    }
}
