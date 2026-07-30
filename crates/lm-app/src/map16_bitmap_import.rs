//! Staged cross-domain Map16 bitmap import planning.

use lm_graphics::{
    BitmapImportError, GraphicsFile4bpp, GraphicsOwnership, IndexedBitmapImport,
    PaletteImportError, PaletteOwnership, Rgba8, TransparentPaletteRowImport,
};
use lm_level::{Map16Page, Map16Tile, Subtile};
use std::fmt;

pub const MAP16_BITMAP_WIDTH: usize = 256;
pub const MAP16_BITMAP_HEIGHT: usize = 256;
pub const MAP16_BITMAP_PIXELS: usize = MAP16_BITMAP_WIDTH * MAP16_BITMAP_HEIGHT;
const SUBTILE_PLANE_WIDTH: usize = MAP16_BITMAP_WIDTH / 8;

#[derive(Clone, Copy)]
pub struct Map16BitmapImportRequest<'a> {
    pub pixels: &'a [Rgba8],
    pub palette_row: u8,
    pub acts_like: u16,
    pub palette: &'a lm_graphics::Palette,
    pub palette_ownership: &'a PaletteOwnership,
    pub graphics: &'a GraphicsFile4bpp,
    pub graphics_ownership: &'a GraphicsOwnership,
    pub occupied: &'a [bool],
}

/// All four semantic products of a Map16 bitmap conversion.
///
/// The native dialog previews this value and the commit boundary consumes that same value. No
/// quantization, tile allocation, or Map16 construction is repeated after user approval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16BitmapImportPlan {
    pub palette: lm_graphics::Palette,
    pub graphics: GraphicsFile4bpp,
    pub occupied: Vec<bool>,
    pub page: Map16Page,
    pub indexed_pixels: Vec<u8>,
    pub generated_colors: usize,
    pub newly_occupied_tiles: usize,
}

impl Map16BitmapImportPlan {
    /// Quantizes and materializes one complete 256×256 Map16 page without changing its inputs.
    ///
    /// # Errors
    ///
    /// Rejects wrong pixel counts, unavailable palette rows, protected palette/graphics slots,
    /// invalid ownership maps, exhausted 10-bit graphics space, or malformed Map16 output.
    pub fn prepare(request: Map16BitmapImportRequest<'_>) -> Result<Self, Map16BitmapImportError> {
        if request.pixels.len() != MAP16_BITMAP_PIXELS {
            return Err(Map16BitmapImportError::WrongPixelCount {
                expected: MAP16_BITMAP_PIXELS,
                actual: request.pixels.len(),
            });
        }
        if request.palette_row > 7 {
            return Err(Map16BitmapImportError::PaletteRow(request.palette_row));
        }
        let palette = TransparentPaletteRowImport::quantize(
            request.pixels,
            usize::from(request.palette_row),
            request.palette,
            request.palette_ownership,
        )
        .map_err(Map16BitmapImportError::Palette)?;
        let materialized = IndexedBitmapImport::materialize(
            MAP16_BITMAP_WIDTH,
            MAP16_BITMAP_HEIGHT,
            &palette.indices,
            request.graphics,
            request.graphics_ownership,
            request.occupied,
        )
        .map_err(Map16BitmapImportError::Graphics)?;
        let page = build_page(&materialized, request.palette_row, request.acts_like)?;
        let newly_occupied_tiles = materialized
            .occupied
            .iter()
            .zip(request.occupied)
            .filter(|(after, before)| **after && !**before)
            .count();
        Ok(Self {
            palette: palette.palette,
            graphics: materialized.graphics,
            occupied: materialized.occupied,
            page,
            indexed_pixels: palette.indices,
            generated_colors: palette.generated_colors,
            newly_occupied_tiles,
        })
    }
}

fn build_page(
    imported: &IndexedBitmapImport,
    palette_row: u8,
    acts_like: u16,
) -> Result<Map16Page, Map16BitmapImportError> {
    if imported.width_in_tiles != 32 || imported.height_in_tiles != 32 {
        return Err(Map16BitmapImportError::WrongMaterializedShape {
            width: imported.width_in_tiles,
            height: imported.height_in_tiles,
        });
    }
    let mut tiles = Vec::with_capacity(Map16Page::TILE_COUNT);
    for tile_y in 0..16 {
        for tile_x in 0..16 {
            let top_left = tile_y * 2 * SUBTILE_PLANE_WIDTH + tile_x * 2;
            tiles.push(Map16Tile {
                top_left: descriptor(imported.placements[top_left], palette_row),
                top_right: descriptor(imported.placements[top_left + 1], palette_row),
                bottom_left: descriptor(
                    imported.placements[top_left + SUBTILE_PLANE_WIDTH],
                    palette_row,
                ),
                bottom_right: descriptor(
                    imported.placements[top_left + SUBTILE_PLANE_WIDTH + 1],
                    palette_row,
                ),
                acts_like,
            });
        }
    }
    Map16Page::new(tiles).map_err(|tiles| Map16BitmapImportError::Map16TileCount(tiles.len()))
}

fn descriptor(placement: lm_graphics::ImportedTilePlacement, palette_row: u8) -> Subtile {
    let mut word = placement.tile | (u16::from(palette_row) << 10);
    if placement.x_flip {
        word |= 0x4000;
    }
    if placement.y_flip {
        word |= 0x8000;
    }
    Subtile(word)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16BitmapImportError {
    WrongPixelCount { expected: usize, actual: usize },
    PaletteRow(u8),
    Palette(PaletteImportError),
    Graphics(BitmapImportError),
    WrongMaterializedShape { width: usize, height: usize },
    Map16TileCount(usize),
}

impl fmt::Display for Map16BitmapImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Map16 bitmap import planning failed: {self:?}")
    }
}

impl std::error::Error for Map16BitmapImportError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette};

    #[test]
    fn one_plan_carries_palette_graphics_occupancy_and_map16_results() {
        let pixels = vec![
            Rgba8 {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255,
            };
            MAP16_BITMAP_PIXELS
        ];
        let palette = Palette {
            colors: vec![Bgr555(0); 128],
        };
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0; IndexedTile::PIXEL_COUNT]); 0x300],
        };
        let occupied = vec![false; graphics.tiles.len()];
        let plan = Map16BitmapImportPlan::prepare(Map16BitmapImportRequest {
            pixels: &pixels,
            palette_row: 2,
            acts_like: 0x130,
            palette: &palette,
            palette_ownership: &PaletteOwnership::editable(palette.colors.len()),
            graphics: &graphics,
            graphics_ownership: &GraphicsOwnership::editable(graphics.tiles.len()),
            occupied: &occupied,
        })
        .unwrap();

        assert_eq!(plan.generated_colors, 1);
        assert_eq!(plan.newly_occupied_tiles, 1);
        assert_eq!(plan.page.tiles.len(), 256);
        assert!(plan.page.tiles.iter().all(|tile| tile.acts_like == 0x130));
        assert!(plan.indexed_pixels.iter().all(|pixel| *pixel == 1));
        assert_eq!(plan.page.tiles[0].top_left.0 & 0x1c00, 2 << 10);
    }

    #[test]
    fn failures_do_not_mutate_any_input_domain() {
        let pixels = vec![
            Rgba8 {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            };
            MAP16_BITMAP_PIXELS - 1
        ];
        let palette = Palette {
            colors: vec![Bgr555(0); 128],
        };
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0; IndexedTile::PIXEL_COUNT]); 0x300],
        };
        let occupied = vec![false; graphics.tiles.len()];
        let original_palette = palette.clone();
        let original_graphics = graphics.clone();
        let original_occupied = occupied.clone();

        assert!(matches!(
            Map16BitmapImportPlan::prepare(Map16BitmapImportRequest {
                pixels: &pixels,
                palette_row: 2,
                acts_like: 0,
                palette: &palette,
                palette_ownership: &PaletteOwnership::editable(palette.colors.len()),
                graphics: &graphics,
                graphics_ownership: &GraphicsOwnership::editable(graphics.tiles.len()),
                occupied: &occupied,
            }),
            Err(Map16BitmapImportError::WrongPixelCount { .. })
        ));
        assert_eq!(palette, original_palette);
        assert_eq!(graphics, original_graphics);
        assert_eq!(occupied, original_occupied);
    }
}
