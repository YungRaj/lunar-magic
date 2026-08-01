//! Complete pristine SMW US v1 Lfix3 and expanded-secondary-exit installation plan.

use crate::{
    Lfix3RuntimeLengthError, SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_LFIX3_SEARCH_END,
    SMW_US_V1_LFIX3_SEARCH_START, SMW_US_V1_SECONDARY_EXIT_BASE_SUPPORT,
    SMW_US_V1_SECONDARY_EXIT_FIXED_PLANES, SMW_US_V1_SECONDARY_EXIT_INDEX_SUPPORT,
    smw_us_v1_lfix3_installation_plan, smw_us_v1_lfix3_runtime_template,
    smw_us_v1_secondary_exit_first_reader, smw_us_v1_secondary_exit_second_reader,
};
use lm_level::{SecondaryExitEncodingError, SecondaryExitTable};
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

const EXTENDED_RUNTIME_HEX: &str = concat!(
    "2280dc05a829870c2a199829084a4a4a850f2285dc058502297f8504228adc05",
    "aa29c08dcd138a29200a0c2a19a60ebf00fc0629800404bf00fe06293f0ccd13",
    "988940f01f29300a0a0a85942a8595a5014a29700494a5000a0a0a0a8596a502",
    "293f8597a5023009a5004a4a4a4a85026ba90c8d00019cae0d9caf0d9cb00d98",
    "8910f009a5008df61dee9c1b988920f006a5018dea1d982907c907d002a9808dd5",
    "0d0af006eece13eee91dfa68abfa68e2305cf79300ffffffffffff4c4d1001",
);
const COMPATIBILITY_RUNTIME: [u8; 0x1f] = [
    0x8d, 0xb8, 0x19, 0x9c, 0xd8, 0x19, 0x9c, 0x93, 0x1b, 0xee, 0x1a, 0x14, 0x64, 0x95, 0x64, 0x97,
    0x64, 0x94, 0x64, 0x96, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4c, 0x4d, 0x10, 0x01,
];

#[derive(Debug)]
pub enum SecondaryExitInstallBuildError {
    Lfix3(Lfix3RuntimeLengthError),
    Table(SecondaryExitEncodingError),
}

impl std::fmt::Display for SecondaryExitInstallBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "secondary-exit installation construction failed: {self:?}"
        )
    }
}

impl std::error::Error for SecondaryExitInstallBuildError {}

impl From<Lfix3RuntimeLengthError> for SecondaryExitInstallBuildError {
    fn from(value: Lfix3RuntimeLengthError) -> Self {
        Self::Lfix3(value)
    }
}

impl From<SecondaryExitEncodingError> for SecondaryExitInstallBuildError {
    fn from(value: SecondaryExitEncodingError) -> Self {
        Self::Table(value)
    }
}

/// Builds one failure-atomic pristine installation for Lfix3 and expanded secondary exits.
///
/// # Errors
///
/// Rejects a malformed Lfix3 template or an unrepresentable table. This compatibility builder
/// assumes fixed-plane bytes are unchanged; use the source-aware variant for replacements.
pub fn smw_us_v1_secondary_exit_installation_plan(
    lfix3_template: &[u8],
    table: &SecondaryExitTable,
) -> Result<RelocatablePatchPlan, SecondaryExitInstallBuildError> {
    smw_us_v1_secondary_exit_installation_plan_from_source(lfix3_template, table, table)
}

/// Builds a pristine installation whose fixed-plane preconditions come from `source_table`.
///
/// # Errors
///
/// Rejects a malformed Lfix3 template or an unrepresentable source or destination table.
pub fn smw_us_v1_secondary_exit_installation_plan_from_source(
    lfix3_template: &[u8],
    source_table: &SecondaryExitTable,
    table: &SecondaryExitTable,
) -> Result<RelocatablePatchPlan, SecondaryExitInstallBuildError> {
    let mut plan = smw_us_v1_lfix3_installation_plan(lfix3_template)?;
    plan.description = "install SMW US v1 expanded secondary exits".into();
    let source_encoded = source_table.encode()?;
    let encoded = table.encode()?;
    let used_len = used_plane_len(&encoded).max(1);
    let fixed_prefix = usize::from(used_len <= 0x200) * 4;
    let mut plane_targets = [None; 6];
    for (plane, plane_target) in plane_targets.iter_mut().enumerate().skip(fixed_prefix) {
        let start = plane * SecondaryExitTable::ENTRY_COUNT;
        let target = plan.payloads.len();
        plan.payloads.push(PatchPayload {
            bytes: encoded[start..start + used_len].to_vec(),
            fixups: Vec::new(),
        });
        *plane_target = Some(target);
    }
    plan.writes.extend(secondary_fixed_writes(
        &source_encoded,
        &encoded,
        fixed_prefix,
        plane_targets,
    ));
    plan.allocation =
        AllocationPolicy::lorom(SMW_US_V1_LFIX3_SEARCH_START..SMW_US_V1_LFIX3_SEARCH_END);
    plan.checksum_field = SMW_US_V1_CHECKSUM_FIELD;
    plan.mapper = Mapper::LoRom;
    Ok(plan)
}

/// Builds the pristine installation from the revision profile's bundled runtime template.
///
/// # Errors
///
/// Rejects an unrepresentable secondary-exit table or inconsistent bundled template.
pub fn smw_us_v1_builtin_secondary_exit_installation_plan(
    table: &SecondaryExitTable,
) -> Result<RelocatablePatchPlan, SecondaryExitInstallBuildError> {
    smw_us_v1_secondary_exit_installation_plan(&smw_us_v1_lfix3_runtime_template(), table)
}

/// Builds the bundled pristine installation with detected fixed-plane preconditions.
///
/// # Errors
///
/// Rejects an unrepresentable source or destination table.
pub fn smw_us_v1_builtin_secondary_exit_installation_plan_from_source(
    source_table: &SecondaryExitTable,
    table: &SecondaryExitTable,
) -> Result<RelocatablePatchPlan, SecondaryExitInstallBuildError> {
    smw_us_v1_secondary_exit_installation_plan_from_source(
        &smw_us_v1_lfix3_runtime_template(),
        source_table,
        table,
    )
}

fn secondary_fixed_writes(
    source_encoded: &[u8],
    encoded: &[u8],
    fixed_prefix: usize,
    plane_targets: [Option<usize>; 6],
) -> Vec<PatchWrite> {
    let mut writes = vec![
        direct(
            0x0002_d7ce,
            &[0xf0, 0x02, 0xa9, 0x01],
            &[0x22, 0x50, 0xdc, 0x05],
        ),
        direct(
            0x0002_dc50,
            &[0xff; 0x30],
            &SMW_US_V1_SECONDARY_EXIT_BASE_SUPPORT,
        ),
        direct(0x0006_a532, &[0x01], &[0x0f]),
        direct(0x0006_a536, &[0xa5, 0x0b], &[0x29, 0x02]),
        direct(
            0x0002_d836,
            &[0x29, 0x07, 0x8d, 0x2a, 0x19],
            &[0xbb, 0x22, 0xe0, 0xbc, 0x03],
        ),
        direct(
            0x0001_bce0,
            &[0xff; 0xc0],
            &decode_hex(EXTENDED_RUNTIME_HEX),
        ),
        reader_write(
            0x0002_dc80,
            second_reader_template(),
            [3, 4, 5],
            plane_targets,
        ),
        direct(
            0x0002_65f1,
            &[0xc9, 0x02, 0xd0, 0x03, 0xee, 0xea, 0x1d],
            &[0x22, 0xb0, 0xdc, 0x05, 0xea, 0xea, 0xea],
        ),
        direct(0x0000_49d7, &[0xd0], &[0x80]),
        direct(
            0x0002_d7e2,
            &[0xb9, 0x00, 0xf8, 0x85, 0x0e],
            &[0xbb, 0x22, 0x90, 0xe1, 0x0d],
        ),
        direct(
            0x0002_d7ea,
            &[0xb9, 0x00, 0xfa, 0x85, 0x00],
            &[0xbb, 0x22, 0x97, 0xe1, 0x0d],
        ),
        direct(
            0x0002_d81c,
            &[0xb9, 0x00, 0xfc, 0x85, 0x01],
            &[0xbb, 0x22, 0x9e, 0xe1, 0x0d],
        ),
        reader_write(
            0x0006_e190,
            smw_us_v1_secondary_exit_first_reader().to_vec(),
            [0, 1, 2],
            plane_targets,
        ),
        direct(
            0x0006_e1b0,
            &[0xff; 0x20],
            &SMW_US_V1_SECONDARY_EXIT_INDEX_SUPPORT,
        ),
        direct(0x0006_a115, &[0x00, 0x00, 0x00], &[0xb0, 0xe1, 0x0d]),
        direct(
            0x0002_dbc2,
            &[0x9d, 0xb8, 0x19, 0xee, 0x1a, 0x14],
            &[0x22, 0x00, 0xbb, 0x03, 0xea, 0xea],
        ),
        direct(0x0001_bb00, &[0xff; 0x1f], &COMPATIBILITY_RUNTIME),
        direct(
            0x0002_d9e8,
            &[0x95, 0x4c, 0x17, 0xda],
            &[0x01, 0xea, 0xea, 0xea],
        ),
        direct(0x0000_72db, &[0xf0, 0x03], &[0xea, 0xea]),
        direct(0x0002_d9c3, &[0x8d], &[0xad]),
    ];
    if fixed_prefix == 4 {
        for (plane, fixed_plane) in SMW_US_V1_SECONDARY_EXIT_FIXED_PLANES
            .into_iter()
            .enumerate()
        {
            let start = plane * SecondaryExitTable::ENTRY_COUNT;
            writes.push(direct(
                fixed_plane,
                &source_encoded[start..start + 0x200],
                &encoded[start..start + 0x200],
            ));
        }
    }
    writes
}

fn reader_write(
    offset: usize,
    mut replacement: Vec<u8>,
    planes: [usize; 3],
    targets: [Option<usize>; 6],
) -> PatchWrite {
    let pointer_offsets = if offset == 0x0006_e190 {
        [1, 8, 15]
    } else {
        [1, 6, 11]
    };
    let mut fixups = Vec::new();
    for (plane, pointer_offset) in planes.into_iter().zip(pointer_offsets) {
        if let Some(target_payload) = targets[plane] {
            replacement[pointer_offset..pointer_offset + 3].fill(0);
            fixups.push(PatchFixup {
                offset: pointer_offset,
                target_payload,
                target_addend: 0,
                encoding: PatchFixupEncoding::Long24LowBank,
            });
        }
    }
    PatchWrite {
        offset,
        expected: vec![0xff; replacement.len()],
        replacement,
        fixups,
    }
}

fn second_reader_template() -> Vec<u8> {
    let mut bytes = smw_us_v1_secondary_exit_second_reader(0x0002_fe00, 0x0002_fe00)
        .expect("fixed LoROM address")
        .to_vec();
    bytes[1..4].copy_from_slice(&[0x00, 0xfe, 0x05]);
    bytes
}

fn direct(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    }
}

fn used_plane_len(encoded: &[u8]) -> usize {
    (0..SecondaryExitTable::ENTRY_COUNT)
        .rfind(|index| {
            (0..6).any(|plane| encoded[plane * SecondaryExitTable::ENTRY_COUNT + index] != 0)
        })
        .map_or(0, |index| index + 1)
}

fn decode_hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).expect("ASCII hex");
            u8::from_str_radix(pair, 16).expect("valid embedded hex")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smw_us_v1_secondary_exit_locator;
    use lm_level::SecondaryExit;
    use lm_project::{Project, SecondaryExitStorage};
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_install_reopens_semantically_and_undoes_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
        let template = pe_rva(&executable, 0x1b_7f78, 0x510);
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let table = project
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap()
            .table;
        let result = project
            .install_relocatable_patch(
                &smw_us_v1_secondary_exit_installation_plan(template, &table).unwrap(),
            )
            .unwrap();
        assert_eq!(result.blocks.len(), 3);
        let reopened = project
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap();
        assert_eq!(reopened.table, table);
        assert!(matches!(
            reopened.storage,
            SecondaryExitStorage::Installed {
                fixed_prefix_planes: 4,
                tagged_planes,
                ..
            } if tagged_planes.len() == 2
        ));
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn extended_used_range_selects_six_owned_planes_and_reopens() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
        let template = pe_rva(&executable, 0x1b_7f78, 0x510);
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project = Project::open_supported(RomImage::from_bytes(original).unwrap()).unwrap();
        let mut table = project
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap()
            .table;
        table.entries[0x400] = SecondaryExit {
            destination_level: 0x105,
            ..SecondaryExit::default()
        };
        let result = project
            .install_relocatable_patch(
                &smw_us_v1_secondary_exit_installation_plan(template, &table).unwrap(),
            )
            .unwrap();
        assert_eq!(result.blocks.len(), 7);
        let reopened = project
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap();
        assert_eq!(reopened.table, table);
        assert!(matches!(
            reopened.storage,
            SecondaryExitStorage::Installed {
                fixed_prefix_planes: 0,
                used_len: 0x401,
                tagged_planes,
            } if tagged_planes.len() == 6
        ));
    }

    #[test]
    fn compact_install_can_replace_pristine_fixed_planes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
        let template = pe_rva(&executable, 0x1b_7f78, 0x510);
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project = Project::open_supported(RomImage::from_bytes(original).unwrap()).unwrap();
        let source = project
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap()
            .table;
        let cleared = SecondaryExitTable {
            entries: vec![SecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT],
        };

        project
            .install_relocatable_patch(
                &smw_us_v1_secondary_exit_installation_plan_from_source(
                    template, &source, &cleared,
                )
                .unwrap(),
            )
            .unwrap();
        let reopened = project
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap();
        assert_eq!(reopened.table, cleared);
        assert!(matches!(
            reopened.storage,
            SecondaryExitStorage::Installed {
                fixed_prefix_planes: 4,
                ..
            }
        ));
    }

    #[test]
    fn late_hook_precondition_failure_rolls_back_expansion_and_allocations() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
        let template = pe_rva(&executable, 0x1b_7f78, 0x510);
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut image = RomImage::from_bytes(original).unwrap();
        image.write(0x0002_d9c3, &[0xff]).unwrap();
        let snapshot = image.as_file_bytes().to_vec();
        let mut project = Project::new(image);
        let table = project
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap()
            .table;
        assert!(
            project
                .install_relocatable_patch(
                    &smw_us_v1_secondary_exit_installation_plan(template, &table).unwrap(),
                )
                .is_err()
        );
        assert_eq!(project.save_snapshot(), snapshot);
        assert_eq!(project.history.undo_len(), 0);
    }

    fn pe_rva(image: &[u8], rva: usize, len: usize) -> &[u8] {
        let pe =
            usize::try_from(u32::from_le_bytes(image[0x3c..0x40].try_into().unwrap())).unwrap();
        let count = usize::from(u16::from_le_bytes(
            image[pe + 6..pe + 8].try_into().unwrap(),
        ));
        let optional = usize::from(u16::from_le_bytes(
            image[pe + 20..pe + 22].try_into().unwrap(),
        ));
        for index in 0..count {
            let entry = pe + 24 + optional + index * 40;
            let size = usize::try_from(u32::from_le_bytes(
                image[entry + 8..entry + 12].try_into().unwrap(),
            ))
            .unwrap();
            let address = usize::try_from(u32::from_le_bytes(
                image[entry + 12..entry + 16].try_into().unwrap(),
            ))
            .unwrap();
            if (address..address + size).contains(&rva) {
                let raw = usize::try_from(u32::from_le_bytes(
                    image[entry + 20..entry + 24].try_into().unwrap(),
                ))
                .unwrap();
                let start = raw + rva - address;
                return &image[start..start + len];
            }
        }
        panic!("RVA not present");
    }
}
