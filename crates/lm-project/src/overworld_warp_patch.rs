//! Detection and loading of Lunar Magic's relocatable overworld warp-link patch.

use crate::{OverworldWarpLinkIoError, OverworldWarpLinkRomLayout, Project};
use lm_overworld::{OverworldWarpLinkTable, OverworldWarpLinkTableError};
use lm_rom::{Mapper, RomError, snes_to_pc};

const CURRENT_MARKER: [u8; 4] = [b'L', b'M', 0x10, 0x01];
const LEGACY_MARKER: [u8; 4] = [0xff; 4];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverworldWarpPatchLocator {
    pub mapper: Mapper,
    pub entry_hook_offset: usize,
    pub return_hook_offset: usize,
    pub fixed: OverworldWarpLinkRomLayout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverworldWarpLinkStorage {
    Fixed,
    CurrentPatch {
        patch_offset: usize,
        planes: OverworldWarpLinkRomLayout,
    },
    LegacyPatch {
        patch_offset: usize,
        planes: OverworldWarpLinkRomLayout,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedOverworldWarpLinks {
    pub table: OverworldWarpLinkTable,
    pub storage: OverworldWarpLinkStorage,
}

#[derive(Debug)]
pub enum OverworldWarpPatchError {
    HookRange,
    ReturnHookMismatch,
    UnsupportedHookOpcode(u8),
    PatchRange { offset: usize },
    UnknownVariant([u8; 4]),
    InvalidEntryCount(usize),
    Pointer(RomError),
    Table(OverworldWarpLinkTableError),
    Fixed(OverworldWarpLinkIoError),
}

impl std::fmt::Display for OverworldWarpPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native overworld warp patch detection failed: {self:?}"
        )
    }
}

impl std::error::Error for OverworldWarpPatchError {}

impl From<RomError> for OverworldWarpPatchError {
    fn from(value: RomError) -> Self {
        Self::Pointer(value)
    }
}

impl From<OverworldWarpLinkTableError> for OverworldWarpPatchError {
    fn from(value: OverworldWarpLinkTableError) -> Self {
        Self::Table(value)
    }
}

impl From<OverworldWarpLinkIoError> for OverworldWarpPatchError {
    fn from(value: OverworldWarpLinkIoError) -> Self {
        Self::Fixed(value)
    }
}

impl Project {
    /// Loads either the pristine fixed table or a recognized current/legacy Lunar Magic patch.
    ///
    /// # Errors
    ///
    /// Rejects truncated hooks and patches, unknown patch markers, invalid counts, mapper-invalid
    /// pointers, out-of-image planes, and malformed tables.
    pub fn load_overworld_warp_links_detected(
        &self,
        locator: OverworldWarpPatchLocator,
    ) -> Result<LoadedOverworldWarpLinks, OverworldWarpPatchError> {
        let hook = self
            .rom
            .logical_bytes()
            .get(locator.entry_hook_offset..locator.entry_hook_offset + 4)
            .ok_or(OverworldWarpPatchError::HookRange)?;
        if hook[0] != 0x22 {
            return Ok(LoadedOverworldWarpLinks {
                table: self.load_overworld_warp_links(locator.fixed)?,
                storage: OverworldWarpLinkStorage::Fixed,
            });
        }
        let patch_offset = decode_pointer(locator.mapper, &hook[1..4])?;
        let return_hook = self
            .rom
            .logical_bytes()
            .get(locator.return_hook_offset..locator.return_hook_offset + 4)
            .ok_or(OverworldWarpPatchError::HookRange)?;
        if return_hook[0] != 0x22
            || decode_pointer(locator.mapper, &return_hook[1..4])?
                != patch_offset.saturating_add(0x40)
        {
            return Err(OverworldWarpPatchError::ReturnHookMismatch);
        }
        let patch = self
            .rom
            .logical_bytes()
            .get(patch_offset..patch_offset.saturating_add(0x80))
            .ok_or(OverworldWarpPatchError::PatchRange {
                offset: patch_offset,
            })?;
        let marker = [patch[0x3c], patch[0x3d], patch[0x3e], patch[0x3f]];
        let (entries, pointer_offsets, legacy) = if marker == CURRENT_MARKER {
            (
                usize::from(u16::from_le_bytes([patch[0x10], patch[0x11]])),
                [0x17, 0x27, 0x4c, 0x5e],
                false,
            )
        } else if marker == LEGACY_MARKER {
            let count = usize::from(patch[0x10]);
            (
                if count == 0 { 256 } else { count },
                [0x14, 0x24, 0x47, 0x59],
                true,
            )
        } else {
            return Err(OverworldWarpPatchError::UnknownVariant(marker));
        };
        if entries == 0 || entries > OverworldWarpLinkTable::MAX_LINKS {
            return Err(OverworldWarpPatchError::InvalidEntryCount(entries));
        }
        let offsets = pointer_offsets
            .map(|offset| decode_pointer(locator.mapper, &patch[offset..offset + 3]))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let planes = OverworldWarpLinkRomLayout {
            mapper: locator.mapper,
            source_vertical_offset: offsets[0],
            source_horizontal_offset: offsets[1],
            destination_vertical_offset: offsets[2],
            destination_horizontal_offset: offsets[3],
            entries,
        };
        let table = self.load_overworld_warp_links(planes)?;
        let storage = if legacy {
            OverworldWarpLinkStorage::LegacyPatch {
                patch_offset,
                planes,
            }
        } else {
            OverworldWarpLinkStorage::CurrentPatch {
                patch_offset,
                planes,
            }
        };
        Ok(LoadedOverworldWarpLinks { table, storage })
    }
}

fn decode_pointer(mapper: Mapper, bytes: &[u8]) -> Result<usize, RomError> {
    let address = u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16);
    snes_to_pc(mapper, address)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{RomImage, pc_to_snes};

    fn locator() -> OverworldWarpPatchLocator {
        OverworldWarpPatchLocator {
            mapper: Mapper::LoRom,
            entry_hook_offset: 0x100,
            return_hook_offset: 0x110,
            fixed: OverworldWarpLinkRomLayout {
                mapper: Mapper::LoRom,
                source_vertical_offset: 0x200,
                source_horizontal_offset: 0x204,
                destination_vertical_offset: 0x208,
                destination_horizontal_offset: 0x20c,
                entries: 2,
            },
        }
    }

    #[test]
    fn current_patch_count_and_all_four_pointers_are_followed() {
        let mut bytes = vec![0xff; 0x8000];
        let patch = 0x300;
        let address = pc_to_snes(Mapper::LoRom, patch).unwrap().to_le_bytes();
        bytes[0x100..0x104].copy_from_slice(&[0x22, address[0], address[1], address[2]]);
        let return_address = pc_to_snes(Mapper::LoRom, patch + 0x40)
            .unwrap()
            .to_le_bytes();
        bytes[0x110..0x114].copy_from_slice(&[
            0x22,
            return_address[0],
            return_address[1],
            return_address[2],
        ]);
        bytes[patch + 0x10..patch + 0x12].copy_from_slice(&2_u16.to_le_bytes());
        bytes[patch + 0x3c..patch + 0x40].copy_from_slice(&CURRENT_MARKER);
        for (pointer, target) in [0x17, 0x27, 0x4c, 0x5e]
            .into_iter()
            .zip([0x400, 0x404, 0x408, 0x40c])
        {
            let encoded = pc_to_snes(Mapper::LoRom, target).unwrap().to_le_bytes();
            bytes[patch + pointer..patch + pointer + 3].copy_from_slice(&encoded[..3]);
        }
        bytes[0x400..0x410].copy_from_slice(&[1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 8, 0]);
        let project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let loaded = project
            .load_overworld_warp_links_detected(locator())
            .unwrap();
        assert!(matches!(
            loaded.storage,
            OverworldWarpLinkStorage::CurrentPatch {
                patch_offset: 0x300,
                ..
            }
        ));
        assert_eq!(loaded.table.links[1].destination.horizontal_tile, 8);
    }

    #[test]
    fn unknown_marker_and_invalid_pointer_fail_closed() {
        let mut bytes = vec![0xff; 0x8000];
        bytes[0x100..0x104].copy_from_slice(&[0x22, 0x00, 0x80, 0x00]);
        bytes[0x110..0x114].copy_from_slice(&[0x22, 0x40, 0x80, 0x00]);
        bytes[0x3c..0x40].fill(0);
        let project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        assert!(matches!(
            project.load_overworld_warp_links_detected(locator()),
            Err(OverworldWarpPatchError::UnknownVariant(_))
        ));
        bytes[0x101..0x104].copy_from_slice(&[0x00, 0x00, 0x7e]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert!(matches!(
            project.load_overworld_warp_links_detected(locator()),
            Err(OverworldWarpPatchError::Pointer(_))
        ));
    }
}
