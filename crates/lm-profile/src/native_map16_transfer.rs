//! Lunar Magic's installed SMW US revision-0 Map16 transfer tables.

use lm_codec::{
    CodecError, decode_sized_rle_prefix, decode_terminated_rle_prefix, encode_sized_rle,
    encode_terminated_rle,
};
use lm_project::{
    PayloadPointer, PayloadSaveError, PayloadSaveRequest, PayloadSaveResult, Project, RomWrite,
};
use lm_rats::{AllocationPolicy, HEADER_LEN, HeaderError, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, pc_to_snes, snes_to_pc};
use std::fmt;

pub const SMW_US_V1_MAP16_DEFINITION_WORD_OFFSET: usize = 0x25c72;
pub const SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET: usize = 0x25c79;
pub const SMW_US_V1_MAP16_DEFINITION_ODD_WORD_OFFSET: usize = 0x25c8d;
pub const SMW_US_V1_MAP16_ACTS_HIGH_WORD_OFFSET: usize = 0x25d45;
pub const SMW_US_V1_MAP16_ACTS_HIGH_BANK_OFFSET: usize = 0x25d4a;
pub const SMW_US_V1_MAP16_ACTS_LOW_WORD_OFFSET: usize = 0x264bb;
pub const SMW_US_V1_MAP16_ACTS_LOW_BANK_OFFSET: usize = 0x264b0;
pub const SMW_US_V1_MAP16_DEFINITION_BYTES: usize = 0x4000;
pub const SMW_US_V1_MAP16_MAX_ENTRIES: usize = 0x4000;
pub const SMW_US_V1_MAP16_DEFAULT_ACTS_LIKE: u16 = 0xfc7a;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1TransferredMap16 {
    pub definitions: Vec<u16>,
    pub acts_like: Vec<u16>,
    pub definition_block: Option<RatsBlock>,
    pub acts_low_block: Option<RatsBlock>,
    pub acts_high_block: Option<RatsBlock>,
    pub definition_odd_stream_offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1TransferredMap16SaveOptions {
    pub allocation: AllocationPolicy,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedSmwUsV1TransferredMap16 {
    pub definitions: PayloadSaveResult,
    pub acts_low: PayloadSaveResult,
    pub acts_high: PayloadSaveResult,
}

#[derive(Debug)]
pub enum SmwUsV1TransferredMap16Error {
    Rom(RomError),
    Header(HeaderError),
    Codec(CodecError),
    PointerNotTagged(usize),
    StreamPointerMismatch { expected: usize, actual: usize },
    ActsPlaneLengthMismatch { low: usize, high: usize },
    TooManyActsLikeEntries(usize),
    DefinitionWordCount(usize),
    DefinitionByteLengthOverflow,
    DefinitionOddPointerBank { even: u32, odd: u32 },
    EmptyActsLike,
    Save(PayloadSaveError),
}

impl fmt::Display for SmwUsV1TransferredMap16Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "transferred SMW Map16 table load failed: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1TransferredMap16Error {}

impl From<RomError> for SmwUsV1TransferredMap16Error {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<HeaderError> for SmwUsV1TransferredMap16Error {
    fn from(value: HeaderError) -> Self {
        Self::Header(value)
    }
}

impl From<CodecError> for SmwUsV1TransferredMap16Error {
    fn from(value: CodecError) -> Self {
        Self::Codec(value)
    }
}

impl From<PayloadSaveError> for SmwUsV1TransferredMap16Error {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

/// Loads the exact installed Map16 tables written by Lunar Magic's overworld save.
///
/// # Errors
///
/// Rejects invalid split pointers, unowned payloads, malformed RLE, inconsistent plane lengths,
/// and tables exceeding Lunar Magic's recovered 16K-entry bound.
pub fn load_smw_us_v1_transferred_map16(
    project: &Project,
) -> Result<LoadedSmwUsV1TransferredMap16, SmwUsV1TransferredMap16Error> {
    let bytes = project.rom.logical_bytes();
    let definition_offset = split_pointer(
        bytes,
        SMW_US_V1_MAP16_DEFINITION_WORD_OFFSET,
        SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET,
    )?;
    let odd_offset = split_pointer(
        bytes,
        SMW_US_V1_MAP16_DEFINITION_ODD_WORD_OFFSET,
        SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET,
    )?;
    let definition_block = optional_exact_block(bytes, definition_offset)?;
    let even = decode_sized_rle_prefix(
        bounded_stream(bytes, definition_offset)?,
        SMW_US_V1_MAP16_DEFINITION_BYTES / 2,
    )?;
    let odd = decode_sized_rle_prefix(
        bounded_stream(bytes, odd_offset)?,
        SMW_US_V1_MAP16_DEFINITION_BYTES / 2,
    )?;
    if let Some(block) = &definition_block {
        let expected_odd = definition_offset + even.consumed;
        if odd_offset != expected_odd {
            return Err(SmwUsV1TransferredMap16Error::StreamPointerMismatch {
                expected: expected_odd,
                actual: odd_offset,
            });
        }
        let expected_end = odd_offset + odd.consumed;
        if block.payload.end != expected_end {
            return Err(SmwUsV1TransferredMap16Error::StreamPointerMismatch {
                expected: block.payload.end,
                actual: expected_end,
            });
        }
    }
    let definition_bytes = interleave(&even.bytes, &odd.bytes);

    let acts_low_offset = split_pointer(
        bytes,
        SMW_US_V1_MAP16_ACTS_LOW_WORD_OFFSET,
        SMW_US_V1_MAP16_ACTS_LOW_BANK_OFFSET,
    )?;
    let acts_high_offset = split_pointer(
        bytes,
        SMW_US_V1_MAP16_ACTS_HIGH_WORD_OFFSET,
        SMW_US_V1_MAP16_ACTS_HIGH_BANK_OFFSET,
    )?;
    let acts_low_block = optional_exact_block(bytes, acts_low_offset)?;
    let acts_high_block = optional_exact_block(bytes, acts_high_offset)?;
    let high = decode_terminated_rle_prefix(
        bounded_stream(bytes, acts_high_offset)?,
        SMW_US_V1_MAP16_MAX_ENTRIES,
    )?;
    if high.bytes.len() > SMW_US_V1_MAP16_MAX_ENTRIES {
        return Err(SmwUsV1TransferredMap16Error::TooManyActsLikeEntries(
            high.bytes.len(),
        ));
    }
    if let Some(block) = &acts_high_block
        && block.payload.len() != high.consumed
    {
        return Err(SmwUsV1TransferredMap16Error::StreamPointerMismatch {
            expected: block.payload.len(),
            actual: high.consumed,
        });
    }
    let low = bytes
        .get(acts_low_offset..acts_low_offset + high.bytes.len())
        .ok_or(RomError::RangeOutOfBounds {
            offset: acts_low_offset,
            len: high.bytes.len(),
            image_len: bytes.len(),
        })?;
    if let Some(block) = &acts_low_block
        && block.payload.len() != low.len()
    {
        return Err(SmwUsV1TransferredMap16Error::ActsPlaneLengthMismatch {
            low: block.payload.len(),
            high: high.bytes.len(),
        });
    }
    let mut acts_like: Vec<_> = low
        .iter()
        .zip(high.bytes)
        .map(|(&low, high)| match u16::from_le_bytes([low, high]) {
            0x0cba => SMW_US_V1_MAP16_DEFAULT_ACTS_LIKE,
            value => value,
        })
        .collect();
    while acts_like.last() == Some(&SMW_US_V1_MAP16_DEFAULT_ACTS_LIKE) {
        acts_like.pop();
    }
    Ok(LoadedSmwUsV1TransferredMap16 {
        definitions: le_words(&definition_bytes),
        acts_like,
        definition_block,
        acts_low_block,
        acts_high_block,
        definition_odd_stream_offset: odd_offset,
    })
}

/// Saves Lunar Magic's installed split-pointer Map16 transfer tables and checksum.
///
/// The definition even/odd streams share one RATS allocation and bank byte. Acts-Like low bytes
/// and terminated-RLE high bytes remain independently allocated, matching the recovered runtime.
///
/// # Errors
///
/// Rejects malformed table shapes, empty Acts-Like state, allocation or mapping failures, and any
/// definition stream pair that cannot share the recovered bank byte.
pub fn save_smw_us_v1_transferred_map16(
    project: &mut Project,
    definitions: &[u16],
    acts_like: &[u16],
    checksum_field: usize,
    options: &SmwUsV1TransferredMap16SaveOptions,
) -> Result<SavedSmwUsV1TransferredMap16, SmwUsV1TransferredMap16Error> {
    let mut staged = project.clone();
    let saved = save_smw_us_v1_transferred_map16_staged(
        &mut staged,
        definitions,
        acts_like,
        checksum_field,
        options,
    )?;
    *project = staged;
    Ok(saved)
}

fn save_smw_us_v1_transferred_map16_staged(
    project: &mut Project,
    definitions: &[u16],
    acts_like: &[u16],
    checksum_field: usize,
    options: &SmwUsV1TransferredMap16SaveOptions,
) -> Result<SavedSmwUsV1TransferredMap16, SmwUsV1TransferredMap16Error> {
    if definitions
        .len()
        .checked_mul(2)
        .ok_or(SmwUsV1TransferredMap16Error::DefinitionByteLengthOverflow)?
        != SMW_US_V1_MAP16_DEFINITION_BYTES
    {
        return Err(SmwUsV1TransferredMap16Error::DefinitionWordCount(
            definitions.len(),
        ));
    }
    if acts_like.is_empty() {
        return Err(SmwUsV1TransferredMap16Error::EmptyActsLike);
    }
    if acts_like.len() > SMW_US_V1_MAP16_MAX_ENTRIES {
        return Err(SmwUsV1TransferredMap16Error::TooManyActsLikeEntries(
            acts_like.len(),
        ));
    }
    let (definition_payload, odd_relative) = encoded_definition_payload(definitions);
    let definitions_saved =
        save_definition_payload(project, definition_payload, checksum_field, options)?;
    let odd_snes = definition_odd_pointer(&definitions_saved, odd_relative)?;
    let saved = save_acts_payloads(project, acts_like, odd_snes, checksum_field, options)?;
    Ok(SavedSmwUsV1TransferredMap16 {
        definitions: definitions_saved,
        acts_low: saved[0].clone(),
        acts_high: saved[1].clone(),
    })
}

fn save_definition_payload(
    project: &mut Project,
    payload: Vec<u8>,
    checksum_field: usize,
    options: &SmwUsV1TransferredMap16SaveOptions,
) -> Result<PayloadSaveResult, SmwUsV1TransferredMap16Error> {
    let mut definition_allocation = options.allocation.clone();
    definition_allocation.bank_size = Some(0x8000);
    Ok(project
        .save_tagged_payloads_with_checksum(
            "save installed SMW Map16 definitions",
            &[PayloadSaveRequest {
                description: "save installed SMW Map16 definitions".into(),
                payload,
                pointer: PayloadPointer::Split {
                    low_word_offset: SMW_US_V1_MAP16_DEFINITION_WORD_OFFSET,
                    bank_offset: SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET,
                    shared_bank: false,
                },
                mapper: Mapper::LoRom,
                allocation_policy: definition_allocation,
                previous_block: None,
                reuse_identical: options.reuse_identical,
                maximum_payload_len: 0x8000,
                erase_fill: options.erase_fill,
            }],
            checksum_field,
        )?
        .remove(0))
}

fn definition_odd_pointer(
    definitions: &PayloadSaveResult,
    odd_relative: usize,
) -> Result<u32, SmwUsV1TransferredMap16Error> {
    let odd_pc = definitions
        .block
        .payload
        .start
        .checked_add(odd_relative)
        .ok_or(SmwUsV1TransferredMap16Error::DefinitionByteLengthOverflow)?;
    let odd_snes = pc_to_snes(Mapper::LoRom, odd_pc)?;
    if (definitions.snes_pointer >> 16) != (odd_snes >> 16) {
        return Err(SmwUsV1TransferredMap16Error::DefinitionOddPointerBank {
            even: definitions.snes_pointer,
            odd: odd_snes,
        });
    }
    Ok(odd_snes)
}

fn save_acts_payloads(
    project: &mut Project,
    acts_like: &[u16],
    odd_snes: u32,
    checksum_field: usize,
    options: &SmwUsV1TransferredMap16SaveOptions,
) -> Result<Vec<PayloadSaveResult>, SmwUsV1TransferredMap16Error> {
    let native_acts = acts_like
        .iter()
        .copied()
        .map(|word| {
            if word == SMW_US_V1_MAP16_DEFAULT_ACTS_LIKE {
                0x0cba
            } else {
                word
            }
        })
        .collect::<Vec<_>>();
    let acts_low = native_acts
        .iter()
        .map(|word| word.to_le_bytes()[0])
        .collect();
    let acts_high_bytes = native_acts
        .iter()
        .map(|word| word.to_le_bytes()[1])
        .collect::<Vec<_>>();
    let acts_high = encode_terminated_rle(&acts_high_bytes);
    let odd_word = odd_snes.to_le_bytes();
    Ok(project.save_tagged_payloads_with_checksum_and_writes(
        "save installed SMW Map16 Acts-Like tables",
        &[
            PayloadSaveRequest {
                description: "save installed SMW Map16 Acts-Like low bytes".into(),
                payload: acts_low,
                pointer: PayloadPointer::Split {
                    low_word_offset: SMW_US_V1_MAP16_ACTS_LOW_WORD_OFFSET,
                    bank_offset: SMW_US_V1_MAP16_ACTS_LOW_BANK_OFFSET,
                    shared_bank: false,
                },
                mapper: Mapper::LoRom,
                allocation_policy: options.allocation.clone(),
                previous_block: None,
                reuse_identical: options.reuse_identical,
                maximum_payload_len: SMW_US_V1_MAP16_MAX_ENTRIES,
                erase_fill: options.erase_fill,
            },
            PayloadSaveRequest {
                description: "save installed SMW Map16 Acts-Like high bytes".into(),
                payload: acts_high,
                pointer: PayloadPointer::Split {
                    low_word_offset: SMW_US_V1_MAP16_ACTS_HIGH_WORD_OFFSET,
                    bank_offset: SMW_US_V1_MAP16_ACTS_HIGH_BANK_OFFSET,
                    shared_bank: false,
                },
                mapper: Mapper::LoRom,
                allocation_policy: options.allocation.clone(),
                previous_block: None,
                reuse_identical: options.reuse_identical,
                maximum_payload_len: 0x8000,
                erase_fill: options.erase_fill,
            },
        ],
        &[RomWrite {
            offset: SMW_US_V1_MAP16_DEFINITION_ODD_WORD_OFFSET,
            bytes: odd_word[..2].to_vec(),
        }],
        checksum_field,
    )?)
}

fn encoded_definition_payload(definitions: &[u16]) -> (Vec<u8>, usize) {
    let definition_bytes = words_to_le_bytes(definitions);
    let even = encode_sized_rle(
        &definition_bytes
            .iter()
            .step_by(2)
            .copied()
            .collect::<Vec<_>>(),
    );
    let odd = encode_sized_rle(
        &definition_bytes
            .iter()
            .skip(1)
            .step_by(2)
            .copied()
            .collect::<Vec<_>>(),
    );
    let odd_relative = even.len();
    let mut payload = even;
    payload.extend_from_slice(&odd);
    (payload, odd_relative)
}

fn words_to_le_bytes(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn split_pointer(
    bytes: &[u8],
    word_offset: usize,
    bank_offset: usize,
) -> Result<usize, SmwUsV1TransferredMap16Error> {
    let word = bytes
        .get(word_offset..word_offset + 2)
        .ok_or(RomError::RangeOutOfBounds {
            offset: word_offset,
            len: 2,
            image_len: bytes.len(),
        })?;
    let bank = *bytes.get(bank_offset).ok_or(RomError::RangeOutOfBounds {
        offset: bank_offset,
        len: 1,
        image_len: bytes.len(),
    })?;
    let address = u32::from_le_bytes([word[0], word[1], bank, 0]);
    Ok(snes_to_pc(Mapper::LoRom, address)?)
}

fn optional_exact_block(
    bytes: &[u8],
    payload_offset: usize,
) -> Result<Option<RatsBlock>, SmwUsV1TransferredMap16Error> {
    let Some(header) = payload_offset.checked_sub(HEADER_LEN) else {
        return Ok(None);
    };
    match parse_at(bytes, header) {
        Ok(block) if block.payload.start == payload_offset => Ok(Some(block)),
        Ok(_) | Err(HeaderError::Signature) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn bounded_stream(bytes: &[u8], offset: usize) -> Result<&[u8], SmwUsV1TransferredMap16Error> {
    let bank_end = (offset | 0x7fff).saturating_add(1).min(bytes.len());
    bytes
        .get(offset..bank_end)
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len: bank_end.saturating_sub(offset),
            image_len: bytes.len(),
        })
        .map_err(Into::into)
}

fn interleave(even: &[u8], odd: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(even.len() + odd.len());
    for (&even, &odd) in even.iter().zip(odd) {
        bytes.push(even);
        bytes.push(odd);
    }
    bytes
}

fn le_words(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::ProtectedRange;
    use lm_rom::{RomImage, compute_snes_checksum};
    use std::{fs, path::Path};

    #[test]
    fn loads_the_exact_wine_transfer_tables() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image = RomImage::from_bytes(
            fs::read(
                root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc"),
            )
            .unwrap(),
        )
        .unwrap();
        let loaded = load_smw_us_v1_transferred_map16(&Project::new(image)).unwrap();
        assert_eq!(loaded.definitions.len(), 0x2000);
        assert_eq!(loaded.acts_like.len(), 2884);
        assert_eq!(loaded.definition_block.unwrap().payload, 0x80008..0x82f30);
        assert_eq!(loaded.acts_low_block.unwrap().payload, 0x82f38..0x83a7c);
        assert_eq!(loaded.acts_high_block.unwrap().payload, 0x83a84..0x84088);
        assert_eq!(loaded.definition_odd_stream_offset, 0x8197a);
        assert_eq!(loaded.definitions[..4], [0x1c75; 4]);
    }

    #[test]
    fn pristine_and_transferred_tables_have_identical_editor_meaning() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/overworld-transfer-positive");
        let load = |name| {
            let image = RomImage::from_bytes(fs::read(root.join(name)).unwrap()).unwrap();
            load_smw_us_v1_transferred_map16(&Project::new(image)).unwrap()
        };
        let before = load("before.smc");
        let after = load("after.smc");
        assert_eq!(before.definitions, after.definitions);
        assert_eq!(before.acts_like, after.acts_like);
        assert!(before.definition_block.is_none());
        assert!(before.acts_low_block.is_none());
        assert!(before.acts_high_block.is_none());
    }

    #[test]
    fn pristine_tables_edit_repoint_checksum_and_reopen_atomically() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/overworld-transfer-positive");
        let image = RomImage::from_bytes(fs::read(root.join("before.smc")).unwrap()).unwrap();
        let mut project = Project::new(image);
        let mut loaded = load_smw_us_v1_transferred_map16(&project).unwrap();
        loaded.definitions[0] ^= 1;
        loaded.acts_like[0] ^= 1;
        project
            .expand_rom(Mapper::LoRom, 0x10_0000, 0xff, 0x7fdc)
            .unwrap();
        let options = SmwUsV1TransferredMap16SaveOptions {
            allocation: AllocationPolicy {
                search: 0x80_000..0x10_0000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![
                    ProtectedRange(0x7fc0..0x8000),
                    ProtectedRange(
                        SMW_US_V1_MAP16_DEFINITION_WORD_OFFSET
                            ..SMW_US_V1_MAP16_DEFINITION_WORD_OFFSET + 2,
                    ),
                    ProtectedRange(
                        SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET
                            ..SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET + 1,
                    ),
                    ProtectedRange(
                        SMW_US_V1_MAP16_DEFINITION_ODD_WORD_OFFSET
                            ..SMW_US_V1_MAP16_DEFINITION_ODD_WORD_OFFSET + 2,
                    ),
                    ProtectedRange(
                        SMW_US_V1_MAP16_ACTS_LOW_WORD_OFFSET
                            ..SMW_US_V1_MAP16_ACTS_LOW_WORD_OFFSET + 2,
                    ),
                    ProtectedRange(
                        SMW_US_V1_MAP16_ACTS_LOW_BANK_OFFSET
                            ..SMW_US_V1_MAP16_ACTS_LOW_BANK_OFFSET + 1,
                    ),
                    ProtectedRange(
                        SMW_US_V1_MAP16_ACTS_HIGH_WORD_OFFSET
                            ..SMW_US_V1_MAP16_ACTS_HIGH_WORD_OFFSET + 2,
                    ),
                    ProtectedRange(
                        SMW_US_V1_MAP16_ACTS_HIGH_BANK_OFFSET
                            ..SMW_US_V1_MAP16_ACTS_HIGH_BANK_OFFSET + 1,
                    ),
                ],
            },
            reuse_identical: true,
            erase_fill: 0xff,
        };
        save_smw_us_v1_transferred_map16(
            &mut project,
            &loaded.definitions,
            &loaded.acts_like,
            0x7fdc,
            &options,
        )
        .unwrap();
        let reopened = load_smw_us_v1_transferred_map16(&project).unwrap();
        assert_eq!(reopened.definitions, loaded.definitions);
        assert_eq!(reopened.acts_like, loaded.acts_like);
        let checksum = compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap();
        assert_eq!(project.rom.read(0x7fdc, 4).unwrap(), checksum.encoded());

        let before_failure = project.save_snapshot();
        assert!(matches!(
            save_smw_us_v1_transferred_map16(
                &mut project,
                &loaded.definitions,
                &[],
                0x7fdc,
                &options,
            ),
            Err(SmwUsV1TransferredMap16Error::EmptyActsLike)
        ));
        assert_eq!(project.save_snapshot(), before_failure);
    }
}
