//! Lunar Magic's complete foreground Map16 definition namespace for SMW US revision 0.

use crate::{SmwUsV1TransferredMap16Error, load_smw_us_v1_transferred_map16};
use lm_project::{
    PayloadPointer, PayloadSaveError, PayloadSaveRequest, PayloadSaveResult, Project, RomWrite,
};
use lm_rats::{AllocationPolicy, HEADER_LEN, HeaderError, ProtectedRange, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, snes_to_pc};
use std::fmt;

pub const SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE: usize = 0x37_540;
pub const SMW_US_V1_PRIMARY_MAP16_RUNTIME_MARKER_OFFSET: usize = 0x28_da4;
pub const SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT: usize = 8;
pub const SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES: usize = 0x8000;
pub const SMW_US_V1_PRIMARY_MAP16_DEFINITION_WORDS: usize =
    SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT * SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES / 2;
pub const SMW_US_V1_PRIMARY_MAP16_ACTS_LIKE_WORDS: usize = 0x8000;
pub const SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES: usize = 0x1000;
pub const SMW_US_V1_PRIMARY_MAP16_FIRST_AUXILIARY_POINTER_OFFSET: usize = 0x37_624;
pub const SMW_US_V1_PRIMARY_MAP16_SECOND_AUXILIARY_POINTER_OFFSET: usize = 0x37_63a;
pub const SMW_US_V1_PRIMARY_MAP16_AUXILIARY_BYTES: usize = 0x8000;

const BLANK_MAP16_WORD: u16 = 0x1004;
const BLANK_ACTS_LIKE_WORD: u16 = 0x0130;
const SECOND_AUXILIARY_DISPLACEMENT: u16 = 0x8000;
const SECOND_AUXILIARY_SENTINEL: u32 = 0xff_8000;
const LOW_WORD_OFFSETS: [usize; SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT] =
    [0x13, 0x1c, 0x27, 0x30, 0x54, 0x5d, 0x68, 0x71];
const BANK_BYTE_OFFSETS: [usize; SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT] =
    [0x17, 0x20, 0x2b, 0x34, 0x58, 0x61, 0x6c, 0x75];
const DISPLACEMENTS: [u16; SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT] = [
    0x1000, 0x8000, 0x0001, 0x8001, 0x0000, 0x8000, 0x0001, 0x8001,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1PrimaryMap16 {
    /// Four words per foreground definition for tiles `$0000-$7fff`.
    pub definitions: Vec<u16>,
    /// One gameplay-behavior word per foreground definition.
    pub acts_like: Vec<u16>,
    pub installed: bool,
    pub blocks: [Option<RatsBlock>; SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT],
    /// The first `$8000`-byte Acts-Like table for tiles `$0000-$3fff`.
    pub first_auxiliary_block: Option<RatsBlock>,
    /// The second `$8000`-byte Acts-Like table for tiles `$4000-$7fff`.
    pub second_auxiliary_block: Option<RatsBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1PrimaryMap16SaveOptions {
    pub allocation: AllocationPolicy,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedSmwUsV1PrimaryMap16 {
    pub blocks: [Option<PayloadSaveResult>; SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT],
    pub first_auxiliary: Option<PayloadSaveResult>,
    pub second_auxiliary: Option<PayloadSaveResult>,
}

#[derive(Debug)]
pub enum SmwUsV1PrimaryMap16Error {
    Rom(RomError),
    Transfer(SmwUsV1TransferredMap16Error),
    Header {
        block: usize,
        source: HeaderError,
    },
    AuxiliaryHeader(HeaderError),
    PointerBeforeRatsHeader {
        block: usize,
        offset: usize,
    },
    PointerNotTagged {
        block: usize,
        offset: usize,
    },
    AuxiliaryPointerBeforeRatsHeader(usize),
    AuxiliaryPointerNotTagged(usize),
    BlockTooLarge {
        block: usize,
        len: usize,
        maximum: usize,
    },
    BlockNotWordAligned {
        block: usize,
        len: usize,
    },
    AuxiliaryLength(usize),
    RuntimeNotInstalled,
    DefinitionWordCount(usize),
    ActsLikeWordCount(usize),
    Save(PayloadSaveError),
}

impl fmt::Display for SmwUsV1PrimaryMap16Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot access SMW US primary Map16 definitions: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1PrimaryMap16Error {}

impl From<RomError> for SmwUsV1PrimaryMap16Error {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<SmwUsV1TransferredMap16Error> for SmwUsV1PrimaryMap16Error {
    fn from(value: SmwUsV1TransferredMap16Error) -> Self {
        Self::Transfer(value)
    }
}

impl From<PayloadSaveError> for SmwUsV1PrimaryMap16Error {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

/// Loads all `$0000-$7fff` foreground definitions from the transferred vanilla prefix plus the
/// eight installed primary overlays.
///
/// # Errors
///
/// Rejects malformed transferred data, invalid displaced pointers, non-exact RATS ownership,
/// oversized blocks, odd payloads, or an invalid second auxiliary table.
pub fn load_smw_us_v1_primary_map16(
    project: &Project,
) -> Result<LoadedSmwUsV1PrimaryMap16, SmwUsV1PrimaryMap16Error> {
    let transferred = load_smw_us_v1_transferred_map16(project)?;
    let bytes = project.rom.logical_bytes();
    let installed = bytes
        .get(SMW_US_V1_PRIMARY_MAP16_RUNTIME_MARKER_OFFSET)
        .copied()
        == Some(0x22);
    let mut definition_bytes = initial_definition_bytes(&transferred.definitions, installed);
    let mut blocks = std::array::from_fn(|_| None);
    let mut acts_like = default_acts_like();
    let mut first_auxiliary_block = None;
    let mut second_auxiliary_block = None;
    if installed {
        for (block_index, owned_block) in blocks.iter_mut().enumerate() {
            let pointer = resolved_block_pointer(bytes, block_index)?;
            if pointer == 0 {
                continue;
            }
            let payload_offset = snes_to_pc(Mapper::LoRom, pointer)?;
            let header_offset = payload_offset.checked_sub(HEADER_LEN).ok_or(
                SmwUsV1PrimaryMap16Error::PointerBeforeRatsHeader {
                    block: block_index,
                    offset: payload_offset,
                },
            )?;
            let block = parse_at(bytes, header_offset).map_err(|source| {
                SmwUsV1PrimaryMap16Error::Header {
                    block: block_index,
                    source,
                }
            })?;
            if block.payload.start != payload_offset {
                return Err(SmwUsV1PrimaryMap16Error::PointerNotTagged {
                    block: block_index,
                    offset: payload_offset,
                });
            }
            let maximum = if block_index == 0 {
                SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES - SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES
            } else {
                SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES
            };
            if block.payload.len() > maximum {
                return Err(SmwUsV1PrimaryMap16Error::BlockTooLarge {
                    block: block_index,
                    len: block.payload.len(),
                    maximum,
                });
            }
            if block.payload.len() % 2 != 0 {
                return Err(SmwUsV1PrimaryMap16Error::BlockNotWordAligned {
                    block: block_index,
                    len: block.payload.len(),
                });
            }
            let destination = if block_index == 0 {
                SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES
            } else {
                block_index * SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES
            };
            definition_bytes[destination..destination + block.payload.len()]
                .copy_from_slice(&bytes[block.payload.clone()]);
            *owned_block = Some(block);
        }
        first_auxiliary_block = Some(load_auxiliary_block(
            bytes,
            direct_pointer(
                bytes,
                SMW_US_V1_PRIMARY_MAP16_FIRST_AUXILIARY_POINTER_OFFSET,
            )?,
        )?);
        if let Some(block) = &first_auxiliary_block {
            copy_auxiliary_words(bytes, block, &mut acts_like[..0x4000]);
        }
        second_auxiliary_block = load_second_auxiliary_block(bytes)?;
        if let Some(block) = &second_auxiliary_block {
            copy_auxiliary_words(bytes, block, &mut acts_like[0x4000..]);
        }
    }

    Ok(LoadedSmwUsV1PrimaryMap16 {
        definitions: definition_bytes
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect(),
        acts_like,
        installed,
        blocks,
        first_auxiliary_block,
        second_auxiliary_block,
    })
}

/// Saves changed foreground definition blocks using Lunar Magic's eight displaced pointer pairs.
///
/// Block zero retains its first `$1000` bytes in the transferred legacy representation. Its
/// allocated overlay begins at byte `$1000`; every later block stores a trimmed full prefix.
/// Entering blocks four through seven also materializes Lunar Magic's second default Acts-Like
/// table when it is not already installed.
///
/// # Errors
///
/// Requires the complete Map16 runtime and exact full definition shape. Any validation,
/// allocation, pointer, direct-write, or checksum failure leaves the project unchanged.
#[allow(clippy::too_many_lines)]
pub fn save_smw_us_v1_primary_map16(
    project: &mut Project,
    definitions: &[u16],
    acts_like: &[u16],
    checksum_field: usize,
    options: &SmwUsV1PrimaryMap16SaveOptions,
) -> Result<SavedSmwUsV1PrimaryMap16, SmwUsV1PrimaryMap16Error> {
    if project
        .rom
        .logical_bytes()
        .get(SMW_US_V1_PRIMARY_MAP16_RUNTIME_MARKER_OFFSET)
        .copied()
        != Some(0x22)
    {
        return Err(SmwUsV1PrimaryMap16Error::RuntimeNotInstalled);
    }
    if definitions.len() != SMW_US_V1_PRIMARY_MAP16_DEFINITION_WORDS {
        return Err(SmwUsV1PrimaryMap16Error::DefinitionWordCount(
            definitions.len(),
        ));
    }
    if acts_like.len() != SMW_US_V1_PRIMARY_MAP16_ACTS_LIKE_WORDS {
        return Err(SmwUsV1PrimaryMap16Error::ActsLikeWordCount(acts_like.len()));
    }
    let loaded = load_smw_us_v1_primary_map16(project)?;
    let mut allocation = options.allocation.clone();
    protect_runtime_fields(&mut allocation, checksum_field, project.rom.logical_len())?;

    let mut requests = Vec::new();
    let mut request_kinds = Vec::new();
    let mut writes = Vec::new();
    if acts_like[..0x4000] != loaded.acts_like[..0x4000] {
        let mut auxiliary_allocation = allocation.clone();
        auxiliary_allocation.bank_size = None;
        requests.push(PayloadSaveRequest {
            description: "save primary Map16 first auxiliary table".into(),
            payload: words_to_bytes(&acts_like[..0x4000]),
            pointer: PayloadPointer::ContiguousLowBank {
                offset: SMW_US_V1_PRIMARY_MAP16_FIRST_AUXILIARY_POINTER_OFFSET,
            },
            mapper: Mapper::LoRom,
            allocation_policy: auxiliary_allocation,
            previous_block: loaded.first_auxiliary_block.clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len: SMW_US_V1_PRIMARY_MAP16_AUXILIARY_BYTES,
            erase_fill: options.erase_fill,
        });
        request_kinds.push(RequestKind::FirstAuxiliary);
    }

    let high_blocks_present = definitions[4 * SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES / 2..]
        .iter()
        .any(|word| *word != BLANK_MAP16_WORD);
    if high_blocks_present
        && (acts_like[0x4000..] != loaded.acts_like[0x4000..]
            || loaded.second_auxiliary_block.is_none())
    {
        let mut auxiliary_allocation = allocation.clone();
        // A complete `$8000` payload plus its header necessarily crosses a LoROM allocation-bank
        // boundary. The runtime uses long indexed reads, so the payload remains relocatable.
        auxiliary_allocation.bank_size = None;
        requests.push(PayloadSaveRequest {
            description: "save primary Map16 second auxiliary table".into(),
            payload: words_to_bytes(&acts_like[0x4000..]),
            pointer: PayloadPointer::DisplacedContiguous {
                offset: SMW_US_V1_PRIMARY_MAP16_SECOND_AUXILIARY_POINTER_OFFSET,
                displacement: SECOND_AUXILIARY_DISPLACEMENT,
                low_bank: true,
            },
            mapper: Mapper::LoRom,
            allocation_policy: auxiliary_allocation,
            previous_block: None,
            reuse_identical: options.reuse_identical,
            maximum_payload_len: SMW_US_V1_PRIMARY_MAP16_AUXILIARY_BYTES,
            erase_fill: options.erase_fill,
        });
        request_kinds.push(RequestKind::SecondAuxiliary);
    } else if !high_blocks_present && loaded.second_auxiliary_block.is_some() {
        writes.push(RomWrite {
            offset: SMW_US_V1_PRIMARY_MAP16_SECOND_AUXILIARY_POINTER_OFFSET,
            bytes: SECOND_AUXILIARY_SENTINEL.to_le_bytes()[..3].to_vec(),
        });
    }

    for block_index in 0..SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT {
        if !block_changed(definitions, &loaded.definitions, block_index) {
            continue;
        }
        let words_per_block = SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES / 2;
        let first_word = block_index * words_per_block;
        let words = &definitions[first_word..first_word + words_per_block];
        let retained = words
            .iter()
            .rposition(|word| *word != BLANK_MAP16_WORD)
            .map_or(0, |last| (last + 1) * 2)
            .next_multiple_of(8);
        let source_start =
            usize::from(block_index == 0) * SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES;
        if retained <= source_start {
            writes.extend(sentinel_writes(block_index));
            continue;
        }
        let payload = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .skip(source_start)
            .take(retained - source_start)
            .collect::<Vec<_>>();
        let mut block_allocation = allocation.clone();
        if payload.len() == SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES {
            block_allocation.bank_size = None;
        }
        requests.push(PayloadSaveRequest {
            description: format!("save primary Map16 block {block_index}"),
            payload,
            pointer: PayloadPointer::DisplacedWordAndBank {
                low_word_offset: SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE
                    + LOW_WORD_OFFSETS[block_index],
                bank_offset: SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE + BANK_BYTE_OFFSETS[block_index],
                displacement: DISPLACEMENTS[block_index],
                low_bank: true,
            },
            mapper: Mapper::LoRom,
            allocation_policy: block_allocation,
            previous_block: loaded.blocks[block_index].clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len: SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES - source_start,
            erase_fill: options.erase_fill,
        });
        request_kinds.push(RequestKind::Block(block_index));
    }

    let results = project.save_tagged_payloads_with_checksum_and_writes(
        "save complete primary Map16 definitions",
        &requests,
        &writes,
        checksum_field,
    )?;
    let mut blocks = std::array::from_fn(|_| None);
    let mut first_auxiliary = None;
    let mut second_auxiliary = None;
    for (kind, result) in request_kinds.into_iter().zip(results) {
        match kind {
            RequestKind::Block(block) => blocks[block] = Some(result),
            RequestKind::FirstAuxiliary => first_auxiliary = Some(result),
            RequestKind::SecondAuxiliary => second_auxiliary = Some(result),
        }
    }
    Ok(SavedSmwUsV1PrimaryMap16 {
        blocks,
        first_auxiliary,
        second_auxiliary,
    })
}

#[derive(Clone, Copy)]
enum RequestKind {
    Block(usize),
    FirstAuxiliary,
    SecondAuxiliary,
}

fn blank_words(len: usize) -> Vec<u16> {
    vec![BLANK_MAP16_WORD; len]
}

fn initial_definition_bytes(transferred: &[u16], installed: bool) -> Vec<u8> {
    let mut definitions = blank_words(SMW_US_V1_PRIMARY_MAP16_DEFINITION_WORDS)
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let transferred = transferred
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();
    let retained = if installed {
        SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES
    } else {
        transferred.len()
    };
    definitions[..retained].copy_from_slice(&transferred[..retained]);
    definitions
}

fn default_acts_like() -> Vec<u16> {
    let mut acts_like = vec![BLANK_ACTS_LIKE_WORD; SMW_US_V1_PRIMARY_MAP16_ACTS_LIKE_WORDS];
    for (tile, word) in acts_like[..0x200].iter_mut().enumerate() {
        *word = u16::try_from(tile).expect("0x200-entry identity table");
    }
    acts_like
}

fn words_to_bytes(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn block_changed(definitions: &[u16], loaded: &[u16], block: usize) -> bool {
    let words = SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES / 2;
    let first = block * words;
    let compare_first = if block == 0 {
        first + SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES / 2
    } else {
        first
    };
    definitions[compare_first..first + words] != loaded[compare_first..first + words]
}

fn resolved_block_pointer(bytes: &[u8], block: usize) -> Result<u32, SmwUsV1PrimaryMap16Error> {
    let low_offset = SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE + LOW_WORD_OFFSETS[block];
    let bank_offset = SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE + BANK_BYTE_OFFSETS[block];
    let low = u16::from_le_bytes([
        *bytes.get(low_offset).ok_or(RomError::RangeOutOfBounds {
            offset: low_offset,
            len: 2,
            image_len: bytes.len(),
        })?,
        *bytes
            .get(low_offset + 1)
            .ok_or(RomError::RangeOutOfBounds {
                offset: low_offset,
                len: 2,
                image_len: bytes.len(),
            })?,
    ]);
    let bank = *bytes.get(bank_offset).ok_or(RomError::RangeOutOfBounds {
        offset: bank_offset,
        len: 1,
        image_len: bytes.len(),
    })?;
    Ok(u32::from(low.wrapping_add(DISPLACEMENTS[block])) | (u32::from(bank) << 16))
}

fn load_second_auxiliary_block(
    bytes: &[u8],
) -> Result<Option<RatsBlock>, SmwUsV1PrimaryMap16Error> {
    let encoded = bytes
        .get(
            SMW_US_V1_PRIMARY_MAP16_SECOND_AUXILIARY_POINTER_OFFSET
                ..SMW_US_V1_PRIMARY_MAP16_SECOND_AUXILIARY_POINTER_OFFSET + 3,
        )
        .ok_or(RomError::RangeOutOfBounds {
            offset: SMW_US_V1_PRIMARY_MAP16_SECOND_AUXILIARY_POINTER_OFFSET,
            len: 3,
            image_len: bytes.len(),
        })?;
    let pointer = u32::from_le_bytes([encoded[0], encoded[1], encoded[2], 0]);
    if pointer == SECOND_AUXILIARY_SENTINEL {
        return Ok(None);
    }
    let pointer_low = u16::from_le_bytes([encoded[0], encoded[1]]);
    let resolved =
        (pointer & 0xff_0000) | u32::from(pointer_low.wrapping_add(SECOND_AUXILIARY_DISPLACEMENT));
    Ok(Some(load_auxiliary_block(bytes, resolved)?))
}

fn direct_pointer(bytes: &[u8], offset: usize) -> Result<u32, SmwUsV1PrimaryMap16Error> {
    let encoded = bytes
        .get(offset..offset + 3)
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len: 3,
            image_len: bytes.len(),
        })?;
    Ok(u32::from_le_bytes([encoded[0], encoded[1], encoded[2], 0]))
}

fn load_auxiliary_block(bytes: &[u8], pointer: u32) -> Result<RatsBlock, SmwUsV1PrimaryMap16Error> {
    let payload_offset = snes_to_pc(Mapper::LoRom, pointer)?;
    let header_offset = payload_offset.checked_sub(HEADER_LEN).ok_or(
        SmwUsV1PrimaryMap16Error::AuxiliaryPointerBeforeRatsHeader(payload_offset),
    )?;
    let block =
        parse_at(bytes, header_offset).map_err(SmwUsV1PrimaryMap16Error::AuxiliaryHeader)?;
    if block.payload.start != payload_offset {
        return Err(SmwUsV1PrimaryMap16Error::AuxiliaryPointerNotTagged(
            payload_offset,
        ));
    }
    if block.payload.len() != SMW_US_V1_PRIMARY_MAP16_AUXILIARY_BYTES {
        return Err(SmwUsV1PrimaryMap16Error::AuxiliaryLength(
            block.payload.len(),
        ));
    }
    Ok(block)
}

fn copy_auxiliary_words(bytes: &[u8], block: &RatsBlock, destination: &mut [u16]) {
    for (word, source) in destination
        .iter_mut()
        .zip(bytes[block.payload.clone()].chunks_exact(2))
    {
        *word = u16::from_le_bytes([source[0], source[1]]);
    }
}

fn sentinel_writes(block: usize) -> [RomWrite; 2] {
    [
        RomWrite {
            offset: SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE + LOW_WORD_OFFSETS[block],
            bytes: 0_u16
                .wrapping_sub(DISPLACEMENTS[block])
                .to_le_bytes()
                .to_vec(),
        },
        RomWrite {
            offset: SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE + BANK_BYTE_OFFSETS[block],
            bytes: vec![0],
        },
    ]
}

fn protect_runtime_fields(
    allocation: &mut AllocationPolicy,
    checksum_field: usize,
    image_len: usize,
) -> Result<(), RomError> {
    let checksum_end = checksum_field
        .checked_add(4)
        .ok_or(RomError::RangeOutOfBounds {
            offset: checksum_field,
            len: 4,
            image_len,
        })?;
    let mut ranges = Vec::with_capacity(SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT * 2 + 3);
    for block in 0..SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT {
        ranges.push(
            SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE + LOW_WORD_OFFSETS[block]
                ..SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE + LOW_WORD_OFFSETS[block] + 2,
        );
        ranges.push(
            SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE + BANK_BYTE_OFFSETS[block]
                ..SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE + BANK_BYTE_OFFSETS[block] + 1,
        );
    }
    ranges.push(
        SMW_US_V1_PRIMARY_MAP16_FIRST_AUXILIARY_POINTER_OFFSET
            ..SMW_US_V1_PRIMARY_MAP16_FIRST_AUXILIARY_POINTER_OFFSET + 3,
    );
    ranges.push(
        SMW_US_V1_PRIMARY_MAP16_SECOND_AUXILIARY_POINTER_OFFSET
            ..SMW_US_V1_PRIMARY_MAP16_SECOND_AUXILIARY_POINTER_OFFSET + 3,
    );
    ranges.push(checksum_field..checksum_end);
    for range in ranges {
        if !allocation
            .protected
            .iter()
            .any(|protected| protected.0.start <= range.start && range.end <= protected.0.end)
        {
            allocation.protected.push(ProtectedRange(range));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smw_us_v1_map16_runtime_installation_plan;
    use lm_rom::{RomImage, compute_snes_checksum};

    fn options() -> SmwUsV1PrimaryMap16SaveOptions {
        SmwUsV1PrimaryMap16SaveOptions {
            allocation: AllocationPolicy {
                search: 0x80_000..0x10_0000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0, 0xff],
                protected: vec![ProtectedRange(0x7fdc..0x7fe0)],
            },
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    fn installed_project() -> Project {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
        let plan =
            smw_us_v1_map16_runtime_installation_plan(&original, options().allocation, 0x7fdc)
                .unwrap();
        project.install_relocatable_patch(&plan).unwrap();
        project
    }

    #[test]
    fn installed_control_has_full_baseline_and_no_primary_overlays() {
        let project = installed_project();
        let transferred = load_smw_us_v1_transferred_map16(&project).unwrap();
        let loaded = load_smw_us_v1_primary_map16(&project).unwrap();
        assert!(loaded.installed);
        assert_eq!(
            loaded.definitions.len(),
            SMW_US_V1_PRIMARY_MAP16_DEFINITION_WORDS
        );
        let first_overlay_word = SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES / 2;
        assert_ne!(
            transferred.definitions[first_overlay_word],
            BLANK_MAP16_WORD
        );
        assert_eq!(loaded.definitions[first_overlay_word], BLANK_MAP16_WORD);
        assert_eq!(loaded.acts_like, default_acts_like());
        assert!(loaded.blocks.iter().all(Option::is_none));
        assert!(loaded.first_auxiliary_block.is_some());
        assert!(loaded.second_auxiliary_block.is_none());
    }

    #[test]
    fn tile_0800_uses_the_authenticated_3008_byte_overlay() {
        let mut project = installed_project();
        let loaded = load_smw_us_v1_primary_map16(&project).unwrap();
        let mut definitions = loaded.definitions;
        let acts_like = loaded.acts_like;
        definitions[0x800 * 4..0x800 * 4 + 4].copy_from_slice(&[1, 2, 3, 4]);
        let saved = save_smw_us_v1_primary_map16(
            &mut project,
            &definitions,
            &acts_like,
            0x7fdc,
            &options(),
        )
        .unwrap();
        assert_eq!(
            saved.blocks[0].as_ref().unwrap().block.payload.len(),
            0x3008
        );
        assert_eq!(
            load_smw_us_v1_primary_map16(&project).unwrap().definitions,
            definitions
        );
        let checksum = compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap();
        assert_eq!(
            &project.rom.logical_bytes()[0x7fdc..0x7fe0],
            checksum.encoded()
        );
    }

    #[test]
    fn high_block_materializes_second_auxiliary_and_reopens() {
        let mut project = installed_project();
        let loaded = load_smw_us_v1_primary_map16(&project).unwrap();
        let mut definitions = loaded.definitions;
        let acts_like = loaded.acts_like;
        definitions[0x4000 * 4..0x4000 * 4 + 4].copy_from_slice(&[1, 2, 3, 4]);
        let saved = save_smw_us_v1_primary_map16(
            &mut project,
            &definitions,
            &acts_like,
            0x7fdc,
            &options(),
        )
        .unwrap();
        assert_eq!(saved.blocks[4].as_ref().unwrap().block.payload.len(), 8);
        assert_eq!(
            saved.second_auxiliary.as_ref().unwrap().block.payload.len(),
            SMW_US_V1_PRIMARY_MAP16_AUXILIARY_BYTES
        );
        let reopened = load_smw_us_v1_primary_map16(&project).unwrap();
        assert_eq!(reopened.definitions, definitions);
        assert!(reopened.second_auxiliary_block.is_some());
    }

    #[test]
    fn wine_single_tile_matrix_has_exact_trimmed_payload_lengths() {
        for (tile, block, expected_len) in [
            (0x1000, 1, 0x0008),
            (0x2000, 2, 0x0008),
            (0x4000, 4, 0x0008),
            (0x7fff, 7, 0x8000),
        ] {
            let mut project = installed_project();
            let loaded = load_smw_us_v1_primary_map16(&project).unwrap();
            let mut definitions = loaded.definitions;
            let acts_like = loaded.acts_like;
            definitions[tile * 4..tile * 4 + 4].copy_from_slice(&[1, 2, 3, 4]);

            let saved = save_smw_us_v1_primary_map16(
                &mut project,
                &definitions,
                &acts_like,
                0x7fdc,
                &options(),
            )
            .unwrap();

            assert_eq!(
                saved.blocks[block].as_ref().unwrap().block.payload.len(),
                expected_len,
                "tile {tile:04x}"
            );
            assert_eq!(
                load_smw_us_v1_primary_map16(&project).unwrap().definitions,
                definitions,
                "tile {tile:04x}"
            );
        }
    }

    #[test]
    fn acts_like_changes_relocate_the_exact_raw_auxiliary_half() {
        let mut project = installed_project();
        let loaded = load_smw_us_v1_primary_map16(&project).unwrap();
        let definitions = loaded.definitions;
        let mut acts_like = loaded.acts_like;
        acts_like[0x1000] = 0x0123;

        let saved = save_smw_us_v1_primary_map16(
            &mut project,
            &definitions,
            &acts_like,
            0x7fdc,
            &options(),
        )
        .unwrap();

        assert_eq!(
            saved.first_auxiliary.as_ref().unwrap().block.payload.len(),
            0x8000
        );
        assert!(saved.second_auxiliary.is_none());
        let reopened = load_smw_us_v1_primary_map16(&project).unwrap();
        assert_eq!(reopened.acts_like, acts_like);
        assert_eq!(reopened.definitions, definitions);
    }
}
