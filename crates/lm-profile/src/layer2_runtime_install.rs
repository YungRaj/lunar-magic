//! Authenticated Lunar Magic legacy to format-$103 Layer 2 table migration.

use crate::{
    SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET,
    SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET,
};
use lm_level::{Layer2Storage, level_mode_layer2_storage};
use lm_project::{PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::{Mapper, RomError, snes_to_pc};
use std::fmt;

const LAYER1_POINTER_TABLE_OFFSET: usize = 0x2e000;
const LAYER2_POINTER_TABLE_OFFSET: usize = 0x2e600;
const POINTER_TABLE_LEN: usize = 0x600;
const DESCRIPTOR_TABLE_LEN: usize = 0x200;
const LEGACY_DEFAULT_BANK: u8 = 0x0c;
const SPECIAL_LAYER1_POINTER: u32 = 0x06_8000;
const SPECIAL_OLD_LAYER2_POINTER: u32 = 0xff_d900;
const SPECIAL_NEW_LAYER2_POINTER: u32 = 0xff_de54;
const LAYER2_BANK_BOUNDARY: u32 = 0xff_e8fe;

const FORMAT_101_HOOK: [u8; 0x40] = [
    0xc9, 0xff, 0xd0, 0x04, 0x5c, 0x3f, 0x80, 0x05, 0x8b, 0x4b, 0xab, 0xa4, 0x0e, 0xb9, 0x10, 0xf3,
    0xaa, 0x29, 0x02, 0xd0, 0x05, 0xab, 0x5c, 0x74, 0x80, 0x05, 0x8a, 0x29, 0x01, 0xd0, 0x05, 0x8a,
    0x4a, 0x4a, 0x4a, 0x4a, 0xa0, 0x00, 0x00, 0xa2, 0x00, 0x00, 0x9f, 0x00, 0xbd, 0x7e, 0x9f, 0x00,
    0xbf, 0x7e, 0xe8, 0xe0, 0x00, 0x02, 0xd0, 0xf2, 0xab, 0x5c, 0x64, 0x80, 0x05, 0xff, 0xff, 0xff,
];

const FORMAT_102_HOOK: [u8; 0x40] = [
    0xc9, 0xff, 0xd0, 0x09, 0x1a, 0x8f, 0x0b, 0xc0, 0x7f, 0x5c, 0x3f, 0x80, 0x05, 0xa6, 0x0e, 0xbf,
    0x10, 0xf3, 0x0e, 0x8f, 0x0b, 0xc0, 0x7f, 0x89, 0x02, 0xd0, 0x04, 0x5c, 0x74, 0x80, 0x05, 0x89,
    0x04, 0xd0, 0x15, 0x4a, 0x4a, 0x4a, 0x4a, 0xa2, 0x00, 0x00, 0x9f, 0x00, 0xbd, 0x7e, 0x9f, 0x00,
    0xbf, 0x7e, 0xe8, 0xe0, 0x00, 0x02, 0xd0, 0xf2, 0x5c, 0x64, 0x80, 0x05, 0xff, 0xff, 0xff, 0xff,
];

const FORMAT_103_HOOK: [u8; 0x40] = [
    0x38, 0xe9, 0x7f, 0xd0, 0x08, 0x8f, 0x0b, 0xc0, 0x7f, 0x5c, 0x3f, 0x80, 0x05, 0xa6, 0x0e, 0xbf,
    0x10, 0xf3, 0x0e, 0x8f, 0x0b, 0xc0, 0x7f, 0x89, 0x0a, 0xd0, 0x04, 0x5c, 0x74, 0x80, 0x05, 0x89,
    0x04, 0xd0, 0x12, 0x4a, 0x4a, 0x4a, 0x4a, 0xa2, 0xff, 0x01, 0x9f, 0x00, 0xbd, 0x7e, 0x9f, 0x00,
    0xbf, 0x7e, 0xca, 0x10, 0xf5, 0x5c, 0x64, 0x80, 0x05, 0xff, 0xff, 0xff, 0x4c, 0x4d, 0x03, 0x01,
];

#[derive(Debug)]
pub enum SmwUsV1Layer2Format102MigrationError {
    MissingFormat101,
    MissingFormat102,
    SourceRange {
        offset: usize,
        len: usize,
    },
    HookMismatch {
        offset: usize,
        expected: u8,
        actual: u8,
    },
    PointerAddress {
        level: usize,
        source: RomError,
    },
    LevelModeRange {
        level: usize,
        offset: usize,
    },
}

impl fmt::Display for SmwUsV1Layer2Format102MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot migrate legacy SMW-US Layer 2 runtime: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1Layer2Format102MigrationError {}

/// Builds the exact table and hook conversion used by Lunar Magic when opening format `$101`.
///
/// In addition to the pointer conversion shared with `$102`, `$101` first normalizes the legacy
/// descriptor flag layout. The hook is authenticated byte-for-byte before a plan is returned.
///
/// # Errors
///
/// Rejects a missing or modified `$101` hook, truncated tables, invalid mapped pointers, or a
/// pointer whose level-mode byte lies outside the source image.
pub fn smw_us_v1_layer2_format_101_migration(
    bytes: &[u8],
) -> Result<RelocatablePatchPlan, SmwUsV1Layer2Format102MigrationError> {
    authenticate_hook(bytes, &FORMAT_101_HOOK, 0x4b, true)?;
    build_migration(bytes, &FORMAT_101_HOOK, true, "$101")
}

/// Builds the exact table and hook conversion used by Lunar Magic when opening format `$102`.
///
/// The legacy hook is authenticated byte-for-byte. Both live pointer tables and the descriptor
/// table are captured as exact transaction preconditions. Sentinel pointers are materialized in
/// bank `$0C`, descriptors are derived from the pointed-to level modes, and the current `$103`
/// hook is installed without allocating or reclaiming unrelated data.
///
/// # Errors
///
/// Rejects a missing or modified `$102` hook, truncated tables, invalid mapped pointers, or a
/// pointer whose level-mode byte lies outside the source image.
pub fn smw_us_v1_layer2_format_102_migration(
    bytes: &[u8],
) -> Result<RelocatablePatchPlan, SmwUsV1Layer2Format102MigrationError> {
    authenticate_hook(bytes, &FORMAT_102_HOOK, 0x5c, false)?;
    build_migration(bytes, &FORMAT_102_HOOK, false, "$102")
}

fn build_migration(
    bytes: &[u8],
    source_hook: &[u8; 0x40],
    normalize_format_101_descriptors: bool,
    source_format: &str,
) -> Result<RelocatablePatchPlan, SmwUsV1Layer2Format102MigrationError> {
    let layer1 = source_range(bytes, LAYER1_POINTER_TABLE_OFFSET, POINTER_TABLE_LEN)?;
    let layer2 = source_range(bytes, LAYER2_POINTER_TABLE_OFFSET, POINTER_TABLE_LEN)?;
    let descriptors = source_range(
        bytes,
        SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET,
        DESCRIPTOR_TABLE_LEN,
    )?;
    let (migrated_layer2, migrated_descriptors) = migrate_legacy_tables(
        bytes,
        layer1,
        layer2,
        descriptors,
        normalize_format_101_descriptors,
    )?;

    Ok(RelocatablePatchPlan {
        description: format!("Migrate SMW US Layer 2 runtime format {source_format} to $103"),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: 0..bytes.len(),
            bank_size: None,
            fill_bytes: vec![0, 0xff],
            protected: Vec::new(),
        },
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: Vec::new(),
        writes: vec![
            PatchWrite {
                offset: LAYER1_POINTER_TABLE_OFFSET,
                expected: layer1.to_vec(),
                replacement: layer1.to_vec(),
                fixups: Vec::new(),
            },
            PatchWrite {
                offset: LAYER2_POINTER_TABLE_OFFSET,
                expected: layer2.to_vec(),
                replacement: migrated_layer2,
                fixups: Vec::new(),
            },
            PatchWrite {
                offset: SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET,
                expected: descriptors.to_vec(),
                replacement: migrated_descriptors,
                fixups: Vec::new(),
            },
            PatchWrite {
                offset: SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET,
                expected: source_hook.to_vec(),
                replacement: FORMAT_103_HOOK.to_vec(),
                fixups: Vec::new(),
            },
        ],
    })
}

fn migrate_legacy_tables(
    bytes: &[u8],
    layer1: &[u8],
    layer2: &[u8],
    descriptors: &[u8],
    normalize_format_101_descriptors: bool,
) -> Result<(Vec<u8>, Vec<u8>), SmwUsV1Layer2Format102MigrationError> {
    let mut migrated_layer2 = layer2.to_vec();
    let mut migrated_descriptors = descriptors.to_vec();
    if normalize_format_101_descriptors {
        for descriptor in &mut migrated_descriptors {
            if *descriptor & 1 != 0 {
                *descriptor = (*descriptor & 0x0e) | 0x10;
            }
        }
    }
    for (level, descriptor) in migrated_descriptors.iter_mut().enumerate() {
        let pointer_offset = level * 3;
        let layer1_pointer = read_pointer(layer1, pointer_offset);
        let layer2_pointer = read_pointer(layer2, pointer_offset);
        if layer2[pointer_offset + 2] == 0xff {
            let resolved = if layer1_pointer == SPECIAL_LAYER1_POINTER
                && layer2_pointer == SPECIAL_OLD_LAYER2_POINTER
            {
                SPECIAL_NEW_LAYER2_POINTER
            } else {
                layer2_pointer
            };
            let encoded = resolved.to_le_bytes();
            migrated_layer2[pointer_offset..pointer_offset + 2].copy_from_slice(&encoded[..2]);
            migrated_layer2[pointer_offset + 2] = LEGACY_DEFAULT_BANK;
            *descriptor = if resolved < LAYER2_BANK_BOUNDARY {
                0x08
            } else {
                0x18
            };
            continue;
        }

        let payload = snes_to_pc(Mapper::LoRom, layer2_pointer).map_err(|source| {
            SmwUsV1Layer2Format102MigrationError::PointerAddress { level, source }
        })?;
        let mode_offset =
            payload
                .checked_add(1)
                .ok_or(SmwUsV1Layer2Format102MigrationError::LevelModeRange {
                    level,
                    offset: payload,
                })?;
        let mode = bytes.get(mode_offset).copied().ok_or(
            SmwUsV1Layer2Format102MigrationError::LevelModeRange {
                level,
                offset: mode_offset,
            },
        )?;
        *descriptor = match level_mode_layer2_storage(mode) {
            Layer2Storage::Objects => 0,
            Layer2Storage::CompressedTilemap => *descriptor & 0xf6,
        };
    }
    Ok((migrated_layer2, migrated_descriptors))
}

fn authenticate_hook(
    bytes: &[u8],
    expected_hook: &[u8; 0x40],
    generation_opcode: u8,
    format_101: bool,
) -> Result<(), SmwUsV1Layer2Format102MigrationError> {
    let Some(actual) = bytes.get(
        SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET
            ..SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET + expected_hook.len(),
    ) else {
        return Err(SmwUsV1Layer2Format102MigrationError::SourceRange {
            offset: SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET,
            len: expected_hook.len(),
        });
    };
    if actual.get(9).copied() != Some(generation_opcode) {
        return Err(if format_101 {
            SmwUsV1Layer2Format102MigrationError::MissingFormat101
        } else {
            SmwUsV1Layer2Format102MigrationError::MissingFormat102
        });
    }
    for (index, (&expected, &actual)) in expected_hook.iter().zip(actual).enumerate() {
        if expected != actual {
            return Err(SmwUsV1Layer2Format102MigrationError::HookMismatch {
                offset: SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET + index,
                expected,
                actual,
            });
        }
    }
    Ok(())
}

fn source_range(
    bytes: &[u8],
    offset: usize,
    len: usize,
) -> Result<&[u8], SmwUsV1Layer2Format102MigrationError> {
    bytes
        .get(offset..offset + len)
        .ok_or(SmwUsV1Layer2Format102MigrationError::SourceRange { offset, len })
}

fn read_pointer(bytes: &[u8], offset: usize) -> u32 {
    u32::from(bytes[offset])
        | (u32::from(bytes[offset + 1]) << 8)
        | (u32::from(bytes[offset + 2]) << 16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rom::{RomImage, apply_ips};
    use std::{env, fs};

    #[test]
    #[ignore = "requires authenticated Lunar Magic 3.01 and 3.63 before/after ROMs"]
    fn external_lunar_magic_301_migration_matches_363_tables_and_hook_exactly() {
        let before = RomImage::from_bytes(
            fs::read(env::var_os("LM_LAYER2_FORMAT_102_ROM").expect("format-$102 ROM")).unwrap(),
        )
        .unwrap();
        let after = RomImage::from_bytes(
            fs::read(env::var_os("LM_LAYER2_FORMAT_103_ROM").expect("format-$103 ROM")).unwrap(),
        )
        .unwrap();
        let original = before.logical_bytes().to_vec();
        let plan = smw_us_v1_layer2_format_102_migration(before.logical_bytes()).unwrap();
        let mut project = Project::new(before);
        project.install_relocatable_patch(&plan).unwrap();

        for range in [
            LAYER2_POINTER_TABLE_OFFSET..LAYER2_POINTER_TABLE_OFFSET + POINTER_TABLE_LEN,
            SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET
                ..SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET + DESCRIPTOR_TABLE_LEN,
            SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET
                ..SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET + FORMAT_103_HOOK.len(),
        ] {
            assert_eq!(
                &project.rom.logical_bytes()[range.clone()],
                &after.logical_bytes()[range]
            );
        }
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.logical_bytes(), original);
    }

    #[test]
    fn exact_hook_authentication_and_late_table_changes_are_atomic() {
        let mut bytes = vec![0xff; 0x80_000];
        bytes[SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET
            ..SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET + FORMAT_102_HOOK.len()]
            .copy_from_slice(&FORMAT_102_HOOK);
        bytes[LAYER1_POINTER_TABLE_OFFSET..LAYER1_POINTER_TABLE_OFFSET + POINTER_TABLE_LEN].fill(0);
        for pointer in bytes
            [LAYER2_POINTER_TABLE_OFFSET..LAYER2_POINTER_TABLE_OFFSET + POINTER_TABLE_LEN]
            .chunks_exact_mut(3)
        {
            pointer.copy_from_slice(&[0x54, 0xde, 0xff]);
        }
        bytes[SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET
            ..SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET + DESCRIPTOR_TABLE_LEN]
            .fill(0);
        let image = RomImage::from_bytes(bytes).unwrap();
        let source = image.logical_bytes().to_vec();
        let plan = smw_us_v1_layer2_format_102_migration(image.logical_bytes()).unwrap();
        let mut project = Project::new(image);
        project
            .rom
            .write(LAYER2_POINTER_TABLE_OFFSET, &[0x55])
            .unwrap();
        let changed = project.rom.logical_bytes().to_vec();
        assert!(project.install_relocatable_patch(&plan).is_err());
        assert_eq!(project.rom.logical_bytes(), changed);

        let mut corrupt = source;
        corrupt[SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET + 1] ^= 1;
        assert!(matches!(
            smw_us_v1_layer2_format_102_migration(&corrupt),
            Err(SmwUsV1Layer2Format102MigrationError::HookMismatch { .. })
        ));
    }

    #[test]
    fn format_101_hook_and_descriptor_flags_migrate_transactionally() {
        let mut bytes = vec![0xff; 0x80_000];
        bytes[SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET
            ..SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET + FORMAT_101_HOOK.len()]
            .copy_from_slice(&FORMAT_101_HOOK);
        bytes[LAYER1_POINTER_TABLE_OFFSET..LAYER1_POINTER_TABLE_OFFSET + POINTER_TABLE_LEN].fill(0);
        for pointer in bytes
            [LAYER2_POINTER_TABLE_OFFSET..LAYER2_POINTER_TABLE_OFFSET + POINTER_TABLE_LEN]
            .chunks_exact_mut(3)
        {
            pointer.copy_from_slice(&[0x54, 0xde, 0xff]);
        }
        bytes[LAYER2_POINTER_TABLE_OFFSET..LAYER2_POINTER_TABLE_OFFSET + 3]
            .copy_from_slice(&[0x00, 0x80, 0x00]);
        bytes[1] = 0;
        bytes[SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET] = 0x0f;

        let image = RomImage::from_bytes(bytes).unwrap();
        let original = image.logical_bytes().to_vec();
        let plan = smw_us_v1_layer2_format_101_migration(image.logical_bytes()).unwrap();
        let mut project = Project::new(image);
        project.install_relocatable_patch(&plan).unwrap();

        assert_eq!(
            project.rom.logical_bytes()[SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET],
            0x16
        );
        assert_eq!(
            &project.rom.logical_bytes()[SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET
                ..SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET + FORMAT_103_HOOK.len()],
            &FORMAT_103_HOOK
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.logical_bytes(), original);

        let mut corrupt = original;
        corrupt[SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET + 20] ^= 1;
        assert!(matches!(
            smw_us_v1_layer2_format_101_migration(&corrupt),
            Err(SmwUsV1Layer2Format102MigrationError::HookMismatch { .. })
        ));
    }

    #[test]
    #[ignore = "requires an authorized SMW base ROM and authenticated format-$101 IPS"]
    fn external_lunar_magic_format_101_patch_migrates_and_undoes_exactly() {
        let base =
            fs::read(env::var_os("LM_SMW_US_ROM").expect("authorized SMW base ROM")).unwrap();
        let patch = fs::read(
            env::var_os("LM_LAYER2_FORMAT_101_IPS").expect("authenticated format-$101 IPS"),
        )
        .unwrap();
        let source = apply_ips(&base, &patch).unwrap();
        let image = RomImage::from_bytes(source).unwrap();
        let original = image.logical_bytes().to_vec();
        let plan = smw_us_v1_layer2_format_101_migration(image.logical_bytes()).unwrap();
        let mut project = Project::new(image);
        project.install_relocatable_patch(&plan).unwrap();
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.logical_bytes(), original);
    }
}
