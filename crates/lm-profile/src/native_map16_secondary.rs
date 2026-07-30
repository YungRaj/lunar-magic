//! Lunar Magic's full background Map16 definition namespace for SMW US revision 0.

use lm_project::{
    PayloadPointer, PayloadSaveError, PayloadSaveRequest, PayloadSaveResult, Project, RomWrite,
};
use lm_rats::{AllocationPolicy, HEADER_LEN, HeaderError, ProtectedRange, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, pc_to_snes, snes_to_pc};
use std::fmt;

pub const SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET: usize = 0x77d50;
pub const SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET: usize = 0x28da4;
pub const SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET: usize = 0x69100;
pub const SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_BYTES: usize = 0x1000;
pub const SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT: usize = 8;
pub const SMW_US_V1_SECONDARY_MAP16_BLOCK_BYTES: usize = 0x8000;
pub const SMW_US_V1_SECONDARY_MAP16_DEFINITION_WORDS: usize =
    SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT * SMW_US_V1_SECONDARY_MAP16_BLOCK_BYTES / 2;
const BLANK_MAP16_WORD: u16 = 0x1004;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1SecondaryMap16 {
    /// Four words per definition for background tiles `$8000-$ffff`.
    pub definitions: Vec<u16>,
    /// True when Lunar Magic's eight-pointer secondary runtime is installed.
    pub installed: bool,
    /// Exact owned blocks reached by installed non-fixed pointers.
    pub blocks: [Option<RatsBlock>; SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1SecondaryMap16SaveOptions {
    pub allocation: AllocationPolicy,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedSmwUsV1SecondaryMap16 {
    pub blocks: [Option<PayloadSaveResult>; SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT],
    pub fixed_first_block: bool,
}

#[derive(Debug)]
pub enum SmwUsV1SecondaryMap16Error {
    Rom(RomError),
    Header { block: usize, source: HeaderError },
    PointerBeforeRatsHeader { block: usize, offset: usize },
    PointerNotTagged { block: usize, offset: usize },
    BlockTooLarge { block: usize, len: usize },
    BlockNotWordAligned { block: usize, len: usize },
    RuntimeNotInstalled,
    DefinitionWordCount(usize),
    Save(PayloadSaveError),
}

impl fmt::Display for SmwUsV1SecondaryMap16Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot load SMW US secondary Map16 definitions: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1SecondaryMap16Error {}

impl From<RomError> for SmwUsV1SecondaryMap16Error {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<PayloadSaveError> for SmwUsV1SecondaryMap16Error {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

/// Loads Lunar Magic's complete `$8000-$ffff` background definition namespace.
///
/// A pristine ROM contributes its fixed first `0x1000` bytes and blank-fills the rest. Once the
/// full runtime is installed, its eight packed low-bank pointers select independently trimmed,
/// RATS-protected `0x8000`-byte blocks. Zero pointers denote all-blank blocks.
///
/// # Errors
///
/// Rejects invalid pointers, malformed or non-exact RATS ownership, blocks over `0x8000` bytes,
/// odd payload lengths, and ROM ranges outside the image.
pub fn load_smw_us_v1_secondary_map16(
    project: &Project,
) -> Result<LoadedSmwUsV1SecondaryMap16, SmwUsV1SecondaryMap16Error> {
    let bytes = project.rom.logical_bytes();
    let mut definition_bytes =
        vec![0; SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT * SMW_US_V1_SECONDARY_MAP16_BLOCK_BYTES];
    for word in definition_bytes.chunks_exact_mut(2) {
        word.copy_from_slice(&BLANK_MAP16_WORD.to_le_bytes());
    }
    let installed = bytes
        .get(SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET)
        .copied()
        == Some(0x22);
    let mut blocks: [Option<RatsBlock>; SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT] =
        std::array::from_fn(|_| None);
    if installed {
        let pointers = project.rom.read(
            SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET,
            SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT * 3,
        )?;
        for (block_index, owned_block) in blocks.iter_mut().enumerate() {
            let pointer_offset = block_index * 3;
            let pointer = u32::from_le_bytes([
                pointers[pointer_offset],
                pointers[pointer_offset + 1],
                pointers[pointer_offset + 2],
                0,
            ]);
            if pointer == 0 {
                continue;
            }
            let payload_offset = snes_to_pc(Mapper::LoRom, pointer)?;
            let payload = if block_index == 0
                && payload_offset == SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET
            {
                project.rom.read(
                    SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET,
                    SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_BYTES,
                )?
            } else {
                let header_offset = payload_offset.checked_sub(HEADER_LEN).ok_or(
                    SmwUsV1SecondaryMap16Error::PointerBeforeRatsHeader {
                        block: block_index,
                        offset: payload_offset,
                    },
                )?;
                let block = parse_at(bytes, header_offset).map_err(|source| {
                    SmwUsV1SecondaryMap16Error::Header {
                        block: block_index,
                        source,
                    }
                })?;
                if block.payload.start != payload_offset {
                    return Err(SmwUsV1SecondaryMap16Error::PointerNotTagged {
                        block: block_index,
                        offset: payload_offset,
                    });
                }
                *owned_block = Some(block.clone());
                &bytes[block.payload]
            };
            if payload.len() > SMW_US_V1_SECONDARY_MAP16_BLOCK_BYTES {
                return Err(SmwUsV1SecondaryMap16Error::BlockTooLarge {
                    block: block_index,
                    len: payload.len(),
                });
            }
            if payload.len() % 2 != 0 {
                return Err(SmwUsV1SecondaryMap16Error::BlockNotWordAligned {
                    block: block_index,
                    len: payload.len(),
                });
            }
            let destination = block_index * SMW_US_V1_SECONDARY_MAP16_BLOCK_BYTES;
            definition_bytes[destination..destination + payload.len()].copy_from_slice(payload);
        }
    } else {
        let fixed = project.rom.read(
            SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET,
            SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_BYTES,
        )?;
        definition_bytes[..fixed.len()].copy_from_slice(fixed);
    }
    Ok(LoadedSmwUsV1SecondaryMap16 {
        definitions: definition_bytes
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect(),
        installed,
        blocks,
    })
}

/// Saves all eight installed secondary blocks with Lunar Magic's exact trimming rules.
///
/// Each block is trimmed after its last word other than `0x1004`, then rounded to eight bytes.
/// Block zero remains in vanilla fixed storage when it fits within `0x1000` bytes; larger block
/// zero data and every nonempty later block use independently allocated RATS payloads. Pointer
/// updates, fixed writes, and checksum repair publish as one undoable transaction.
///
/// This operation requires the full Lunar Magic runtime to already be installed. Installing its
/// executable hooks is a separate revision-specific operation.
///
/// # Errors
///
/// Rejects an absent runtime, a definition table other than exactly 32,768 four-word entries, and
/// any allocation, pointer, protected-write, mapper, or checksum failure without mutation.
#[allow(clippy::too_many_lines)]
pub fn save_smw_us_v1_secondary_map16(
    project: &mut Project,
    definitions: &[u16],
    checksum_field: usize,
    options: &SmwUsV1SecondaryMap16SaveOptions,
) -> Result<SavedSmwUsV1SecondaryMap16, SmwUsV1SecondaryMap16Error> {
    if project
        .rom
        .logical_bytes()
        .get(SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET)
        .copied()
        != Some(0x22)
    {
        return Err(SmwUsV1SecondaryMap16Error::RuntimeNotInstalled);
    }
    if definitions.len() != SMW_US_V1_SECONDARY_MAP16_DEFINITION_WORDS {
        return Err(SmwUsV1SecondaryMap16Error::DefinitionWordCount(
            definitions.len(),
        ));
    }
    let loaded = load_smw_us_v1_secondary_map16(project)?;
    let mut allocation = options.allocation.clone();
    let checksum_end = checksum_field
        .checked_add(4)
        .ok_or(RomError::RangeOutOfBounds {
            offset: checksum_field,
            len: 4,
            image_len: project.rom.logical_len(),
        })?;
    for range in [
        SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET
            ..SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET
                + SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT * 3,
        SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET
            ..SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET
                + SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_BYTES,
        checksum_field..checksum_end,
    ] {
        if !allocation
            .protected
            .iter()
            .any(|protected| protected.0.start <= range.start && range.end <= protected.0.end)
        {
            allocation.protected.push(ProtectedRange(range));
        }
    }
    let mut requests = Vec::new();
    let mut request_blocks = Vec::new();
    let mut writes = Vec::new();
    let mut fixed_first_block = false;
    for block_index in 0..SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT {
        let first_word = block_index * SMW_US_V1_SECONDARY_MAP16_BLOCK_BYTES / 2;
        let words =
            &definitions[first_word..first_word + SMW_US_V1_SECONDARY_MAP16_BLOCK_BYTES / 2];
        let retained = words
            .iter()
            .rposition(|word| *word != BLANK_MAP16_WORD)
            .map_or(0, |last| (last + 1) * 2)
            .next_multiple_of(8);
        let pointer_offset = SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET + block_index * 3;
        if retained == 0 {
            writes.push(RomWrite {
                offset: pointer_offset,
                bytes: vec![0; 3],
            });
            continue;
        }
        let payload = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(retained)
            .collect::<Vec<_>>();
        if block_index == 0 && retained <= SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_BYTES {
            let mut pointer =
                pc_to_snes(Mapper::LoRom, SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET)?
                    .to_le_bytes();
            pointer[2] &= 0x7f;
            writes.push(RomWrite {
                offset: pointer_offset,
                bytes: pointer[..3].to_vec(),
            });
            let mut fixed_payload = Vec::with_capacity(SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_BYTES);
            for _ in 0..SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_BYTES / 2 {
                fixed_payload.extend_from_slice(&BLANK_MAP16_WORD.to_le_bytes());
            }
            fixed_payload[..payload.len()].copy_from_slice(&payload);
            writes.push(RomWrite {
                offset: SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET,
                bytes: fixed_payload,
            });
            fixed_first_block = true;
            continue;
        }
        requests.push(PayloadSaveRequest {
            description: format!("save secondary Map16 block {block_index}"),
            payload,
            pointer: PayloadPointer::contiguous_low_bank(pointer_offset),
            mapper: Mapper::LoRom,
            allocation_policy: allocation.clone(),
            previous_block: loaded.blocks[block_index].clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len: SMW_US_V1_SECONDARY_MAP16_BLOCK_BYTES,
            erase_fill: options.erase_fill,
        });
        request_blocks.push(block_index);
    }
    let results = project.save_tagged_payloads_with_checksum_and_writes(
        "save complete secondary Map16 definitions",
        &requests,
        &writes,
        checksum_field,
    )?;
    let mut blocks: [Option<PayloadSaveResult>; SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT] =
        std::array::from_fn(|_| None);
    for (block, result) in request_blocks.into_iter().zip(results) {
        blocks[block] = Some(result);
    }
    Ok(SavedSmwUsV1SecondaryMap16 {
        blocks,
        fixed_first_block,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::make_header;
    use lm_rom::{RomImage, pc_to_snes};

    fn project(mut bytes: Vec<u8>) -> Project {
        bytes.resize(bytes.len().max(0x80_000), 0xff);
        Project::new(RomImage::from_bytes(bytes).unwrap())
    }

    #[test]
    fn pristine_rom_uses_fixed_first_block_and_blank_fills_the_rest() {
        let mut bytes = vec![0xff; 0x80_000];
        bytes[SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET
            ..SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET + 4]
            .copy_from_slice(&[0x11, 0x11, 0x22, 0x22]);
        let loaded = load_smw_us_v1_secondary_map16(&project(bytes)).unwrap();
        assert!(!loaded.installed);
        assert_eq!(loaded.definitions[..2], [0x1111, 0x2222]);
        assert_eq!(
            loaded.definitions[SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_BYTES / 2],
            BLANK_MAP16_WORD
        );
        assert!(loaded.blocks.iter().all(Option::is_none));
    }

    #[test]
    fn installed_runtime_loads_trimmed_low_bank_rats_block() {
        let payload_offset = 0x80_008;
        let payload = [0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44];
        let mut bytes = vec![0xff; 0x90_000];
        bytes[SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET] = 0x22;
        bytes[0x80_000..payload_offset].copy_from_slice(&make_header(payload.len()).unwrap());
        bytes[payload_offset..payload_offset + payload.len()].copy_from_slice(&payload);
        let mut pointer = pc_to_snes(Mapper::LoRom, payload_offset)
            .unwrap()
            .to_le_bytes();
        pointer[2] &= 0x7f;
        bytes[SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET
            ..SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET + 3]
            .copy_from_slice(&pointer[..3]);
        bytes[SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET + 3
            ..SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET
                + SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT * 3]
            .fill(0);

        let loaded = load_smw_us_v1_secondary_map16(&project(bytes)).unwrap();
        assert!(loaded.installed);
        assert_eq!(loaded.definitions[..4], [0x1111, 0x2222, 0x3333, 0x4444]);
        assert_eq!(loaded.definitions[4], BLANK_MAP16_WORD);
        assert_eq!(
            loaded.blocks[0].as_ref().unwrap().payload,
            payload_offset..payload_offset + 8
        );
        assert!(loaded.blocks[1..].iter().all(Option::is_none));
    }

    #[test]
    fn installed_runtime_rejects_payload_larger_than_one_block() {
        let payload_offset = 0x80_008;
        let payload_len = SMW_US_V1_SECONDARY_MAP16_BLOCK_BYTES + 8;
        let mut bytes = vec![0xff; payload_offset + payload_len];
        bytes[SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET] = 0x22;
        bytes[0x80_000..payload_offset].copy_from_slice(&make_header(payload_len).unwrap());
        bytes[payload_offset..payload_offset + payload_len].fill(0);
        let mut pointer = pc_to_snes(Mapper::LoRom, payload_offset)
            .unwrap()
            .to_le_bytes();
        pointer[2] &= 0x7f;
        bytes[SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET
            ..SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET + 3]
            .copy_from_slice(&pointer[..3]);
        bytes[SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET + 3
            ..SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET
                + SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT * 3]
            .fill(0);
        assert!(matches!(
            load_smw_us_v1_secondary_map16(&project(bytes)),
            Err(SmwUsV1SecondaryMap16Error::BlockTooLarge {
                block: 0,
                len
            }) if len == payload_len
        ));
    }

    fn installed_blank_project() -> Project {
        let mut bytes = vec![0xff; 0x10_0000];
        bytes[SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET] = 0x22;
        bytes[SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET
            ..SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET
                + SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT * 3]
            .fill(0);
        project(bytes)
    }

    fn save_options() -> SmwUsV1SecondaryMap16SaveOptions {
        SmwUsV1SecondaryMap16SaveOptions {
            allocation: AllocationPolicy {
                search: 0x80_000..0x10_0000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![],
            },
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    #[test]
    fn save_uses_fixed_first_block_when_trimmed_data_fits() {
        let mut project = installed_blank_project();
        let original = project.save_snapshot();
        let mut definitions = vec![BLANK_MAP16_WORD; SMW_US_V1_SECONDARY_MAP16_DEFINITION_WORDS];
        definitions[..4].copy_from_slice(&[1, 2, 3, 4]);
        let saved =
            save_smw_us_v1_secondary_map16(&mut project, &definitions, 0x7fdc, &save_options())
                .unwrap();
        assert!(saved.fixed_first_block);
        assert!(saved.blocks.iter().all(Option::is_none));
        assert_eq!(
            load_smw_us_v1_secondary_map16(&project)
                .unwrap()
                .definitions,
            definitions
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn save_allocates_trimmed_block_zero_beyond_fixed_capacity() {
        let mut project = installed_blank_project();
        let mut definitions = vec![BLANK_MAP16_WORD; SMW_US_V1_SECONDARY_MAP16_DEFINITION_WORDS];
        let tile_8200_word = 0x200 * 4;
        definitions[tile_8200_word..tile_8200_word + 4].copy_from_slice(&[1, 2, 3, 4]);
        let saved =
            save_smw_us_v1_secondary_map16(&mut project, &definitions, 0x7fdc, &save_options())
                .unwrap();
        assert!(!saved.fixed_first_block);
        assert_eq!(
            saved.blocks[0].as_ref().unwrap().block.payload.len(),
            0x1008
        );
        assert_eq!(
            load_smw_us_v1_secondary_map16(&project)
                .unwrap()
                .definitions,
            definitions
        );
    }

    #[test]
    fn malformed_definition_shape_and_missing_runtime_are_atomic() {
        let mut project = installed_blank_project();
        let original = project.save_snapshot();
        assert!(matches!(
            save_smw_us_v1_secondary_map16(&mut project, &[], 0x7fdc, &save_options()),
            Err(SmwUsV1SecondaryMap16Error::DefinitionWordCount(0))
        ));
        assert_eq!(project.save_snapshot(), original);
        project
            .rom
            .write(SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET, &[0])
            .unwrap();
        let before = project.save_snapshot();
        assert!(matches!(
            save_smw_us_v1_secondary_map16(
                &mut project,
                &vec![BLANK_MAP16_WORD; SMW_US_V1_SECONDARY_MAP16_DEFINITION_WORDS],
                0x7fdc,
                &save_options()
            ),
            Err(SmwUsV1SecondaryMap16Error::RuntimeNotInstalled)
        ));
        assert_eq!(project.save_snapshot(), before);
    }
}
