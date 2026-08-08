//! Transactional publication of the recovered expanded-ExAnimation fresh runtime family.

use crate::{
    ExpandedExAnimationRuntimeError, ExpandedExAnimationRuntimeOptionalRelocations,
    ExpandedExAnimationRuntimeRelocations, OPTIONAL_MAPPING_HELPER_SNES_ADDRESS,
    SMW_US_V1_CHECKSUM_FIELD, empty_expanded_exanimation_pointer_table,
    exanimation_runtime::{
        IRAM_WORD_OFFSETS, LOCAL_WORD_TABLE_ENTRIES, LOCAL_WORD_TABLE_OFFSET, MAPPING_BYTE_OFFSETS,
        SNES_POINTER_OFFSETS, TEMPLATE_LOCAL_WORD_BASE,
    },
    expanded_exanimation_runtime_template, relocate_expanded_exanimation_runtime,
    relocate_expanded_exanimation_runtime_with_optional_suffix,
};
use lm_project::{PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchPlan};
use lm_rats::{AllocationPolicy, HEADER_LEN, HeaderError, RatsBlock, parse_at};
use lm_rom::{
    LunarMagicRomMetadata, LunarMagicRomMetadataError, Mapper, RomError, pc_to_snes, snes_to_pc,
};
use sha2::{Digest, Sha256};
use std::fmt;
use std::ops::Range;

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
const GENERATION_101_SHARED_PALETTE_RUNTIME_A: [u8; 0x0d] = [
    0xa5, 0x0e, 0x1a, 0x85, 0xfe, 0x3a, 0x0a, 0xa8, 0x6b, 0xff, 0xff, 0xff, 0xff,
];
const GENERATION_101_SHARED_PALETTE_RUNTIME_B: [u8; 0x10] = [
    0x64, 0xfe, 0x64, 0xff, 0x84, 0x76, 0x84, 0x89, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

const GENERATION_101_CORE_DIGEST: &str =
    "10dbd77a94ddb479cf4ac83360c824ccf97f58dbbc55f61770465369e6cc90e0";
const RUNTIME_IRAM_ADDENDS: [u16; 12] = [
    0x05af, 0x05af, 0x0b4a, 0x05cf, 0x0b82, 0x0b82, 0x0b82, 0x0b82, 0x0b82, 0x0b82, 0x0b82, 0x0b82,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwUsV1ExpandedExAnimationRuntimeGeneration {
    Absent,
    LegacyPointerHooks,
    LegacyGlobalTable,
    Current,
}

/// Authenticated storage resolved by Lunar Magic's legacy-global-table generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1LegacyGlobalExAnimationRuntime {
    pub runtime: RatsBlock,
    pub pointer_table: Range<usize>,
    pub auxiliary_table: Range<usize>,
}

const MAPPER_COMPATIBILITY_HOOK_WORD_OFFSET: usize = 0x2d2b;
const EXLOROM_CONFIGURATION_PRESENT_BIT: u32 = 1 << 1;
const SA1_CONFIGURATION_PRESENT_BIT: u32 = 1 << 2;
const EXLOROM_COMPATIBILITY_RUNTIME_BIT: u32 = 1 << 17;
const SA1_COMPATIBILITY_RUNTIME_BIT: u32 = 1 << 18;

fn mapper_rom_offset(mapper: Mapper, smw_lorom_offset: usize) -> usize {
    if mapper == Mapper::ExLoRom {
        0x40_0000 + smw_lorom_offset
    } else {
        smw_lorom_offset
    }
}

#[derive(Debug)]
pub enum SmwUsV1ExpandedExAnimationRuntimeDetectError {
    HookRange,
    HookAddress(RomError),
    RuntimeBeforeHeader(usize),
    RuntimeHeader(HeaderError),
    RuntimeOwnership {
        expected: usize,
        actual: usize,
    },
    RuntimeLength(usize),
    PointerAddress(RomError),
    PointerBeforeHeader(usize),
    PointerHeader(HeaderError),
    PointerOwnership {
        expected: usize,
        actual: usize,
    },
    PointerLength(usize),
    Relocation(ExpandedExAnimationRuntimeError),
    RuntimeMismatch,
    FixedRangeMismatch {
        offset: usize,
    },
    HelperAddress {
        offset: usize,
        source: RomError,
    },
    HelperBeforeHeader {
        offset: usize,
        target: usize,
    },
    HelperHeader {
        offset: usize,
        source: HeaderError,
    },
    HelperOwnership {
        offset: usize,
        expected: usize,
        actual: usize,
    },
    HelperLength {
        offset: usize,
        expected: usize,
        actual: usize,
    },
    HelperPayloadMismatch {
        offset: usize,
    },
    LegacyGenerationSignalRange {
        offset: usize,
    },
    LegacyGenerationSignalMismatch {
        offset: usize,
    },
    LegacyRuntimeAddress(RomError),
    LegacyRuntimeBeforeHeader(usize),
    LegacyRuntimeHeader(HeaderError),
    LegacyRuntimeOwnership {
        expected: usize,
        actual: usize,
    },
    LegacyRuntimeTooShort {
        required: usize,
        actual: usize,
    },
    LegacyPointerAddress(RomError),
    LegacyPointerRange {
        offset: usize,
        len: usize,
    },
    LegacyAuxiliaryAddress(RomError),
    LegacyAuxiliaryRange {
        offset: usize,
        len: usize,
    },
    LegacyStorageOverlap {
        first: usize,
        second: usize,
    },
    MetadataPartialInstallation,
    Metadata(LunarMagicRomMetadataError),
    MapperCompatibilityHookRange,
}

/// Builds the relocatable core payload for either the ordinary `$C30` or mapper-compatible `$C50`
/// runtime form.
///
/// The returned payload retains allocation-dependent fixups. For ExLoROM and SA-1 those pointers
/// use their canonical mapper addresses; ordinary LoROM retains Lunar Magic's equivalent low-bank
/// mirror. The mapper form applies the complete 37+3 IRAM pass before adding the suffix, fixes the
/// suffix self-pointer at `+$78A`, and writes the fixed `$7FC020` helper pointer at `+$792`.
pub fn smw_us_v1_expanded_exanimation_runtime_payload(
    mapper: Mapper,
    mapper_runtime: bool,
) -> Result<PatchPayload, ExpandedExAnimationRuntimeError> {
    let mut runtime = expanded_exanimation_runtime_template()?;
    for offset in MAPPING_BYTE_OFFSETS {
        runtime[offset] = 0;
    }
    for (offset, value) in IRAM_WORD_OFFSETS.into_iter().zip(SMW_US_V1_IRAM_WORDS) {
        runtime[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    if mapper_runtime {
        crate::relocate_expanded_exanimation_mapper_iram(&mut runtime)?;
        runtime.extend(crate::expanded_exanimation_runtime_optional_suffix()?);
        runtime[crate::OPTIONAL_MAPPING_HELPER_POINTER_OFFSET
            ..crate::OPTIONAL_MAPPING_HELPER_POINTER_OFFSET + 3]
            .copy_from_slice(&crate::OPTIONAL_MAPPING_HELPER_SNES_ADDRESS.to_le_bytes()[..3]);
    }

    let pointer_encoding = if mapper == Mapper::LoRom {
        PatchFixupEncoding::Long24LowBank
    } else {
        PatchFixupEncoding::Long24
    };
    let mut fixups = CORE_POINTER_FIXUPS
        .into_iter()
        .map(|(offset, target_payload, target_addend)| PatchFixup {
            offset,
            target_payload,
            target_addend,
            encoding: pointer_encoding,
        })
        .collect::<Vec<_>>();
    for (offset, addend) in IRAM_WORD_OFFSETS.into_iter().zip(RUNTIME_IRAM_ADDENDS) {
        fixups.push(PatchFixup {
            offset,
            target_payload: 0,
            target_addend: usize::from(addend),
            encoding: PatchFixupEncoding::Low16,
        });
    }
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
    if mapper_runtime {
        fixups.push(PatchFixup {
            offset: crate::OPTIONAL_SUFFIX_POINTER_OFFSET,
            target_payload: 0,
            target_addend: crate::EXPANDED_EXANIMATION_RUNTIME_CORE_LEN,
            encoding: PatchFixupEncoding::Long24,
        });
    }
    Ok(PatchPayload {
        bytes: runtime,
        fixups,
    })
}

impl fmt::Display for SmwUsV1ExpandedExAnimationRuntimeDetectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "expanded ExAnimation runtime detection failed: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1ExpandedExAnimationRuntimeDetectError {}

/// Reproduces the optional `$C50` runtime predicate initialized while Lunar Magic opens a ROM.
///
/// ExLoROM uses feature bit 17 when declaration bit 1 is present; SA-1 uses feature bit 18 when
/// declaration bit 2 is present. Older metadata omits the declaration, so both mapper families
/// fall back to the descriptor-selected 16-bit hook word at logical `$002D2B`, where values above
/// `$1FFF` identify the compatibility mapping. Ordinary LoROM never selects the suffix.
pub fn smw_us_v1_expanded_exanimation_uses_mapper_runtime(
    bytes: &[u8],
    mapper: Mapper,
) -> Result<bool, SmwUsV1ExpandedExAnimationRuntimeDetectError> {
    if mapper == Mapper::LoRom {
        return Ok(false);
    }
    let metadata_base = if mapper == Mapper::ExLoRom {
        0x40_0000
    } else {
        0
    };
    let attribution_offset = metadata_base + crate::SMW_US_V1_LM_ATTRIBUTION_OFFSET;
    let vram_offset = metadata_base + crate::SMW_US_V1_LM_VRAM_VERSION_OFFSET;
    let feature_offset = metadata_base + crate::SMW_US_V1_LM_FEATURE_RECORD_OFFSET;
    let attribution = bytes
        .get(attribution_offset..attribution_offset + LunarMagicRomMetadata::ATTRIBUTION_LEN)
        .ok_or(SmwUsV1ExpandedExAnimationRuntimeDetectError::MetadataPartialInstallation)?;
    let vram = *bytes
        .get(vram_offset)
        .ok_or(SmwUsV1ExpandedExAnimationRuntimeDetectError::MetadataPartialInstallation)?;
    let feature = bytes
        .get(feature_offset..feature_offset + LunarMagicRomMetadata::FEATURE_LEN)
        .ok_or(SmwUsV1ExpandedExAnimationRuntimeDetectError::MetadataPartialInstallation)?;
    let attribution_absent = attribution.iter().all(|byte| *byte == 0xff);
    let record_absent = vram == 0xff && feature.iter().all(|byte| *byte == 0xff);
    let metadata = if attribution_absent && record_absent {
        None
    } else if attribution_absent || record_absent {
        return Err(SmwUsV1ExpandedExAnimationRuntimeDetectError::MetadataPartialInstallation);
    } else {
        Some(
            LunarMagicRomMetadata::from_parts(attribution, vram, feature)
                .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::Metadata)?,
        )
    };
    let (declaration, enabled) = match mapper {
        Mapper::ExLoRom => (
            EXLOROM_CONFIGURATION_PRESENT_BIT,
            EXLOROM_COMPATIBILITY_RUNTIME_BIT,
        ),
        Mapper::Sa1 => (SA1_CONFIGURATION_PRESENT_BIT, SA1_COMPATIBILITY_RUNTIME_BIT),
        Mapper::LoRom => unreachable!("ordinary LoROM returned above"),
    };
    if let Some(bits) = metadata.as_ref().map(LunarMagicRomMetadata::feature_bits)
        && bits & declaration != 0
    {
        return Ok(bits & enabled != 0);
    }
    let hook_offset = mapper_rom_offset(mapper, MAPPER_COMPATIBILITY_HOOK_WORD_OFFSET);
    let hook = bytes
        .get(hook_offset..hook_offset + 2)
        .ok_or(SmwUsV1ExpandedExAnimationRuntimeDetectError::MapperCompatibilityHookRange)?;
    Ok(u16::from_le_bytes([hook[0], hook[1]]) > 0x1fff)
}

/// Authenticates the complete ordinary-LoROM current runtime family.
///
/// Mutable feature/global-pointer operands are retained, while every code byte, fixed relocation,
/// RATS owner, pointer-table relationship, helper allocation, sentinel, and shared-palette hook is
/// checked against the recovered Lunar Magic 3.63 family.
pub fn detect_smw_us_v1_current_expanded_exanimation_runtime(
    bytes: &[u8],
) -> Result<RatsBlock, SmwUsV1ExpandedExAnimationRuntimeDetectError> {
    detect_smw_us_v1_current_expanded_exanimation_runtime_for_mapper(bytes, Mapper::LoRom, false)
}

fn generation_101_core_matches(
    installed: &[u8],
    expected_relocations: &[u8],
    runtime_offset: usize,
) -> bool {
    if installed.len() != 0xc30 || expected_relocations.len() != installed.len() {
        return false;
    }
    for (offset, len) in SNES_POINTER_OFFSETS
        .into_iter()
        .map(|offset| (offset, 3))
        .chain(std::iter::once((
            LOCAL_WORD_TABLE_OFFSET,
            LOCAL_WORD_TABLE_ENTRIES * 2,
        )))
    {
        if installed[offset..offset + len] != expected_relocations[offset..offset + len] {
            return false;
        }
    }
    let Ok(runtime_base) = pc_to_snes(Mapper::LoRom, runtime_offset) else {
        return false;
    };
    for (offset, addend) in IRAM_WORD_OFFSETS.into_iter().zip(RUNTIME_IRAM_ADDENDS) {
        let expected = (runtime_base as u16).wrapping_add(addend);
        if installed[offset..offset + 2] != expected.to_le_bytes() {
            return false;
        }
    }

    generation_101_core_digest_matches(installed)
}

fn generation_101_core_digest_matches(installed: &[u8]) -> bool {
    if installed.len() != 0xc30 {
        return false;
    }
    let mut canonical = installed.to_vec();
    for offset in MAPPING_BYTE_OFFSETS {
        canonical[offset] = 0;
    }
    canonical[0x46..0x49].fill(0);
    canonical[0x65] = 0;
    for offset in SNES_POINTER_OFFSETS {
        canonical[offset..offset + 3].fill(0);
    }
    for offset in IRAM_WORD_OFFSETS {
        canonical[offset..offset + 2].fill(0);
    }
    canonical[LOCAL_WORD_TABLE_OFFSET..LOCAL_WORD_TABLE_OFFSET + LOCAL_WORD_TABLE_ENTRIES * 2]
        .fill(0);
    format!("{:x}", Sha256::digest(canonical)) == GENERATION_101_CORE_DIGEST
}

/// Authenticates the complete current runtime family for a concrete mapper/runtime form.
pub fn detect_smw_us_v1_current_expanded_exanimation_runtime_for_mapper(
    bytes: &[u8],
    mapper: Mapper,
    mapper_runtime: bool,
) -> Result<RatsBlock, SmwUsV1ExpandedExAnimationRuntimeDetectError> {
    let core_hook_offset = mapper_rom_offset(mapper, 0x283ad);
    let hook = bytes
        .get(core_hook_offset..core_hook_offset + 5)
        .ok_or(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookRange)?;
    if hook[0] != 0x22 || hook[4] != 0xea {
        return Err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookRange);
    }
    let runtime_offset = mapped_operand(mapper, &hook[1..4])
        .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?;
    let runtime = owned_block(bytes, runtime_offset).map_err(|error| match error {
        OwnedBlockError::BeforeHeader => {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::RuntimeBeforeHeader(runtime_offset)
        }
        OwnedBlockError::Header(source) => {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::RuntimeHeader(source)
        }
        OwnedBlockError::Ownership(actual) => {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::RuntimeOwnership {
                expected: runtime_offset,
                actual,
            }
        }
    })?;
    let required_runtime_len = if mapper_runtime { 0xc50 } else { 0xc30 };
    if runtime.payload.len() != required_runtime_len {
        return Err(SmwUsV1ExpandedExAnimationRuntimeDetectError::RuntimeLength(
            runtime.payload.len(),
        ));
    }
    let installed = &bytes[runtime.payload.clone()];
    let pointer_offset = mapped_operand(mapper, &installed[0xea..0xed])
        .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::PointerAddress)?;
    let pointer = owned_block(bytes, pointer_offset).map_err(|error| match error {
        OwnedBlockError::BeforeHeader => {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::PointerBeforeHeader(pointer_offset)
        }
        OwnedBlockError::Header(source) => {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::PointerHeader(source)
        }
        OwnedBlockError::Ownership(actual) => {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::PointerOwnership {
                expected: pointer_offset,
                actual,
            }
        }
    })?;
    if pointer.payload.len() != 0x600 {
        return Err(SmwUsV1ExpandedExAnimationRuntimeDetectError::PointerLength(
            pointer.payload.len(),
        ));
    }
    let mapped_address = |offset| -> Result<u32, RomError> {
        let address = pc_to_snes(mapper, offset)?;
        Ok(if mapper == Mapper::LoRom {
            address & 0x7f_ffff
        } else {
            address
        })
    };
    let runtime_low = mapped_address(runtime_offset)
        .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?
        as u16;
    let relocations = ExpandedExAnimationRuntimeRelocations {
        mapping_bytes: [installed[0x5c], installed[0x66]],
        snes_pointers: [
            mapped_address(pointer_offset + 1)
                .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::PointerAddress)?,
            mapped_address(pointer_offset)
                .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::PointerAddress)?,
            mapped_address(runtime_offset + 0xb14)
                .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?,
            mapped_address(runtime_offset + 0xb24)
                .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?,
            mapped_address(runtime_offset + 0xb1c)
                .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?,
            mapped_address(runtime_offset + 0xb1c)
                .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?,
            mapped_address(runtime_offset + 0xb1c)
                .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?,
            mapped_address(runtime_offset + 0xb1c)
                .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?,
        ],
        iram_words: RUNTIME_IRAM_ADDENDS.map(|addend| runtime_low.wrapping_add(addend)),
        local_word_base: mapped_address(runtime_offset + 0x4b0)
            .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?
            as u16,
    };
    let mut expected = if mapper_runtime {
        relocate_expanded_exanimation_runtime_with_optional_suffix(
            &relocations,
            ExpandedExAnimationRuntimeOptionalRelocations {
                suffix_snes_pointer: mapped_address(runtime_offset + 0xc30)
                    .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?,
                mapping_helper_snes_pointer: OPTIONAL_MAPPING_HELPER_SNES_ADDRESS,
            },
        )
    } else {
        relocate_expanded_exanimation_runtime(&relocations)
    }
    .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::Relocation)?;
    expected[0x46..0x49].copy_from_slice(&installed[0x46..0x49]);
    expected[0x65] = installed[0x65];
    let generation_101 = installed != expected
        && mapper == Mapper::LoRom
        && !mapper_runtime
        && generation_101_core_matches(installed, &expected, runtime_offset);
    if installed != expected && !generation_101 {
        return Err(SmwUsV1ExpandedExAnimationRuntimeDetectError::RuntimeMismatch);
    }
    let pointer_hook_target = mapped_address(runtime_offset + 0x170)
        .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?;
    let pointer_hook = [
        0x22,
        pointer_hook_target as u8,
        (pointer_hook_target >> 8) as u8,
        (pointer_hook_target >> 16) as u8,
        0x60,
    ];
    let shared_a_offset = mapper_rom_offset(mapper, 0x77550);
    let shared_b_offset = mapper_rom_offset(mapper, 0x77560);
    let shared_a_target = mapped_address(shared_a_offset)
        .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?;
    let shared_b_target = mapped_address(shared_b_offset)
        .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HookAddress)?;
    let shared_a_hook = [
        0x22,
        shared_a_target as u8,
        (shared_a_target >> 8) as u8,
        (shared_a_target >> 16) as u8,
    ];
    let shared_b_hook = [
        0x22,
        shared_b_target as u8,
        (shared_b_target >> 8) as u8,
        (shared_b_target >> 16) as u8,
    ];
    let mut shared_a = if generation_101 {
        GENERATION_101_SHARED_PALETTE_RUNTIME_A
    } else {
        SHARED_PALETTE_RUNTIME_A
    };
    let mut shared_b = if generation_101 {
        GENERATION_101_SHARED_PALETTE_RUNTIME_B
    } else {
        SHARED_PALETTE_RUNTIME_B
    };
    let mut level_graphics = LEVEL_GRAPHICS_RUNTIME;
    if mapper_runtime {
        add_mapper_iram_word(&mut shared_a, 3);
        add_mapper_iram_word(&mut shared_b, 1);
        add_mapper_iram_word(&mut level_graphics, 1);
    }
    for (offset, expected) in [
        (mapper_rom_offset(mapper, 0x2390), &pointer_hook[..]),
        (mapper_rom_offset(mapper, 0x25e1), &[0xea, 0xea][..]),
        (mapper_rom_offset(mapper, 0x1bcc0), &[0; 0x10][..]),
        (mapper_rom_offset(mapper, 0x2d8e2), &shared_a_hook[..]),
        (shared_a_offset, &shared_a[..]),
        (mapper_rom_offset(mapper, 0x26b8), &shared_b_hook[..]),
        (shared_b_offset, &shared_b[..]),
    ] {
        let actual = bytes.get(offset..offset + expected.len());
        let mirrored_generation_101_hook = generation_101
            && matches!(offset, 0x2d8e2 | 0x26b8)
            && actual.is_some_and(|actual| {
                actual.len() == 4 && actual[..3] == expected[..3] && actual[3] == expected[3] ^ 0x80
            });
        if actual != Some(expected) && !mirrored_generation_101_hook {
            return Err(
                SmwUsV1ExpandedExAnimationRuntimeDetectError::FixedRangeMismatch { offset },
            );
        }
    }
    authenticate_helper(
        bytes,
        mapper,
        mapper_rom_offset(mapper, 0x25e3),
        false,
        &level_graphics,
    )?;
    authenticate_helper(
        bytes,
        mapper,
        mapper_rom_offset(mapper, 0x0a4e),
        true,
        &GRAPHICS_RUNTIME,
    )?;
    Ok(runtime)
}

/// Classifies the three coordinator branches without treating malformed installed signals as
/// absence.
pub fn probe_smw_us_v1_expanded_exanimation_runtime_generation(
    bytes: &[u8],
) -> Result<SmwUsV1ExpandedExAnimationRuntimeGeneration, SmwUsV1ExpandedExAnimationRuntimeDetectError>
{
    probe_smw_us_v1_expanded_exanimation_runtime_generation_for_mapper(bytes, Mapper::LoRom, false)
}

/// Classifies the coordinator state while authenticating the selected mapper/runtime form.
pub fn probe_smw_us_v1_expanded_exanimation_runtime_generation_for_mapper(
    bytes: &[u8],
    mapper: Mapper,
    mapper_runtime: bool,
) -> Result<SmwUsV1ExpandedExAnimationRuntimeGeneration, SmwUsV1ExpandedExAnimationRuntimeDetectError>
{
    // `EnsureExpandedExAnimationRuntimeInstalled` tests these two descriptor-selected bytes in
    // this order. The first JSL is shared by the legacy-pointer and current generations, so its
    // owned marker/runtime distinguishes those forms. Never collapse a malformed installed signal
    // into `Absent`.
    if generation_signal(bytes, mapper_rom_offset(mapper, 0x2390))? == 0x22 {
        if mapper == Mapper::LoRom
            && crate::smw_us_v1_legacy_exanimation_hook_migration(bytes).is_ok()
        {
            return Ok(SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyPointerHooks);
        }
        detect_smw_us_v1_current_expanded_exanimation_runtime_for_mapper(
            bytes,
            mapper,
            mapper_runtime,
        )?;
        return Ok(SmwUsV1ExpandedExAnimationRuntimeGeneration::Current);
    }
    if mapper == Mapper::LoRom && generation_signal(bytes, 0x2418)? == 0x22 {
        detect_smw_us_v1_legacy_global_exanimation_runtime(bytes)?;
        return Ok(SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyGlobalTable);
    }
    let core_hook = mapper_rom_offset(mapper, 0x283ad);
    if bytes.get(core_hook..core_hook + 5) == Some(&[0xe2, 0x30, 0x9c, 0x33, 0x19]) {
        return Ok(SmwUsV1ExpandedExAnimationRuntimeGeneration::Absent);
    }
    detect_smw_us_v1_current_expanded_exanimation_runtime_for_mapper(
        bytes,
        mapper,
        mapper_runtime,
    )?;
    Ok(SmwUsV1ExpandedExAnimationRuntimeGeneration::Current)
}

/// Resolves and authenticates the storage consumed by `MigrateLegacyGlobalExAnimations`.
///
/// The original coordinator selects this generation from the JSL opcode at descriptor entry
/// `$169` (logical `$02418`). Its operand names the obsolete `$140` auxiliary table. Descriptor
/// entry `$16A` (logical `$0283AD`) names a runtime contained by one RATS owner whose `+$1A`
/// operand names the copied `$600`, 512-entry legacy pointer table. Historical 1.6x installations
/// may place that runtime after other code in the same owner rather than at its payload start.
pub fn detect_smw_us_v1_legacy_global_exanimation_runtime(
    bytes: &[u8],
) -> Result<SmwUsV1LegacyGlobalExAnimationRuntime, SmwUsV1ExpandedExAnimationRuntimeDetectError> {
    if generation_signal(bytes, 0x2418)? != 0x22 {
        return Err(
            SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyGenerationSignalMismatch {
                offset: 0x2418,
            },
        );
    }
    let auxiliary_offset = mapped_operand(
        Mapper::LoRom,
        bytes.get(0x2419..0x241c).ok_or(
            SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyGenerationSignalRange {
                offset: 0x2419,
            },
        )?,
    )
    .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyAuxiliaryAddress)?;
    let auxiliary_table = checked_legacy_range(bytes, auxiliary_offset, 0x140, true)?;

    let runtime_hook = bytes.get(0x283ad..0x283b1).ok_or(
        SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyGenerationSignalRange {
            offset: 0x283ad,
        },
    )?;
    if runtime_hook[0] != 0x22 {
        return Err(
            SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyGenerationSignalMismatch {
                offset: 0x283ad,
            },
        );
    }
    let runtime_offset = mapped_operand(Mapper::LoRom, &runtime_hook[1..4])
        .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyRuntimeAddress)?;
    let required = 0x1d;
    let runtime = lm_rats::scan(bytes)
        .into_iter()
        .find(|block| block.payload.start <= runtime_offset && runtime_offset < block.payload.end)
        .ok_or(
            SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyRuntimeOwnership {
                expected: runtime_offset,
                actual: runtime_offset,
            },
        )?;
    let available = runtime.payload.end.saturating_sub(runtime_offset);
    if available < required {
        return Err(
            SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyRuntimeTooShort {
                required,
                actual: available,
            },
        );
    }
    let pointer_offset = mapped_operand(
        Mapper::LoRom,
        &bytes[runtime_offset + 0x1a..runtime_offset + 0x1d],
    )
    .map_err(SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyPointerAddress)?;
    let pointer_table = checked_legacy_range(bytes, pointer_offset, 0x600, false)?;

    let runtime_prefix = runtime_offset..runtime_offset + required;
    for (first, second) in [
        (&runtime_prefix, &pointer_table),
        (&pointer_table, &auxiliary_table),
    ] {
        if first.start < second.end && second.start < first.end {
            return Err(
                SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyStorageOverlap {
                    first: first.start,
                    second: second.start,
                },
            );
        }
    }
    Ok(SmwUsV1LegacyGlobalExAnimationRuntime {
        runtime,
        pointer_table,
        auxiliary_table,
    })
}

fn generation_signal(
    bytes: &[u8],
    offset: usize,
) -> Result<u8, SmwUsV1ExpandedExAnimationRuntimeDetectError> {
    bytes
        .get(offset)
        .copied()
        .ok_or(SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyGenerationSignalRange { offset })
}

fn checked_legacy_range(
    bytes: &[u8],
    offset: usize,
    len: usize,
    auxiliary: bool,
) -> Result<Range<usize>, SmwUsV1ExpandedExAnimationRuntimeDetectError> {
    let range = offset..offset.saturating_add(len);
    if range.end < offset || bytes.get(range.clone()).is_none() {
        return Err(if auxiliary {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyAuxiliaryRange { offset, len }
        } else {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyPointerRange { offset, len }
        });
    }
    Ok(range)
}

#[derive(Debug)]
enum OwnedBlockError {
    BeforeHeader,
    Header(HeaderError),
    Ownership(usize),
}

fn owned_block(bytes: &[u8], payload: usize) -> Result<RatsBlock, OwnedBlockError> {
    let header = payload
        .checked_sub(HEADER_LEN)
        .ok_or(OwnedBlockError::BeforeHeader)?;
    let block = parse_at(bytes, header).map_err(OwnedBlockError::Header)?;
    if block.payload.start != payload {
        return Err(OwnedBlockError::Ownership(block.payload.start));
    }
    Ok(block)
}

fn mapped_operand(mapper: Mapper, bytes: &[u8]) -> Result<usize, RomError> {
    let address = u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16;
    snes_to_pc(mapper, address)
}

fn authenticate_helper(
    bytes: &[u8],
    mapper: Mapper,
    hook_offset: usize,
    trailing_nop: bool,
    expected: &[u8],
) -> Result<(), SmwUsV1ExpandedExAnimationRuntimeDetectError> {
    let hook_len = if trailing_nop { 5 } else { 4 };
    let hook = bytes.get(hook_offset..hook_offset + hook_len).ok_or(
        SmwUsV1ExpandedExAnimationRuntimeDetectError::FixedRangeMismatch {
            offset: hook_offset,
        },
    )?;
    if hook[0] != 0x22 || trailing_nop && hook[4] != 0xea {
        return Err(
            SmwUsV1ExpandedExAnimationRuntimeDetectError::FixedRangeMismatch {
                offset: hook_offset,
            },
        );
    }
    let target = mapped_operand(mapper, &hook[1..4]).map_err(|source| {
        SmwUsV1ExpandedExAnimationRuntimeDetectError::HelperAddress {
            offset: hook_offset,
            source,
        }
    })?;
    let block = owned_block(bytes, target).map_err(|error| match error {
        OwnedBlockError::BeforeHeader => {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::HelperBeforeHeader {
                offset: hook_offset,
                target,
            }
        }
        OwnedBlockError::Header(source) => {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::HelperHeader {
                offset: hook_offset,
                source,
            }
        }
        OwnedBlockError::Ownership(actual) => {
            SmwUsV1ExpandedExAnimationRuntimeDetectError::HelperOwnership {
                offset: hook_offset,
                expected: target,
                actual,
            }
        }
    })?;
    if block.payload.len() != expected.len() {
        return Err(SmwUsV1ExpandedExAnimationRuntimeDetectError::HelperLength {
            offset: hook_offset,
            expected: expected.len(),
            actual: block.payload.len(),
        });
    }
    if &bytes[block.payload] != expected {
        return Err(
            SmwUsV1ExpandedExAnimationRuntimeDetectError::HelperPayloadMismatch {
                offset: hook_offset,
            },
        );
    }
    Ok(())
}

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
    smw_us_v1_expanded_exanimation_runtime_installation_plan_for_mapper(
        Mapper::LoRom,
        AllocationPolicy::lorom(
            SMW_US_V1_EXPANDED_EXANIMATION_CORE_SEARCH_START
                ..SMW_US_V1_EXPANDED_EXANIMATION_CORE_SEARCH_END,
        ),
        false,
    )
}

/// Builds the complete descriptor-recovered runtime family for a concrete mapper and allocation
/// policy. The caller must select `mapper_runtime` through
/// [`smw_us_v1_expanded_exanimation_uses_mapper_runtime`].
pub fn smw_us_v1_expanded_exanimation_runtime_installation_plan_for_mapper(
    mapper: Mapper,
    allocation: AllocationPolicy,
    mapper_runtime: bool,
) -> Result<RelocatablePatchPlan, ExpandedExAnimationRuntimeError> {
    let encoding = if mapper == Mapper::LoRom {
        PatchFixupEncoding::Long24LowBank
    } else {
        PatchFixupEncoding::Long24
    };
    let mut level_graphics = LEVEL_GRAPHICS_RUNTIME.to_vec();
    let mut shared_a = SHARED_PALETTE_RUNTIME_A.to_vec();
    let mut shared_b = SHARED_PALETTE_RUNTIME_B.to_vec();
    if mapper_runtime {
        add_mapper_iram_word(&mut level_graphics, 1);
        add_mapper_iram_word(&mut shared_a, 3);
        add_mapper_iram_word(&mut shared_b, 1);
    }
    let fixed_hook = |offset: usize, expected: &[u8], target: usize| {
        let offset = mapper_rom_offset(mapper, offset);
        let target = mapper_rom_offset(mapper, target);
        let mut address = pc_to_snes(mapper, target)
            .expect("SMW fixed helper offsets are representable in every supported mapper");
        if mapper == Mapper::LoRom {
            address &= 0x7f_ffff;
        }
        direct(
            offset,
            expected,
            &[
                0x22,
                address as u8,
                (address >> 8) as u8,
                (address >> 16) as u8,
            ],
        )
    };
    Ok(RelocatablePatchPlan {
        description: format!("install SMW US v1 {mapper:?} expanded ExAnimation runtime"),
        mapper,
        allocation,
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: vec![
            smw_us_v1_expanded_exanimation_runtime_payload(mapper, mapper_runtime)?,
            PatchPayload {
                bytes: empty_expanded_exanimation_pointer_table(),
                fixups: Vec::new(),
            },
            PatchPayload {
                bytes: level_graphics,
                fixups: Vec::new(),
            },
            PatchPayload {
                bytes: GRAPHICS_RUNTIME.to_vec(),
                fixups: Vec::new(),
            },
        ],
        writes: vec![
            payload_hook(
                mapper_rom_offset(mapper, 0x0002_83ad),
                &[0xe2, 0x30, 0x9c, 0x33, 0x19],
                0,
                0,
                true,
                encoding,
            ),
            payload_return_hook(
                mapper_rom_offset(mapper, 0x0000_2390),
                &[0xc2, 0x20, 0xa0, 0x80, 0x8c],
                0,
                0x170,
                encoding,
            ),
            direct(
                mapper_rom_offset(mapper, 0x0000_25e1),
                &[0xa9, 0xef],
                &[0xea, 0xea],
            ),
            payload_hook(
                mapper_rom_offset(mapper, 0x0000_25e3),
                &[0x01, 0x54, 0x00, 0x00],
                2,
                0,
                false,
                encoding,
            ),
            payload_hook(
                mapper_rom_offset(mapper, 0x0000_0a4e),
                &[0xc2, 0x30, 0xa2, 0xfe, 0x1f],
                3,
                0,
                true,
                encoding,
            ),
            direct(
                mapper_rom_offset(mapper, 0x0001_bcc0),
                &[0xff; 0x10],
                &[0; 0x10],
            ),
            fixed_hook(0x0002_d8e2, &[0xa5, 0x0e, 0x0a, 0xa8], 0x0007_7550),
            direct(
                mapper_rom_offset(mapper, 0x0007_7550),
                &[0xff; 0x0d],
                &shared_a,
            ),
            fixed_hook(0x0000_26b8, &[0x84, 0x76, 0x84, 0x89], 0x0007_7560),
            direct(
                mapper_rom_offset(mapper, 0x0007_7560),
                &[0xff; 0x10],
                &shared_b,
            ),
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
    target_addend: usize,
    trailing_nop: bool,
    encoding: PatchFixupEncoding,
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
            encoding,
        }],
    }
}

fn payload_return_hook(
    offset: usize,
    expected: &[u8],
    target_payload: usize,
    target_addend: usize,
    encoding: PatchFixupEncoding,
) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: vec![0x22, 0, 0, 0, 0x60],
        fixups: vec![PatchFixup {
            offset: 1,
            target_payload,
            target_addend,
            encoding,
        }],
    }
}

fn add_mapper_iram_word(bytes: &mut [u8], offset: usize) {
    let value = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
    debug_assert!(value < 0x2000);
    bytes[offset..offset + 2].copy_from_slice(&(value + 0x6000).to_le_bytes());
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
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use flate2::read::GzDecoder;
    use lm_project::{ExAnimationRomLayout, LevelPointerTable, Project, RelocatablePatchError};
    use lm_rats::make_header;
    use lm_rom::{RomImage, SnesChecksum, pc_to_snes};
    use std::{fs, io::Read, path::PathBuf};

    const EXLOROM_RUNTIME_ORACLE_GZIP_BASE64: &str =
        include_str!("assets/exanimation_exlorom_runtime.bin.gz.b64");

    fn generation_101_payload_modes(
        image: &RomImage,
        pointers: LevelPointerTable,
    ) -> ([bool; 256], usize) {
        let mut recovered = [None; 256];
        let mut populated = 0usize;
        for slot in 0..pointers.entries {
            let pointer = pointers.pointer_offset(slot).unwrap();
            let raw = &image.logical_bytes()[pointer..pointer + 3];
            if raw[2] == 0 {
                continue;
            }
            populated += 1;
            let address = lm_rom::SnesPointer24::decode(raw).unwrap().get();
            let offset = lm_rom::snes_to_pc(Mapper::LoRom, address).unwrap();
            let owner = owned_block(image.logical_bytes(), offset).unwrap();
            let payload = &image.logical_bytes()[owner.payload];
            let record_count = usize::from(payload[0]);
            let trigger_mask = u16::from_le_bytes([payload[6], payload[7]]);
            let offset_base = 8 + trigger_mask.count_ones() as usize;
            let table_end = offset_base + record_count * 2;
            let mut starts = payload[offset_base..table_end]
                .chunks_exact(2)
                .filter_map(|pair| {
                    let relative = usize::from(u16::from_le_bytes([pair[0], pair[1]]));
                    (relative != 0).then_some(offset_base + relative)
                })
                .collect::<Vec<_>>();
            starts.sort_unstable();
            starts.dedup();
            for (index, start) in starts.iter().copied().enumerate() {
                let end = starts.get(index + 1).copied().unwrap_or(payload.len());
                let metadata = &payload[start..start + 5];
                let base = if (0x18..=0x1b).contains(&metadata[0]) || metadata[0] == 0 {
                    0
                } else {
                    (usize::from(metadata[2]) + 1) * 2
                };
                let actual = end - (start + 5);
                let double = match (actual == base, actual == base * 2) {
                    (true, false) => false,
                    (false, true) => true,
                    (true, true) => false,
                    _ => panic!(
                        "generation-1.01 slot {slot:#05x} mode {:#04x} has {actual} frame bytes, expected {base} or {}",
                        metadata[1],
                        base * 2
                    ),
                };
                let mode = usize::from(metadata[1]);
                if let Some(previous) = recovered[mode] {
                    assert_eq!(previous, double, "conflicting size for mode {mode:#04x}");
                }
                recovered[mode] = Some(double);
            }
        }
        (recovered.map(|value| value.unwrap_or(false)), populated)
    }

    fn verify_generation_101_level_payloads(image: &RomImage, runtime: &RatsBlock) -> usize {
        let pointer_offset = mapped_operand(
            Mapper::LoRom,
            &image.logical_bytes()[runtime.payload.start + 0xea..runtime.payload.start + 0xed],
        )
        .unwrap();
        let layout = ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: pointer_offset,
                entries: 0x200,
                stride: 3,
            },
            maximum_records: 0x40,
            maximum_encoded_len: 0x8000,
        };
        let project = Project::new(image.clone());
        let (modes, populated) = generation_101_payload_modes(image, layout.pointers);
        for slot in 0..0x200 {
            let pointer = layout.pointers.pointer_offset(slot).unwrap();
            let raw = &image.logical_bytes()[pointer..pointer + 3];
            if raw[2] == 0 {
                continue;
            }
            let animation = project
                .load_exanimation(slot, layout, &modes)
                .unwrap_or_else(|error| panic!("generation-1.01 slot {slot:#05x}: {error}"));
            let encoded = animation.encode(&modes).unwrap();
            let (decoded, consumed) =
                lm_graphics::CompactExAnimation::decode(&encoded, layout.maximum_records, &modes)
                    .unwrap();
            assert_eq!(consumed, encoded.len());
            assert_eq!(decoded, animation, "generation-1.01 slot {slot:#05x}");
        }
        populated
    }

    fn mapper_metadata_rom(mapper: Mapper, feature_bits: Option<u32>, hook_word: u16) -> Vec<u8> {
        let mut bytes = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap()
            .logical_bytes()
            .to_vec();
        let metadata_base = if mapper == Mapper::ExLoRom {
            bytes.resize(0x50_0000, 0xff);
            0x40_0000
        } else {
            0
        };
        let hook_offset = mapper_rom_offset(mapper, MAPPER_COMPATIBILITY_HOOK_WORD_OFFSET);
        bytes[hook_offset..hook_offset + 2].copy_from_slice(&hook_word.to_le_bytes());
        if let Some(bits) = feature_bits {
            let attribution_offset = metadata_base + crate::SMW_US_V1_LM_ATTRIBUTION_OFFSET;
            let attribution = &mut bytes
                [attribution_offset..attribution_offset + LunarMagicRomMetadata::ATTRIBUTION_LEN];
            attribution.fill(b' ');
            attribution[..LunarMagicRomMetadata::SIGNATURE.len()]
                .copy_from_slice(LunarMagicRomMetadata::SIGNATURE);
            bytes[metadata_base + crate::SMW_US_V1_LM_VRAM_VERSION_OFFSET] = 1;
            let feature_offset = metadata_base + crate::SMW_US_V1_LM_FEATURE_RECORD_OFFSET;
            let feature =
                &mut bytes[feature_offset..feature_offset + LunarMagicRomMetadata::FEATURE_LEN];
            feature.fill(0);
            feature[..4].copy_from_slice(&bits.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn mapper_runtime_selector_uses_declared_metadata_bits_and_exact_legacy_threshold() {
        for (mapper, declaration, enabled) in [
            (
                Mapper::ExLoRom,
                EXLOROM_CONFIGURATION_PRESENT_BIT,
                EXLOROM_COMPATIBILITY_RUNTIME_BIT,
            ),
            (
                Mapper::Sa1,
                SA1_CONFIGURATION_PRESENT_BIT,
                SA1_COMPATIBILITY_RUNTIME_BIT,
            ),
        ] {
            assert!(
                !smw_us_v1_expanded_exanimation_uses_mapper_runtime(
                    &mapper_metadata_rom(mapper, Some(declaration), 0xffff),
                    mapper,
                )
                .unwrap()
            );
            assert!(
                smw_us_v1_expanded_exanimation_uses_mapper_runtime(
                    &mapper_metadata_rom(mapper, Some(declaration | enabled), 0),
                    mapper,
                )
                .unwrap()
            );
            assert!(
                !smw_us_v1_expanded_exanimation_uses_mapper_runtime(
                    &mapper_metadata_rom(mapper, Some(enabled), 0x1fff),
                    mapper,
                )
                .unwrap()
            );
            assert!(
                smw_us_v1_expanded_exanimation_uses_mapper_runtime(
                    &mapper_metadata_rom(mapper, Some(enabled), 0x2000),
                    mapper,
                )
                .unwrap()
            );
            assert!(
                !smw_us_v1_expanded_exanimation_uses_mapper_runtime(
                    &mapper_metadata_rom(mapper, None, 0x1fff),
                    mapper,
                )
                .unwrap()
            );
            assert!(
                smw_us_v1_expanded_exanimation_uses_mapper_runtime(
                    &mapper_metadata_rom(mapper, None, 0x2000),
                    mapper,
                )
                .unwrap()
            );
        }
        assert!(
            !smw_us_v1_expanded_exanimation_uses_mapper_runtime(
                &mapper_metadata_rom(
                    Mapper::LoRom,
                    Some(
                        EXLOROM_CONFIGURATION_PRESENT_BIT
                            | EXLOROM_COMPATIBILITY_RUNTIME_BIT
                            | SA1_CONFIGURATION_PRESENT_BIT
                            | SA1_COMPATIBILITY_RUNTIME_BIT,
                    ),
                    0xffff,
                ),
                Mapper::LoRom,
            )
            .unwrap()
        );
    }

    #[test]
    fn authentic_lunar_magic_exlorom_runtime_family_matches_every_owned_byte() {
        // Lunar Magic 3.63 command $23B4 converted the authenticated pristine SMW-US ROM, then
        // `-ImportLevel` imported the retained active-ExAnimation level. The compact fixture is
        // the complete runtime family extracted from that before/after result, in the order below.
        let compressed = STANDARD
            .decode(EXLOROM_RUNTIME_ORACLE_GZIP_BASE64.trim())
            .unwrap();
        let mut oracle = Vec::new();
        GzDecoder::new(compressed.as_slice())
            .read_to_end(&mut oracle)
            .unwrap();
        assert_eq!(oracle.len(), 4810);

        let mapper = Mapper::ExLoRom;
        let runtime = 0x20_0549;
        let pointer = 0x20_1181;
        let core = &oracle[..0xc30];
        let address = |offset| pc_to_snes(mapper, offset).unwrap();
        let relocations = ExpandedExAnimationRuntimeRelocations {
            mapping_bytes: [core[0x5c], core[0x66]],
            snes_pointers: [
                address(pointer + 1),
                address(pointer),
                address(runtime + 0xb14),
                address(runtime + 0xb24),
                address(runtime + 0xb1c),
                address(runtime + 0xb1c),
                address(runtime + 0xb1c),
                address(runtime + 0xb1c),
            ],
            iram_words: SMW_US_V1_IRAM_WORDS,
            local_word_base: address(runtime + 0x4b0) as u16,
        };
        let mut expected_core = relocate_expanded_exanimation_runtime(&relocations).unwrap();
        expected_core[0x46..0x49].copy_from_slice(&core[0x46..0x49]);
        expected_core[0x65] = core[0x65];
        assert_eq!(core, expected_core);

        let mut cursor = 0xc30;
        let mut take = |length: usize| {
            let bytes = &oracle[cursor..cursor + length];
            cursor += length;
            bytes
        };
        let mut expected_pointer_table = empty_expanded_exanimation_pointer_table();
        expected_pointer_table[..3].copy_from_slice(&address(0x20_17e9).to_le_bytes()[..3]);
        assert_eq!(take(0x600), expected_pointer_table);
        assert_eq!(take(0x20), LEVEL_GRAPHICS_RUNTIME);
        assert_eq!(take(0x30), GRAPHICS_RUNTIME);
        let hook = |target: usize, trailing_nop: bool| {
            let address = address(target).to_le_bytes();
            let mut bytes = vec![0x22, address[0], address[1], address[2]];
            if trailing_nop {
                bytes.push(0xea);
            }
            bytes
        };
        assert_eq!(take(5), hook(runtime, true));
        let mut pointer_hook = hook(runtime + 0x170, false);
        pointer_hook.push(0x60);
        assert_eq!(take(5), pointer_hook);
        assert_eq!(take(2), [0xea, 0xea]);
        assert_eq!(take(0x10), [0; 0x10]);
        assert_eq!(take(4), hook(0x47_7550, false));
        assert_eq!(take(0x0d), SHARED_PALETTE_RUNTIME_A);
        assert_eq!(take(4), hook(0x47_7560, false));
        assert_eq!(take(0x10), SHARED_PALETTE_RUNTIME_B);
        assert_eq!(take(4), hook(0x20_1789, false));
        assert_eq!(take(5), hook(0x20_17b1, true));
        assert_eq!(cursor, oracle.len());
    }

    #[test]
    fn mapper_runtime_payload_resolves_every_fixup_for_exlorom_and_sa1() {
        for mapper in [Mapper::ExLoRom, Mapper::Sa1] {
            let payload = smw_us_v1_expanded_exanimation_runtime_payload(mapper, true).unwrap();
            assert_eq!(payload.bytes.len(), 0xc50);
            assert_eq!(payload.fixups.len(), 8 + 12 + 108 + 1);
            assert!(
                payload.fixups[..8]
                    .iter()
                    .all(|fixup| fixup.encoding == PatchFixupEncoding::Long24)
            );
            let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x50_0000]).unwrap());
            let plan = RelocatablePatchPlan {
                description: format!("test {mapper:?} mapper ExAnimation payload"),
                mapper,
                allocation: AllocationPolicy {
                    search: 0x40_0000..0x41_0000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0x00, 0xff],
                    protected: Vec::new(),
                },
                checksum_field: 0x7fdc,
                expansion_fill: 0xff,
                payloads: vec![
                    payload,
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
                writes: Vec::new(),
            };
            let result = project.install_relocatable_patch(&plan).unwrap();
            let address = |payload: usize, addend: usize| {
                pc_to_snes(mapper, result.blocks[payload].payload.start + addend).unwrap()
            };
            let core = result.blocks[0].payload.start;
            let expected = crate::relocate_expanded_exanimation_runtime_with_optional_suffix(
                &ExpandedExAnimationRuntimeRelocations {
                    mapping_bytes: [0, 0],
                    snes_pointers: [
                        address(1, 1),
                        address(1, 0),
                        address(0, 0xb14),
                        address(0, 0xb24),
                        address(0, 0xb1c),
                        address(0, 0xb1c),
                        address(0, 0xb1c),
                        address(0, 0xb1c),
                    ],
                    iram_words: RUNTIME_IRAM_ADDENDS
                        .map(|addend| (address(0, 0) as u16).wrapping_add(addend)),
                    local_word_base: pc_to_snes(mapper, core + 0x4b0).unwrap() as u16,
                },
                crate::ExpandedExAnimationRuntimeOptionalRelocations {
                    suffix_snes_pointer: address(0, 0xc30),
                    mapping_helper_snes_pointer: crate::OPTIONAL_MAPPING_HELPER_SNES_ADDRESS,
                },
            )
            .unwrap();
            assert_eq!(project.rom.read(core, 0xc50).unwrap(), expected);
            assert!(
                SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc)
                    .unwrap()
                    .is_complementary()
            );
        }
    }

    #[test]
    fn complete_mapper_plans_install_every_helper_checksum_and_exact_undo() {
        let pristine = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap()
            .logical_bytes()
            .to_vec();
        for mapper in [Mapper::ExLoRom, Mapper::Sa1] {
            let mut original = if mapper == Mapper::ExLoRom {
                let mut converted = Project::open_supported(
                    RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap(),
                )
                .unwrap();
                converted.convert_to_64_mbit_exlorom().unwrap();
                converted.rom.logical_bytes().to_vec()
            } else {
                let mut bytes = pristine.clone();
                bytes.resize(0x50_0000, 0xff);
                bytes
            };
            if mapper == Mapper::Sa1 {
                original.resize(0x50_0000, 0xff);
            }
            let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
            let plan = smw_us_v1_expanded_exanimation_runtime_installation_plan_for_mapper(
                mapper,
                AllocationPolicy {
                    search: if mapper == Mapper::ExLoRom {
                        0x10_0000..0x40_0000
                    } else {
                        0x40_0000..0x41_0000
                    },
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0x00, 0xff],
                    protected: Vec::new(),
                },
                true,
            )
            .unwrap();
            let result = project.install_relocatable_patch(&plan).unwrap();
            assert_eq!(result.blocks[0].payload.len(), 0xc50);
            assert_eq!(result.blocks[1].payload.len(), 0x600);
            assert_eq!(result.blocks[2].payload.len(), 0x20);
            assert_eq!(result.blocks[3].payload.len(), 0x30);
            assert_eq!(
                project
                    .rom
                    .read(mapper_rom_offset(mapper, 0x25e1), 2)
                    .unwrap(),
                &[0xea, 0xea]
            );
            let fixed_a = pc_to_snes(mapper, mapper_rom_offset(mapper, 0x77550))
                .unwrap()
                .to_le_bytes();
            assert_eq!(
                project
                    .rom
                    .read(mapper_rom_offset(mapper, 0x2d8e2), 4)
                    .unwrap(),
                &[0x22, fixed_a[0], fixed_a[1], fixed_a[2]]
            );
            assert_eq!(
                project
                    .rom
                    .read(result.blocks[2].payload.start + 1, 2)
                    .unwrap(),
                &(0x0d9b_u16 + 0x6000).to_le_bytes()
            );
            assert_eq!(
                project
                    .rom
                    .read(mapper_rom_offset(mapper, 0x77553), 2)
                    .unwrap(),
                &(0x010b_u16 + 0x6000).to_le_bytes()
            );
            assert_eq!(
                project
                    .rom
                    .read(mapper_rom_offset(mapper, 0x77561), 2)
                    .unwrap(),
                &(0x13cd_u16 + 0x6000).to_le_bytes()
            );
            assert!(
                SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc)
                    .unwrap()
                    .is_complementary()
            );
            assert_eq!(
                detect_smw_us_v1_current_expanded_exanimation_runtime_for_mapper(
                    project.rom.logical_bytes(),
                    mapper,
                    true,
                )
                .unwrap(),
                result.blocks[0]
            );
            let installed = project.rom.logical_bytes().to_vec();
            for offset in [
                result.blocks[0].payload.start,
                result.blocks[0].payload.start + 0xc30,
                result.blocks[2].payload.start,
                mapper_rom_offset(mapper, 0x25e1),
            ] {
                let mut corrupt = installed.clone();
                corrupt[offset] ^= 1;
                assert!(
                    detect_smw_us_v1_current_expanded_exanimation_runtime_for_mapper(
                        &corrupt, mapper, true,
                    )
                    .is_err(),
                    "{mapper:?} corruption at {offset:#x} was accepted"
                );
            }
            assert!(project.undo().unwrap());
            assert_eq!(project.rom.logical_bytes(), original);
            assert!(!project.undo().unwrap());
        }
    }

    #[test]
    fn retained_metadata_and_partial_metadata_obey_the_mapper_selector_boundary() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let retained = fs::read(
            root.join("oracle-work/lm363/pristine-us/exanimation-install-positive/after.smc"),
        )
        .unwrap();
        let retained = RomImage::from_bytes(retained).unwrap();
        for mapper in [Mapper::ExLoRom, Mapper::Sa1] {
            let mut bytes = retained.logical_bytes().to_vec();
            if mapper == Mapper::ExLoRom {
                bytes.resize(0x50_0000, 0xff);
                let attribution = retained
                    .read(
                        crate::SMW_US_V1_LM_ATTRIBUTION_OFFSET,
                        LunarMagicRomMetadata::ATTRIBUTION_LEN,
                    )
                    .unwrap();
                let record = retained
                    .read(
                        crate::SMW_US_V1_LM_VRAM_VERSION_OFFSET,
                        1 + LunarMagicRomMetadata::FEATURE_LEN,
                    )
                    .unwrap();
                bytes[0x40_0000 + crate::SMW_US_V1_LM_ATTRIBUTION_OFFSET
                    ..0x40_0000 + crate::SMW_US_V1_LM_ATTRIBUTION_OFFSET + attribution.len()]
                    .copy_from_slice(attribution);
                bytes[0x40_0000 + crate::SMW_US_V1_LM_VRAM_VERSION_OFFSET
                    ..0x40_0000 + crate::SMW_US_V1_LM_VRAM_VERSION_OFFSET + record.len()]
                    .copy_from_slice(record);
                bytes[0x40_0000 + MAPPER_COMPATIBILITY_HOOK_WORD_OFFSET
                    ..0x40_0000 + MAPPER_COMPATIBILITY_HOOK_WORD_OFFSET + 2]
                    .copy_from_slice(
                        retained
                            .read(MAPPER_COMPATIBILITY_HOOK_WORD_OFFSET, 2)
                            .unwrap(),
                    );
            }
            assert!(!smw_us_v1_expanded_exanimation_uses_mapper_runtime(&bytes, mapper).unwrap());
        }

        let mut partial = mapper_metadata_rom(Mapper::ExLoRom, None, 0x2000);
        partial[0x40_0000 + crate::SMW_US_V1_LM_ATTRIBUTION_OFFSET
            ..0x40_0000
                + crate::SMW_US_V1_LM_ATTRIBUTION_OFFSET
                + LunarMagicRomMetadata::SIGNATURE.len()]
            .copy_from_slice(LunarMagicRomMetadata::SIGNATURE);
        assert!(matches!(
            smw_us_v1_expanded_exanimation_uses_mapper_runtime(&partial, Mapper::ExLoRom),
            Err(SmwUsV1ExpandedExAnimationRuntimeDetectError::MetadataPartialInstallation)
        ));
    }

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
                0x25e1..0x25e7,
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
    fn irregular_core_allocation_relocates_all_twelve_iram_words() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
        let plan = smw_us_v1_expanded_exanimation_runtime_installation_plan_for_mapper(
            Mapper::LoRom,
            AllocationPolicy::lorom(0x0a_0000..0x10_0000),
            false,
        )
        .unwrap();
        let result = project.install_relocatable_patch(&plan).unwrap();
        let runtime = &result.blocks[0];
        assert_eq!(runtime.header_offset, 0x0a_0000);
        let runtime_low = pc_to_snes(Mapper::LoRom, runtime.payload.start).unwrap() as u16;
        for (offset, addend) in IRAM_WORD_OFFSETS.into_iter().zip(RUNTIME_IRAM_ADDENDS) {
            assert_eq!(
                project.rom.read(runtime.payload.start + offset, 2).unwrap(),
                runtime_low.wrapping_add(addend).to_le_bytes(),
                "IRAM word at core+{offset:#x}"
            );
        }
        assert_eq!(
            detect_smw_us_v1_current_expanded_exanimation_runtime(project.rom.logical_bytes())
                .unwrap()
                .payload,
            runtime.payload
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
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

    #[test]
    fn generation_probe_authenticates_generated_and_retained_current_families() {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        let pristine_image = RomImage::from_bytes(pristine.clone()).unwrap();
        assert_eq!(
            probe_smw_us_v1_expanded_exanimation_runtime_generation(pristine_image.logical_bytes())
                .unwrap(),
            SmwUsV1ExpandedExAnimationRuntimeGeneration::Absent
        );
        let mut generated = Project::new(pristine_image);
        generated
            .install_relocatable_patch(
                &smw_us_v1_expanded_exanimation_runtime_installation_plan().unwrap(),
            )
            .unwrap();
        assert_eq!(
            probe_smw_us_v1_expanded_exanimation_runtime_generation(generated.rom.logical_bytes())
                .unwrap(),
            SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
        );

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let retained = fs::read(
            root.join("oracle-work/lm363/pristine-us/exanimation-install-positive/after.smc"),
        )
        .unwrap();
        let retained = RomImage::from_bytes(retained).unwrap();
        assert_eq!(
            probe_smw_us_v1_expanded_exanimation_runtime_generation(retained.logical_bytes())
                .unwrap(),
            SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
        );
    }

    #[test]
    fn external_generation_101_current_family_authenticates_when_supplied() {
        let Ok(path) = std::env::var("LM_EXPANDED_EXANIMATION_101_ROM") else {
            return;
        };
        let image = RomImage::from_bytes(fs::read(path).unwrap()).unwrap();
        let runtime =
            detect_smw_us_v1_current_expanded_exanimation_runtime(image.logical_bytes()).unwrap();
        assert_eq!(runtime.payload.len(), 0xc30);
        assert_eq!(
            &image.logical_bytes()[runtime.payload.start + 0x169..runtime.payload.start + 0x16d],
            &[0x4c, 0x4d, 0x01, 0x01]
        );
        assert_eq!(
            probe_smw_us_v1_expanded_exanimation_runtime_generation(image.logical_bytes()).unwrap(),
            SmwUsV1ExpandedExAnimationRuntimeGeneration::Current
        );

        for relative in [0x120, IRAM_WORD_OFFSETS[0]] {
            let mut corrupt = image.logical_bytes().to_vec();
            corrupt[runtime.payload.start + relative] ^= 1;
            assert!(
                detect_smw_us_v1_current_expanded_exanimation_runtime(&corrupt).is_err(),
                "generation-1.01 corruption at core+{relative:#x} was accepted"
            );
        }
        let mut corrupt_owner = image.logical_bytes().to_vec();
        corrupt_owner[runtime.header_offset + 6] ^= 1;
        assert!(detect_smw_us_v1_current_expanded_exanimation_runtime(&corrupt_owner).is_err());
    }

    #[test]
    fn external_generation_101_level_payloads_round_trip_when_supplied() {
        let Ok(path) = std::env::var("LM_EXPANDED_EXANIMATION_101_PAYLOAD_ROM") else {
            return;
        };
        let image = RomImage::from_bytes(fs::read(path).unwrap()).unwrap();
        let marker = image
            .logical_bytes()
            .windows(4)
            .position(|window| window == [0x4c, 0x4d, 0x01, 0x01])
            .expect("generation-1.01 core marker is absent");
        let runtime_offset = marker.checked_sub(0x169).unwrap();
        let runtime = owned_block(image.logical_bytes(), runtime_offset).unwrap();
        assert_eq!(runtime.payload.len(), 0xc30);
        assert!(generation_101_core_digest_matches(
            &image.logical_bytes()[runtime.payload.clone()]
        ));
        assert!(
            verify_generation_101_level_payloads(&image, &runtime) > 0,
            "historical payload oracle has no populated level slots"
        );
    }

    #[test]
    fn current_probe_rejects_core_and_dependent_runtime_corruption() {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project = Project::new(RomImage::from_bytes(pristine).unwrap());
        project
            .install_relocatable_patch(
                &smw_us_v1_expanded_exanimation_runtime_installation_plan().unwrap(),
            )
            .unwrap();
        for offset in [0x80549 + 0x120, 0x81789 + 3, 0x77560 + 5, 0x2392] {
            let mut corrupt = project.rom.logical_bytes().to_vec();
            corrupt[offset] ^= 1;
            assert!(
                probe_smw_us_v1_expanded_exanimation_runtime_generation(&corrupt).is_err(),
                "corruption at {offset:#x} was accepted"
            );
        }
    }

    fn legacy_global_table_rom() -> (Vec<u8>, SmwUsV1LegacyGlobalExAnimationRuntime) {
        let pristine = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(pristine).unwrap();
        let mut bytes = image.logical_bytes().to_vec();
        bytes.resize(0x10_0000, 0xff);
        let runtime_header = 0x8_0000;
        let runtime_len = 0x200;
        bytes[runtime_header..runtime_header + HEADER_LEN]
            .copy_from_slice(&make_header(runtime_len).unwrap());
        let runtime = runtime_header + HEADER_LEN;
        bytes[runtime..runtime + runtime_len].fill(0xea);
        let pointer_table = 0x8_1000;
        let auxiliary_table = 0x8_2000;
        bytes[pointer_table..pointer_table + 0x600].fill(0);
        bytes[auxiliary_table..auxiliary_table + 0x140].fill(0x5a);
        let low_bank = |offset| pc_to_snes(Mapper::LoRom, offset).unwrap() & 0x7f_ffff;
        let write_operand = |target: &mut [u8], value: u32| {
            target.copy_from_slice(&value.to_le_bytes()[..3]);
        };
        bytes[0x283ad] = 0x22;
        write_operand(&mut bytes[0x283ae..0x283b1], low_bank(runtime));
        write_operand(
            &mut bytes[runtime + 0x1a..runtime + 0x1d],
            low_bank(pointer_table),
        );
        bytes[0x2418] = 0x22;
        write_operand(&mut bytes[0x2419..0x241c], low_bank(auxiliary_table));
        let detected = detect_smw_us_v1_legacy_global_exanimation_runtime(&bytes).unwrap();
        (bytes, detected)
    }

    #[test]
    fn generation_probe_distinguishes_and_resolves_legacy_global_table_storage() {
        let (bytes, detected) = legacy_global_table_rom();
        assert_eq!(detected.runtime.payload, 0x8_0008..0x8_0208);
        assert_eq!(detected.pointer_table, 0x8_1000..0x8_1600);
        assert_eq!(detected.auxiliary_table, 0x8_2000..0x8_2140);
        assert_eq!(
            probe_smw_us_v1_expanded_exanimation_runtime_generation(&bytes).unwrap(),
            SmwUsV1ExpandedExAnimationRuntimeGeneration::LegacyGlobalTable
        );
    }

    #[test]
    fn legacy_global_signal_rejects_unowned_runtime_bad_pointers_and_overlap() {
        let (mut corrupt, detected) = legacy_global_table_rom();
        corrupt[detected.runtime.header_offset] ^= 1;
        assert!(probe_smw_us_v1_expanded_exanimation_runtime_generation(&corrupt).is_err());

        let (mut bad_pointer, detected) = legacy_global_table_rom();
        bad_pointer[detected.runtime.payload.start + 0x1c] = 0x7e;
        assert!(probe_smw_us_v1_expanded_exanimation_runtime_generation(&bad_pointer).is_err());

        let (mut overlap, detected) = legacy_global_table_rom();
        let address = pc_to_snes(Mapper::LoRom, detected.pointer_table.start).unwrap() & 0x7f_ffff;
        overlap[0x2419..0x241c].copy_from_slice(&address.to_le_bytes()[..3]);
        assert!(matches!(
            detect_smw_us_v1_legacy_global_exanimation_runtime(&overlap),
            Err(SmwUsV1ExpandedExAnimationRuntimeDetectError::LegacyStorageOverlap { .. })
        ));
    }
}
