//! Identity-checked installation plan for the SMW US v1 Lfix3 core.

use crate::{Lfix3RuntimeLengthError, SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_lfix3_runtime_payload};
use lm_project::{PatchFixup, PatchFixupEncoding, PatchWrite, RelocatablePatchPlan};
use lm_rats::{AllocationPolicy, HEADER_LEN, HeaderError, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, pc_to_snes, snes_to_pc};
use std::fmt;

pub const SMW_US_V1_LFIX3_SEARCH_START: usize = 0x0008_0000;
pub const SMW_US_V1_LFIX3_SEARCH_END: usize = 0x0010_0000;

const CORE_HELPER: [u8; 0x20] = [
    0x2c, 0x2a, 0x19, 0x10, 0x04, 0xa9, 0x80, 0x85, 0x86, 0x50, 0x04, 0xa9, 0x01, 0x85, 0x85, 0xa9,
    0xc0, 0x1c, 0x2a, 0x19, 0xc2, 0x20, 0xa5, 0x1c, 0xc5, 0x06, 0xe2, 0x20, 0x6b, 0xff, 0xff, 0xff,
];

const TABLE_HELPER: [u8; 0x50] = [
    0x4a, 0x8d, 0x2a, 0x19, 0xbb, 0xbf, 0x00, 0xfc, 0x06, 0x85, 0x04, 0xbf, 0x00, 0xfe, 0x06, 0x8d,
    0xcd, 0x13, 0xb9, 0x00, 0xde, 0xaa, 0x29, 0xc0, 0x0c, 0x2a, 0x19, 0x8a, 0x89, 0x20, 0xf0, 0x25,
    0x29, 0x18, 0x0a, 0x0a, 0x0a, 0x0a, 0x85, 0x94, 0x2a, 0x85, 0x95, 0xb9, 0x00, 0xf2, 0x0a, 0x0a,
    0x0a, 0x0a, 0x29, 0x70, 0x04, 0x94, 0xb9, 0x00, 0xf0, 0x0a, 0x0a, 0x0a, 0x0a, 0x85, 0x96, 0xa5,
    0x04, 0x29, 0x3f, 0x85, 0x97, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4c, 0x4d, 0x11, 0x01,
];

const CURRENT_MARKER_OFFSET: usize = 0x0002_dd30 + 0x4c;
const MUTABLE_TABLE_OFFSETS: [usize; 3] = [0x0002_de00, 0x0003_7c00, 0x0003_7e00];

#[derive(Debug)]
pub enum SmwUsV1Lfix3DetectError {
    Plan(Lfix3RuntimeLengthError),
    PlanMissingRuntimeHook,
    RuntimeAddress(RomError),
    RuntimeBeforeHeader(usize),
    RuntimeHeader(HeaderError),
    RuntimeOwnership {
        expected: usize,
        actual: usize,
    },
    RuntimeLength(usize),
    FixupTargetOverflow,
    FixedByteMismatch {
        offset: usize,
        expected: u8,
        actual: Option<u8>,
    },
    RuntimePayloadMismatch,
}

impl fmt::Display for SmwUsV1Lfix3DetectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot authenticate current SMW-US Lfix3 runtime: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1Lfix3DetectError {}

/// Authenticates Lunar Magic's current Lfix3 core without constraining its mutable level tables.
///
/// Returns `Ok(None)` only when both the fixed current-version marker and primary hook are absent.
/// Either current-format signal requires every helper and entry hook to match, all seven hooks to resolve through their typed
/// addends to one exactly owned `$510`-byte RATS payload, and that complete relocated payload to
/// equal the recovered runtime. The three initialized 512-byte tables are deliberately excluded
/// because ordinary level editing changes their contents.
///
/// # Errors
///
/// Rejects malformed bundled evidence, truncated or modified hooks/helpers, invalid runtime
/// addresses, malformed/non-exact RATS ownership, and any relocated runtime payload difference.
pub fn detect_smw_us_v1_current_lfix3_runtime(
    bytes: &[u8],
) -> Result<Option<RatsBlock>, SmwUsV1Lfix3DetectError> {
    let marker_present =
        bytes.get(CURRENT_MARKER_OFFSET..CURRENT_MARKER_OFFSET + 4) == Some(&TABLE_HELPER[0x4c..]);
    let primary_hook_present = bytes.get(0x0002_da17).copied() == Some(0x22);
    if !marker_present && !primary_hook_present {
        return Ok(None);
    }
    let plan =
        smw_us_v1_builtin_lfix3_installation_plan().map_err(SmwUsV1Lfix3DetectError::Plan)?;
    let first_hook = plan
        .writes
        .iter()
        .find(|write| !write.fixups.is_empty())
        .ok_or(SmwUsV1Lfix3DetectError::PlanMissingRuntimeHook)?;
    let operand = bytes
        .get(first_hook.offset + 1..first_hook.offset + 4)
        .ok_or(SmwUsV1Lfix3DetectError::FixedByteMismatch {
            offset: first_hook.offset + 1,
            expected: 0,
            actual: None,
        })?;
    let runtime_offset = snes_to_pc(
        Mapper::LoRom,
        u32::from_le_bytes([operand[0], operand[1], operand[2], 0]),
    )
    .map_err(SmwUsV1Lfix3DetectError::RuntimeAddress)?;
    let header_offset = runtime_offset
        .checked_sub(HEADER_LEN)
        .ok_or(SmwUsV1Lfix3DetectError::RuntimeBeforeHeader(runtime_offset))?;
    let block = parse_at(bytes, header_offset).map_err(SmwUsV1Lfix3DetectError::RuntimeHeader)?;
    if block.payload.start != runtime_offset {
        return Err(SmwUsV1Lfix3DetectError::RuntimeOwnership {
            expected: runtime_offset,
            actual: block.payload.start,
        });
    }
    if block.payload.len() != crate::SMW_US_V1_LFIX3_RUNTIME_LEN {
        return Err(SmwUsV1Lfix3DetectError::RuntimeLength(block.payload.len()));
    }
    for write in &plan.writes {
        if MUTABLE_TABLE_OFFSETS.contains(&write.offset) {
            continue;
        }
        let mut expected = write.replacement.clone();
        relocate_for_runtime(
            &mut expected,
            &write.fixups,
            runtime_offset,
            block.payload.len(),
        )?;
        require_exact(bytes, write.offset, &expected)?;
    }
    let mut expected_runtime = plan.payloads[0].bytes.clone();
    relocate_for_runtime(
        &mut expected_runtime,
        &plan.payloads[0].fixups,
        runtime_offset,
        block.payload.len(),
    )?;
    if bytes.get(block.payload.clone()) != Some(expected_runtime.as_slice()) {
        return Err(SmwUsV1Lfix3DetectError::RuntimePayloadMismatch);
    }
    Ok(Some(block))
}

fn require_exact(
    bytes: &[u8],
    offset: usize,
    expected: &[u8],
) -> Result<(), SmwUsV1Lfix3DetectError> {
    for (index, expected) in expected.iter().copied().enumerate() {
        let actual = bytes.get(offset + index).copied();
        if actual != Some(expected) {
            return Err(SmwUsV1Lfix3DetectError::FixedByteMismatch {
                offset: offset + index,
                expected,
                actual,
            });
        }
    }
    Ok(())
}

fn relocate_for_runtime(
    bytes: &mut [u8],
    fixups: &[PatchFixup],
    runtime_offset: usize,
    runtime_len: usize,
) -> Result<(), SmwUsV1Lfix3DetectError> {
    for fixup in fixups {
        let target = runtime_offset
            .checked_add(fixup.target_addend)
            .filter(|target| *target < runtime_offset + runtime_len)
            .ok_or(SmwUsV1Lfix3DetectError::FixupTargetOverflow)?;
        let mut encoded = pc_to_snes(Mapper::LoRom, target)
            .map_err(SmwUsV1Lfix3DetectError::RuntimeAddress)?
            .to_le_bytes();
        if matches!(
            fixup.encoding,
            PatchFixupEncoding::Long24LowBank | PatchFixupEncoding::Bank8LowBank
        ) {
            encoded[2] &= 0x7f;
        }
        let replacement: &[u8] = match fixup.encoding {
            PatchFixupEncoding::Long24 | PatchFixupEncoding::Long24LowBank => &encoded[..3],
            PatchFixupEncoding::Low16 => &encoded[..2],
            PatchFixupEncoding::Bank8 | PatchFixupEncoding::Bank8LowBank => &encoded[2..3],
        };
        bytes[fixup.offset..fixup.offset + replacement.len()].copy_from_slice(replacement);
    }
    Ok(())
}

/// Builds the complete independently recovered Lfix3 core plan.
///
/// # Errors
///
/// Rejects a malformed embedded runtime template.
pub fn smw_us_v1_lfix3_installation_plan(
    runtime_template: &[u8],
) -> Result<RelocatablePatchPlan, Lfix3RuntimeLengthError> {
    Ok(RelocatablePatchPlan {
        description: "install SMW US v1 Lfix3 core".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy::lorom(
            SMW_US_V1_LFIX3_SEARCH_START..SMW_US_V1_LFIX3_SEARCH_END,
        ),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![smw_us_v1_lfix3_runtime_payload(runtime_template)?],
        writes: fixed_writes(),
    })
}

/// Builds the complete Lfix3 core plan from the revision profile's bundled runtime template.
///
/// # Errors
///
/// Rejects an inconsistent bundled runtime template.
pub fn smw_us_v1_builtin_lfix3_installation_plan()
-> Result<RelocatablePatchPlan, Lfix3RuntimeLengthError> {
    smw_us_v1_lfix3_installation_plan(&crate::smw_us_v1_lfix3_runtime_template())
}

fn fixed_writes() -> Vec<PatchWrite> {
    let mut writes = vec![
        direct(
            0x0000_26cc,
            &[0xa5, 0x1c, 0xc9, 0xc0],
            &[0x22, 0x00, 0xdd, 0x05],
        ),
        direct(0x0002_dd00, &[0xff; 0x20], &CORE_HELPER),
        direct(
            0x0002_d97d,
            &[0x4a, 0x8d, 0x2a, 0x19],
            &[0x22, 0x30, 0xdd, 0x05],
        ),
        direct(0x0002_dd30, &[0xff; 0x50], &TABLE_HELPER),
        direct(0x0002_de00, &[0xff; 0x200], &[0x00; 0x200]),
        direct(0x0003_7c00, &[0xff; 0x200], &[0x00; 0x200]),
        direct(0x0003_7e00, &[0xff; 0x200], &[0x1a; 0x200]),
        fixed(
            0x0000_52b2,
            &[0xa9, 0x40, 0x85, 0x7b],
            &[0xa5, 0xf9, 0x85, 0x7b],
        ),
    ];
    for (offset, expected, opcode, addend, trailing_nop) in [
        (
            0x0002_da17,
            &[0xe2, 0x30, 0xad, 0xbf][..],
            0x22,
            0x000,
            true,
        ),
        (
            0x0000_1708,
            &[0xa9, 0x20, 0x85, 0x5e][..],
            0x22,
            0x280,
            false,
        ),
        (
            0x0000_7871,
            &[0xa0, 0x04, 0x80, 0x0c][..],
            0x5c,
            0x2d0,
            false,
        ),
        (
            0x0000_777b,
            &[0x38, 0xf9, 0x2c, 0x14][..],
            0x5c,
            0x2f0,
            false,
        ),
        (
            0x0000_779d,
            &[0xac, 0x13, 0x14, 0xf0][..],
            0x5c,
            0x300,
            false,
        ),
        (
            0x0002_bca5,
            &[0xa9, 0x04, 0x8d, 0x56][..],
            0x5c,
            0x410,
            false,
        ),
        (
            0x0000_6966,
            &[0xc2, 0x20, 0xa5, 0x94][..],
            0x5c,
            0x4c0,
            false,
        ),
    ] {
        writes.push(payload_hook(offset, expected, opcode, addend, trailing_nop));
    }
    writes
}

fn direct(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    }
}

fn fixed(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    direct(offset, expected, replacement)
}

fn payload_hook(
    offset: usize,
    expected: &[u8],
    opcode: u8,
    addend: usize,
    trailing_nop: bool,
) -> PatchWrite {
    let mut replacement = vec![opcode, 0, 0, 0];
    if trailing_nop {
        replacement.push(0xea);
    }
    let mut expected = expected.to_vec();
    if trailing_nop {
        expected.push(0x13);
    }
    PatchWrite {
        offset,
        expected,
        replacement,
        fixups: vec![PatchFixup {
            offset: 1,
            target_payload: 0,
            target_addend: addend,
            encoding: PatchFixupEncoding::Long24LowBank,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rom::{RomImage, SnesChecksum};
    use std::{fs, path::PathBuf};

    #[test]
    fn plan_reproduces_the_recovered_lfix3_regions_and_undoes_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let executable = fs::read(root.join("lm363/Lunar Magic.exe")).unwrap();
        let template = pe_rva(&executable, 0x1b_7f78, 0x510);
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let result = project
            .install_relocatable_patch(&smw_us_v1_lfix3_installation_plan(template).unwrap())
            .unwrap();
        assert_eq!(result.blocks[0].payload.start, 0x0008_0008);
        for (offset, expected) in [
            (0x0002_da17, &[0x22, 0x08, 0x80, 0x10, 0xea][..]),
            (0x0000_1708, &[0x22, 0x88, 0x82, 0x10][..]),
            (0x0000_7871, &[0x5c, 0xd8, 0x82, 0x10][..]),
            (0x0000_777b, &[0x5c, 0xf8, 0x82, 0x10][..]),
            (0x0000_779d, &[0x5c, 0x08, 0x83, 0x10][..]),
            (0x0002_bca5, &[0x5c, 0x18, 0x84, 0x10][..]),
            (0x0000_6966, &[0x5c, 0xc8, 0x84, 0x10][..]),
        ] {
            assert_eq!(project.rom.read(offset, expected.len()).unwrap(), expected);
        }
        assert_eq!(project.rom.read(0x0002_dd00, 0x20).unwrap(), CORE_HELPER);
        assert_eq!(project.rom.read(0x0002_dd30, 0x50).unwrap(), TABLE_HELPER);
        assert!(
            project
                .rom
                .read(0x0002_de00, 0x200)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0)
        );
        assert!(
            project
                .rom
                .read(0x0003_7c00, 0x200)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0)
        );
        assert!(
            project
                .rom
                .read(0x0003_7e00, 0x200)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0x1a)
        );
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
                .is_complementary()
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn current_detector_authenticates_runtime_but_allows_mutable_level_tables() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        assert!(
            detect_smw_us_v1_current_lfix3_runtime(&original)
                .unwrap()
                .is_none()
        );
        let plan = smw_us_v1_builtin_lfix3_installation_plan().unwrap();
        let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
        let installed = project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(
            detect_smw_us_v1_current_lfix3_runtime(project.rom.logical_bytes())
                .unwrap()
                .unwrap(),
            installed.blocks[0]
        );

        let mut edited_tables = project.rom.logical_bytes().to_vec();
        for offset in MUTABLE_TABLE_OFFSETS {
            edited_tables[offset] ^= 0x5a;
        }
        assert!(
            detect_smw_us_v1_current_lfix3_runtime(&edited_tables)
                .unwrap()
                .is_some()
        );

        let mut marker_only = original;
        marker_only[CURRENT_MARKER_OFFSET..CURRENT_MARKER_OFFSET + 4]
            .copy_from_slice(&TABLE_HELPER[0x4c..]);
        assert!(matches!(
            detect_smw_us_v1_current_lfix3_runtime(&marker_only),
            Err(SmwUsV1Lfix3DetectError::RuntimeAddress(_)
                | SmwUsV1Lfix3DetectError::RuntimeHeader(_)
                | SmwUsV1Lfix3DetectError::RuntimeBeforeHeader(_))
        ));

        let mut modified = project.rom.logical_bytes().to_vec();
        modified[0x0000_26cc] ^= 1;
        assert!(matches!(
            detect_smw_us_v1_current_lfix3_runtime(&modified),
            Err(SmwUsV1Lfix3DetectError::FixedByteMismatch { offset: 0x26cc, .. })
        ));
        let mut modified = project.rom.logical_bytes().to_vec();
        modified[CURRENT_MARKER_OFFSET] ^= 1;
        assert!(matches!(
            detect_smw_us_v1_current_lfix3_runtime(&modified),
            Err(SmwUsV1Lfix3DetectError::FixedByteMismatch {
                offset: CURRENT_MARKER_OFFSET,
                ..
            })
        ));
        let mut modified = project.rom.logical_bytes().to_vec();
        modified[installed.blocks[0].payload.start] ^= 1;
        assert!(matches!(
            detect_smw_us_v1_current_lfix3_runtime(&modified),
            Err(SmwUsV1Lfix3DetectError::RuntimePayloadMismatch)
        ));
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
