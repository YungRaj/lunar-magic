//! Authenticated Lunar Magic 3.63 overworld-animation runtime for pristine SMW-US LoROM.

use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::{AllocationPolicy, HEADER_LEN, HeaderError, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, pc_to_snes, snes_to_pc};
use std::fmt;

use crate::SMW_US_V1_CHECKSUM_FIELD;

pub const SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_LEN: usize = 0xc20;
pub const SMW_US_V1_OVERWORLD_ANIMATION_AUXILIARY_LEN: usize = 0x15;
pub const SMW_US_V1_OVERWORLD_ANIMATION_OPTIONS_LEN: usize = 7;
pub const SMW_US_V1_OVERWORLD_ANIMATION_SEARCH_START: usize = 0x0008_0000;
pub const SMW_US_V1_OVERWORLD_ANIMATION_SEARCH_END: usize = 0x0010_0000;

const HOOK_A: usize = 0x0002_0086;
const HOOK_B: usize = 0x0000_24e3;
const HOOK_C_OPERAND: usize = 0x0002_00e0;
const MODE_BYTES: [usize; 3] = [0x0002_0102, 0x0002_010d, 0x0002_013b];

const FIXED_OW_TABLE: usize = 0x0001_bcc0;
const FIXED_ANIMATION_RAM: usize = 0x0002_0413;
const FIXED_HELPER: usize = 0x0000_360c;

const LOCAL_WORD_TABLE_OFFSET: usize = 0xb3b;
const LOCAL_WORD_TABLE_ENTRIES: usize = 108;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1OverworldAnimationRuntimeError {
    InvalidBase64,
    WrongTemplateLength(usize),
    FixedAddress,
    LocalWordBelowRuntime {
        index: usize,
        value: u16,
    },
    FixedRange {
        offset: usize,
    },
    FixedMismatch {
        offset: usize,
    },
    Pointer(RomError),
    OwnedBeforeHeader {
        target: usize,
    },
    OwnedHeader {
        target: usize,
        source: HeaderError,
    },
    OwnedStart {
        expected: usize,
        actual: usize,
    },
    OwnedLength {
        target: usize,
        expected: usize,
        actual: usize,
    },
    RuntimeMismatch,
    AuxiliaryMismatch,
}

impl fmt::Display for SmwUsV1OverworldAnimationRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "overworld animation runtime failed: {self:?}")
    }
}

impl std::error::Error for SmwUsV1OverworldAnimationRuntimeError {}

fn decode_base64(text: &str) -> Result<Vec<u8>, SmwUsV1OverworldAnimationRuntimeError> {
    let mut output = Vec::new();
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    let mut saw_padding = false;
    for byte in text.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        if byte == b'=' {
            saw_padding = true;
            continue;
        }
        if saw_padding {
            return Err(SmwUsV1OverworldAnimationRuntimeError::InvalidBase64);
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(SmwUsV1OverworldAnimationRuntimeError::InvalidBase64),
        };
        accumulator = accumulator << 6 | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1_u32 << bits).wrapping_sub(1);
        }
    }
    Ok(output)
}

/// Returns the exact `$C20` stack-buffer image assembled by `InstallOverworldAnimationRuntime`
/// (`Lunar Magic.exe` `$004B2440`) before its LoROM relocation pass.
pub fn smw_us_v1_overworld_animation_runtime_template()
-> Result<Vec<u8>, SmwUsV1OverworldAnimationRuntimeError> {
    let bytes = decode_base64(include_str!("assets/overworld_animation_runtime_core.b64"))?;
    if bytes.len() != SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_LEN {
        return Err(SmwUsV1OverworldAnimationRuntimeError::WrongTemplateLength(
            bytes.len(),
        ));
    }
    Ok(bytes)
}

fn low_bank_address(offset: usize) -> Result<u32, SmwUsV1OverworldAnimationRuntimeError> {
    Ok(pc_to_snes(Mapper::LoRom, offset)
        .map_err(|_| SmwUsV1OverworldAnimationRuntimeError::FixedAddress)?
        & 0x7f_ffff)
}

fn write_fixed_long(
    bytes: &mut [u8],
    offset: usize,
    target: usize,
) -> Result<(), SmwUsV1OverworldAnimationRuntimeError> {
    let address = low_bank_address(target)?.to_le_bytes();
    bytes[offset..offset + 3].copy_from_slice(&address[..3]);
    Ok(())
}

fn write_fixed_low(
    bytes: &mut [u8],
    offset: usize,
    target: usize,
) -> Result<(), SmwUsV1OverworldAnimationRuntimeError> {
    let address = low_bank_address(target)?.to_le_bytes();
    bytes[offset..offset + 2].copy_from_slice(&address[..2]);
    Ok(())
}

fn runtime_payload() -> Result<PatchPayload, SmwUsV1OverworldAnimationRuntimeError> {
    let mut bytes = smw_us_v1_overworld_animation_runtime_template()?;

    // Ordinary LoROM selects mapping byte zero and skips every mapper-only IRAM conversion.
    bytes[0x58] = 0;
    bytes[0x61..0x63].copy_from_slice(&0_u16.to_le_bytes());

    for (offset, target) in [
        (0xb1, FIXED_OW_TABLE),
        (0xb8, FIXED_OW_TABLE + 1),
        (0x132, FIXED_OW_TABLE),
        (0x139, FIXED_OW_TABLE + 1),
        (0x168, HOOK_A + 6),
        (0x551, HOOK_C_OPERAND + 6),
        (0x555, HOOK_C_OPERAND + 0x43),
    ] {
        write_fixed_long(&mut bytes, offset, target)?;
    }
    for (offset, target) in [
        (0x165, FIXED_ANIMATION_RAM),
        (0x4d6, FIXED_HELPER),
        (0x4dc, FIXED_HELPER + 1),
        (0x4e7, FIXED_HELPER + 0x10),
        (0x4ed, FIXED_HELPER + 0x11),
        (0x548, FIXED_ANIMATION_RAM),
    ] {
        write_fixed_low(&mut bytes, offset, target)?;
    }

    let long = PatchFixupEncoding::Long24LowBank;
    let low = PatchFixupEncoding::Low16;
    let mut fixups = vec![
        PatchFixup {
            offset: 0x4a,
            target_payload: 2,
            target_addend: 0,
            encoding: long,
        },
        PatchFixup {
            offset: 0xd6,
            target_payload: 1,
            target_addend: 1,
            encoding: long,
        },
        PatchFixup {
            offset: 0xe1,
            target_payload: 1,
            target_addend: 0,
            encoding: long,
        },
        PatchFixup {
            offset: 0x179,
            target_payload: 0,
            target_addend: 0x500,
            encoding: long,
        },
        PatchFixup {
            offset: 0x1b9,
            target_payload: 0,
            target_addend: 0x1f0,
            encoding: long,
        },
        PatchFixup {
            offset: 0x1c5,
            target_payload: 0,
            target_addend: 0x1f0,
            encoding: long,
        },
        PatchFixup {
            offset: 0x5b7,
            target_payload: 0,
            target_addend: 0x618,
            encoding: low,
        },
        PatchFixup {
            offset: 0x602,
            target_payload: 0,
            target_addend: 0x618,
            encoding: low,
        },
        PatchFixup {
            offset: 0x619,
            target_payload: 0,
            target_addend: 0xafe,
            encoding: long,
        },
        PatchFixup {
            offset: 0x628,
            target_payload: 0,
            target_addend: 0xb3b,
            encoding: low,
        },
        PatchFixup {
            offset: 0x62c,
            target_payload: 0,
            target_addend: 0x638,
            encoding: low,
        },
        PatchFixup {
            offset: 0x63f,
            target_payload: 0,
            target_addend: 0xb73,
            encoding: low,
        },
        PatchFixup {
            offset: 0x6a7,
            target_payload: 0,
            target_addend: 0xb73,
            encoding: low,
        },
        PatchFixup {
            offset: 0x724,
            target_payload: 0,
            target_addend: 0xb73,
            encoding: low,
        },
        PatchFixup {
            offset: 0x7dd,
            target_payload: 0,
            target_addend: 0xb73,
            encoding: low,
        },
        PatchFixup {
            offset: 0x7eb,
            target_payload: 0,
            target_addend: 0xb73,
            encoding: low,
        },
        PatchFixup {
            offset: 0x84a,
            target_payload: 0,
            target_addend: 0xb73,
            encoding: low,
        },
        PatchFixup {
            offset: 0x858,
            target_payload: 0,
            target_addend: 0xb73,
            encoding: low,
        },
        PatchFixup {
            offset: 0x8b5,
            target_payload: 0,
            target_addend: 0xb73,
            encoding: low,
        },
        PatchFixup {
            offset: 0x66a,
            target_payload: 0,
            target_addend: 0xb15,
            encoding: long,
        },
        PatchFixup {
            offset: 0x8f9,
            target_payload: 0,
            target_addend: 0xb06,
            encoding: long,
        },
        PatchFixup {
            offset: 0xa68,
            target_payload: 0,
            target_addend: 0xb0d,
            encoding: long,
        },
        PatchFixup {
            offset: 0xa94,
            target_payload: 0,
            target_addend: 0xb0d,
            encoding: long,
        },
        PatchFixup {
            offset: 0xab9,
            target_payload: 0,
            target_addend: 0xb0d,
            encoding: long,
        },
        PatchFixup {
            offset: 0xade,
            target_payload: 0,
            target_addend: 0xb0d,
            encoding: long,
        },
    ];
    for index in 0..LOCAL_WORD_TABLE_ENTRIES {
        let offset = LOCAL_WORD_TABLE_OFFSET + index * 2;
        let source = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        let relative = usize::from(source & 0x7fff);
        if relative >= 0x720 {
            return Err(
                SmwUsV1OverworldAnimationRuntimeError::LocalWordBelowRuntime {
                    index,
                    value: source,
                },
            );
        }
        fixups.push(PatchFixup {
            offset,
            target_payload: 0,
            target_addend: 0x500 + relative,
            encoding: low,
        });
    }
    Ok(PatchPayload { bytes, fixups })
}

fn payload_write(
    offset: usize,
    expected: &[u8],
    opcode: Option<u8>,
    target_payload: usize,
    target_addend: usize,
) -> PatchWrite {
    let operand_offset = usize::from(opcode.is_some());
    let mut replacement = vec![0; expected.len()];
    if let Some(opcode) = opcode {
        replacement[0] = opcode;
    }
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement,
        fixups: vec![PatchFixup {
            offset: operand_offset,
            target_payload,
            target_addend,
            encoding: PatchFixupEncoding::Long24LowBank,
        }],
    }
}

/// Constructs Lunar Magic 3.63's exact pristine SMW-US LoROM runtime transaction.
pub fn smw_us_v1_overworld_animation_runtime_installation_plan()
-> Result<RelocatablePatchPlan, SmwUsV1OverworldAnimationRuntimeError> {
    let mut auxiliary = vec![0; SMW_US_V1_OVERWORLD_ANIMATION_AUXILIARY_LEN];
    for offset in (0..SMW_US_V1_OVERWORLD_ANIMATION_AUXILIARY_LEN).step_by(3) {
        auxiliary[offset] = 0xff;
    }
    let mut writes = vec![
        payload_write(HOOK_A, &[0xc2, 0x30, 0x64, 0x03], Some(0x22), 0, 0),
        payload_write(HOOK_B, &[0xc2, 0x10, 0xa9, 0x80], Some(0x22), 0, 0x1f0),
        payload_write(HOOK_C_OPERAND, &[0xa5, 0x13, 0x29], None, 0, 0x500),
    ];
    writes.extend(MODE_BYTES.into_iter().map(|offset| PatchWrite {
        offset,
        expected: vec![0x13],
        replacement: vec![0x14],
        fixups: Vec::new(),
    }));
    Ok(RelocatablePatchPlan {
        description: "install SMW US v1 LoROM overworld animation runtime".to_owned(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy::lorom(
            SMW_US_V1_OVERWORLD_ANIMATION_SEARCH_START..SMW_US_V1_OVERWORLD_ANIMATION_SEARCH_END,
        ),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![
            runtime_payload()?,
            PatchPayload {
                bytes: auxiliary,
                fixups: Vec::new(),
            },
            PatchPayload {
                bytes: vec![0; SMW_US_V1_OVERWORLD_ANIMATION_OPTIONS_LEN],
                fixups: Vec::new(),
            },
        ],
        writes,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1OverworldAnimationRuntime {
    pub runtime: RatsBlock,
    pub auxiliary: RatsBlock,
    pub options: RatsBlock,
}

fn read_low_bank_pointer(
    bytes: &[u8],
    offset: usize,
) -> Result<usize, SmwUsV1OverworldAnimationRuntimeError> {
    let operand = bytes
        .get(offset..offset + 3)
        .ok_or(SmwUsV1OverworldAnimationRuntimeError::FixedRange { offset })?;
    snes_to_pc(
        Mapper::LoRom,
        u32::from_le_bytes([operand[0], operand[1], operand[2], 0]),
    )
    .map_err(SmwUsV1OverworldAnimationRuntimeError::Pointer)
}

fn owned_block(
    bytes: &[u8],
    target: usize,
    expected_len: usize,
) -> Result<RatsBlock, SmwUsV1OverworldAnimationRuntimeError> {
    let header = target
        .checked_sub(HEADER_LEN)
        .ok_or(SmwUsV1OverworldAnimationRuntimeError::OwnedBeforeHeader { target })?;
    let block = parse_at(bytes, header)
        .map_err(|source| SmwUsV1OverworldAnimationRuntimeError::OwnedHeader { target, source })?;
    if block.payload.start != target {
        return Err(SmwUsV1OverworldAnimationRuntimeError::OwnedStart {
            expected: target,
            actual: block.payload.start,
        });
    }
    if block.payload.len() != expected_len {
        return Err(SmwUsV1OverworldAnimationRuntimeError::OwnedLength {
            target,
            expected: expected_len,
            actual: block.payload.len(),
        });
    }
    Ok(block)
}

fn apply_materialized_fixups(
    bytes: &mut [u8],
    fixups: &[PatchFixup],
    blocks: &[RatsBlock],
) -> Result<(), SmwUsV1OverworldAnimationRuntimeError> {
    for fixup in fixups {
        let target = blocks[fixup.target_payload].payload.start + fixup.target_addend;
        let mut encoded = pc_to_snes(Mapper::LoRom, target)
            .map_err(SmwUsV1OverworldAnimationRuntimeError::Pointer)?
            .to_le_bytes();
        if matches!(
            fixup.encoding,
            PatchFixupEncoding::Long24LowBank | PatchFixupEncoding::Bank8LowBank
        ) {
            encoded[2] &= 0x7f;
        }
        let replacement = match fixup.encoding {
            PatchFixupEncoding::Long24 | PatchFixupEncoding::Long24LowBank => &encoded[..3],
            PatchFixupEncoding::Low16 => &encoded[..2],
            PatchFixupEncoding::Low8 => &encoded[..1],
            PatchFixupEncoding::High8 => &encoded[1..2],
            PatchFixupEncoding::Bank8 | PatchFixupEncoding::Bank8LowBank => &encoded[2..3],
        };
        bytes[fixup.offset..fixup.offset + replacement.len()].copy_from_slice(replacement);
    }
    Ok(())
}

/// Authenticates every fixed write, allocation owner, immutable auxiliary byte, and relocated
/// runtime byte. The seven option bytes are deliberately mutable and are authenticated only by
/// their exact seven-byte RATS owner and the runtime's `+$4A` pointer.
pub fn detect_smw_us_v1_overworld_animation_runtime(
    bytes: &[u8],
) -> Result<Option<SmwUsV1OverworldAnimationRuntime>, SmwUsV1OverworldAnimationRuntimeError> {
    let marker = *bytes
        .get(HOOK_B)
        .ok_or(SmwUsV1OverworldAnimationRuntimeError::FixedRange { offset: HOOK_B })?;
    if marker != 0x22 {
        if bytes.get(HOOK_A..HOOK_A + 4) == Some(&[0xc2, 0x30, 0x64, 0x03])
            && bytes.get(HOOK_B..HOOK_B + 4) == Some(&[0xc2, 0x10, 0xa9, 0x80])
            && bytes.get(HOOK_C_OPERAND..HOOK_C_OPERAND + 3) == Some(&[0xa5, 0x13, 0x29])
            && MODE_BYTES
                .iter()
                .all(|offset| bytes.get(*offset) == Some(&0x13))
        {
            return Ok(None);
        }
        return Err(SmwUsV1OverworldAnimationRuntimeError::FixedMismatch { offset: HOOK_B });
    }
    if bytes.get(HOOK_A) != Some(&0x22) {
        return Err(SmwUsV1OverworldAnimationRuntimeError::FixedMismatch { offset: HOOK_A });
    }
    if MODE_BYTES
        .iter()
        .any(|offset| bytes.get(*offset) != Some(&0x14))
    {
        return Err(SmwUsV1OverworldAnimationRuntimeError::FixedMismatch {
            offset: MODE_BYTES[0],
        });
    }

    let runtime_target = read_low_bank_pointer(bytes, HOOK_A + 1)?;
    let runtime = owned_block(
        bytes,
        runtime_target,
        SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_LEN,
    )?;
    let hook_b_target = read_low_bank_pointer(bytes, HOOK_B + 1)?;
    let hook_c_target = read_low_bank_pointer(bytes, HOOK_C_OPERAND)?;
    if hook_b_target != runtime.payload.start + 0x1f0
        || hook_c_target != runtime.payload.start + 0x500
    {
        return Err(SmwUsV1OverworldAnimationRuntimeError::FixedMismatch { offset: HOOK_B });
    }
    let auxiliary_target = read_low_bank_pointer(bytes, runtime.payload.start + 0xe1)?;
    let auxiliary = owned_block(
        bytes,
        auxiliary_target,
        SMW_US_V1_OVERWORLD_ANIMATION_AUXILIARY_LEN,
    )?;
    let options_target = read_low_bank_pointer(bytes, runtime.payload.start + 0x4a)?;
    let options = owned_block(
        bytes,
        options_target,
        SMW_US_V1_OVERWORLD_ANIMATION_OPTIONS_LEN,
    )?;
    let blocks = [runtime.clone(), auxiliary.clone(), options.clone()];
    let payload = runtime_payload()?;
    let mut expected_runtime = payload.bytes;
    apply_materialized_fixups(&mut expected_runtime, &payload.fixups, &blocks)?;
    if bytes.get(runtime.payload.clone()) != Some(expected_runtime.as_slice()) {
        return Err(SmwUsV1OverworldAnimationRuntimeError::RuntimeMismatch);
    }
    let mut expected_auxiliary = vec![0; SMW_US_V1_OVERWORLD_ANIMATION_AUXILIARY_LEN];
    for offset in (0..SMW_US_V1_OVERWORLD_ANIMATION_AUXILIARY_LEN).step_by(3) {
        expected_auxiliary[offset] = 0xff;
    }
    if bytes.get(auxiliary.payload.clone()) != Some(expected_auxiliary.as_slice()) {
        return Err(SmwUsV1OverworldAnimationRuntimeError::AuxiliaryMismatch);
    }
    Ok(Some(SmwUsV1OverworldAnimationRuntime {
        runtime,
        auxiliary,
        options,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rats::parse_at;
    use lm_rom::{RomImage, snes_to_pc};
    use sha2::{Digest, Sha256};

    fn source() -> Project {
        let mut bytes = vec![0xff; 0x10_0000];
        bytes[HOOK_A..HOOK_A + 4].copy_from_slice(&[0xc2, 0x30, 0x64, 0x03]);
        bytes[HOOK_B..HOOK_B + 4].copy_from_slice(&[0xc2, 0x10, 0xa9, 0x80]);
        bytes[HOOK_C_OPERAND..HOOK_C_OPERAND + 3].copy_from_slice(&[0xa5, 0x13, 0x29]);
        for offset in MODE_BYTES {
            bytes[offset] = 0x13;
        }
        Project::new(RomImage::from_bytes(bytes).unwrap())
    }

    #[test]
    fn recovered_template_has_exact_fragment_boundaries_and_local_table() {
        let bytes = smw_us_v1_overworld_animation_runtime_template().unwrap();
        assert_eq!(bytes.len(), 0xc20);
        assert_eq!(&bytes[..4], &[0xe2, 0x30, 0xae, 0xb3]);
        assert_eq!(&bytes[0x1f0..0x1f4], &[0x0b, 0xc2, 0x20, 0xa9]);
        assert_eq!(&bytes[0x500..0x504], &[0x8b, 0xa2, 0x7f, 0xda]);
        assert_eq!(&bytes[0xb3b..0xb3f], &[0x2a, 0x81, 0x38, 0x81]);
        assert_eq!(&bytes[0xc18..], &[0xff; 8]);
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            "e4a615bc2d0cb5306bc719b1f3527a40cb61458522cd4c485437a7c12bd7ff02"
        );
    }

    #[test]
    fn pristine_install_publishes_all_three_owned_blocks_and_pointer_chain() {
        let mut project = source();
        assert_eq!(
            detect_smw_us_v1_overworld_animation_runtime(project.rom.logical_bytes()).unwrap(),
            None
        );
        let original = project.rom.logical_bytes().to_vec();
        let plan = smw_us_v1_overworld_animation_runtime_installation_plan().unwrap();
        let result = project.install_relocatable_patch(&plan).unwrap();
        assert_eq!(result.blocks.len(), 3);
        assert_eq!(result.blocks[0].payload.len(), 0xc20);
        assert_eq!(result.blocks[1].payload.len(), 0x15);
        assert_eq!(result.blocks[2].payload.len(), 7);
        let bytes = project.rom.logical_bytes();
        assert_eq!(bytes[HOOK_A], 0x22);
        assert_eq!(bytes[HOOK_B], 0x22);
        let runtime_entry = (pc_to_snes(Mapper::LoRom, result.blocks[0].payload.start + 0x500)
            .unwrap()
            & 0x7f_ffff)
            .to_le_bytes();
        assert_eq!(
            &bytes[HOOK_C_OPERAND..HOOK_C_OPERAND + 3],
            &runtime_entry[..3]
        );
        let runtime = result.blocks[0].payload.start;
        let options_address = u32::from_le_bytes([
            bytes[runtime + 0x4a],
            bytes[runtime + 0x4b],
            bytes[runtime + 0x4c],
            0,
        ]);
        assert_eq!(
            snes_to_pc(Mapper::LoRom, options_address).unwrap(),
            result.blocks[2].payload.start
        );
        assert!(parse_at(bytes, result.blocks[1].header_offset).is_ok());
        let detected = detect_smw_us_v1_overworld_animation_runtime(bytes)
            .unwrap()
            .unwrap();
        assert_eq!(detected.runtime, result.blocks[0]);
        assert_eq!(detected.auxiliary, result.blocks[1]);
        assert_eq!(detected.options, result.blocks[2]);
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.logical_bytes(), original);
        assert!(project.redo().unwrap());
        assert_eq!(project.rom.logical_bytes()[HOOK_A], 0x22);
    }

    #[test]
    fn late_precondition_failure_is_atomic() {
        let mut project = source();
        project.rom.write(MODE_BYTES[2], &[0x12]).unwrap();
        let original = project.rom.logical_bytes().to_vec();
        let history_len = project.history.undo_len();
        let error = project
            .install_relocatable_patch(
                &smw_us_v1_overworld_animation_runtime_installation_plan().unwrap(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            lm_project::RelocatablePatchError::HookPreconditionMismatch { .. }
        ));
        assert_eq!(project.rom.logical_bytes(), original);
        assert_eq!(project.history.undo_len(), history_len);
    }

    #[test]
    fn detector_rejects_owned_runtime_or_auxiliary_corruption() {
        let mut project = source();
        let result = project
            .install_relocatable_patch(
                &smw_us_v1_overworld_animation_runtime_installation_plan().unwrap(),
            )
            .unwrap();
        let runtime_byte = result.blocks[0].payload.start + 0x100;
        let original_runtime_byte = project.rom.read(runtime_byte, 1).unwrap()[0];
        project
            .rom
            .write(runtime_byte, &[original_runtime_byte ^ 1])
            .unwrap();
        assert!(matches!(
            detect_smw_us_v1_overworld_animation_runtime(project.rom.logical_bytes()),
            Err(SmwUsV1OverworldAnimationRuntimeError::RuntimeMismatch)
        ));
        project
            .rom
            .write(runtime_byte, &[original_runtime_byte])
            .unwrap();
        let auxiliary_byte = result.blocks[1].payload.start + 1;
        let original_auxiliary_byte = project.rom.read(auxiliary_byte, 1).unwrap()[0];
        project
            .rom
            .write(auxiliary_byte, &[original_auxiliary_byte ^ 1])
            .unwrap();
        assert!(matches!(
            detect_smw_us_v1_overworld_animation_runtime(project.rom.logical_bytes()),
            Err(SmwUsV1OverworldAnimationRuntimeError::AuxiliaryMismatch)
        ));
    }

    #[test]
    fn authenticated_pristine_fixture_installs_saves_options_reopens_and_undoes() {
        let source = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project = Project::new(RomImage::from_bytes(source.clone()).unwrap());
        assert_eq!(
            detect_smw_us_v1_overworld_animation_runtime(project.rom.logical_bytes()).unwrap(),
            None
        );
        let result = project
            .install_relocatable_patch(
                &smw_us_v1_overworld_animation_runtime_installation_plan().unwrap(),
            )
            .unwrap();
        assert_eq!(project.rom.logical_len(), 0x10_0000);
        let detected = detect_smw_us_v1_overworld_animation_runtime(project.rom.logical_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(detected.runtime, result.blocks[0]);
        let options = [1, 2, 4, 8, 0x10, 0x20, 0x40];
        assert!(
            project
                .save_installed_overworld_animation_options(
                    options,
                    0xa5,
                    crate::smw_us_v1_overworld_animation_options_layout(),
                    SMW_US_V1_CHECKSUM_FIELD,
                )
                .unwrap()
        );
        let loaded = project
            .load_installed_overworld_animation_options(
                crate::smw_us_v1_overworld_animation_options_layout(),
            )
            .unwrap();
        assert!(loaded.runtime_installed);
        assert_eq!(loaded.feature_bytes, options);
        assert_eq!(loaded.lightning_disable_mask, 0xa5);
        assert!(
            detect_smw_us_v1_overworld_animation_runtime(project.rom.logical_bytes())
                .unwrap()
                .is_some()
        );
        assert!(project.undo().unwrap());
        assert!(project.undo().unwrap());
        assert_eq!(project.rom.logical_bytes(), source);
    }
}
