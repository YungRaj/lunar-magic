//! Lunar Magic-compatible decoding of SNES graphics-set and screen-map Map16 imports.

use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile};
use lm_level::{Map16Page, Map16Tile, Subtile};
use std::fmt;

use crate::is_lunar_magic_blank_map16_tile;

pub const SNES_TILESET_GRAPHICS_LEN: usize = 0x8000;
pub const SNES_TILESET_MAP_LEN: usize = 0x800;
pub const SNES_TILESET_PALETTE_ROW_LEN: usize = 0x20;
const TILE_COUNT: usize = 0x400;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnesMap16TilesetImport {
    pub source_graphics: GraphicsFile4bpp,
    pub source_tilemap: Vec<u16>,
    pub palette_row: Option<[Bgr555; 16]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedSnesMap16Tileset {
    pub graphics: GraphicsFile4bpp,
    pub page: Map16Page,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnesMap16DefinitionPlacement {
    Direct,
    DeduplicatedIntoBlankDefinitions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedSnesMap16Page {
    /// Global Map16 index selected for each of the 256 imported source definitions.
    pub assignments: Vec<u16>,
    pub written_definitions: usize,
}

impl SnesMap16TilesetImport {
    /// Decodes the three original file-dialog products without changing editor state.
    ///
    /// Lunar Magic clears a complete `$8000`-byte source buffer before reading at most that many
    /// graphics bytes. Rust accepts the same short graphics shape, but rejects an overlong source
    /// instead of silently truncating it. The screen map and optional palette row must be complete;
    /// accepting their truncated native reads would expose uninitialized stack bytes.
    pub fn decode(
        graphics: &[u8],
        tilemap: &[u8],
        palette_row: Option<&[u8]>,
    ) -> Result<Self, SnesMap16TilesetImportError> {
        if graphics.len() > SNES_TILESET_GRAPHICS_LEN {
            return Err(SnesMap16TilesetImportError::GraphicsTooLong(graphics.len()));
        }
        if tilemap.len() != SNES_TILESET_MAP_LEN {
            return Err(SnesMap16TilesetImportError::TilemapLength(tilemap.len()));
        }
        if let Some(row) = palette_row
            && row.len() != SNES_TILESET_PALETTE_ROW_LEN
        {
            return Err(SnesMap16TilesetImportError::PaletteRowLength(row.len()));
        }

        let mut padded = vec![0; SNES_TILESET_GRAPHICS_LEN];
        padded[..graphics.len()].copy_from_slice(graphics);
        let source_graphics = GraphicsFile4bpp::decode(&padded)
            .map_err(|error| SnesMap16TilesetImportError::Graphics(error.to_string()))?;
        let source_tilemap = tilemap
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect();
        let palette_row = palette_row.map(|row| {
            std::array::from_fn(|index| {
                let offset = index * 2;
                Bgr555(u16::from_le_bytes([row[offset], row[offset + 1]]))
            })
        });
        Ok(Self {
            source_graphics,
            source_tilemap,
            palette_row,
        })
    }

    /// Applies the active 1,024-entry graphics remap and assembles native page geometry.
    ///
    /// Only each screen word's low ten tile bits are replaced. Palette, priority, and flip bits
    /// remain exact. Referenced graphics are copied in screen traversal order, reproducing the
    /// original's last-write behavior when a non-bijective remap aliases destinations.
    pub fn materialize(
        &self,
        remap: &[u16; TILE_COUNT],
    ) -> Result<MaterializedSnesMap16Tileset, SnesMap16TilesetImportError> {
        if self.source_graphics.tiles.len() != TILE_COUNT || self.source_tilemap.len() != TILE_COUNT
        {
            return Err(SnesMap16TilesetImportError::InternalShape);
        }
        let mut graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0; IndexedTile::PIXEL_COUNT]); TILE_COUNT],
        };
        let mut tilemap = Vec::with_capacity(TILE_COUNT);
        for word in self.source_tilemap.iter().copied() {
            let source = usize::from(word & 0x03ff);
            let destination = usize::from(remap[source]);
            if destination >= TILE_COUNT {
                return Err(SnesMap16TilesetImportError::RemapTarget {
                    source,
                    destination,
                });
            }
            graphics.tiles[destination] = self.source_graphics.tiles[source].clone();
            tilemap.push((word & 0xfc00) | remap[source]);
        }

        let tiles = (0..Map16Page::TILE_COUNT)
            .map(|index| {
                let top_left = (index >> 4) * 0x40 + (index & 0x0f) * 2;
                Map16Tile {
                    top_left: Subtile(tilemap[top_left]),
                    top_right: Subtile(tilemap[top_left + 1]),
                    bottom_left: Subtile(tilemap[top_left + 0x20]),
                    bottom_right: Subtile(tilemap[top_left + 0x21]),
                    // Native allocation replaces only the four graphics words.
                    acts_like: 0,
                }
            })
            .collect();
        let page = Map16Page::new(tiles).map_err(|_| SnesMap16TilesetImportError::InternalShape)?;
        Ok(MaterializedSnesMap16Tileset { graphics, page })
    }
}

impl MaterializedSnesMap16Tileset {
    /// Applies the materialized definitions to one existing page while retaining Acts Like.
    ///
    /// The optimized path reproduces Lunar Magic's failure-atomic, page-local stable
    /// deduplication: equal four-word definitions share the first source's assignment, unique
    /// definitions occupy blank targets in ascending order, and an insufficient blank count leaves
    /// the page unchanged. Returned global assignments are the index grid used by the original
    /// background-page paste path.
    pub fn apply_to_page(
        &self,
        target: &mut Map16Page,
        page_number: u8,
        placement: SnesMap16DefinitionPlacement,
    ) -> Result<AppliedSnesMap16Page, SnesMap16TilesetImportError> {
        if self.page.tiles.len() != Map16Page::TILE_COUNT
            || target.tiles.len() != Map16Page::TILE_COUNT
        {
            return Err(SnesMap16TilesetImportError::InternalShape);
        }
        let page_base = u16::from(page_number) << 8;
        match placement {
            SnesMap16DefinitionPlacement::Direct => {
                for (destination, source) in target.tiles.iter_mut().zip(&self.page.tiles) {
                    copy_graphics(destination, *source);
                }
                Ok(AppliedSnesMap16Page {
                    assignments: (0_u16..=255).map(|index| page_base | index).collect(),
                    written_definitions: Map16Page::TILE_COUNT,
                })
            }
            SnesMap16DefinitionPlacement::DeduplicatedIntoBlankDefinitions => {
                let mut canonical_sources = Vec::with_capacity(Map16Page::TILE_COUNT);
                let mut source_to_canonical = Vec::with_capacity(Map16Page::TILE_COUNT);
                for source in &self.page.tiles {
                    let canonical = canonical_sources
                        .iter()
                        .position(|candidate| same_graphics(*candidate, *source))
                        .unwrap_or_else(|| {
                            canonical_sources.push(*source);
                            canonical_sources.len() - 1
                        });
                    source_to_canonical.push(canonical);
                }
                let blanks: Vec<_> = target
                    .tiles
                    .iter()
                    .copied()
                    .enumerate()
                    .filter_map(|(index, tile)| {
                        is_lunar_magic_blank_map16_tile(tile).then_some(index)
                    })
                    .collect();
                if blanks.len() < canonical_sources.len() {
                    return Err(SnesMap16TilesetImportError::NotEnoughBlankDefinitions {
                        available: blanks.len(),
                        needed: canonical_sources.len(),
                    });
                }
                for (&destination, source) in blanks.iter().zip(&canonical_sources) {
                    copy_graphics(&mut target.tiles[destination], *source);
                }
                let assignments = source_to_canonical
                    .into_iter()
                    .map(|canonical| page_base | u16::try_from(blanks[canonical]).unwrap())
                    .collect();
                Ok(AppliedSnesMap16Page {
                    assignments,
                    written_definitions: canonical_sources.len(),
                })
            }
        }
    }
}

const fn same_graphics(left: Map16Tile, right: Map16Tile) -> bool {
    left.top_left.0 == right.top_left.0
        && left.top_right.0 == right.top_right.0
        && left.bottom_left.0 == right.bottom_left.0
        && left.bottom_right.0 == right.bottom_right.0
}

const fn copy_graphics(target: &mut Map16Tile, source: Map16Tile) {
    target.top_left = source.top_left;
    target.top_right = source.top_right;
    target.bottom_left = source.bottom_left;
    target.bottom_right = source.bottom_right;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnesMap16TilesetImportError {
    GraphicsTooLong(usize),
    TilemapLength(usize),
    PaletteRowLength(usize),
    Graphics(String),
    RemapTarget { source: usize, destination: usize },
    NotEnoughBlankDefinitions { available: usize, needed: usize },
    InternalShape,
}

impl fmt::Display for SnesMap16TilesetImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid SNES Map16 tileset import: {self:?}")
    }
}

impl std::error::Error for SnesMap16TilesetImportError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LUNAR_MAGIC_BLANK_MAP16_WORD;

    fn encoded_tile(fill: u8) -> Vec<u8> {
        GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([fill; IndexedTile::PIXEL_COUNT])],
        }
        .encode()
        .unwrap()
    }

    fn tilemap(words: impl IntoIterator<Item = u16>) -> Vec<u8> {
        words.into_iter().flat_map(u16::to_le_bytes).collect()
    }

    fn blank_page() -> Map16Page {
        Map16Page::new(
            (0_u16..256)
                .map(|acts_like| Map16Tile {
                    top_left: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                    top_right: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                    bottom_left: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                    bottom_right: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
                    acts_like,
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn decode_zero_pads_short_graphics_and_reads_optional_native_palette_row() {
        let graphics = encoded_tile(7);
        let map = tilemap((0_u16..0x400).map(|index| index | 0xc000));
        let palette = tilemap(0_u16..16);
        let decoded = SnesMap16TilesetImport::decode(&graphics, &map, Some(&palette)).unwrap();
        assert_eq!(decoded.source_graphics.tiles.len(), 0x400);
        assert_eq!(decoded.source_graphics.tiles[0].pixels(), &[7; 64]);
        assert_eq!(decoded.source_graphics.tiles[1].pixels(), &[0; 64]);
        assert_eq!(decoded.source_tilemap[0x3ff], 0xc3ff);
        assert_eq!(decoded.palette_row.unwrap()[15], Bgr555(15));
    }

    #[test]
    fn materialization_preserves_attributes_and_native_32_by_32_quadrants() {
        let graphics = [encoded_tile(1), encoded_tile(2)].concat();
        let mut words = vec![0_u16; 0x400];
        words[0] = 0x0400;
        words[1] = 0x4801;
        words[0x20] = 0x8401;
        words[0x21] = 0xc000;
        words[2] = 0x0001;
        let decoded = SnesMap16TilesetImport::decode(&graphics, &tilemap(words), None).unwrap();
        let mut remap = std::array::from_fn(|index| u16::try_from(index).unwrap());
        remap[0] = 9;
        remap[1] = 10;
        let output = decoded.materialize(&remap).unwrap();
        assert_eq!(output.page.tiles[0].top_left.0, 0x0409);
        assert_eq!(output.page.tiles[0].top_right.0, 0x480a);
        assert_eq!(output.page.tiles[0].bottom_left.0, 0x840a);
        assert_eq!(output.page.tiles[0].bottom_right.0, 0xc009);
        assert_eq!(output.page.tiles[1].top_left.0, 10);
        assert_eq!(output.graphics.tiles[9].pixels(), &[1; 64]);
        assert_eq!(output.graphics.tiles[10].pixels(), &[2; 64]);
    }

    #[test]
    fn aliased_remap_uses_last_source_and_malformed_shapes_reject() {
        let graphics = [encoded_tile(3), encoded_tile(4)].concat();
        let mut words = vec![1_u16; 0x400];
        words[0] = 0;
        let decoded = SnesMap16TilesetImport::decode(&graphics, &tilemap(words), None).unwrap();
        let mut remap = std::array::from_fn(|index| u16::try_from(index).unwrap());
        remap[0] = 7;
        remap[1] = 7;
        assert_eq!(
            decoded.materialize(&remap).unwrap().graphics.tiles[7].pixels(),
            &[4; 64]
        );
        remap[1] = 0x400;
        assert!(matches!(
            decoded.materialize(&remap),
            Err(SnesMap16TilesetImportError::RemapTarget {
                source: 1,
                destination: 0x400
            })
        ));
        assert!(SnesMap16TilesetImport::decode(&vec![0; 0x8001], &vec![0; 0x800], None).is_err());
        assert!(SnesMap16TilesetImport::decode(&[], &vec![0; 0x7ff], None).is_err());
        assert!(SnesMap16TilesetImport::decode(&[], &vec![0; 0x800], Some(&[0; 31])).is_err());
    }

    #[test]
    fn direct_page_application_replaces_only_graphics_and_returns_global_index_grid() {
        let decoded =
            SnesMap16TilesetImport::decode(&encoded_tile(3), &tilemap(vec![0x6400; 0x400]), None)
                .unwrap();
        let materialized = decoded
            .materialize(&std::array::from_fn(|index| index as u16))
            .unwrap();
        let mut target = blank_page();
        let result = materialized
            .apply_to_page(&mut target, 0x82, SnesMap16DefinitionPlacement::Direct)
            .unwrap();
        assert_eq!(result.assignments[0], 0x8200);
        assert_eq!(result.assignments[255], 0x82ff);
        assert_eq!(result.written_definitions, 256);
        assert_eq!(target.tiles[17].top_left.0, 0x6400);
        assert_eq!(target.tiles[17].acts_like, 17);
    }

    #[test]
    fn optimized_page_application_is_stable_deduplicated_and_failure_atomic() {
        let mut words = vec![0_u16; 0x400];
        // Definition one differs from definition zero in its top-left quadrant only.
        words[2] = 1;
        let decoded = SnesMap16TilesetImport::decode(
            &[encoded_tile(1), encoded_tile(2)].concat(),
            &tilemap(words),
            None,
        )
        .unwrap();
        let materialized = decoded
            .materialize(&std::array::from_fn(|index| index as u16))
            .unwrap();
        let mut target = blank_page();
        target.tiles[0].top_left = Subtile(0x1234);
        let result = materialized
            .apply_to_page(
                &mut target,
                3,
                SnesMap16DefinitionPlacement::DeduplicatedIntoBlankDefinitions,
            )
            .unwrap();
        assert_eq!(result.written_definitions, 2);
        assert_eq!(result.assignments[0], 0x0301);
        assert_eq!(result.assignments[1], 0x0302);
        assert_eq!(result.assignments[2], 0x0301);
        assert_eq!(target.tiles[1].acts_like, 1);
        assert_eq!(target.tiles[2].acts_like, 2);

        let mut insufficient = blank_page();
        for tile in insufficient.tiles.iter_mut().skip(1) {
            tile.top_left = Subtile(0x2222);
        }
        let before = insufficient.clone();
        assert_eq!(
            materialized.apply_to_page(
                &mut insufficient,
                0,
                SnesMap16DefinitionPlacement::DeduplicatedIntoBlankDefinitions,
            ),
            Err(SnesMap16TilesetImportError::NotEnoughBlankDefinitions {
                available: 1,
                needed: 2,
            })
        );
        assert_eq!(insufficient, before);
    }
}
