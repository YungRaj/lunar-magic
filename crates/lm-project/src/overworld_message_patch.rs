//! Detection and semantic loading of Lunar Magic's expanded overworld-message runtime.

use crate::Project;
use lm_overworld::OverworldMessage;
use lm_rats::{RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, snes_to_pc};

const EXPANDED_RUNTIME_LEN: usize = 0x110;
const EXPANDED_MARKER: [u8; 4] = [b'L', b'M', 0x10, 0x01];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldMessagePatchLocator {
    pub mapper: Mapper,
    pub hook_offset: usize,
    pub runtime_offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedOverworldMessageStorage {
    pub runtime_offset: usize,
    pub pointer_table_offset: usize,
    pub pointer_table_len: usize,
    pub message_pools: Vec<RatsBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedExpandedOverworldMessages {
    pub messages: Vec<OverworldMessage>,
    pub storage: ExpandedOverworldMessageStorage,
}

#[derive(Debug)]
pub enum OverworldMessagePatchError {
    HookRange,
    HookShape,
    RuntimeTarget { actual: usize, expected: usize },
    RuntimeRange,
    RuntimeMarker([u8; 4]),
    PointerOperands,
    MissingPointerTableTag,
    MissingMessagePoolTag { group: usize },
    MessageOutsidePool { index: usize },
    InvalidPointerTableLength(usize),
    Pointer(RomError),
    UnterminatedMessage { index: usize },
}

impl std::fmt::Display for OverworldMessagePatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "expanded native overworld-message detection failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldMessagePatchError {}

impl From<RomError> for OverworldMessagePatchError {
    fn from(value: RomError) -> Self {
        Self::Pointer(value)
    }
}

impl Project {
    /// Loads a recognized Lunar Magic 1.10 expanded pointer table and all addressed messages.
    ///
    /// Each message is independently bounded to the editor's exact 144-byte workspace. Short
    /// `$FE`-terminated strings are padded with the native blank glyph `$1F`.
    ///
    /// # Errors
    ///
    /// Rejects altered hooks/runtimes, untagged or malformed pointer tables, invalid mappings,
    /// excessive counts, and strings without a terminator inside the fixed record bound.
    pub fn load_expanded_overworld_messages_detected(
        &self,
        locator: OverworldMessagePatchLocator,
    ) -> Result<LoadedExpandedOverworldMessages, OverworldMessagePatchError> {
        let rom = self.rom.logical_bytes();
        let hook = rom
            .get(locator.hook_offset..locator.hook_offset + 7)
            .ok_or(OverworldMessagePatchError::HookRange)?;
        if hook[0] != 0x22 || hook[4..7] != [0x4c, 0x50, 0xb2] {
            return Err(OverworldMessagePatchError::HookShape);
        }
        let runtime_target = decode_pointer(locator.mapper, &hook[1..4])?;
        if runtime_target != locator.runtime_offset {
            return Err(OverworldMessagePatchError::RuntimeTarget {
                actual: runtime_target,
                expected: locator.runtime_offset,
            });
        }
        let runtime = rom
            .get(locator.runtime_offset..locator.runtime_offset + EXPANDED_RUNTIME_LEN)
            .ok_or(OverworldMessagePatchError::RuntimeRange)?;
        let marker = [
            runtime[0x10c],
            runtime[0x10d],
            runtime[0x10e],
            runtime[0x10f],
        ];
        if marker != EXPANDED_MARKER {
            return Err(OverworldMessagePatchError::RuntimeMarker(marker));
        }
        let pointer_table_offset = decode_pointer(locator.mapper, &runtime[0x49..0x4c])?;
        let adjacent_operand = decode_pointer(locator.mapper, &runtime[0x4f..0x52])?;
        if adjacent_operand != pointer_table_offset + 1 {
            return Err(OverworldMessagePatchError::PointerOperands);
        }
        let header_offset = pointer_table_offset
            .checked_sub(8)
            .ok_or(OverworldMessagePatchError::MissingPointerTableTag)?;
        let table = parse_at(rom, header_offset)
            .map_err(|_| OverworldMessagePatchError::MissingPointerTableTag)?;
        if table.payload.start != pointer_table_offset {
            return Err(OverworldMessagePatchError::MissingPointerTableTag);
        }
        let pointer_table_len = table.payload.len();
        if pointer_table_len % 3 != 0 {
            return Err(OverworldMessagePatchError::InvalidPointerTableLength(
                pointer_table_len,
            ));
        }
        let count = pointer_table_len / 3;
        if !(194..=512).contains(&count) || count % 2 != 0 {
            return Err(OverworldMessagePatchError::InvalidPointerTableLength(
                pointer_table_len,
            ));
        }

        let pools = detect_message_pools(rom, locator.mapper, pointer_table_offset, count)?;
        let messages = load_messages(rom, locator.mapper, pointer_table_offset, count, &pools)?;
        Ok(LoadedExpandedOverworldMessages {
            messages,
            storage: ExpandedOverworldMessageStorage {
                runtime_offset: locator.runtime_offset,
                pointer_table_offset,
                pointer_table_len,
                message_pools: pools,
            },
        })
    }
}

fn detect_message_pools(
    rom: &[u8],
    mapper: Mapper,
    pointer_table_offset: usize,
    count: usize,
) -> Result<Vec<RatsBlock>, OverworldMessagePatchError> {
    let mut pools = Vec::with_capacity(count.div_ceil(0xc0));
    for group_start in (0..count).step_by(0xc0) {
        let operand = &rom
            [pointer_table_offset + group_start * 3..pointer_table_offset + group_start * 3 + 3];
        let first_message = decode_pointer(mapper, operand)?;
        let group = group_start / 0xc0;
        let header_offset = first_message
            .checked_sub(8)
            .ok_or(OverworldMessagePatchError::MissingMessagePoolTag { group })?;
        let pool = parse_at(rom, header_offset)
            .map_err(|_| OverworldMessagePatchError::MissingMessagePoolTag { group })?;
        if pool.payload.start != first_message {
            return Err(OverworldMessagePatchError::MissingMessagePoolTag { group });
        }
        pools.push(pool);
    }
    Ok(pools)
}

fn load_messages(
    rom: &[u8],
    mapper: Mapper,
    pointer_table_offset: usize,
    count: usize,
    pools: &[RatsBlock],
) -> Result<Vec<OverworldMessage>, OverworldMessagePatchError> {
    let mut messages = Vec::with_capacity(count);
    for index in 0..count {
        let operand = &rom[pointer_table_offset + index * 3..pointer_table_offset + index * 3 + 3];
        let message_offset = decode_pointer(mapper, operand)?;
        let pool = &pools[index / 0xc0];
        if !pool.payload.contains(&message_offset) {
            return Err(OverworldMessagePatchError::MessageOutsidePool { index });
        }
        let bounded_end = pool
            .payload
            .end
            .min(message_offset.saturating_add(OverworldMessage::ENCODED_LEN));
        let bytes = &rom[message_offset..bounded_end];
        let terminator = bytes
            .iter()
            .position(|byte| *byte == 0xfe)
            .unwrap_or(bytes.len());
        if terminator == bytes.len() && bytes.len() < OverworldMessage::ENCODED_LEN {
            return Err(OverworldMessagePatchError::UnterminatedMessage { index });
        }
        let mut record = [0x1f; OverworldMessage::ENCODED_LEN];
        record[..terminator].copy_from_slice(&bytes[..terminator]);
        messages.push(OverworldMessage(record));
    }
    Ok(messages)
}

fn decode_pointer(mapper: Mapper, bytes: &[u8]) -> Result<usize, RomError> {
    let address = u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16;
    snes_to_pc(mapper, address)
}
