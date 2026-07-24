//! Detection and loading of original and Lunar Magic-expanded overworld level names.

use crate::Project;
use lm_overworld::{NativeOverworldLevelNameError, NativeOverworldLevelNameTable};
use lm_rats::parse_at;
use lm_rom::{Mapper, RomError, snes_to_pc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldLevelNameLocator {
    pub mapper: Mapper,
    pub primary_hook_offset: usize,
    pub secondary_hook_offset: usize,
    pub fixed_runtime_offset: usize,
    pub vanilla_codes_offset: usize,
    pub vanilla_offsets_offset: usize,
    pub vanilla_text_offset: usize,
    pub vanilla_text_len: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverworldLevelNameStorage {
    Vanilla,
    Expanded {
        runtime_offset: usize,
        table_offset: usize,
        table_len: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedOverworldLevelNames {
    pub table: NativeOverworldLevelNameTable,
    pub storage: OverworldLevelNameStorage,
}

#[derive(Debug)]
pub enum OverworldLevelNameIoError {
    HookRange,
    HookMismatch,
    RuntimeRange,
    RuntimeMismatch { offset: usize },
    Pointer(RomError),
    MissingAllocation,
    InvalidAllocationLength(usize),
    Table(NativeOverworldLevelNameError),
    Rom(RomError),
}

impl std::fmt::Display for OverworldLevelNameIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native overworld level-name I/O failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldLevelNameIoError {}

impl From<NativeOverworldLevelNameError> for OverworldLevelNameIoError {
    fn from(value: NativeOverworldLevelNameError) -> Self {
        Self::Table(value)
    }
}

impl From<RomError> for OverworldLevelNameIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl Project {
    /// Loads either the original segmented SMW names or Lunar Magic's direct expanded table.
    ///
    /// # Errors
    ///
    /// Fails closed on partial/mismatched hooks, unexpected runtime code, invalid pointers,
    /// untagged tables, malformed RATS sizes, or malformed name records.
    pub fn load_overworld_level_names_detected(
        &self,
        locator: OverworldLevelNameLocator,
        runtime_template: &[u8],
    ) -> Result<LoadedOverworldLevelNames, OverworldLevelNameIoError> {
        let bytes = self.rom.logical_bytes();
        let primary = bytes
            .get(locator.primary_hook_offset..locator.primary_hook_offset + 4)
            .ok_or(OverworldLevelNameIoError::HookRange)?;
        if primary[0] != 0x22 {
            let codes = self.rom.read(
                locator.vanilla_codes_offset,
                NativeOverworldLevelNameTable::VANILLA_NAMES * 2,
            )?;
            let offsets = self.rom.read(locator.vanilla_offsets_offset, 59 * 2)?;
            let text = self
                .rom
                .read(locator.vanilla_text_offset, locator.vanilla_text_len)?;
            return Ok(LoadedOverworldLevelNames {
                table: NativeOverworldLevelNameTable::decode_vanilla(codes, offsets, text)?,
                storage: OverworldLevelNameStorage::Vanilla,
            });
        }
        let runtime_offset = decode_pointer(locator.mapper, &primary[1..4])?;
        if runtime_offset != locator.fixed_runtime_offset {
            return Err(OverworldLevelNameIoError::HookMismatch);
        }
        let secondary = bytes
            .get(locator.secondary_hook_offset..locator.secondary_hook_offset + 4)
            .ok_or(OverworldLevelNameIoError::HookRange)?;
        if secondary[0] != 0x22
            || decode_pointer(locator.mapper, &secondary[1..4])? != runtime_offset
        {
            return Err(OverworldLevelNameIoError::HookMismatch);
        }
        let runtime = bytes
            .get(runtime_offset..runtime_offset.saturating_add(runtime_template.len()))
            .ok_or(OverworldLevelNameIoError::RuntimeRange)?;
        if runtime_template.len() < 0x3a
            || runtime.iter().zip(runtime_template).enumerate().any(
                |(index, (actual, expected))| !(0x37..0x3a).contains(&index) && actual != expected,
            )
        {
            return Err(OverworldLevelNameIoError::RuntimeMismatch {
                offset: runtime_offset,
            });
        }
        let table_offset =
            decode_pointer(locator.mapper, &runtime[0x37..0x3a]).map_err(Self::pointer_error)?;
        let header_offset = table_offset
            .checked_sub(8)
            .ok_or(OverworldLevelNameIoError::MissingAllocation)?;
        let block = parse_at(bytes, header_offset)
            .map_err(|_| OverworldLevelNameIoError::MissingAllocation)?;
        if block.payload.start != table_offset {
            return Err(OverworldLevelNameIoError::MissingAllocation);
        }
        let table_len = block.payload.len();
        if table_len == 0
            || table_len % NativeOverworldLevelNameTable::RECORD_LEN != 0
            || table_len
                > NativeOverworldLevelNameTable::MAX_NAMES
                    * NativeOverworldLevelNameTable::RECORD_LEN
        {
            return Err(OverworldLevelNameIoError::InvalidAllocationLength(
                table_len,
            ));
        }
        Ok(LoadedOverworldLevelNames {
            table: NativeOverworldLevelNameTable::decode(&bytes[block.payload])?,
            storage: OverworldLevelNameStorage::Expanded {
                runtime_offset,
                table_offset,
                table_len,
            },
        })
    }

    fn pointer_error(error: RomError) -> OverworldLevelNameIoError {
        OverworldLevelNameIoError::Pointer(error)
    }
}

fn decode_pointer(mapper: Mapper, bytes: &[u8]) -> Result<usize, RomError> {
    let address = u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16);
    snes_to_pc(mapper, address)
}
