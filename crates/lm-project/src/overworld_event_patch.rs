//! Detection of pristine and RATS-expanded native overworld event-reveal planes.

use crate::{EventRevealIoError, EventRevealRomLayout, LevelPointerTable, Project};
use lm_overworld::{EventRevealTable, EventTableError};
use lm_rats::{RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, snes_to_pc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldEventRevealLocator {
    pub mapper: Mapper,
    pub source_operand_offset: usize,
    pub destination_operand_offset: usize,
    pub fixed_entries: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldEventRevealStorage {
    Fixed,
    TransferredSources {
        source_block: RatsBlock,
    },
    Expanded {
        source_block: RatsBlock,
        destination_block: RatsBlock,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedOverworldEventReveals {
    pub table: EventRevealTable,
    pub layout: EventRevealRomLayout,
    pub storage: OverworldEventRevealStorage,
}

#[derive(Debug)]
pub enum OverworldEventRevealPatchError {
    OperandRange,
    Pointer(RomError),
    MixedStorage,
    PlaneLength { sources: usize, destinations: usize },
    InvalidCount(usize),
    Table(EventTableError),
    Io(EventRevealIoError),
}

impl std::fmt::Display for OverworldEventRevealPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native overworld event-reveal detection failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldEventRevealPatchError {}

impl From<RomError> for OverworldEventRevealPatchError {
    fn from(value: RomError) -> Self {
        Self::Pointer(value)
    }
}

impl From<EventTableError> for OverworldEventRevealPatchError {
    fn from(value: EventTableError) -> Self {
        Self::Table(value)
    }
}

impl From<EventRevealIoError> for OverworldEventRevealPatchError {
    fn from(value: EventRevealIoError) -> Self {
        Self::Io(value)
    }
}

impl Project {
    /// Loads pristine fixed planes, Lunar Magic's transferred tagged-source/fixed-destination
    /// representation, or a matched pair of expanded RATS planes.
    ///
    /// # Errors
    ///
    /// Rejects malformed operands, unsupported destination-only tagging, unequal/odd paired
    /// storage, counts outside Lunar Magic's 255-entry workspace, and values that cannot
    /// semantically reopen.
    pub fn load_overworld_event_reveals_detected(
        &self,
        locator: OverworldEventRevealLocator,
    ) -> Result<LoadedOverworldEventReveals, OverworldEventRevealPatchError> {
        let source_offset = read_operand(
            self.rom.logical_bytes(),
            locator.source_operand_offset,
            locator.mapper,
        )?;
        let destination_offset = read_operand(
            self.rom.logical_bytes(),
            locator.destination_operand_offset,
            locator.mapper,
        )?;
        let source_block = tagged_block(self.rom.logical_bytes(), source_offset);
        let destination_block = tagged_block(self.rom.logical_bytes(), destination_offset);
        let (entries, storage) = match (source_block, destination_block) {
            (None, None) => (locator.fixed_entries, OverworldEventRevealStorage::Fixed),
            (Some(source_block), None) => {
                if source_block.payload.len() % 2 != 0 {
                    return Err(OverworldEventRevealPatchError::PlaneLength {
                        sources: source_block.payload.len(),
                        destinations: source_block.payload.len(),
                    });
                }
                (
                    source_block.payload.len() / 2,
                    OverworldEventRevealStorage::TransferredSources { source_block },
                )
            }
            (Some(source_block), Some(destination_block)) => {
                if source_block.payload.len() != destination_block.payload.len()
                    || source_block.payload.len() % 2 != 0
                {
                    return Err(OverworldEventRevealPatchError::PlaneLength {
                        sources: source_block.payload.len(),
                        destinations: destination_block.payload.len(),
                    });
                }
                (
                    source_block.payload.len() / 2,
                    OverworldEventRevealStorage::Expanded {
                        source_block,
                        destination_block,
                    },
                )
            }
            _ => return Err(OverworldEventRevealPatchError::MixedStorage),
        };
        if entries == 0 || entries > EventRevealTable::MAX_ENTRIES {
            return Err(OverworldEventRevealPatchError::InvalidCount(entries));
        }
        let layout = EventRevealRomLayout {
            mapper: locator.mapper,
            sources: LevelPointerTable {
                offset: locator.source_operand_offset,
                entries: 1,
                stride: 3,
            },
            destinations: LevelPointerTable {
                offset: locator.destination_operand_offset,
                entries: 1,
                stride: 3,
            },
            entries_per_slot: entries,
        };
        let table = match &storage {
            OverworldEventRevealStorage::TransferredSources { source_block } => {
                let plane_len = entries
                    .checked_mul(2)
                    .ok_or(OverworldEventRevealPatchError::InvalidCount(entries))?;
                let sources = &self.rom.logical_bytes()
                    [source_block.payload.start..source_block.payload.start + plane_len];
                let destinations = self.rom.read(destination_offset, plane_len)?;
                EventRevealTable::decode(sources, destinations)?
            }
            OverworldEventRevealStorage::Fixed | OverworldEventRevealStorage::Expanded { .. } => {
                self.load_event_reveals(0, layout)?
            }
        };
        table.validate()?;
        Ok(LoadedOverworldEventReveals {
            table,
            layout,
            storage,
        })
    }
}

fn read_operand(
    bytes: &[u8],
    offset: usize,
    mapper: Mapper,
) -> Result<usize, OverworldEventRevealPatchError> {
    let operand = bytes
        .get(offset..offset + 3)
        .ok_or(OverworldEventRevealPatchError::OperandRange)?;
    let address = u32::from(operand[0]) | u32::from(operand[1]) << 8 | u32::from(operand[2]) << 16;
    Ok(snes_to_pc(mapper, address)?)
}

fn tagged_block(bytes: &[u8], payload_offset: usize) -> Option<RatsBlock> {
    let header = payload_offset.checked_sub(lm_rats::HEADER_LEN)?;
    parse_at(bytes, header)
        .ok()
        .filter(|block| block.payload.start == payload_offset)
}
