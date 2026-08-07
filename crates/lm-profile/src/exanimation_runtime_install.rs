//! Transactional publication of the recovered expanded-ExAnimation fresh runtime family.

use crate::{
    ExpandedExAnimationRuntimeError, SMW_US_V1_CHECKSUM_FIELD,
    empty_expanded_exanimation_pointer_table,
    exanimation_runtime::{
        IRAM_WORD_OFFSETS, LOCAL_WORD_TABLE_ENTRIES, LOCAL_WORD_TABLE_OFFSET, MAPPING_BYTE_OFFSETS,
        SNES_POINTER_OFFSETS, TEMPLATE_LOCAL_WORD_BASE,
    },
    expanded_exanimation_runtime_template,
};
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

/// The first search byte after Lunar Magic's authenticated prerequisite allocations.
pub const SMW_US_V1_EXPANDED_EXANIMATION_CORE_SEARCH_START: usize = 0x0008_0541;
/// The one-megabyte fresh-install expansion boundary.
pub const SMW_US_V1_EXPANDED_EXANIMATION_CORE_SEARCH_END: usize = 0x0010_0000;

const CORE_POINTER_FIXUPS: [(usize, usize, usize); 8] = [
    (SNES_POINTER_OFFSETS[0], 1, 1),
    (SNES_POINTER_OFFSETS[1], 1, 0),
    (SNES_POINTER_OFFSETS[2], 0, 0xb14),
    (SNES_POINTER_OFFSETS[3], 0, 0xb24),
    (SNES_POINTER_OFFSETS[4], 0, 0xb1c),
    (SNES_POINTER_OFFSETS[5], 0, 0xb1c),
    (SNES_POINTER_OFFSETS[6], 0, 0xb1c),
    (SNES_POINTER_OFFSETS[7], 0, 0xb1c),
];

const SMW_US_V1_IRAM_WORDS: [u16; 12] = [
    0x8af8, 0x8af8, 0x9093, 0x8b18, 0x90cb, 0x90cb, 0x90cb, 0x90cb, 0x90cb, 0x90cb, 0x90cb, 0x90cb,
];

const LEVEL_GRAPHICS_RUNTIME: [u8; 0x20] = [
    0xad, 0x9b, 0x0d, 0x29, 0xff, 0x00, 0xc9, 0x80, 0x00, 0xf0, 0x07, 0xa9, 0xff, 0x01, 0x54, 0x00,
    0x00, 0x6b, 0xa9, 0xef, 0x01, 0x54, 0x00, 0x00, 0x6b, 0x4c, 0x4d, 0x00, 0x01, 0xff, 0xff, 0xff,
];
const GRAPHICS_RUNTIME: [u8; 0x30] = [
    0xc2, 0x30, 0xa9, 0x00, 0x00, 0x8f, 0xc0, 0xc0, 0x7f, 0x8f, 0xc7, 0xc0, 0x7f, 0x8f, 0xce, 0xc0,
    0x7f, 0x8f, 0xd5, 0xc0, 0x7f, 0x8f, 0xdc, 0xc0, 0x7f, 0x8f, 0xe3, 0xc0, 0x7f, 0x8f, 0xea, 0xc0,
    0x7f, 0x8f, 0xf1, 0xc0, 0x7f, 0xa2, 0xfe, 0x1f, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];
const SHARED_PALETTE_RUNTIME_A: [u8; 0x0d] = [
    0xa5, 0x0e, 0x8d, 0x0b, 0x01, 0x1a, 0x85, 0xfe, 0x3a, 0x0a, 0xa8, 0x6b, 0xff,
];
const SHARED_PALETTE_RUNTIME_B: [u8; 0x10] = [
    0x9c, 0xcd, 0x13, 0x64, 0xfe, 0x64, 0xff, 0x84, 0x76, 0x84, 0x89, 0x6b, 0xff, 0xff, 0xff, 0xff,
];

/// Builds the recovered ordinary-LoROM fresh runtime allocations and authenticated fixed writes.
///
/// This covers `InstallExpandedExAnimationRuntime`'s `$C30`, `$600`, `$20`, and `$30` allocations,
/// missing-graphics sentinel initialization, and both shared-palette runtime hooks. Earlier general
/// save prerequisites and the later imported-level payload remain separate transactions.
///
/// # Errors
///
/// Rejects a malformed bundled runtime template before constructing a plan.
pub fn smw_us_v1_expanded_exanimation_runtime_installation_plan()
-> Result<RelocatablePatchPlan, ExpandedExAnimationRuntimeError> {
    let mut runtime = expanded_exanimation_runtime_template()?;
    for offset in MAPPING_BYTE_OFFSETS {
        runtime[offset] = 0;
    }
    for (offset, value) in IRAM_WORD_OFFSETS.into_iter().zip(SMW_US_V1_IRAM_WORDS) {
        runtime[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    let mut fixups = CORE_POINTER_FIXUPS
        .into_iter()
        .map(|(offset, target_payload, target_addend)| PatchFixup {
            offset,
            target_payload,
            target_addend,
            encoding: PatchFixupEncoding::Long24LowBank,
        })
        .collect::<Vec<_>>();
    for index in 0..LOCAL_WORD_TABLE_ENTRIES {
        let offset = LOCAL_WORD_TABLE_OFFSET + index * 2;
        let source = u16::from_le_bytes([runtime[offset], runtime[offset + 1]]);
        let relative = usize::from(source - TEMPLATE_LOCAL_WORD_BASE);
        fixups.push(PatchFixup {
            offset,
            target_payload: 0,
            target_addend: 0x4b0 + relative,
            encoding: PatchFixupEncoding::Low16,
        });
    }

    Ok(RelocatablePatchPlan {
        description: "install SMW US v1 expanded ExAnimation runtime".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy::lorom(
            SMW_US_V1_EXPANDED_EXANIMATION_CORE_SEARCH_START
                ..SMW_US_V1_EXPANDED_EXANIMATION_CORE_SEARCH_END,
        ),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![
            PatchPayload {
                bytes: runtime,
                fixups,
            },
            PatchPayload {
                bytes: empty_expanded_exanimation_pointer_table(),
                fixups: Vec::new(),
            },
            PatchPayload {
                bytes: LEVEL_GRAPHICS_RUNTIME.to_vec(),
                fixups: Vec::new(),
            },
            PatchPayload {
                bytes: GRAPHICS_RUNTIME.to_vec(),
                fixups: Vec::new(),
            },
        ],
        writes: vec![
            payload_hook(0x0002_83ad, &[0xe2, 0x30, 0x9c, 0x33, 0x19], 0, true),
            payload_hook(0x0000_25e3, &[0x01, 0x54, 0x00, 0x00], 2, false),
            payload_hook(0x0000_0a4e, &[0xc2, 0x30, 0xa2, 0xfe, 0x1f], 3, true),
            direct(0x0001_bcc0, &[0xff; 0x10], &[0; 0x10]),
            direct(
                0x0002_d8e2,
                &[0xa5, 0x0e, 0x0a, 0xa8],
                &[0x22, 0x50, 0xf5, 0x0e],
            ),
            direct(0x0007_7550, &[0xff; 0x0d], &SHARED_PALETTE_RUNTIME_A),
            direct(
                0x0000_26b8,
                &[0x84, 0x76, 0x84, 0x89],
                &[0x22, 0x60, 0xf5, 0x0e],
            ),
            direct(0x0007_7560, &[0xff; 0x10], &SHARED_PALETTE_RUNTIME_B),
        ],
    })
}

/// Backward-compatible name for the fresh-runtime family plan.
///
/// # Errors
///
/// Propagates malformed bundled-runtime errors from the preferred constructor.
pub fn smw_us_v1_expanded_exanimation_core_installation_plan()
-> Result<RelocatablePatchPlan, ExpandedExAnimationRuntimeError> {
    smw_us_v1_expanded_exanimation_runtime_installation_plan()
}

fn payload_hook(
    offset: usize,
    expected: &[u8],
    target_payload: usize,
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
            target_addend: 0,
            encoding: PatchFixupEncoding::Long24LowBank,
        }],
    }
}

fn direct(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{Project, RelocatablePatchError};
    use lm_rom::{RomImage, SnesChecksum};
    use std::{fs, path::PathBuf};

    #[test]
    fn core_plan_matches_retained_allocations_reopens_checksum_and_undoes_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/exanimation-install-positive/before.smc"),
        )
        .unwrap();
        let after = fs::read(
            root.join("oracle-work/lm363/pristine-us/exanimation-install-positive/after.smc"),
        )
        .unwrap();
        let after = RomImage::from_bytes(after).unwrap();
        for original in [before.clone(), before[0x200..].to_vec()] {
            let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
            let result = project
                .install_relocatable_patch(
                    &smw_us_v1_expanded_exanimation_runtime_installation_plan().unwrap(),
                )
                .unwrap();
            assert_eq!(result.blocks[0].header_offset, 0x80541);
            assert_eq!(result.blocks[0].payload, 0x80549..0x81179);
            assert_eq!(result.blocks[1].header_offset, 0x81179);
            assert_eq!(result.blocks[1].payload, 0x81181..0x81781);
            assert_eq!(result.blocks[2].payload, 0x81789..0x817a9);
            assert_eq!(result.blocks[3].payload, 0x817b1..0x817e1);
            assert_eq!(
                project.rom.read(0x80549, 0xc30).unwrap(),
                after.read(0x80549, 0xc30).unwrap()
            );
            // The retained import immediately publishes level `$000` after installing the
            // runtime; every other entry remains the installer's exact empty sentinel.
            assert_eq!(
                project.rom.read(0x81184, 0x5fd).unwrap(),
                after.read(0x81184, 0x5fd).unwrap()
            );
            assert_eq!(
                project.rom.read(0x283ad, 5).unwrap(),
                &[0x22, 0x49, 0x85, 0x10, 0xea]
            );
            for range in [
                0x81789..0x817e1,
                0x1bcc0..0x1bcd0,
                0x2d8e2..0x2d8e6,
                0x77550..0x7756d,
                0x26b8..0x26bc,
            ] {
                assert_eq!(
                    project.rom.read(range.start, range.len()).unwrap(),
                    after.read(range.start, range.len()).unwrap(),
                    "range {range:x?}"
                );
            }
            assert!(
                SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                    .unwrap()
                    .is_complementary()
            );
            project.undo().unwrap();
            assert_eq!(project.save_snapshot(), original);
        }
    }

    #[test]
    fn changed_pristine_hook_rejects_before_expansion_allocation_or_history() {
        let mut original = crate::test_support::pristine_smw_us_rom_bytes();
        original[0x283ad] ^= 0x01;
        let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
        let error = project
            .install_relocatable_patch(
                &smw_us_v1_expanded_exanimation_runtime_installation_plan().unwrap(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            RelocatablePatchError::HookPreconditionMismatch {
                index: 0,
                offset: 0x283ad
            }
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.undo().unwrap());
    }
}
