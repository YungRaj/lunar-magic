//! Lunar Magic-compatible decoding of SNES graphics-set and screen-map Map16 imports.

use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile};
use lm_level::{Map16Page, Map16Tile, Subtile};
use std::fmt;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnesMap16TilesetImportError {
    GraphicsTooLong(usize),
    TilemapLength(usize),
    PaletteRowLength(usize),
    Graphics(String),
    RemapTarget { source: usize, destination: usize },
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
}
