use crate::{GraphicsIoError, GraphicsRomLayout, Project};
use lm_graphics::GraphicsFile4bpp;
use lm_level::{ExpandedLevelHeader, ExpandedLevelSettingsRecord, SuperGraphicsBypass};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSuperGraphicsBypass {
    pub selection: SuperGraphicsBypass,
    pub foreground_background: Vec<GraphicsFile4bpp>,
    pub sprites: Vec<GraphicsFile4bpp>,
}

#[derive(Debug)]
pub enum SuperGraphicsIoError {
    ForegroundBackground {
        slot: usize,
        file: u16,
        error: GraphicsIoError,
    },
    Sprite {
        slot: usize,
        file: u16,
        error: GraphicsIoError,
    },
}

impl std::fmt::Display for SuperGraphicsIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Super GFX bypass load failed: {self:?}")
    }
}

impl std::error::Error for SuperGraphicsIoError {}

impl Project {
    /// Resolves every file selected by an enabled expanded-header Super GFX bypass.
    ///
    /// # Errors
    ///
    /// Identifies the exact FG/BG or sprite slot and file whose pointer, compression stream, or
    /// 4bpp tile data is invalid.
    pub fn load_super_graphics_bypass(
        &self,
        settings: &ExpandedLevelSettingsRecord,
        layout: GraphicsRomLayout,
    ) -> Result<Option<LoadedSuperGraphicsBypass>, SuperGraphicsIoError> {
        let selection = ExpandedLevelHeader::from(settings).super_graphics_bypass();
        if !selection.enabled {
            return Ok(None);
        }
        let foreground_background = selection
            .foreground_background
            .iter()
            .copied()
            .enumerate()
            .map(|(slot, file)| {
                self.load_graphics_file(usize::from(file), layout)
                    .map_err(|error| SuperGraphicsIoError::ForegroundBackground {
                        slot,
                        file,
                        error,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let sprites = selection
            .sprites
            .iter()
            .copied()
            .enumerate()
            .map(|(slot, file)| {
                self.load_graphics_file(usize::from(file), layout)
                    .map_err(|error| SuperGraphicsIoError::Sprite { slot, file, error })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(LoadedSuperGraphicsBypass {
            selection,
            foreground_background,
            sprites,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GraphicsCompression, LevelPointerTable};
    use lm_codec::encode_lz2;
    use lm_graphics::IndexedTile;
    use lm_level::SuperGraphicsBypass;
    use lm_rom::{Mapper, RomImage};

    #[test]
    fn resolves_all_ten_enabled_bypass_files_in_native_slot_order() {
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([3; IndexedTile::PIXEL_COUNT])],
        };
        let encoded = encode_lz2(&graphics.encode().unwrap());
        let mut bytes = vec![0xff; 0x8000];
        for slot in 0..10 {
            let pointer = 0x20 + slot * 3;
            bytes[pointer..pointer + 3].copy_from_slice(&[0x00, 0x81, 0x80]);
        }
        bytes[0x100..0x100 + encoded.len()].copy_from_slice(&encoded);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let layout = GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x20,
                entries: 10,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
        };
        let selection = SuperGraphicsBypass {
            enabled: true,
            foreground_background: [0, 1, 2, 3, 4, 5],
            sprites: [6, 7, 8, 9],
        };
        let mut header = ExpandedLevelHeader::default();
        header.set_super_graphics_bypass(selection).unwrap();
        let settings = ExpandedLevelSettingsRecord::from(header);
        let loaded = project
            .load_super_graphics_bypass(&settings, layout)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.selection, selection);
        assert_eq!(loaded.foreground_background, vec![graphics.clone(); 6]);
        assert_eq!(loaded.sprites, vec![graphics; 4]);
    }

    #[test]
    fn disabled_bypass_does_not_read_graphics_pointers() {
        let project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
        let settings = ExpandedLevelSettingsRecord::from(ExpandedLevelHeader::default());
        let layout = GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: usize::MAX,
                entries: 0,
                stride: 0,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0,
            maximum_decompressed_len: 0,
        };
        assert_eq!(
            project
                .load_super_graphics_bypass(&settings, layout)
                .unwrap(),
            None
        );
    }
}
