//! Authenticated Lunar Magic 3.63 overworld-animation runtime for pristine SMW-US LoROM.

use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::{AllocationPolicy, HEADER_LEN, HeaderError, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, pc_to_snes, snes_to_pc};
use std::fmt;

use crate::SMW_US_V1_CHECKSUM_FIELD;

pub const SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_LEN: usize = 0xc20;
pub const SMW_US_V1_OVERWORLD_ANIMATION_MAPPER_RUNTIME_LEN: usize = 0xc40;
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
const SA1_IRAM_WORD_OFFSETS: [usize; 8] = [0x76b, 0x774, 0x78f, 0x7a7, 0x824, 0x894, 0x8e3, 0x8f4];
const SA1_IRAM_BYTE_OFFSET: usize = 0x7d1;
const MAPPER_SUFFIX_POINTER_OFFSET: usize = 0x8f6;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1OverworldAnimationRuntimeError {
    InvalidBase64,
    WrongTemplateLength(usize),
    WrongMapperTemplateLength(usize),
    MapperIramWordOutOfRange {
        offset: usize,
        value: u16,
    },
    MapperSuffix(crate::ExpandedExAnimationRuntimeError),
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
    AuxiliarySentinel {
        submap: usize,
    },
    AuxiliaryPointer {
        submap: usize,
        source: RomError,
    },
    AuxiliaryBeforeHeader {
        submap: usize,
        target: usize,
    },
    AuxiliaryHeader {
        submap: usize,
        target: usize,
        source: HeaderError,
    },
    AuxiliaryStart {
        submap: usize,
        expected: usize,
        actual: usize,
    },
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

fn mapper_rom_offset(mapper: Mapper, smw_lorom_offset: usize) -> usize {
    if mapper == Mapper::ExLoRom {
        0x40_0000 + smw_lorom_offset
    } else {
        smw_lorom_offset
    }
}

fn fixed_address(
    mapper: Mapper,
    offset: usize,
) -> Result<u32, SmwUsV1OverworldAnimationRuntimeError> {
    let mut address = pc_to_snes(mapper, mapper_rom_offset(mapper, offset))
        .map_err(|_| SmwUsV1OverworldAnimationRuntimeError::FixedAddress)?;
    if mapper == Mapper::LoRom {
        address &= 0x7f_ffff;
    }
    Ok(address)
}

fn write_fixed_long_for_mapper(
    bytes: &mut [u8],
    offset: usize,
    target: usize,
    mapper: Mapper,
) -> Result<(), SmwUsV1OverworldAnimationRuntimeError> {
    let address = fixed_address(mapper, target)?.to_le_bytes();
    bytes[offset..offset + 3].copy_from_slice(&address[..3]);
    Ok(())
}

fn write_fixed_low_for_mapper(
    bytes: &mut [u8],
    offset: usize,
    target: usize,
    mapper: Mapper,
) -> Result<(), SmwUsV1OverworldAnimationRuntimeError> {
    let address = fixed_address(mapper, target)?.to_le_bytes();
    bytes[offset..offset + 2].copy_from_slice(&address[..2]);
    Ok(())
}

fn pointer_encoding(mapper: Mapper) -> PatchFixupEncoding {
    if mapper == Mapper::LoRom {
        PatchFixupEncoding::Long24LowBank
    } else {
        PatchFixupEncoding::Long24
    }
}

fn relocate_mapper_iram(
    bytes: &mut [u8],
    mapper: Mapper,
) -> Result<(), SmwUsV1OverworldAnimationRuntimeError> {
    if bytes.len() != SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_LEN {
        return Err(SmwUsV1OverworldAnimationRuntimeError::WrongMapperTemplateLength(bytes.len()));
    }
    for offset in SA1_IRAM_WORD_OFFSETS {
        let value = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        if value > 0x1fff {
            return Err(
                SmwUsV1OverworldAnimationRuntimeError::MapperIramWordOutOfRange { offset, value },
            );
        }
    }
    let compact =
        u16::from_le_bytes([bytes[SA1_IRAM_BYTE_OFFSET], bytes[SA1_IRAM_BYTE_OFFSET + 1]]);
    let compact_limit = if mapper == Mapper::Sa1 { 0xff } else { 0x1fff };
    if compact > compact_limit {
        return Err(
            SmwUsV1OverworldAnimationRuntimeError::MapperIramWordOutOfRange {
                offset: SA1_IRAM_BYTE_OFFSET,
                value: compact,
            },
        );
    }
    for offset in SA1_IRAM_WORD_OFFSETS {
        let value = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) + 0x6000;
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    let relocated = compact
        + if mapper == Mapper::Sa1 {
            0x3000
        } else {
            0x6000
        };
    bytes[SA1_IRAM_BYTE_OFFSET..SA1_IRAM_BYTE_OFFSET + 2].copy_from_slice(&relocated.to_le_bytes());
    Ok(())
}

/// Builds the exact mapper-conditioned overworld-animation payload recovered from
/// `InstallExAnimationRomPatch` (`$004B2440`). SA-1 applies the eight word and one compact-byte
/// IRAM conversions before appending the shared `$20` compatibility suffix. ExLoROM applies the
/// same word conversion at all nine sites, while SA-1 uses its compact `$3000` form at `+$7D1`.
pub fn smw_us_v1_overworld_animation_runtime_payload_for_mapper(
    mapper: Mapper,
    mapper_runtime: bool,
) -> Result<PatchPayload, SmwUsV1OverworldAnimationRuntimeError> {
    runtime_payload_for_mapper(mapper, mapper_runtime)
}

fn runtime_payload_for_mapper(
    mapper: Mapper,
    mapper_runtime: bool,
) -> Result<PatchPayload, SmwUsV1OverworldAnimationRuntimeError> {
    let mut bytes = smw_us_v1_overworld_animation_runtime_template()?;

    // Ordinary LoROM selects mapping byte zero and skips every mapper-only IRAM conversion.
    bytes[0x58] = 0;
    bytes[0x61..0x63].copy_from_slice(&0_u16.to_le_bytes());
    if mapper_runtime {
        match mapper {
            Mapper::Sa1 | Mapper::ExLoRom => relocate_mapper_iram(&mut bytes, mapper)?,
            Mapper::LoRom => {}
        }
        bytes.extend(
            crate::expanded_exanimation_runtime_optional_suffix()
                .map_err(SmwUsV1OverworldAnimationRuntimeError::MapperSuffix)?,
        );
    }

    for (offset, target) in [
        (0xb1, FIXED_OW_TABLE),
        (0xb8, FIXED_OW_TABLE + 1),
        (0x132, FIXED_OW_TABLE),
        (0x139, FIXED_OW_TABLE + 1),
        (0x168, HOOK_A + 6),
        (0x551, HOOK_C_OPERAND + 6),
        (0x555, HOOK_C_OPERAND + 0x43),
    ] {
        write_fixed_long_for_mapper(&mut bytes, offset, target, mapper)?;
    }
    for (offset, target) in [
        (0x165, FIXED_ANIMATION_RAM),
        (0x4d6, FIXED_HELPER),
        (0x4dc, FIXED_HELPER + 1),
        (0x4e7, FIXED_HELPER + 0x10),
        (0x4ed, FIXED_HELPER + 0x11),
        (0x548, FIXED_ANIMATION_RAM),
    ] {
        write_fixed_low_for_mapper(&mut bytes, offset, target, mapper)?;
    }

    let long = pointer_encoding(mapper);
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
    if mapper_runtime {
        fixups.push(PatchFixup {
            offset: MAPPER_SUFFIX_POINTER_OFFSET,
            target_payload: 0,
            target_addend: SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_LEN,
            encoding: long,
        });
    }
    Ok(PatchPayload { bytes, fixups })
}

fn payload_write(
    mapper: Mapper,
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
        offset: mapper_rom_offset(mapper, offset),
        expected: expected.to_vec(),
        replacement,
        fixups: vec![PatchFixup {
            offset: operand_offset,
            target_payload,
            target_addend,
            encoding: pointer_encoding(mapper),
        }],
    }
}

/// Constructs Lunar Magic 3.63's exact pristine SMW-US LoROM runtime transaction.
pub fn smw_us_v1_overworld_animation_runtime_installation_plan()
-> Result<RelocatablePatchPlan, SmwUsV1OverworldAnimationRuntimeError> {
    smw_us_v1_overworld_animation_runtime_installation_plan_for_mapper(
        Mapper::LoRom,
        AllocationPolicy::lorom(
            SMW_US_V1_OVERWORLD_ANIMATION_SEARCH_START..SMW_US_V1_OVERWORLD_ANIMATION_SEARCH_END,
        ),
        false,
    )
}

/// Builds the complete descriptor-routed overworld-animation transaction for the selected mapper.
pub fn smw_us_v1_overworld_animation_runtime_installation_plan_for_mapper(
    mapper: Mapper,
    allocation: AllocationPolicy,
    mapper_runtime: bool,
) -> Result<RelocatablePatchPlan, SmwUsV1OverworldAnimationRuntimeError> {
    let mut auxiliary = vec![0; SMW_US_V1_OVERWORLD_ANIMATION_AUXILIARY_LEN];
    for offset in (0..SMW_US_V1_OVERWORLD_ANIMATION_AUXILIARY_LEN).step_by(3) {
        auxiliary[offset] = 0xff;
    }
    let mut writes = vec![
        payload_write(mapper, HOOK_A, &[0xc2, 0x30, 0x64, 0x03], Some(0x22), 0, 0),
        payload_write(
            mapper,
            HOOK_B,
            &[0xc2, 0x10, 0xa9, 0x80],
            Some(0x22),
            0,
            0x1f0,
        ),
        payload_write(mapper, HOOK_C_OPERAND, &[0xa5, 0x13, 0x29], None, 0, 0x500),
    ];
    writes.extend(MODE_BYTES.into_iter().map(|offset| PatchWrite {
        offset: mapper_rom_offset(mapper, offset),
        expected: vec![0x13],
        replacement: vec![0x14],
        fixups: Vec::new(),
    }));
    Ok(RelocatablePatchPlan {
        description: format!("install SMW US v1 {mapper:?} overworld animation runtime"),
        mapper,
        allocation,
        checksum_field: mapper_rom_offset(mapper, SMW_US_V1_CHECKSUM_FIELD),
        expansion_fill: 0xff,
        payloads: vec![
            runtime_payload_for_mapper(mapper, mapper_runtime)?,
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
    /// RATS-owned compact ExAnimation payload selected by each of Lunar Magic's seven
    /// overworld/submap pointer slots. `None` is encoded by the exact `FF 00 00` sentinel.
    pub submap_animations: [Option<RatsBlock>; 7],
}

fn read_pointer_for_mapper(
    bytes: &[u8],
    offset: usize,
    mapper: Mapper,
) -> Result<usize, SmwUsV1OverworldAnimationRuntimeError> {
    let operand = bytes
        .get(offset..offset + 3)
        .ok_or(SmwUsV1OverworldAnimationRuntimeError::FixedRange { offset })?;
    snes_to_pc(
        mapper,
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
    mapper: Mapper,
) -> Result<(), SmwUsV1OverworldAnimationRuntimeError> {
    for fixup in fixups {
        let target = blocks[fixup.target_payload].payload.start + fixup.target_addend;
        let mut encoded = pc_to_snes(mapper, target)
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

/// Authenticates every fixed write, allocation owner, relocated runtime byte, and auxiliary
/// submap-animation pointer. The seven option bytes and seven auxiliary pointer slots are
/// deliberately mutable. Each nonempty auxiliary slot must resolve to the payload start of a
/// valid RATS-owned compact-animation allocation; empty slots must use Lunar Magic's exact
/// `FF 00 00` sentinel.
pub fn detect_smw_us_v1_overworld_animation_runtime(
    bytes: &[u8],
) -> Result<Option<SmwUsV1OverworldAnimationRuntime>, SmwUsV1OverworldAnimationRuntimeError> {
    detect_smw_us_v1_overworld_animation_runtime_for_mapper(bytes, Mapper::LoRom, false)
}

/// Authenticates the complete descriptor-routed runtime family for one explicit mapper variant.
pub fn detect_smw_us_v1_overworld_animation_runtime_for_mapper(
    bytes: &[u8],
    mapper: Mapper,
    mapper_runtime: bool,
) -> Result<Option<SmwUsV1OverworldAnimationRuntime>, SmwUsV1OverworldAnimationRuntimeError> {
    let hook_a = mapper_rom_offset(mapper, HOOK_A);
    let hook_b = mapper_rom_offset(mapper, HOOK_B);
    let hook_c = mapper_rom_offset(mapper, HOOK_C_OPERAND);
    let mode_bytes = MODE_BYTES.map(|offset| mapper_rom_offset(mapper, offset));
    let marker = *bytes
        .get(hook_b)
        .ok_or(SmwUsV1OverworldAnimationRuntimeError::FixedRange { offset: HOOK_B })?;
    if marker != 0x22 {
        if bytes.get(hook_a..hook_a + 4) == Some(&[0xc2, 0x30, 0x64, 0x03])
            && bytes.get(hook_b..hook_b + 4) == Some(&[0xc2, 0x10, 0xa9, 0x80])
            && bytes.get(hook_c..hook_c + 3) == Some(&[0xa5, 0x13, 0x29])
            && mode_bytes
                .iter()
                .all(|offset| bytes.get(*offset) == Some(&0x13))
        {
            return Ok(None);
        }
        return Err(SmwUsV1OverworldAnimationRuntimeError::FixedMismatch { offset: hook_b });
    }
    if bytes.get(hook_a) != Some(&0x22) {
        return Err(SmwUsV1OverworldAnimationRuntimeError::FixedMismatch { offset: hook_a });
    }
    if mode_bytes
        .iter()
        .any(|offset| bytes.get(*offset) != Some(&0x14))
    {
        return Err(SmwUsV1OverworldAnimationRuntimeError::FixedMismatch {
            offset: mode_bytes[0],
        });
    }

    let runtime_target = read_pointer_for_mapper(bytes, hook_a + 1, mapper)?;
    let runtime = owned_block(
        bytes,
        runtime_target,
        if mapper_runtime {
            SMW_US_V1_OVERWORLD_ANIMATION_MAPPER_RUNTIME_LEN
        } else {
            SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_LEN
        },
    )?;
    let hook_b_target = read_pointer_for_mapper(bytes, hook_b + 1, mapper)?;
    let hook_c_target = read_pointer_for_mapper(bytes, hook_c, mapper)?;
    if hook_b_target != runtime.payload.start + 0x1f0
        || hook_c_target != runtime.payload.start + 0x500
    {
        return Err(SmwUsV1OverworldAnimationRuntimeError::FixedMismatch { offset: hook_b });
    }
    let auxiliary_target = read_pointer_for_mapper(bytes, runtime.payload.start + 0xe1, mapper)?;
    let auxiliary = owned_block(
        bytes,
        auxiliary_target,
        SMW_US_V1_OVERWORLD_ANIMATION_AUXILIARY_LEN,
    )?;
    let options_target = read_pointer_for_mapper(bytes, runtime.payload.start + 0x4a, mapper)?;
    let options = owned_block(
        bytes,
        options_target,
        SMW_US_V1_OVERWORLD_ANIMATION_OPTIONS_LEN,
    )?;
    let blocks = [runtime.clone(), auxiliary.clone(), options.clone()];
    let payload = runtime_payload_for_mapper(mapper, mapper_runtime)?;
    let mut expected_runtime = payload.bytes;
    apply_materialized_fixups(&mut expected_runtime, &payload.fixups, &blocks, mapper)?;
    if bytes.get(runtime.payload.clone()) != Some(expected_runtime.as_slice()) {
        return Err(SmwUsV1OverworldAnimationRuntimeError::RuntimeMismatch);
    }
    let auxiliary_bytes = bytes.get(auxiliary.payload.clone()).ok_or(
        SmwUsV1OverworldAnimationRuntimeError::FixedRange {
            offset: auxiliary.payload.start,
        },
    )?;
    let mut submap_animations: [Option<RatsBlock>; 7] = std::array::from_fn(|_| None);
    for (submap, pointer) in auxiliary_bytes.chunks_exact(3).enumerate() {
        if pointer == [0xff, 0x00, 0x00] {
            continue;
        }
        if pointer[2] == 0 {
            return Err(SmwUsV1OverworldAnimationRuntimeError::AuxiliarySentinel { submap });
        }
        let address = u32::from_le_bytes([pointer[0], pointer[1], pointer[2], 0]);
        let target = snes_to_pc(mapper, address).map_err(|source| {
            SmwUsV1OverworldAnimationRuntimeError::AuxiliaryPointer { submap, source }
        })?;
        let header = target.checked_sub(HEADER_LEN).ok_or(
            SmwUsV1OverworldAnimationRuntimeError::AuxiliaryBeforeHeader { submap, target },
        )?;
        let block = parse_at(bytes, header).map_err(|source| {
            SmwUsV1OverworldAnimationRuntimeError::AuxiliaryHeader {
                submap,
                target,
                source,
            }
        })?;
        if block.payload.start != target {
            return Err(SmwUsV1OverworldAnimationRuntimeError::AuxiliaryStart {
                submap,
                expected: target,
                actual: block.payload.start,
            });
        }
        submap_animations[submap] = Some(block);
    }
    Ok(Some(SmwUsV1OverworldAnimationRuntime {
        runtime,
        auxiliary,
        options,
        submap_animations,
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
    fn sa1_mapper_payload_applies_every_recovered_iram_conversion_and_suffix_pointer() {
        let ordinary = smw_us_v1_overworld_animation_runtime_template().unwrap();
        let payload =
            smw_us_v1_overworld_animation_runtime_payload_for_mapper(Mapper::Sa1, true).unwrap();
        assert_eq!(
            payload.bytes.len(),
            SMW_US_V1_OVERWORLD_ANIMATION_MAPPER_RUNTIME_LEN
        );
        for offset in SA1_IRAM_WORD_OFFSETS {
            let before = u16::from_le_bytes([ordinary[offset], ordinary[offset + 1]]);
            let after = u16::from_le_bytes([payload.bytes[offset], payload.bytes[offset + 1]]);
            assert_eq!(after, before + 0x6000, "IRAM word at +{offset:#x}");
        }
        assert_eq!(
            u16::from_le_bytes([
                payload.bytes[SA1_IRAM_BYTE_OFFSET],
                payload.bytes[SA1_IRAM_BYTE_OFFSET + 1],
            ]),
            u16::from(ordinary[SA1_IRAM_BYTE_OFFSET]) + 0x3000
        );
        assert_eq!(
            &payload.bytes[SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_LEN..],
            crate::expanded_exanimation_runtime_optional_suffix()
                .unwrap()
                .as_slice()
        );
        assert!(payload.fixups.iter().any(|fixup| {
            fixup.offset == MAPPER_SUFFIX_POINTER_OFFSET
                && fixup.target_payload == 0
                && fixup.target_addend == SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_LEN
                && fixup.encoding == PatchFixupEncoding::Long24
        }));
    }

    #[test]
    fn exlorom_mapper_payload_uses_nine_word_conversions_and_canonical_pointers() {
        let ordinary = smw_us_v1_overworld_animation_runtime_template().unwrap();
        let payload =
            smw_us_v1_overworld_animation_runtime_payload_for_mapper(Mapper::ExLoRom, true)
                .unwrap();
        assert_eq!(
            payload.bytes.len(),
            SMW_US_V1_OVERWORLD_ANIMATION_MAPPER_RUNTIME_LEN
        );
        for offset in SA1_IRAM_WORD_OFFSETS
            .into_iter()
            .chain([SA1_IRAM_BYTE_OFFSET])
        {
            let before = u16::from_le_bytes([ordinary[offset], ordinary[offset + 1]]);
            let after = u16::from_le_bytes([payload.bytes[offset], payload.bytes[offset + 1]]);
            assert_eq!(after, before + 0x6000, "IRAM word at +{offset:#x}");
        }
        assert!(
            payload
                .fixups
                .iter()
                .filter(|fixup| { matches!(fixup.encoding, PatchFixupEncoding::Long24) })
                .count()
                >= 1
        );
        assert!(
            !payload
                .fixups
                .iter()
                .any(|fixup| { matches!(fixup.encoding, PatchFixupEncoding::Long24LowBank) })
        );
    }

    #[test]
    fn mapper_payload_keeps_lorom_ordinary() {
        assert_eq!(
            smw_us_v1_overworld_animation_runtime_payload_for_mapper(Mapper::LoRom, false).unwrap(),
            runtime_payload_for_mapper(Mapper::LoRom, false).unwrap()
        );
    }

    #[test]
    fn mapper_runtime_plans_install_detect_checksum_corruption_and_exact_undo() {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        for (mapper, copier_header) in
            [Mapper::ExLoRom, Mapper::Sa1]
                .into_iter()
                .flat_map(|mapper| {
                    [lm_rom::CopierHeader::Absent, lm_rom::CopierHeader::Present]
                        .map(|copier_header| (mapper, copier_header))
                })
        {
            let logical = if mapper == Mapper::ExLoRom {
                let mut converted =
                    Project::open_supported(RomImage::from_bytes(pristine.clone()).unwrap())
                        .unwrap();
                converted.convert_to_64_mbit_exlorom().unwrap();
                converted.rom.logical_bytes().to_vec()
            } else {
                let mut bytes = RomImage::from_bytes(pristine.clone())
                    .unwrap()
                    .logical_bytes()
                    .to_vec();
                bytes.resize(0x50_0000, 0xff);
                bytes
            };
            let mut image = RomImage::from_bytes(logical).unwrap();
            image.set_copier_header(copier_header, 0xa5);
            let original = image.as_file_bytes().to_vec();
            let original_header = image.copier_header_bytes().map(<[u8]>::to_vec);
            let mut project = Project::new(image);
            let allocation = AllocationPolicy {
                search: if mapper == Mapper::ExLoRom {
                    0x10_0000..0x40_0000
                } else {
                    0x40_0000..0x41_0000
                },
                bank_size: Some(0x8000),
                fill_bytes: vec![0x00, 0xff],
                protected: Vec::new(),
            };
            let plan = smw_us_v1_overworld_animation_runtime_installation_plan_for_mapper(
                mapper, allocation, true,
            )
            .unwrap();
            let result = project.install_relocatable_patch(&plan).unwrap();
            assert_eq!(
                result.blocks[0].payload.len(),
                SMW_US_V1_OVERWORLD_ANIMATION_MAPPER_RUNTIME_LEN
            );
            let detected = detect_smw_us_v1_overworld_animation_runtime_for_mapper(
                project.rom.logical_bytes(),
                mapper,
                true,
            )
            .unwrap()
            .unwrap();
            assert_eq!(detected.runtime, result.blocks[0]);
            assert_eq!(detected.auxiliary, result.blocks[1]);
            assert_eq!(detected.options, result.blocks[2]);
            assert_eq!(
                project.rom.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(
                lm_rom::SnesChecksum::decode(
                    project.rom.logical_bytes(),
                    mapper_rom_offset(mapper, SMW_US_V1_CHECKSUM_FIELD),
                )
                .unwrap()
                .is_complementary()
            );
            let layout = crate::smw_us_v1_overworld_animation_options_layout_for_mapper(mapper);
            let before_options = project
                .load_installed_overworld_animation_options(layout)
                .unwrap();
            assert!(before_options.runtime_installed);
            assert_eq!(before_options.feature_bytes, [0; 7]);
            let changed_features = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40];
            assert!(
                project
                    .save_installed_overworld_animation_options(
                        changed_features,
                        0xd7,
                        layout,
                        mapper_rom_offset(mapper, SMW_US_V1_CHECKSUM_FIELD),
                    )
                    .unwrap()
            );
            let changed = project
                .load_installed_overworld_animation_options(layout)
                .unwrap();
            assert_eq!(changed.feature_bytes, changed_features);
            assert_eq!(changed.lightning_disable_mask, 0xd7);
            assert!(project.undo().unwrap());
            assert_eq!(
                project
                    .load_installed_overworld_animation_options(layout)
                    .unwrap(),
                before_options
            );
            let installed = project.rom.logical_bytes().to_vec();
            for offset in [
                result.blocks[0].payload.start,
                result.blocks[0].payload.start + MAPPER_SUFFIX_POINTER_OFFSET,
                mapper_rom_offset(mapper, HOOK_A),
            ] {
                let mut corrupt = installed.clone();
                corrupt[offset] ^= 1;
                assert!(
                    detect_smw_us_v1_overworld_animation_runtime_for_mapper(
                        &corrupt, mapper, true,
                    )
                    .is_err(),
                    "{mapper:?} corruption at {offset:#x} was accepted"
                );
            }
            assert!(project.install_relocatable_patch(&plan).is_err());
            assert_eq!(project.rom.logical_bytes(), installed);
            assert!(project.undo().unwrap());
            assert_eq!(project.rom.as_file_bytes(), original);
            assert!(project.redo().unwrap());
            assert_eq!(project.rom.logical_bytes(), installed);
            assert_eq!(
                project.rom.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
        }
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
    fn authentic_lunar_magic_363_runtime_matches_every_owned_and_fixed_byte() {
        const RUNTIME_HEADER: usize = 0x0008_bc66;
        const AUXILIARY_HEADER: usize = 0x0008_c88e;
        const OPTIONS_HEADER: usize = 0x0008_c8ab;
        const ORACLE_END: usize = 0x0008_c8ba;
        const SUBMAP_HEADER: usize = ORACLE_END;
        const SUBMAP_PAYLOAD: usize = SUBMAP_HEADER + HEADER_LEN;

        let mut oracle = decode_base64(include_str!(
            "assets/overworld_animation_runtime_lm363_oracle.b64"
        ))
        .unwrap();
        assert_eq!(oracle.len(), ORACLE_END - RUNTIME_HEADER);
        assert_eq!(
            format!("{:x}", Sha256::digest(&oracle)),
            "04fb09d57cb18d8d6f6a07cc00c5f15767075a8764182cfb329c8253eb342b26"
        );
        let submap = decode_base64(include_str!(
            "assets/overworld_animation_submap_lm363_oracle.b64"
        ))
        .unwrap();
        assert_eq!(submap.len(), HEADER_LEN + 0x11);
        assert_eq!(
            format!("{:x}", Sha256::digest(&submap)),
            "e6d3ad990be851cbb03cb9d1656eb05bfd0fa16dda71da82163ed3dfc50b980b"
        );

        let mut project = source();
        project
            .rom
            .write(
                SMW_US_V1_OVERWORLD_ANIMATION_SEARCH_START,
                &vec![0x5a; RUNTIME_HEADER - SMW_US_V1_OVERWORLD_ANIMATION_SEARCH_START],
            )
            .unwrap();
        let result = project
            .install_relocatable_patch(
                &smw_us_v1_overworld_animation_runtime_installation_plan().unwrap(),
            )
            .unwrap();
        assert_eq!(result.blocks[0].header_offset, RUNTIME_HEADER);
        assert_eq!(result.blocks[1].header_offset, AUXILIARY_HEADER);
        assert_eq!(result.blocks[2].header_offset, OPTIONS_HEADER);

        // The capture was taken after editing the first submap, so normalize its one mutable
        // auxiliary pointer to the pristine installer sentinel before comparing every byte of
        // all three core owners.
        let first_pointer = AUXILIARY_HEADER + HEADER_LEN - RUNTIME_HEADER;
        assert_eq!(
            &oracle[first_pointer..first_pointer + 3],
            &[0xc2, 0xc8, 0x11]
        );
        oracle[first_pointer..first_pointer + 3].copy_from_slice(&[0xff, 0x00, 0x00]);
        let installed = project.rom.logical_bytes();
        assert_eq!(&installed[RUNTIME_HEADER..ORACLE_END], oracle.as_slice());
        assert_eq!(&installed[HOOK_A..HOOK_A + 4], &[0x22, 0x6e, 0xbc, 0x11]);
        assert_eq!(&installed[HOOK_B..HOOK_B + 4], &[0x22, 0x5e, 0xbe, 0x11]);
        assert_eq!(
            &installed[HOOK_C_OPERAND..HOOK_C_OPERAND + 3],
            &[0x6e, 0xc1, 0x11]
        );
        assert!(
            MODE_BYTES
                .iter()
                .all(|offset| installed.get(*offset) == Some(&0x14))
        );
        let detected = detect_smw_us_v1_overworld_animation_runtime(installed)
            .unwrap()
            .unwrap();
        assert_eq!(detected.runtime, result.blocks[0]);
        assert_eq!(detected.auxiliary, result.blocks[1]);
        assert_eq!(detected.options, result.blocks[2]);
        assert_eq!(detected.submap_animations, std::array::from_fn(|_| None));

        // Reapply the exact authentic edit: the auxiliary pointer selects the adjacent compact
        // ExAnimation RATS payload. Detection must accept and expose that mutable owner chain.
        project.rom.write(SUBMAP_HEADER, &submap).unwrap();
        let pointer =
            (pc_to_snes(Mapper::LoRom, SUBMAP_PAYLOAD).unwrap() & 0x7f_ffff).to_le_bytes();
        project
            .rom
            .write(result.blocks[1].payload.start, &pointer[..3])
            .unwrap();
        assert_eq!(&pointer[..3], &[0xc2, 0xc8, 0x11]);
        let detected = detect_smw_us_v1_overworld_animation_runtime(project.rom.logical_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(
            detected.submap_animations[0].as_ref().unwrap(),
            &parse_at(project.rom.logical_bytes(), SUBMAP_HEADER).unwrap()
        );
        assert!(detected.submap_animations[1..].iter().all(Option::is_none));
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
            Err(SmwUsV1OverworldAnimationRuntimeError::AuxiliarySentinel { submap: 0 })
        ));
        let unowned_target = 0x0009_0000;
        let pointer =
            (pc_to_snes(Mapper::LoRom, unowned_target).unwrap() & 0x7f_ffff).to_le_bytes();
        project
            .rom
            .write(result.blocks[1].payload.start, &pointer[..3])
            .unwrap();
        match detect_smw_us_v1_overworld_animation_runtime(project.rom.logical_bytes()) {
            Err(SmwUsV1OverworldAnimationRuntimeError::AuxiliaryHeader {
                submap: 0,
                target,
                ..
            }) if target == unowned_target => {}
            other => panic!("unexpected unowned auxiliary-pointer result: {other:?}"),
        }
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
