//! Detection and loading of Lunar Magic's expanded special-path runtime.

use crate::{OverworldPathLinkIoError, OverworldPathLinkRomLayout, Project};
use lm_overworld::{OverworldPathLinkTable, OverworldPathLinkTableError};
use lm_rom::{Mapper, RomError, snes_to_pc};

const CURRENT_MARKER: [u8; 4] = [b'L', b'M', 0x00, 0x01];
const RUNTIME_LEN: usize = 0x70;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldPathPatchLocator {
    pub mapper: Mapper,
    pub hook_offset: usize,
    pub fixed: OverworldPathLinkRomLayout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverworldPathLinkStorage {
    Fixed,
    CurrentPatch {
        patch_offset: usize,
        planes: OverworldPathLinkRomLayout,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedOverworldPathLinks {
    pub table: OverworldPathLinkTable,
    pub storage: OverworldPathLinkStorage,
}

#[derive(Debug)]
pub enum OverworldPathPatchError {
    HookRange,
    HookShape,
    PatchRange { offset: usize },
    UnknownVariant([u8; 4]),
    InvalidEntryCount(usize),
    StrideMismatch { actual: u16, expected: usize },
    PointerLayout,
    Pointer(RomError),
    Table(OverworldPathLinkTableError),
    Fixed(OverworldPathLinkIoError),
}

impl std::fmt::Display for OverworldPathPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native overworld path patch detection failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldPathPatchError {}

impl From<RomError> for OverworldPathPatchError {
    fn from(value: RomError) -> Self {
        Self::Pointer(value)
    }
}

impl From<OverworldPathLinkTableError> for OverworldPathPatchError {
    fn from(value: OverworldPathLinkTableError) -> Self {
        Self::Table(value)
    }
}

impl From<OverworldPathLinkIoError> for OverworldPathPatchError {
    fn from(value: OverworldPathLinkIoError) -> Self {
        Self::Fixed(value)
    }
}

impl Project {
    /// Loads either the fixed pristine planes or a recognized current expanded runtime.
    ///
    /// # Errors
    ///
    /// Rejects malformed hooks, runtime markers/counts/stride constants, pointers, plane layouts,
    /// and table bytes rather than falling back to unrelated fixed data.
    pub fn load_overworld_path_links_detected(
        &self,
        locator: OverworldPathPatchLocator,
    ) -> Result<LoadedOverworldPathLinks, OverworldPathPatchError> {
        let hook = self
            .rom
            .logical_bytes()
            .get(locator.hook_offset..locator.hook_offset + 5)
            .ok_or(OverworldPathPatchError::HookRange)?;
        if hook[0] != 0x22 {
            return Ok(LoadedOverworldPathLinks {
                table: self.load_overworld_path_links(locator.fixed)?,
                storage: OverworldPathLinkStorage::Fixed,
            });
        }
        if hook[4] != 0x60 {
            return Err(OverworldPathPatchError::HookShape);
        }
        let patch_offset = decode_pointer(locator.mapper, &hook[1..4])?;
        let patch = self
            .rom
            .logical_bytes()
            .get(patch_offset..patch_offset.saturating_add(RUNTIME_LEN))
            .ok_or(OverworldPathPatchError::PatchRange {
                offset: patch_offset,
            })?;
        let marker = [patch[0x69], patch[0x6a], patch[0x6b], patch[0x6c]];
        if marker != CURRENT_MARKER {
            return Err(OverworldPathPatchError::UnknownVariant(marker));
        }
        let encoded_count = usize::from(u16::from_le_bytes([patch[6], patch[7]]));
        let entries = encoded_count
            .checked_add(1)
            .ok_or(OverworldPathPatchError::InvalidEntryCount(encoded_count))?;
        if entries == 0 || entries > OverworldPathLinkTable::MAX_LINKS {
            return Err(OverworldPathPatchError::InvalidEntryCount(entries));
        }
        let expected_stride = encoded_count
            .checked_mul(5)
            .ok_or(OverworldPathPatchError::InvalidEntryCount(entries))?;
        let stride = u16::from_le_bytes([patch[0x0b], patch[0x0c]]);
        if usize::from(stride) != expected_stride {
            return Err(OverworldPathPatchError::StrideMismatch {
                actual: stride,
                expected: expected_stride,
            });
        }
        let pointers = [0x11, 0x1a, 0x20, 0x2c, 0x33, 0x3a, 0x48, 0x52]
            .map(|offset| decode_pointer(locator.mapper, &patch[offset..offset + 3]))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let source_len = entries * 5;
        let expected = [
            pointers[0],
            pointers[0] + 2,
            pointers[0] + 4,
            pointers[0] + source_len,
            pointers[0] + source_len + 2,
            pointers[0] + source_len + 4,
            pointers[0] + source_len * 2,
            pointers[0] + source_len * 2 + 1,
        ];
        if pointers != expected {
            return Err(OverworldPathPatchError::PointerLayout);
        }
        let planes = OverworldPathLinkRomLayout {
            mapper: locator.mapper,
            source_offset: pointers[0],
            destination_offset: pointers[3],
            target_offset: pointers[6],
            entries,
        };
        Ok(LoadedOverworldPathLinks {
            table: self.load_overworld_path_links(planes)?,
            storage: OverworldPathLinkStorage::CurrentPatch {
                patch_offset,
                planes,
            },
        })
    }
}

fn decode_pointer(mapper: Mapper, bytes: &[u8]) -> Result<usize, RomError> {
    let address = u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16);
    snes_to_pc(mapper, address)
}
