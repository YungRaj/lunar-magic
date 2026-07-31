use crate::{Canvas, CanvasError, Rgba};
use lm_graphics::{IndexedTile, Palette};
use lm_level::{Map16Tile, Subtile};

/// One Map16 cell in world tile coordinates.
///
/// Bits 0–13 select the Map16 definition. Bits 14 and 15 flip the complete 16×16 definition
/// horizontally and vertically, matching Lunar Magic's Layer 2 tilemap words.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeMap16Placement {
    pub x: i32,
    pub y: i32,
    pub word: u16,
}

/// Deterministic native-level framebuffer input.
///
/// `layers` are painted in slice order. Each layer is itself painted in placement order, allowing
/// callers to preserve SMW's object-stream overwrite behavior before rasterization. The camera is
/// expressed in unscaled world pixels.
#[derive(Clone, Copy, Debug)]
pub struct NativeLevelRasterRequest<'a> {
    pub width: usize,
    pub height: usize,
    pub camera_x: i32,
    pub camera_y: i32,
    pub backdrop: Rgba,
    pub layers: &'a [&'a [NativeMap16Placement]],
    pub definitions: &'a [Map16Tile],
    pub tiles: &'a [IndexedTile],
    /// Complete SNES CGRAM order. Map16 palette row `n` addresses colors `n*16..n*16+16`.
    pub palette: &'a Palette,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLevelRasterError {
    Canvas(CanvasError),
    InvalidPaletteLength(usize),
}

impl std::fmt::Display for NativeLevelRasterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "cannot rasterize native level: {self:?}")
    }
}

impl std::error::Error for NativeLevelRasterError {}

impl From<CanvasError> for NativeLevelRasterError {
    fn from(error: CanvasError) -> Self {
        Self::Canvas(error)
    }
}

/// Renders an exact, unscaled RGBA framebuffer from native Map16 placements.
///
/// Missing definitions and graphics tiles are transparent, as they are in the interactive
/// renderer. Color zero is transparent and therefore preserves the backdrop or an earlier layer.
///
/// # Errors
///
/// Rejects an undersized CGRAM palette and invalid output dimensions.
pub fn render_native_level_framebuffer(
    request: NativeLevelRasterRequest<'_>,
) -> Result<Canvas, NativeLevelRasterError> {
    if request.palette.colors.len() < 16 * 8 {
        return Err(NativeLevelRasterError::InvalidPaletteLength(
            request.palette.colors.len(),
        ));
    }
    let pixel_count = request
        .width
        .checked_mul(request.height)
        .ok_or(CanvasError::DimensionOverflow)?;
    let mut canvas = Canvas::from_pixels(
        request.width,
        request.height,
        vec![request.backdrop; pixel_count],
    )?;
    for layer in request.layers {
        for placement in *layer {
            let definition_index = usize::from(placement.word & 0x3fff);
            let Some(definition) = request.definitions.get(definition_index).copied() else {
                continue;
            };
            draw_map16_clipped(
                &mut canvas,
                definition,
                request.tiles,
                request.palette,
                (
                    placement
                        .x
                        .saturating_mul(16)
                        .saturating_sub(request.camera_x),
                    placement
                        .y
                        .saturating_mul(16)
                        .saturating_sub(request.camera_y),
                ),
                (placement.word & 0x4000 != 0, placement.word & 0x8000 != 0),
            );
        }
    }
    Ok(canvas)
}

/// Draws one Lunar Magic standard-sprite preview definition over an existing native framebuffer.
///
/// Sprite preview words address the four ordinary SP slots with a nine-bit tile index and use
/// CGRAM rows 8–15. Bit `$0200` selects Lunar Magic's separate animated-sprite page and must be
/// handled by the caller before invoking this ordinary-page renderer.
pub fn draw_native_sprite_preview_definition(
    canvas: &mut Canvas,
    subtiles: [u16; 4],
    tiles: &[IndexedTile],
    palette: &Palette,
    target_x: i32,
    target_y: i32,
) {
    draw_native_sprite_preview_definition_pages(
        canvas, subtiles, tiles, tiles, palette, target_x, target_y,
    );
}

/// Draws a sprite preview while resolving bit `$0200` through Lunar Magic's separate display page.
pub fn draw_native_sprite_preview_definition_pages(
    canvas: &mut Canvas,
    subtiles: [u16; 4],
    ordinary_tiles: &[IndexedTile],
    animated_tiles: &[IndexedTile],
    palette: &Palette,
    target_x: i32,
    target_y: i32,
) {
    for (quadrant, word) in subtiles.into_iter().enumerate() {
        let x = quadrant / 2;
        let y = quadrant % 2;
        draw_sprite_subtile_clipped(
            canvas,
            word,
            if word & 0x0200 != 0 {
                animated_tiles
            } else {
                ordinary_tiles
            },
            palette,
            (
                target_x.saturating_add(i32::try_from(x * 8).unwrap_or(i32::MAX)),
                target_y.saturating_add(i32::try_from(y * 8).unwrap_or(i32::MAX)),
            ),
        );
    }
}

fn draw_map16_clipped(
    canvas: &mut Canvas,
    definition: Map16Tile,
    tiles: &[IndexedTile],
    palette: &Palette,
    target: (i32, i32),
    outer_flips: (bool, bool),
) {
    let (target_x, target_y) = target;
    let (horizontal_flip, vertical_flip) = outer_flips;
    let subtiles = [
        definition.top_left,
        definition.top_right,
        definition.bottom_left,
        definition.bottom_right,
    ];
    for output_y in 0..2 {
        for output_x in 0..2 {
            let source_x = if horizontal_flip {
                1 - output_x
            } else {
                output_x
            };
            let source_y = if vertical_flip {
                1 - output_y
            } else {
                output_y
            };
            let source_index = source_y * 2 + source_x;
            let subtile = subtiles[source_index];
            draw_subtile_clipped(
                canvas,
                subtile,
                tiles,
                palette,
                (
                    target_x.saturating_add(i32::try_from(output_x * 8).unwrap_or(i32::MAX)),
                    target_y.saturating_add(i32::try_from(output_y * 8).unwrap_or(i32::MAX)),
                ),
                outer_flips,
            );
        }
    }
}

fn draw_subtile_clipped(
    canvas: &mut Canvas,
    subtile: Subtile,
    tiles: &[IndexedTile],
    palette: &Palette,
    target: (i32, i32),
    outer_flips: (bool, bool),
) {
    let (target_x, target_y) = target;
    let (horizontal_flip, vertical_flip) = outer_flips;
    let Some(tile) = tiles.get(usize::from(subtile.tile_number())) else {
        return;
    };
    let x_flip = subtile.x_flip() ^ horizontal_flip;
    let y_flip = subtile.y_flip() ^ vertical_flip;
    let palette_base = usize::from(subtile.palette()) * 16;
    for y in 0..8 {
        for x in 0..8 {
            let source_x = if x_flip { 7 - x } else { x };
            let source_y = if y_flip { 7 - y } else { y };
            let Some(index) = tile.pixel(source_x, source_y) else {
                continue;
            };
            if index == 0 {
                continue;
            }
            let Some(color) = palette
                .colors
                .get(palette_base.saturating_add(usize::from(index)))
            else {
                continue;
            };
            let Some(output_x) = target_x.checked_add(i32::try_from(x).unwrap_or(i32::MAX)) else {
                continue;
            };
            let Some(output_y) = target_y.checked_add(i32::try_from(y).unwrap_or(i32::MAX)) else {
                continue;
            };
            let (Ok(output_x), Ok(output_y)) =
                (usize::try_from(output_x), usize::try_from(output_y))
            else {
                continue;
            };
            let rgb = color.to_rgb8();
            canvas.set(
                output_x,
                output_y,
                Rgba {
                    red: rgb.red,
                    green: rgb.green,
                    blue: rgb.blue,
                    alpha: 255,
                },
            );
        }
    }
}

fn draw_sprite_subtile_clipped(
    canvas: &mut Canvas,
    word: u16,
    tiles: &[IndexedTile],
    palette: &Palette,
    target: (i32, i32),
) {
    let (target_x, target_y) = target;
    let Some(tile) = tiles.get(usize::from(word & 0x01ff)) else {
        return;
    };
    let x_flip = word & 0x4000 != 0;
    let y_flip = word & 0x8000 != 0;
    let palette_base = (8 + usize::from((word >> 10) & 7)) * 16;
    for y in 0..8 {
        for x in 0..8 {
            let source_x = if x_flip { 7 - x } else { x };
            let source_y = if y_flip { 7 - y } else { y };
            let Some(index) = tile.pixel(source_x, source_y) else {
                continue;
            };
            if index == 0 {
                continue;
            }
            let Some(color) = palette
                .colors
                .get(palette_base.saturating_add(usize::from(index)))
            else {
                continue;
            };
            let Some(output_x) = target_x.checked_add(i32::try_from(x).unwrap_or(i32::MAX)) else {
                continue;
            };
            let Some(output_y) = target_y.checked_add(i32::try_from(y).unwrap_or(i32::MAX)) else {
                continue;
            };
            let (Ok(output_x), Ok(output_y)) =
                (usize::try_from(output_x), usize::try_from(output_y))
            else {
                continue;
            };
            let rgb = color.to_rgb8();
            canvas.set(
                output_x,
                output_y,
                Rgba {
                    red: rgb.red,
                    green: rgb.green,
                    blue: rgb.blue,
                    alpha: 255,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::Bgr555;

    fn definition(words: [u16; 4]) -> Map16Tile {
        Map16Tile {
            top_left: Subtile(words[0]),
            top_right: Subtile(words[1]),
            bottom_left: Subtile(words[2]),
            bottom_right: Subtile(words[3]),
            acts_like: 0,
        }
    }

    fn palette() -> Palette {
        let mut colors = vec![Bgr555(0); 128];
        colors[1] = Bgr555(0x001f);
        colors[2] = Bgr555(0x03e0);
        colors[3] = Bgr555(0x7c00);
        colors[4] = Bgr555(0x7fff);
        Palette { colors }
    }

    fn solid(index: u8) -> IndexedTile {
        IndexedTile::new([index; IndexedTile::PIXEL_COUNT])
    }

    #[test]
    fn layer_order_transparency_camera_and_clipping_are_exact() {
        let definitions = [definition([0, 0, 0, 0]), definition([1, 1, 1, 1])];
        let tiles = [solid(1), solid(2)];
        let back = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
        }];
        let front = [NativeMap16Placement {
            x: 1,
            y: 0,
            word: 1,
        }];
        let layers: [&[NativeMap16Placement]; 2] = [&back, &front];
        let backdrop = Rgba {
            red: 3,
            green: 4,
            blue: 5,
            alpha: 255,
        };
        let canvas = render_native_level_framebuffer(NativeLevelRasterRequest {
            width: 17,
            height: 8,
            camera_x: 15,
            camera_y: 0,
            backdrop,
            layers: &layers,
            definitions: &definitions,
            tiles: &tiles,
            palette: &palette(),
        })
        .unwrap();
        assert_eq!(canvas.get(0, 0).unwrap().red, 255);
        assert_eq!(canvas.get(1, 0).unwrap().green, 255);
        assert_eq!(canvas.get(16, 0).unwrap().green, 255);
    }

    #[test]
    fn sprite_preview_uses_sp_tile_order_and_cgram_rows() {
        let mut canvas = Canvas::try_new(16, 16).unwrap();
        let mut tiles = vec![solid(0); 0x200];
        tiles[1] = solid(1);
        tiles[2] = solid(2);
        tiles[3] = solid(3);
        tiles[4] = solid(4);
        let mut palette = Palette {
            colors: vec![Bgr555(0); 16 * 16],
        };
        palette.colors[8 * 16 + 1] = Bgr555(0x001f);
        palette.colors[8 * 16 + 2] = Bgr555(0x03e0);
        palette.colors[8 * 16 + 3] = Bgr555(0x7c00);
        palette.colors[8 * 16 + 4] = Bgr555(0x7fff);

        draw_native_sprite_preview_definition(&mut canvas, [1, 2, 3, 4], &tiles, &palette, 0, 0);

        assert_eq!(canvas.get(0, 0).unwrap().red, 255);
        assert_eq!(canvas.get(0, 8).unwrap().green, 255);
        assert_eq!(canvas.get(8, 0).unwrap().blue, 255);
        assert_eq!(
            canvas.get(8, 8).unwrap(),
            Rgba {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            }
        );

        let mut animated_tiles = tiles.clone();
        animated_tiles[1] = solid(4);
        draw_native_sprite_preview_definition_pages(
            &mut canvas,
            [0x0201, 2, 3, 4],
            &tiles,
            &animated_tiles,
            &palette,
            0,
            0,
        );
        assert_eq!(
            canvas.get(0, 0).unwrap(),
            Rgba {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            }
        );
    }

    #[test]
    fn complete_map16_flips_swap_quadrants_and_compose_subtile_flips() {
        let mut corner = [0; IndexedTile::PIXEL_COUNT];
        corner[0] = 1;
        let tiles = [IndexedTile::new(corner), solid(1), solid(1), solid(1)];
        let definitions = [definition([0, 1, 2, 3])];
        let placements = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0xc000,
        }];
        let layers: [&[NativeMap16Placement]; 1] = [&placements];
        let canvas = render_native_level_framebuffer(NativeLevelRasterRequest {
            width: 16,
            height: 16,
            camera_x: 0,
            camera_y: 0,
            backdrop: Rgba::default(),
            layers: &layers,
            definitions: &definitions,
            tiles: &tiles,
            palette: &palette(),
        })
        .unwrap();
        assert_eq!(canvas.get(15, 15).unwrap().red, 255);
        assert_eq!(canvas.get(8, 8), Some(Rgba::default()));
    }

    #[test]
    fn canonical_map16_fields_land_in_visual_row_major_quadrants() {
        let definitions = [definition([0, 1, 2, 3])];
        let tiles = [solid(1), solid(2), solid(3), solid(4)];
        let placements = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
        }];
        let layers: [&[NativeMap16Placement]; 1] = [&placements];
        let canvas = render_native_level_framebuffer(NativeLevelRasterRequest {
            width: 16,
            height: 16,
            camera_x: 0,
            camera_y: 0,
            backdrop: Rgba::default(),
            layers: &layers,
            definitions: &definitions,
            tiles: &tiles,
            palette: &palette(),
        })
        .unwrap();
        assert_eq!(canvas.get(0, 0).unwrap().red, 255);
        assert_eq!(canvas.get(8, 0).unwrap().green, 255);
        assert_eq!(canvas.get(0, 8).unwrap().blue, 255);
        assert_eq!(
            canvas.get(8, 8),
            Some(Rgba {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            })
        );
    }

    #[test]
    fn palette_and_canvas_failures_are_typed() {
        let layers: [&[NativeMap16Placement]; 0] = [];
        let request = NativeLevelRasterRequest {
            width: 1,
            height: 1,
            camera_x: 0,
            camera_y: 0,
            backdrop: Rgba::default(),
            layers: &layers,
            definitions: &[],
            tiles: &[],
            palette: &Palette { colors: vec![] },
        };
        assert_eq!(
            render_native_level_framebuffer(request),
            Err(NativeLevelRasterError::InvalidPaletteLength(0))
        );
    }
}
