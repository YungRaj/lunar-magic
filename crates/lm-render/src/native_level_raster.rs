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
    pub composition: NativeMap16Composition,
}

/// How one nontransparent Map16 pixel combines with the framebuffer beneath it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NativeMap16Composition {
    /// Replace the destination pixel.
    #[default]
    Opaque,
    /// Average source and destination channels after clearing each low bit.
    ///
    /// This is Lunar Magic's `RenderMap16TileToPixelBuffer` averaged display path.
    Average,
    /// Halve each source RGB channel without sampling the destination.
    ///
    /// Lunar Magic uses this for nontransparent Layer 2 background pixels in level modes whose
    /// recovered render-property byte carries bit 6.
    HalfColor,
}

impl NativeMap16Composition {
    fn compose(self, source: Rgba, destination: Rgba) -> Rgba {
        match self {
            Self::Opaque => source,
            Self::Average => Rgba {
                red: (source.red & 0xfe) / 2 + (destination.red & 0xfe) / 2,
                green: (source.green & 0xfe) / 2 + (destination.green & 0xfe) / 2,
                blue: (source.blue & 0xfe) / 2 + (destination.blue & 0xfe) / 2,
                alpha: 255,
            },
            Self::HalfColor => Rgba {
                red: source.red >> 1,
                green: source.green >> 1,
                blue: source.blue >> 1,
                alpha: 255,
            },
        }
    }
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

/// Lunar Magic's per-layer Map16 palette interpretation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NativeMap16PaletteRouting {
    /// Use the three-bit Map16 palette row without modification.
    #[default]
    Direct,
    /// Route rows 0–3 through rows 4–7, preserving rows 4–7.
    ///
    /// Lunar Magic enables this for object-backed Layer 2 when object tileset 3 is active.
    ShiftLowRowsByFour,
}

impl NativeMap16PaletteRouting {
    #[must_use]
    pub const fn palette_row(self, encoded_row: u8) -> u8 {
        match (self, encoded_row) {
            (Self::ShiftLowRowsByFour, row @ 0..=3) => row + 4,
            (_, row) => row,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLevelRasterError {
    Canvas(CanvasError),
    InvalidPaletteLength(usize),
    InvalidLayerPaletteRoutingLength { layers: usize, routing: usize },
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
    render_native_level_framebuffer_impl(request, None)
}

/// Renders a native framebuffer with one explicit palette-routing rule per input layer.
///
/// # Errors
///
/// Rejects a routing slice whose length differs from `request.layers`, an undersized CGRAM
/// palette, or invalid output dimensions.
pub fn render_native_level_framebuffer_with_layer_palette_routing(
    request: NativeLevelRasterRequest<'_>,
    layer_palette_routing: &[NativeMap16PaletteRouting],
) -> Result<Canvas, NativeLevelRasterError> {
    if layer_palette_routing.len() != request.layers.len() {
        return Err(NativeLevelRasterError::InvalidLayerPaletteRoutingLength {
            layers: request.layers.len(),
            routing: layer_palette_routing.len(),
        });
    }
    render_native_level_framebuffer_impl(request, Some(layer_palette_routing))
}

fn render_native_level_framebuffer_impl(
    request: NativeLevelRasterRequest<'_>,
    layer_palette_routing: Option<&[NativeMap16PaletteRouting]>,
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
    for (layer_index, layer) in request.layers.iter().enumerate() {
        let palette_routing = layer_palette_routing
            .and_then(|routing| routing.get(layer_index))
            .copied()
            .unwrap_or_default();
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
                palette_routing,
                placement.composition,
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
    palette_routing: NativeMap16PaletteRouting,
    composition: NativeMap16Composition,
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
                palette_routing,
                composition,
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
    palette_routing: NativeMap16PaletteRouting,
    composition: NativeMap16Composition,
) {
    let (target_x, target_y) = target;
    let (horizontal_flip, vertical_flip) = outer_flips;
    let Some(tile) = tiles.get(usize::from(subtile.tile_number())) else {
        return;
    };
    let x_flip = subtile.x_flip() ^ horizontal_flip;
    let y_flip = subtile.y_flip() ^ vertical_flip;
    let palette_row = palette_routing.palette_row(subtile.palette());
    let palette_base = usize::from(palette_row) * 16;
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
            let source = Rgba {
                red: rgb.red,
                green: rgb.green,
                blue: rgb.blue,
                alpha: 255,
            };
            let destination = canvas.get(output_x, output_y).unwrap_or_default();
            canvas.set(output_x, output_y, composition.compose(source, destination));
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
            composition: NativeMap16Composition::Opaque,
        }];
        let front = [NativeMap16Placement {
            x: 1,
            y: 0,
            word: 1,
            composition: NativeMap16Composition::Opaque,
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
    fn averaged_map16_pixels_match_lunar_magics_channel_flooring() {
        let definitions = [definition([0, 1, 1, 1])];
        let tiles = [solid(1), solid(0)];
        let placements = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
            composition: NativeMap16Composition::Average,
        }];
        let layers: [&[NativeMap16Placement]; 1] = [&placements];
        let backdrop = Rgba {
            red: 5,
            green: 7,
            blue: 9,
            alpha: 255,
        };
        let canvas = render_native_level_framebuffer(NativeLevelRasterRequest {
            width: 16,
            height: 16,
            camera_x: 0,
            camera_y: 0,
            backdrop,
            layers: &layers,
            definitions: &definitions,
            tiles: &tiles,
            palette: &palette(),
        })
        .unwrap();

        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 129,
                green: 3,
                blue: 4,
                alpha: 255,
            })
        );
        assert_eq!(canvas.get(8, 0), Some(backdrop));
    }

    #[test]
    fn half_color_map16_pixels_ignore_destination_and_preserve_transparency() {
        let definitions = [definition([0, 1, 1, 1])];
        let tiles = [solid(1), solid(0)];
        let placements = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
            composition: NativeMap16Composition::HalfColor,
        }];
        let layers: [&[NativeMap16Placement]; 1] = [&placements];
        let backdrop = Rgba {
            red: 17,
            green: 19,
            blue: 21,
            alpha: 255,
        };
        let canvas = render_native_level_framebuffer(NativeLevelRasterRequest {
            width: 16,
            height: 16,
            camera_x: 0,
            camera_y: 0,
            backdrop,
            layers: &layers,
            definitions: &definitions,
            tiles: &tiles,
            palette: &palette(),
        })
        .unwrap();

        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 127,
                green: 0,
                blue: 0,
                alpha: 255,
            })
        );
        assert_eq!(canvas.get(8, 0), Some(backdrop));
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
            composition: NativeMap16Composition::Opaque,
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
            composition: NativeMap16Composition::Opaque,
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
    fn layer_palette_routing_shifts_only_low_rows_on_the_selected_layer() {
        let definitions = [definition([0x0000, 0x0c00, 0x1000, 0x1c00])];
        let tiles = [solid(1)];
        let shifted = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
            composition: NativeMap16Composition::Opaque,
        }];
        let direct = [NativeMap16Placement {
            x: 1,
            y: 0,
            word: 0,
            composition: NativeMap16Composition::Opaque,
        }];
        let layers: [&[NativeMap16Placement]; 2] = [&shifted, &direct];
        let mut colors = vec![Bgr555(0); 16 * 8];
        for row in 0..8 {
            colors[row * 16 + 1] = Bgr555(u16::try_from(row + 1).unwrap());
        }
        let palette = Palette { colors };
        let canvas = render_native_level_framebuffer_with_layer_palette_routing(
            NativeLevelRasterRequest {
                width: 32,
                height: 16,
                camera_x: 0,
                camera_y: 0,
                backdrop: Rgba::default(),
                layers: &layers,
                definitions: &definitions,
                tiles: &tiles,
                palette: &palette,
            },
            &[
                NativeMap16PaletteRouting::ShiftLowRowsByFour,
                NativeMap16PaletteRouting::Direct,
            ],
        )
        .unwrap();
        let rgba = |row: usize| {
            let color = palette.colors[row * 16 + 1].to_rgb8();
            Rgba {
                red: color.red,
                green: color.green,
                blue: color.blue,
                alpha: 255,
            }
        };
        assert_eq!(canvas.get(0, 0), Some(rgba(4)));
        assert_eq!(canvas.get(8, 0), Some(rgba(7)));
        assert_eq!(canvas.get(0, 8), Some(rgba(4)));
        assert_eq!(canvas.get(8, 8), Some(rgba(7)));
        assert_eq!(canvas.get(16, 0), Some(rgba(0)));
        assert_eq!(canvas.get(24, 0), Some(rgba(3)));
        assert_eq!(canvas.get(16, 8), Some(rgba(4)));
        assert_eq!(canvas.get(24, 8), Some(rgba(7)));
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
        assert_eq!(
            render_native_level_framebuffer_with_layer_palette_routing(
                request,
                &[NativeMap16PaletteRouting::Direct],
            ),
            Err(NativeLevelRasterError::InvalidLayerPaletteRoutingLength {
                layers: 0,
                routing: 1,
            })
        );
    }
}
