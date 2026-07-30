//! Atomic native persistence for every ROM domain changed by one bitmap-to-Map16 import.

use crate::{
    GraphicsIoError, GraphicsRomLayout, GraphicsSaveOptions, Map16IoError, Map16RomLayout,
    Map16SaveOptions, PaletteIoError, PaletteRomLayout, PaletteSaveOptions, PayloadSaveError,
    PayloadSaveResult, Project, graphics_io::graphics_save_request,
    map16_io::map16_page_save_requests, palette_io::palette_save_request,
};
use lm_graphics::{GraphicsFile4bpp, Palette};
use lm_level::Map16Page;
use std::fmt;

pub struct Map16BitmapGraphicsSave<'a> {
    pub file_number: usize,
    pub graphics: &'a GraphicsFile4bpp,
    pub layout: GraphicsRomLayout,
    pub options: &'a GraphicsSaveOptions,
}

pub struct Map16BitmapPaletteSave<'a> {
    pub palette_number: usize,
    pub palette: &'a Palette,
    pub layout: PaletteRomLayout,
    pub options: &'a PaletteSaveOptions,
}

pub struct Map16BitmapPageSave<'a> {
    pub page_number: usize,
    pub page: &'a Map16Page,
    pub layout: Map16RomLayout,
    pub options: &'a Map16SaveOptions,
}

pub struct Map16BitmapRomSave<'a> {
    pub description: &'a str,
    pub graphics: &'a [Map16BitmapGraphicsSave<'a>],
    pub palette: Map16BitmapPaletteSave<'a>,
    pub map16: &'a [Map16BitmapPageSave<'a>],
    pub checksum_field: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedMap16BitmapImport {
    pub graphics: Vec<PayloadSaveResult>,
    pub palette: PayloadSaveResult,
    pub map16_graphics: Vec<PayloadSaveResult>,
    pub map16_acts_like: Vec<PayloadSaveResult>,
}

#[derive(Debug)]
pub enum Map16BitmapRomSaveError {
    EmptyDescription,
    Graphics(GraphicsIoError),
    Palette(PaletteIoError),
    Map16(Map16IoError),
    Save(PayloadSaveError),
    UnexpectedResultCount(usize),
}

impl fmt::Display for Map16BitmapRomSaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native Map16 bitmap import save failed: {self:?}"
        )
    }
}

impl std::error::Error for Map16BitmapRomSaveError {}

impl From<GraphicsIoError> for Map16BitmapRomSaveError {
    fn from(value: GraphicsIoError) -> Self {
        Self::Graphics(value)
    }
}
impl From<PaletteIoError> for Map16BitmapRomSaveError {
    fn from(value: PaletteIoError) -> Self {
        Self::Palette(value)
    }
}
impl From<Map16IoError> for Map16BitmapRomSaveError {
    fn from(value: Map16IoError) -> Self {
        Self::Map16(value)
    }
}
impl From<PayloadSaveError> for Map16BitmapRomSaveError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl Project {
    /// Saves changed GFX/ExGFX files, a palette, and both Map16 planes in one undoable commit.
    ///
    /// Every payload and pointer is prepared before publication. A late compression, allocation,
    /// mapping, pointer, or checksum failure leaves both ROM bytes and history unchanged.
    ///
    /// # Errors
    ///
    /// Rejects missing persistence targets, malformed domain shapes, and grouped save failures.
    pub fn save_map16_bitmap_import(
        &mut self,
        save: &Map16BitmapRomSave<'_>,
    ) -> Result<SavedMap16BitmapImport, Map16BitmapRomSaveError> {
        if save.description.trim().is_empty() {
            return Err(Map16BitmapRomSaveError::EmptyDescription);
        }
        let mut requests = Vec::with_capacity(save.graphics.len() + 1 + save.map16.len() * 2);
        for graphics in save.graphics {
            requests.push(graphics_save_request(
                graphics.file_number,
                graphics.graphics,
                graphics.layout,
                graphics.options,
            )?);
        }
        requests.push(palette_save_request(
            save.palette.palette_number,
            save.palette.palette,
            save.palette.layout,
            save.palette.options,
        )?);
        for page in save.map16 {
            requests.extend(map16_page_save_requests(
                page.page_number,
                page.page,
                page.layout,
                page.options,
            )?);
        }

        let graphics_count = save.graphics.len();
        let mut results = self.save_tagged_payloads_with_checksum(
            save.description,
            &requests,
            save.checksum_field,
        )?;
        if results.len() != graphics_count + 1 + save.map16.len() * 2 {
            return Err(Map16BitmapRomSaveError::UnexpectedResultCount(
                results.len(),
            ));
        }
        let mut domain_results = results.split_off(graphics_count);
        let palette = domain_results.remove(0);
        let mut map16_graphics = Vec::with_capacity(save.map16.len());
        let mut map16_acts_like = Vec::with_capacity(save.map16.len());
        for pair in domain_results.chunks_exact(2) {
            map16_graphics.push(pair[0].clone());
            map16_acts_like.push(pair[1].clone());
        }
        Ok(SavedMap16BitmapImport {
            graphics: results,
            palette,
            map16_graphics,
            map16_acts_like,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GraphicsCompression, LevelPointerTable};
    use lm_graphics::{Bgr555, IndexedTile};
    use lm_level::{Map16Tile, Subtile};
    use lm_rats::{AllocationPolicy, ProtectedRange};
    use lm_rom::{Mapper, RomImage, compute_snes_checksum};

    const CHECKSUM: usize = 0x7fdc;

    fn policy(search: std::ops::Range<usize>) -> AllocationPolicy {
        AllocationPolicy {
            search,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![
                ProtectedRange(0x20..0x23),
                ProtectedRange(0x30..0x33),
                ProtectedRange(0x40..0x46),
                ProtectedRange(0x50..0x56),
                ProtectedRange(0x7fc0..0x8000),
            ],
        }
    }

    fn graphics_layout() -> GraphicsRomLayout {
        GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x20,
                entries: 1,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x1000,
        }
    }

    fn palette_layout() -> PaletteRomLayout {
        PaletteRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x30,
                entries: 1,
                stride: 3,
            },
            colors_per_palette: 16,
        }
    }

    fn map16_layout() -> Map16RomLayout {
        Map16RomLayout {
            mapper: Mapper::LoRom,
            graphics: LevelPointerTable {
                offset: 0x40,
                entries: 2,
                stride: 3,
            },
            acts_like: LevelPointerTable {
                offset: 0x50,
                entries: 2,
                stride: 3,
            },
        }
    }

    fn page() -> Map16Page {
        let mut tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        tiles[0] = Map16Tile {
            top_left: Subtile(0x2200),
            top_right: Subtile(0x2200),
            bottom_left: Subtile(0x2200),
            bottom_right: Subtile(0x2200),
            acts_like: 0x130,
        };
        Map16Page::new(tiles).unwrap()
    }

    #[test]
    fn every_import_domain_commits_reopens_and_undoes_as_one_operation() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let allocation = policy(0x100..0x7000);
        let graphics_options = GraphicsSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let palette_options = PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let map16_options = Map16SaveOptions {
            graphics_allocation: allocation.clone(),
            acts_like_allocation: allocation,
            previous_graphics: None,
            previous_acts_like: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([7; 64]); 0x80],
        };
        let palette = Palette {
            colors: vec![Bgr555(0x1234); 16],
        };
        let map16 = page();
        let graphics_saves = [Map16BitmapGraphicsSave {
            file_number: 0,
            graphics: &graphics,
            layout: graphics_layout(),
            options: &graphics_options,
        }];
        let map16_saves = [Map16BitmapPageSave {
            page_number: 0,
            page: &map16,
            layout: map16_layout(),
            options: &map16_options,
        }];
        let result = project
            .save_map16_bitmap_import(&Map16BitmapRomSave {
                description: "import bitmap as Map16",
                graphics: &graphics_saves,
                palette: Map16BitmapPaletteSave {
                    palette_number: 0,
                    palette: &palette,
                    layout: palette_layout(),
                    options: &palette_options,
                },
                map16: &map16_saves,
                checksum_field: CHECKSUM,
            })
            .unwrap();

        assert_eq!(result.graphics.len(), 1);
        assert_eq!(
            project.load_graphics_file(0, graphics_layout()).unwrap(),
            graphics
        );
        assert_eq!(project.load_palette(0, palette_layout()).unwrap(), palette);
        assert_eq!(project.load_map16_page(0, map16_layout()).unwrap(), map16);
        let stored = project.rom.read(CHECKSUM, 4).unwrap();
        let computed = compute_snes_checksum(project.rom.logical_bytes(), CHECKSUM).unwrap();
        assert_eq!(
            u16::from_le_bytes([stored[2], stored[3]]),
            computed.checksum
        );
        assert_eq!(project.history.undo_len(), 1);
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn multiple_touched_map16_pages_commit_and_undo_atomically() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let allocation = policy(0x100..0x7000);
        let palette_options = PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let map16_options = Map16SaveOptions {
            graphics_allocation: allocation.clone(),
            acts_like_allocation: allocation,
            previous_graphics: None,
            previous_acts_like: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let palette = Palette {
            colors: vec![Bgr555(0x1234); 16],
        };
        let first = page();
        let mut second = page();
        second.tiles[0].top_left = Subtile(0x345);
        second.tiles[0].acts_like = 0x131;
        let map16_saves = [
            Map16BitmapPageSave {
                page_number: 0,
                page: &first,
                layout: map16_layout(),
                options: &map16_options,
            },
            Map16BitmapPageSave {
                page_number: 1,
                page: &second,
                layout: map16_layout(),
                options: &map16_options,
            },
        ];

        let result = project
            .save_map16_bitmap_import(&Map16BitmapRomSave {
                description: "import bitmap across Map16 pages",
                graphics: &[],
                palette: Map16BitmapPaletteSave {
                    palette_number: 0,
                    palette: &palette,
                    layout: palette_layout(),
                    options: &palette_options,
                },
                map16: &map16_saves,
                checksum_field: CHECKSUM,
            })
            .unwrap();

        assert_eq!(result.map16_graphics.len(), 2);
        assert_eq!(result.map16_acts_like.len(), 2);
        assert_eq!(project.load_map16_page(0, map16_layout()).unwrap(), first);
        assert_eq!(project.load_map16_page(1, map16_layout()).unwrap(), second);
        assert_eq!(project.history.undo_len(), 1);
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn tile_reusing_import_can_commit_without_changing_graphics_files() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let allocation = policy(0x100..0x7000);
        let palette_options = PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let map16_options = Map16SaveOptions {
            graphics_allocation: allocation.clone(),
            acts_like_allocation: allocation,
            previous_graphics: None,
            previous_acts_like: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let palette = Palette {
            colors: vec![Bgr555(0x1234); 16],
        };
        let map16 = page();
        let map16_saves = [Map16BitmapPageSave {
            page_number: 0,
            page: &map16,
            layout: map16_layout(),
            options: &map16_options,
        }];

        let result = project
            .save_map16_bitmap_import(&Map16BitmapRomSave {
                description: "import bitmap using existing graphics",
                graphics: &[],
                palette: Map16BitmapPaletteSave {
                    palette_number: 0,
                    palette: &palette,
                    layout: palette_layout(),
                    options: &palette_options,
                },
                map16: &map16_saves,
                checksum_field: CHECKSUM,
            })
            .unwrap();

        assert!(result.graphics.is_empty());
        assert_eq!(project.load_palette(0, palette_layout()).unwrap(), palette);
        assert_eq!(project.load_map16_page(0, map16_layout()).unwrap(), map16);
        assert_eq!(project.history.undo_len(), 1);
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn late_map16_allocation_failure_preserves_rom_and_history() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let allocation = policy(0x100..0x7000);
        let graphics_options = GraphicsSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let palette_options = PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let map16_options = Map16SaveOptions {
            graphics_allocation: allocation,
            acts_like_allocation: policy(0x100..0x110),
            previous_graphics: None,
            previous_acts_like: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([7; 64]); 0x80],
        };
        let palette = Palette {
            colors: vec![Bgr555(0x1234); 16],
        };
        let map16 = page();
        let graphics_saves = [Map16BitmapGraphicsSave {
            file_number: 0,
            graphics: &graphics,
            layout: graphics_layout(),
            options: &graphics_options,
        }];
        let map16_saves = [Map16BitmapPageSave {
            page_number: 0,
            page: &map16,
            layout: map16_layout(),
            options: &map16_options,
        }];
        assert!(
            project
                .save_map16_bitmap_import(&Map16BitmapRomSave {
                    description: "import bitmap as Map16",
                    graphics: &graphics_saves,
                    palette: Map16BitmapPaletteSave {
                        palette_number: 0,
                        palette: &palette,
                        layout: palette_layout(),
                        options: &palette_options,
                    },
                    map16: &map16_saves,
                    checksum_field: CHECKSUM,
                })
                .is_err()
        );
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }
}
