//! Relocatable SMW US v1 `$3D0` Layer 3 main runtime payload.
//!
//! The leading `$200` bytes are the runtime's level-index workspace. They are followed by a
//! 32-entry offset table and the recovered 65C816 implementation. Allocation-dependent operands
//! are represented as typed fixups rather than prepatched bytes.

use crate::SMW_US_V1_CHECKSUM_FIELD;
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_LAYER3_MAIN_RUNTIME_LEN: usize = 0x3d0;
pub const SMW_US_V1_LAYER3_MAIN_RUNTIME_WORKSPACE_LEN: usize = 0x200;
pub const SMW_US_V1_LAYER3_MAIN_RUNTIME_TABLE_OFFSET: usize = 0x200;
pub const SMW_US_V1_LAYER3_MAIN_RUNTIME_CODE_OFFSET: usize = 0x240;
pub const SMW_US_V1_LAYER3_MAIN_RUNTIME_SHARED_HELPER_OFFSET: usize = 0x3a0;
pub const SMW_US_V1_LAYER3_MAIN_RUNTIME_SEARCH_START: usize = 0x0008_567e;
pub const SMW_US_V1_LAYER3_MAIN_RUNTIME_SEARCH_END: usize = 0x0010_0000;

/// Level-mode offsets selected by the runtime before its generated copy/initialization paths.
pub const SMW_US_V1_LAYER3_MAIN_RUNTIME_LEVEL_OFFSETS: [u16; 32] = [
    0x01b0, 0x01c0, 0x01d0, 0x0200, 0x0220, 0x0250, 0x0260, 0x0280, 0x02a0, 0x02c0, 0x02f0, 0x0310,
    0x0340, 0x0380, 0x03b0, 0x0400, 0x0440, 0x04a0, 0x0510, 0x0590, 0x0630, 0x0700, 0x0800, 0x0950,
    0x0b30, 0x0e00, 0x12a0, 0x1c00, 0x3800, 0x0100, 0x00f0, 0x00e0,
];

const CODE_AND_MARKER: [u8; 0x190] = [
    0x64, 0x1d, 0x64, 0x21, 0x9c, 0xf5, 0x0b, 0xda, 0xc2, 0x30, 0xa9, 0xb0, 0x01, 0x8d, 0xd7, 0x13,
    0x38, 0xe9, 0x10, 0x00, 0x8d, 0x36, 0x19, 0xa9, 0x00, 0xc8, 0xa2, 0x00, 0x00, 0x20, 0x10, 0x87,
    0xa9, 0x00, 0x00, 0xe2, 0x30, 0xfa, 0xa7, 0xce, 0x29, 0x1f, 0x6b, 0x85, 0x65, 0xa9, 0xb0, 0x01,
    0x8d, 0xd7, 0x13, 0x38, 0xe9, 0x10, 0x00, 0x8d, 0x36, 0x19, 0xa9, 0x00, 0xc8, 0xa2, 0x00, 0x00,
    0x20, 0x10, 0x87, 0xe2, 0x30, 0x9c, 0xf5, 0x0b, 0x6b, 0xad, 0x9b, 0x0d, 0xc9, 0x80, 0xf0, 0x09,
    0x64, 0x0f, 0xf4, 0x0f, 0x95, 0x5c, 0x40, 0x95, 0x02, 0x5c, 0x3b, 0x95, 0x02, 0x69, 0x00, 0xcd,
    0xd8, 0x13, 0xd0, 0x07, 0xeb, 0xa5, 0x00, 0xcd, 0xd7, 0x13, 0xeb, 0x5c, 0xc2, 0x95, 0x02, 0xe9,
    0x00, 0xcd, 0xd8, 0x13, 0xd0, 0x05, 0xa4, 0x00, 0xcc, 0xd7, 0x13, 0x5c, 0xdb, 0x92, 0x02, 0xad,
    0xf5, 0x0b, 0x29, 0x40, 0x00, 0xf0, 0x03, 0xa9, 0x0f, 0x00, 0x6d, 0xd7, 0x13, 0xe9, 0xef, 0x00,
    0xf4, 0x12, 0xf7, 0x5c, 0xf4, 0xf7, 0x00, 0xa7, 0xce, 0x29, 0x20, 0x8d, 0xf5, 0x0b, 0xa9, 0x7e,
    0xa2, 0x5f, 0x00, 0x9d, 0xf6, 0x0b, 0xca, 0xca, 0xca, 0x10, 0xf8, 0xa9, 0x7f, 0xa2, 0x5f, 0x00,
    0x9d, 0x56, 0x0c, 0xca, 0xca, 0xca, 0x10, 0xf8, 0xa5, 0x5b, 0x29, 0x01, 0xf0, 0x24, 0xc2, 0x20,
    0xa9, 0xb0, 0x01, 0x8d, 0xd7, 0x13, 0x38, 0xe9, 0x10, 0x00, 0x8d, 0x36, 0x19, 0xa9, 0x00, 0xc8,
    0xa2, 0x00, 0x00, 0x20, 0x10, 0x87, 0xa9, 0x00, 0x01, 0x8d, 0xd7, 0x13, 0xa9, 0x01, 0x00, 0xe2,
    0x20, 0x6b, 0xbb, 0xbf, 0x00, 0xde, 0x05, 0x29, 0x07, 0x89, 0x04, 0xf0, 0x02, 0x49, 0x84, 0x8d,
    0xf4, 0x0b, 0xbf, 0x70, 0x83, 0x90, 0x0c, 0xf5, 0x0b, 0xc2, 0x20, 0x29, 0x1f, 0x00, 0x0a, 0xaa,
    0xbf, 0x70, 0x85, 0x90, 0x8d, 0xd7, 0x13, 0x38, 0xe9, 0x10, 0x00, 0x8d, 0x36, 0x19, 0xa9, 0x00,
    0xc8, 0xa2, 0x00, 0x00, 0x20, 0x10, 0x87, 0x2c, 0xf4, 0x0b, 0x10, 0x1a, 0xe0, 0x60, 0x00, 0xb0,
    0x15, 0x8a, 0x89, 0x01, 0x00, 0xf0, 0x04, 0x38, 0xe9, 0x03, 0x00, 0x4a, 0xaa, 0xbd, 0xf6, 0x0b,
    0xa2, 0x30, 0x00, 0x20, 0x10, 0x87, 0xe2, 0x20, 0x5a, 0xa2, 0x5d, 0x00, 0xa0, 0x1f, 0x00, 0xbd,
    0xf6, 0x0b, 0x99, 0xb6, 0x0c, 0xbd, 0xf7, 0x0b, 0x99, 0xd6, 0x0c, 0xca, 0xca, 0xca, 0x88, 0x10,
    0xee, 0x7a, 0xa9, 0x00, 0xeb, 0xa9, 0x00, 0x6b, 0xea, 0xea, 0xea, 0xea, 0xea, 0xea, 0xea, 0xea,
    0x18, 0x9d, 0xf6, 0x0b, 0x9d, 0x56, 0x0c, 0x6d, 0xd7, 0x13, 0xe8, 0xe8, 0xe8, 0xb0, 0x05, 0xe0,
    0x60, 0x00, 0x90, 0xed, 0x60, 0x45, 0x58, 0x4c, 0x45, 0x56, 0x45, 0x4c, 0x2d, 0x47, 0x45, 0x4e,
    0x31, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x4c, 0x4d, 0x02, 0x01,
];

/// Builds the complete address-independent standard-LoROM runtime.
#[must_use]
pub fn smw_us_v1_layer3_main_runtime_payload() -> PatchPayload {
    let mut bytes = vec![0; SMW_US_V1_LAYER3_MAIN_RUNTIME_WORKSPACE_LEN];
    for value in SMW_US_V1_LAYER3_MAIN_RUNTIME_LEVEL_OFFSETS {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    debug_assert_eq!(bytes.len(), SMW_US_V1_LAYER3_MAIN_RUNTIME_CODE_OFFSET);
    bytes.extend_from_slice(&CODE_AND_MARKER);
    debug_assert_eq!(bytes.len(), SMW_US_V1_LAYER3_MAIN_RUNTIME_LEN);

    let mut fixups = [0x25e, 0x281, 0x314, 0x355, 0x374]
        .into_iter()
        .map(|offset| PatchFixup {
            offset,
            target_payload: 0,
            target_addend: SMW_US_V1_LAYER3_MAIN_RUNTIME_SHARED_HELPER_OFFSET,
            encoding: PatchFixupEncoding::Low16,
        })
        .collect::<Vec<_>>();
    fixups.extend([
        PatchFixup {
            offset: 0x333,
            target_payload: 0,
            target_addend: 0,
            encoding: PatchFixupEncoding::Long24LowBank,
        },
        PatchFixup {
            offset: 0x341,
            target_payload: 0,
            target_addend: SMW_US_V1_LAYER3_MAIN_RUNTIME_WORKSPACE_LEN,
            encoding: PatchFixupEncoding::Long24LowBank,
        },
    ]);
    PatchPayload { bytes, fixups }
}

/// Returns every allocation-relative external entry hook into the standard-LoROM payload.
///
/// This catalog is deliberately separate from an installation plan: the installer also relocates
/// payload operands to existing ROM routines and performs mapper-sensitive fixed writes.
#[must_use]
pub fn smw_us_v1_layer3_main_runtime_allocation_hooks() -> Vec<PatchWrite> {
    vec![
        allocation_hook(0x0000_770d, &[0xa9, 0xc0, 0, 0x20], 0x5c, 0x2bf),
        allocation_hook(0x0001_12d7, &[0xe9, 0, 0xc9, 2], 0x5c, 0x2af),
        allocation_hook(0x0001_150b, &[0x64, 0x0f, 0x20, 0x40], 0x5c, 0x289),
        allocation_hook(0x0001_15be, &[0x69, 0, 0xc9, 2], 0x5c, 0x29d),
        allocation_hook(0x0002_da8a, &[0xa7, 0xce, 0x29, 0x3f], 0x22, 0x240),
        allocation_hook(0x0002_db5f, &[0xa7, 0xce, 0x29, 0x7f], 0x22, 0x240),
        allocation_hook(0x0002_d9a1, &[0xa5, 0x5b, 0x29, 1], 0x22, 0x2d7),
        allocation_hook(0x0006_1436, &[0x85, 0x65, 0xe2, 0x30], 0x22, 0x26b),
    ]
}

/// Returns the allocation-independent rewrites installed alongside the main runtime.
///
/// Each write carries the exact pristine bytes as a precondition. Offsets are logical,
/// header-independent PC offsets.
#[must_use]
pub fn smw_us_v1_layer3_main_runtime_verified_fixed_writes() -> Vec<PatchWrite> {
    vec![
        fixed_write(0x0000_3f36, &[0x0a], &[0x08]),
        fixed_write(
            0x0000_3f3c,
            &[
                0x29, 0x01, 0x4c, 0x46, 0xbf, 0xa5, 0x9b, 0x4a, 0xa5, 0x99, 0x2a, 0x0a, 0x0a,
            ],
            &[
                0x80, 0x06, 0xea, 0xa5, 0x9b, 0x4a, 0xa5, 0x99, 0x2a, 0x0a, 0x0a, 0x29, 0x0c,
            ],
        ),
        fixed_write(0x0002_d8fc, &[0x3f], &[0x1f]),
        fixed_write(0x0000_41b5, &[0x01], &[0x3f]),
        fixed_write(0x0000_407d, &[0x01], &[0x3f]),
        fixed_write(0x0000_40ca, &[0x01], &[0x3f]),
        fixed_write(0x0000_43d7, &[0x01], &[0x3f]),
        fixed_write(0x0002_8a1a, &[0x01], &[0x3f]),
        fixed_write(0x0002_8af6, &[0x01], &[0x3f]),
        fixed_write(0x0002_8be8, &[0x01], &[0x3f]),
        fixed_write(0x0002_8cdb, &[0x01], &[0x3f]),
        fixed_write(
            0x0006_a963,
            &[
                0xa5, 0x6b, 0x18, 0x69, 0xb0, 0x85, 0x6b, 0x85, 0x6e, 0xa5, 0x6c, 0x69, 0x01, 0x85,
                0x6c, 0x85, 0x6f,
            ],
            &[
                0xc2, 0x21, 0xa5, 0x6b, 0x6d, 0xd7, 0x13, 0x85, 0x6b, 0x85, 0x6e, 0xa9, 0x00, 0x00,
                0xe2, 0x20, 0xea,
            ],
        ),
        fixed_write(
            0x0006_a9d6,
            &[
                0xa5, 0x6b, 0x38, 0xe9, 0xb0, 0x85, 0x6b, 0x85, 0x6e, 0x85, 0x04, 0xa5, 0x6c, 0xe9,
                0x01, 0x85, 0x6c, 0x85, 0x6f, 0x85, 0x05, 0xce, 0xa1, 0x1b, 0x60,
            ],
            &[
                0xc2, 0x20, 0xa5, 0x6b, 0x38, 0xed, 0xd7, 0x13, 0x85, 0x6b, 0x85, 0x6e, 0x85, 0x04,
                0xa9, 0x00, 0x00, 0xe2, 0x20, 0xce, 0xa1, 0x1b, 0x60, 0xea, 0xea,
            ],
        ),
        fixed_write(
            0x0006_a9ef,
            &[
                0xa5, 0x6b, 0x18, 0x69, 0xb0, 0x85, 0x6b, 0x85, 0x6e, 0x85, 0x04, 0xa5, 0x6c, 0x69,
                0x01, 0x85, 0x6c, 0x85, 0x6f, 0x85, 0x05, 0xee,
            ],
            &[
                0xc2, 0x21, 0xa5, 0x6b, 0x6d, 0xd7, 0x13, 0x85, 0x6b, 0x85, 0x6e, 0x85, 0x04, 0xa9,
                0x00, 0x00, 0xe2, 0x20, 0xee, 0xa1, 0x1b, 0x60,
            ],
        ),
    ]
}

/// Builds the complete failure-atomic standard-LoROM main-runtime installation.
#[must_use]
pub fn smw_us_v1_layer3_main_runtime_installation_plan() -> RelocatablePatchPlan {
    let mut writes = smw_us_v1_layer3_main_runtime_allocation_hooks();
    writes.extend(smw_us_v1_layer3_main_runtime_verified_fixed_writes());
    RelocatablePatchPlan {
        description: "install SMW US Layer 3 main runtime".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: SMW_US_V1_LAYER3_MAIN_RUNTIME_SEARCH_START
                ..SMW_US_V1_LAYER3_MAIN_RUNTIME_SEARCH_END,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: Vec::new(),
        },
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![smw_us_v1_layer3_main_runtime_payload()],
        writes,
    }
}

fn fixed_write(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    }
}

fn allocation_hook(offset: usize, expected: &[u8], opcode: u8, target_addend: usize) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: vec![opcode, 0, 0, 0],
        fixups: vec![PatchFixup {
            offset: 1,
            target_payload: 0,
            target_addend,
            encoding: PatchFixupEncoding::Long24LowBank,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{Mapper, pc_to_snes};
    use std::{fs, path::PathBuf};

    #[test]
    fn typed_fixups_reproduce_the_complete_retained_wine_payload() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let after = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let installed_pc = 0x0008_5686;
        let installed =
            &after[installed_pc + 0x200..installed_pc + 0x200 + SMW_US_V1_LAYER3_MAIN_RUNTIME_LEN];
        let payload = smw_us_v1_layer3_main_runtime_payload();
        let mut resolved = payload.bytes.clone();
        for fixup in &payload.fixups {
            let target_pc = installed_pc + fixup.target_addend;
            let target = pc_to_snes(Mapper::LoRom, target_pc).unwrap();
            match fixup.encoding {
                PatchFixupEncoding::Low16 => {
                    let low = u16::try_from(target & 0xffff).unwrap();
                    resolved[fixup.offset..fixup.offset + 2].copy_from_slice(&low.to_le_bytes());
                }
                PatchFixupEncoding::Long24LowBank => {
                    let encoded = (target & 0x7f_ffff).to_le_bytes();
                    resolved[fixup.offset..fixup.offset + 3].copy_from_slice(&encoded[..3]);
                }
                encoding => panic!("unexpected runtime encoding {encoding:?}"),
            }
        }
        assert_eq!(resolved, installed);
    }

    #[test]
    fn workspace_table_and_marker_have_stable_boundaries() {
        let payload = smw_us_v1_layer3_main_runtime_payload();
        assert!(
            payload.bytes[..SMW_US_V1_LAYER3_MAIN_RUNTIME_WORKSPACE_LEN]
                .iter()
                .all(|byte| *byte == 0)
        );
        assert_eq!(
            &payload.bytes[0x3b5..],
            b"EXLEVEL-GEN1           LM\x02\x01"
        );
        assert_eq!(payload.fixups.len(), 7);
    }

    #[test]
    fn every_allocation_entry_hook_matches_pristine_and_wine_evidence() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive");
        let before = fs::read(fixture.join("before.smc")).unwrap();
        let after = fs::read(fixture.join("after.smc")).unwrap();
        let installed_pc = 0x0008_5686;
        let target_base = pc_to_snes(Mapper::LoRom, installed_pc).unwrap() & 0x7f_ffff;
        let hooks = smw_us_v1_layer3_main_runtime_allocation_hooks();

        assert_eq!(hooks.len(), 8);
        for hook in hooks {
            let raw = hook.offset + 0x200;
            assert_eq!(
                &before[raw..raw + hook.expected.len()],
                hook.expected,
                "pristine hook at {:#x}",
                hook.offset
            );
            let mut expected = hook.replacement;
            let target = target_base + u32::try_from(hook.fixups[0].target_addend).unwrap();
            expected[1..4].copy_from_slice(&target.to_le_bytes()[..3]);
            assert_eq!(
                &after[raw..raw + expected.len()],
                expected,
                "installed hook at {:#x}",
                hook.offset
            );
        }
    }

    #[test]
    fn every_verified_fixed_write_matches_pristine_and_wine_evidence() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive");
        let before = fs::read(fixture.join("before.smc")).unwrap();
        let after = fs::read(fixture.join("after.smc")).unwrap();
        let writes = smw_us_v1_layer3_main_runtime_verified_fixed_writes();

        assert_eq!(writes.len(), 14);
        for write in writes {
            let raw = write.offset + 0x200;
            assert_eq!(
                &before[raw..raw + write.expected.len()],
                write.expected,
                "pristine fixed write at {:#x}",
                write.offset
            );
            assert_eq!(
                &after[raw..raw + write.replacement.len()],
                write.replacement,
                "installed fixed write at {:#x}",
                write.offset
            );
            assert!(write.fixups.is_empty());
        }
    }

    #[test]
    fn plan_installs_reopens_and_undoes_as_one_edit() {
        use lm_project::Project;
        use lm_rom::{RomImage, SnesChecksum};

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive");
        let before = RomImage::from_bytes(fs::read(fixture.join("before.smc")).unwrap()).unwrap();
        let after = RomImage::from_bytes(fs::read(fixture.join("after.smc")).unwrap()).unwrap();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plan = smw_us_v1_layer3_main_runtime_installation_plan();
        let result = project.install_relocatable_patch(&plan).unwrap();

        assert_eq!(result.blocks.len(), 1);
        assert_eq!(
            result.blocks[0].header_offset,
            SMW_US_V1_LAYER3_MAIN_RUNTIME_SEARCH_START
        );
        assert_eq!(
            project
                .rom
                .read(
                    result.blocks[0].payload.start,
                    SMW_US_V1_LAYER3_MAIN_RUNTIME_LEN
                )
                .unwrap(),
            after
                .read(
                    result.blocks[0].payload.start,
                    SMW_US_V1_LAYER3_MAIN_RUNTIME_LEN
                )
                .unwrap()
        );
        for write in &plan.writes {
            assert_eq!(
                project
                    .rom
                    .read(write.offset, write.replacement.len())
                    .unwrap(),
                after.read(write.offset, write.replacement.len()).unwrap()
            );
        }
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
                .is_complementary()
        );
        assert_eq!(project.history.undo_len(), 1);
        project.undo().unwrap();
        assert_eq!(project.rom.logical_bytes(), original);
    }

    #[test]
    fn late_fixed_write_failure_rolls_back_payload_hooks_and_expansion() {
        use lm_project::{Project, RelocatablePatchError};
        use lm_rom::RomImage;

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive");
        let mut before =
            RomImage::from_bytes(fs::read(fixture.join("before.smc")).unwrap()).unwrap();
        before.write(0x0006_a9ef, &[0xff]).unwrap();
        let original = before.logical_bytes().to_vec();
        let mut project = Project::new(before);
        let plan = smw_us_v1_layer3_main_runtime_installation_plan();

        assert!(matches!(
            project.install_relocatable_patch(&plan),
            Err(RelocatablePatchError::HookPreconditionMismatch {
                index: 21,
                offset: 0x0006_a9ef,
            })
        ));
        assert_eq!(project.rom.logical_bytes(), original);
        assert_eq!(project.history.undo_len(), 0);
    }
}
