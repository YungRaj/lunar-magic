use crate::{Canvas, CanvasError, Rgba};
use lm_graphics::{IndexedTile, Palette};
use lm_level::{Map16Tile, Subtile};
use std::collections::HashSet;

/// One Map16 cell in world tile coordinates.
///
/// `word` retains the source cell's exact attributes, including whole-definition flips.
/// `definition_index` independently addresses the selected foreground or background namespace, so
/// compressed Layer 2 can combine its descriptor's active 4K bank with the stored 12-bit tile.
/// Outer flips are explicit because object paints use bit 14 as part of their 15-bit foreground
/// definition identity rather than as a cell attribute.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeMap16Placement {
    pub x: i32,
    pub y: i32,
    pub word: u16,
    pub definition_index: u16,
    pub outer_x_flip: bool,
    pub outer_y_flip: bool,
    pub definition_bank: NativeMap16DefinitionBank,
    pub composition: NativeMap16Composition,
}

/// Which Lunar Magic Map16 definition namespace one placement addresses.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NativeMap16DefinitionBank {
    /// Foreground definitions `$0000-$7FFF`, including Acts-Like behavior.
    #[default]
    Foreground,
    /// Background definitions `$8000-$FFFF`, which have no Acts-Like behavior.
    Background,
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

    fn compose_with_layer_addition(self, source: Rgba, destination: Rgba) -> Rgba {
        let source = match self {
            Self::Opaque => source,
            Self::Average | Self::HalfColor => Rgba {
                red: source.red >> 1,
                green: source.green >> 1,
                blue: source.blue >> 1,
                alpha: 255,
            },
        };
        Rgba {
            red: destination.red.saturating_add(source.red),
            green: destination.green.saturating_add(source.green),
            blue: destination.blue.saturating_add(source.blue),
            alpha: 255,
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
    /// Foreground Map16 definitions in local `$0000`-based order.
    pub definitions: &'a [Map16Tile],
    /// Background Map16 definitions in local `$0000`-based order (global `$8000-$FFFF`).
    pub background_definitions: &'a [Map16Tile],
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
    CanvasDimensionsMismatch {
        canvas_width: usize,
        canvas_height: usize,
        request_width: usize,
        request_height: usize,
    },
    InvalidPaletteLength(usize),
    InvalidLayerPaletteRoutingLength {
        layers: usize,
        routing: usize,
    },
    InvalidLayerAdditiveLength {
        layers: usize,
        additive: usize,
    },
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
    draw_native_level_layers_impl(&mut canvas, request, layer_palette_routing, None)?;
    Ok(canvas)
}

/// Draws native Map16 layers into an existing framebuffer without clearing earlier artwork.
///
/// This is the ordering boundary used when a non-Map16 plane, such as Layer 3, must be inserted
/// between native layers while retaining destination-dependent Map16 composition.
///
/// # Errors
///
/// Rejects canvas/request dimension disagreement, an undersized palette, or a routing slice whose
/// length differs from the input layer count.
pub fn draw_native_level_layers_with_layer_palette_routing(
    canvas: &mut Canvas,
    request: NativeLevelRasterRequest<'_>,
    layer_palette_routing: &[NativeMap16PaletteRouting],
) -> Result<(), NativeLevelRasterError> {
    if layer_palette_routing.len() != request.layers.len() {
        return Err(NativeLevelRasterError::InvalidLayerPaletteRoutingLength {
            layers: request.layers.len(),
            routing: layer_palette_routing.len(),
        });
    }
    draw_native_level_layers_impl(canvas, request, Some(layer_palette_routing), None)
}

/// Draws native Map16 layers with explicit palette routing and Lunar Magic's whole-layer
/// additive flag for every layer.
///
/// Additive layers saturating-add each nontransparent source pixel to the existing framebuffer.
/// A placement on the averaged/half-color path first halves its source channels, matching
/// `RenderMap16TileToPixelBuffer` when the slot additive global is active.
///
/// # Errors
///
/// Rejects palette-routing or additive slices whose lengths differ from `request.layers`, plus
/// the ordinary canvas, palette, and dimension errors.
pub fn draw_native_level_layers_with_layer_palette_routing_and_addition(
    canvas: &mut Canvas,
    request: NativeLevelRasterRequest<'_>,
    layer_palette_routing: &[NativeMap16PaletteRouting],
    layer_additive: &[bool],
) -> Result<(), NativeLevelRasterError> {
    if layer_palette_routing.len() != request.layers.len() {
        return Err(NativeLevelRasterError::InvalidLayerPaletteRoutingLength {
            layers: request.layers.len(),
            routing: layer_palette_routing.len(),
        });
    }
    if layer_additive.len() != request.layers.len() {
        return Err(NativeLevelRasterError::InvalidLayerAdditiveLength {
            layers: request.layers.len(),
            additive: layer_additive.len(),
        });
    }
    draw_native_level_layers_impl(
        canvas,
        request,
        Some(layer_palette_routing),
        Some(layer_additive),
    )
}

fn draw_native_level_layers_impl(
    canvas: &mut Canvas,
    request: NativeLevelRasterRequest<'_>,
    layer_palette_routing: Option<&[NativeMap16PaletteRouting]>,
    layer_additive: Option<&[bool]>,
) -> Result<(), NativeLevelRasterError> {
    if canvas.width() != request.width || canvas.height() != request.height {
        return Err(NativeLevelRasterError::CanvasDimensionsMismatch {
            canvas_width: canvas.width(),
            canvas_height: canvas.height(),
            request_width: request.width,
            request_height: request.height,
        });
    }
    if request.palette.colors.len() < 16 * 8 {
        return Err(NativeLevelRasterError::InvalidPaletteLength(
            request.palette.colors.len(),
        ));
    }
    for (layer_index, layer) in request.layers.iter().enumerate() {
        let palette_routing = layer_palette_routing
            .and_then(|routing| routing.get(layer_index))
            .copied()
            .unwrap_or_default();
        let additive = layer_additive
            .and_then(|flags| flags.get(layer_index))
            .copied()
            .unwrap_or(false);
        // Lunar Magic rasterizes the final Map16 cache cell once. Walking the placement stream in
        // reverse and retaining the first coordinate preserves later-object overwrite semantics
        // without applying destination-dependent composition repeatedly to an overwritten cell.
        let mut rendered_cells = HashSet::with_capacity(layer.len());
        for placement in layer.iter().rev() {
            if !rendered_cells.insert((placement.x, placement.y)) {
                continue;
            }
            let definition_index = usize::from(placement.definition_index);
            let definitions = match placement.definition_bank {
                NativeMap16DefinitionBank::Foreground => request.definitions,
                NativeMap16DefinitionBank::Background => request.background_definitions,
            };
            let Some(definition) = definitions.get(definition_index).copied() else {
                continue;
            };
            draw_map16_clipped(
                canvas,
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
                (placement.outer_x_flip, placement.outer_y_flip),
                palette_routing,
                placement.composition,
                additive,
            );
        }
    }
    Ok(())
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
    draw_native_sprite_preview_definition_pages_with_half_color(
        canvas,
        subtiles,
        ordinary_tiles,
        animated_tiles,
        palette,
        target_x,
        target_y,
        false,
    );
}

/// Draws a sprite preview with optional Lunar Magic packed-channel half-color composition.
#[allow(clippy::too_many_arguments)]
pub fn draw_native_sprite_preview_definition_pages_with_half_color(
    canvas: &mut Canvas,
    subtiles: [u16; 4],
    ordinary_tiles: &[IndexedTile],
    animated_tiles: &[IndexedTile],
    palette: &Palette,
    target_x: i32,
    target_y: i32,
    half_color: bool,
) {
    if let Some(definition_index) = editor_text_definition_index(subtiles) {
        draw_lunar_magic_editor_text_definition(canvas, definition_index, target_x, target_y);
        return;
    }
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
            half_color,
        );
    }
}

const LM363_EDITOR_TEXT_DEFINITIONS: &[u8; 0x800] =
    include_bytes!("assets/lm363-editor-text-definitions.bin");
const LM363_EDITOR_TEXT_GLYPHS: &[u8; 0x13c0] =
    include_bytes!("assets/lm363-editor-text-glyphs.bin");
const LM363_EDITOR_TEXT_PALETTE: &[u8; 0x100] =
    include_bytes!("assets/lm363-editor-text-palette.bin");
const LM363_EDITOR_FONT: &[u8; 590_356] = include_bytes!("assets/lm363-editor-font.bin");
const LM363_EDITOR_FONT_HEADER: usize = 20;
const LM363_EDITOR_FONT_WIDTH: usize = 24;
const LM363_EDITOR_FONT_HEIGHT: usize = 24;
const LM363_EDITOR_FONT_ORIGIN_X: i32 = 4;
const LM363_EDITOR_FONT_TEXT_HEIGHT: usize = 16;
const LM363_EDITOR_FONT_GLYPH_BYTES: usize = LM363_EDITOR_FONT_WIDTH * LM363_EDITOR_FONT_HEIGHT * 4;
const LM363_EDITOR_FONT_RECORD_BYTES: usize = 2 + LM363_EDITOR_FONT_GLYPH_BYTES;

fn editor_text_definition_index(subtiles: [u16; 4]) -> Option<u8> {
    let index = subtiles[0];
    ((0x3c00..=0x3cff).contains(&index) && subtiles[1..] == [0x0019; 3]).then_some(index as u8)
}

/// Draws one definition from Lunar Magic 3.63's dynamic `$3Cxx` editor-label cache.
///
/// These definitions are Windows editor artwork, not SNES SP graphics. Lunar Magic materializes
/// them into its `$880` sidecar tile page after opening a level with sprite previews enabled.
pub(crate) fn draw_lunar_magic_editor_text_definition(
    canvas: &mut Canvas,
    definition_index: u8,
    target_x: i32,
    target_y: i32,
) {
    let definition_offset = usize::from(definition_index) * 8;
    for quadrant in 0..4 {
        let word_offset = definition_offset + quadrant * 2;
        let word = u16::from_le_bytes([
            LM363_EDITOR_TEXT_DEFINITIONS[word_offset],
            LM363_EDITOR_TEXT_DEFINITIONS[word_offset + 1],
        ]);
        let tile = usize::from(word & 0x03ff);
        let tile_offset = tile * 64;
        if tile_offset + 64 > LM363_EDITOR_TEXT_GLYPHS.len() {
            continue;
        }
        let x_flip = word & 0x4000 != 0;
        let y_flip = word & 0x8000 != 0;
        let palette = usize::from((word >> 10) & 7) * 64;
        let quadrant_x = quadrant / 2;
        let quadrant_y = quadrant % 2;
        for y in 0..8 {
            for x in 0..8 {
                let source_x = if x_flip { 7 - x } else { x };
                let source_y = if y_flip { 7 - y } else { y };
                let color_index =
                    usize::from(LM363_EDITOR_TEXT_GLYPHS[tile_offset + source_y * 8 + source_x]);
                if color_index == 0 {
                    continue;
                }
                let color = palette + color_index * 4;
                if color + 3 >= LM363_EDITOR_TEXT_PALETTE.len() {
                    continue;
                }
                let x =
                    target_x.saturating_add(i32::try_from(quadrant_x * 8 + x).unwrap_or(i32::MAX));
                let y =
                    target_y.saturating_add(i32::try_from(quadrant_y * 8 + y).unwrap_or(i32::MAX));
                let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
                    continue;
                };
                if x >= canvas.width() || y >= canvas.height() {
                    continue;
                }
                // Lunar Magic's 32-bit DIB palette is stored as B, G, R, unused.
                canvas.set(
                    x,
                    y,
                    Rgba {
                        red: LM363_EDITOR_TEXT_PALETTE[color + 2],
                        green: LM363_EDITOR_TEXT_PALETTE[color + 1],
                        blue: LM363_EDITOR_TEXT_PALETTE[color],
                        alpha: 255,
                    },
                );
            }
        }
    }
}

/// Draws Lunar Magic's 8-pixel editor-node text from the authenticated `$3Cxx` sidecar cache.
///
/// `RenderEditorTextLabel` places a blue-background `$3C7C` definition and then the character
/// definition at each eight-pixel cell. Successive strings occupy successive eight-pixel rows.
pub fn draw_lunar_magic_editor_node_text_lines(
    canvas: &mut Canvas,
    lines: &[&str],
    target_x: i32,
    target_y: i32,
) {
    for (row, line) in lines.iter().enumerate() {
        let y = target_y.saturating_add(i32::try_from(row * 8).unwrap_or(i32::MAX));
        for (column, character) in line.bytes().enumerate() {
            let x = target_x.saturating_add(i32::try_from(column * 8).unwrap_or(i32::MAX));
            draw_lunar_magic_editor_text_definition(canvas, b'|', x, y);
            draw_lunar_magic_editor_text_definition(canvas, character, x, y);
        }
    }
}

/// Draws Lunar Magic 3.63's bold System-font level-editor annotation text.
///
/// The native editor creates this font at 10 points and 96 DPI, paints white glyphs over an opaque
/// blue text background, and clips at the framebuffer boundary. The retained glyph cache was
/// produced with those exact `CreateFontA` and GDI settings under the audit Wine prefix.
pub fn draw_lunar_magic_editor_label(
    canvas: &mut Canvas,
    text: &str,
    target_x: i32,
    target_y: i32,
) {
    let width = text.bytes().fold(0_i32, |width, character| {
        width.saturating_add(i32::from(lm363_editor_font_advance(character)))
    });
    let Ok(width) = usize::try_from(width) else {
        return;
    };
    let mut source_pixels = vec![
        Rgba {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        };
        width * LM363_EDITOR_FONT_TEXT_HEIGHT
    ];
    let mut cursor = 0_i32;
    for character in text.bytes() {
        let record =
            LM363_EDITOR_FONT_HEADER + usize::from(character) * LM363_EDITOR_FONT_RECORD_BYTES;
        let pixels = record + 2;
        for y in 0..LM363_EDITOR_FONT_HEIGHT.min(LM363_EDITOR_FONT_TEXT_HEIGHT) {
            for x in 0..LM363_EDITOR_FONT_WIDTH {
                let source = pixels + (y * LM363_EDITOR_FONT_WIDTH + x) * 4;
                let blue = LM363_EDITOR_FONT[source];
                let green = LM363_EDITOR_FONT[source + 1];
                let red = LM363_EDITOR_FONT[source + 2];
                if (red, green, blue) == (0, 0, 255) {
                    continue;
                }
                let output_x = cursor
                    .saturating_add(i32::try_from(x).unwrap_or(i32::MAX))
                    .saturating_sub(LM363_EDITOR_FONT_ORIGIN_X);
                let Ok(output_x) = usize::try_from(output_x) else {
                    continue;
                };
                if output_x < width {
                    source_pixels[y * width + output_x] = Rgba {
                        red,
                        green,
                        blue,
                        alpha: 255,
                    };
                }
            }
        }
        cursor = cursor.saturating_add(i32::from(lm363_editor_font_advance(character)));
    }

    for y in 0..LM363_EDITOR_FONT_TEXT_HEIGHT {
        for x in 0..width {
            let output_x = target_x.saturating_add(i32::try_from(x).unwrap_or(i32::MAX));
            let output_y = target_y.saturating_add(i32::try_from(y).unwrap_or(i32::MAX));
            let (Ok(output_x), Ok(output_y)) =
                (usize::try_from(output_x), usize::try_from(output_y))
            else {
                continue;
            };
            if output_x >= canvas.width() || output_y >= canvas.height() {
                continue;
            }
            let destination = canvas.get(output_x, output_y).unwrap_or_default();
            let source = source_pixels[y * width + x];
            canvas.set(
                output_x,
                output_y,
                Rgba {
                    red: ((u16::from(destination.red & 0xfe) + u16::from(source.red & 0xfe)) >> 1)
                        as u8,
                    green: ((u16::from(destination.green & 0xfe) + u16::from(source.green & 0xfe))
                        >> 1) as u8,
                    blue: ((u16::from(destination.blue & 0xfe) + u16::from(source.blue & 0xfe))
                        >> 1) as u8,
                    alpha: 255,
                },
            );
        }
    }
}

fn lm363_editor_font_advance(character: u8) -> i16 {
    let offset = LM363_EDITOR_FONT_HEADER + usize::from(character) * LM363_EDITOR_FONT_RECORD_BYTES;
    i16::from_le_bytes([LM363_EDITOR_FONT[offset], LM363_EDITOR_FONT[offset + 1]])
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
    additive: bool,
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
                additive,
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
    additive: bool,
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
            canvas.set(
                output_x,
                output_y,
                if additive {
                    composition.compose_with_layer_addition(source, destination)
                } else {
                    composition.compose(source, destination)
                },
            );
        }
    }
}

pub(crate) fn draw_sprite_subtile_clipped(
    canvas: &mut Canvas,
    word: u16,
    tiles: &[IndexedTile],
    palette: &Palette,
    target: (i32, i32),
    half_color: bool,
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
            let source = Rgba {
                red: rgb.red,
                green: rgb.green,
                blue: rgb.blue,
                alpha: 255,
            };
            let output = if half_color {
                let destination = canvas.get(output_x, output_y).unwrap_or_default();
                Rgba {
                    red: ((u16::from(destination.red & 0xfe) + u16::from(source.red & 0xfe)) >> 1)
                        as u8,
                    green: ((u16::from(destination.green & 0xfe) + u16::from(source.green & 0xfe))
                        >> 1) as u8,
                    blue: ((u16::from(destination.blue & 0xfe) + u16::from(source.blue & 0xfe))
                        >> 1) as u8,
                    alpha: 255,
                }
            } else {
                source
            };
            canvas.set(output_x, output_y, output);
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
    fn editor_text_definitions_use_the_authenticated_sidecar_cache() {
        let untouched = Rgba {
            red: 255,
            green: 0,
            blue: 255,
            alpha: 255,
        };
        let mut canvas = Canvas::from_pixels(16, 16, vec![untouched; 256]).unwrap();
        let unused_tiles = [solid(0)];
        let unused_palette = palette();

        // Native text emits its background definition first and the ASCII definition second.
        draw_native_sprite_preview_definition_pages(
            &mut canvas,
            [0x3c7c, 0x0019, 0x0019, 0x0019],
            &unused_tiles,
            &unused_tiles,
            &unused_palette,
            0,
            0,
        );
        draw_native_sprite_preview_definition_pages(
            &mut canvas,
            [0x3c44, 0x0019, 0x0019, 0x0019],
            &unused_tiles,
            &unused_tiles,
            &unused_palette,
            0,
            0,
        );

        let pixels = canvas.pixels();
        assert!(pixels.contains(&Rgba {
            red: 208,
            green: 248,
            blue: 248,
            alpha: 255,
        }));
        assert!(pixels.contains(&Rgba {
            red: 0,
            green: 0,
            blue: 128,
            alpha: 255,
        }));
        assert!(pixels.iter().filter(|&&pixel| pixel != untouched).count() > 10);
    }

    #[test]
    fn sprite_half_color_averages_each_packed_channel_with_the_existing_scene() {
        let mut colors = vec![Bgr555(0); 256];
        colors[8 * 16 + 1] = Bgr555(0x001f);
        let palette = Palette { colors };
        let tiles = [solid(1)];
        let backdrop = Rgba {
            red: 20,
            green: 40,
            blue: 60,
            alpha: 255,
        };
        let mut canvas = Canvas::from_pixels(16, 16, vec![backdrop; 256]).unwrap();

        draw_native_sprite_preview_definition_pages_with_half_color(
            &mut canvas,
            [0; 4],
            &tiles,
            &tiles,
            &palette,
            0,
            0,
            true,
        );

        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 137,
                green: 20,
                blue: 30,
                alpha: 255,
            })
        );
    }

    #[test]
    fn editor_node_text_uses_eight_pixel_cells_and_rows() {
        let untouched = Rgba {
            red: 255,
            green: 0,
            blue: 255,
            alpha: 255,
        };
        let mut canvas = Canvas::from_pixels(32, 24, vec![untouched; 32 * 24]).unwrap();
        draw_lunar_magic_editor_node_text_lines(&mut canvas, &["AB", "C "], 0, 0);

        assert_ne!(canvas.get(0, 0), Some(untouched));
        assert_ne!(canvas.get(8, 0), Some(untouched));
        assert_ne!(canvas.get(0, 8), Some(untouched));
        assert_ne!(canvas.get(8, 8), Some(untouched));
        assert_eq!(canvas.get(16, 0), Some(untouched));
        assert_eq!(canvas.get(0, 16), Some(untouched));
    }

    #[test]
    fn blended_editor_label_has_a_stable_native_pixel_hash() {
        let backdrop = Rgba {
            red: 20,
            green: 40,
            blue: 60,
            alpha: 255,
        };
        let mut canvas = Canvas::from_pixels(160, 16, vec![backdrop; 160 * 16]).unwrap();

        draw_lunar_magic_editor_label(&mut canvas, ">Entrance to level C5", 0, 0);

        let hash = canvas
            .pixels()
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, pixel| {
                [pixel.red, pixel.green, pixel.blue, pixel.alpha]
                    .into_iter()
                    .fold(hash, |hash, byte| {
                        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
                    })
            });
        assert_eq!(hash, 0xd206_2d38_c1e5_60a7);
    }

    #[test]
    fn layer_order_transparency_camera_and_clipping_are_exact() {
        let definitions = [definition([0, 0, 0, 0]), definition([1, 1, 1, 1])];
        let tiles = [solid(1), solid(2)];
        let back = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
            definition_index: 0,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
            composition: NativeMap16Composition::Opaque,
        }];
        let front = [NativeMap16Placement {
            x: 1,
            y: 0,
            word: 1,
            definition_index: 1,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
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
            background_definitions: &[],
            tiles: &tiles,
            palette: &palette(),
        })
        .unwrap();
        assert_eq!(canvas.get(0, 0).unwrap().red, 255);
        assert_eq!(canvas.get(1, 0).unwrap().green, 255);
        assert_eq!(canvas.get(16, 0).unwrap().green, 255);
    }

    #[test]
    fn background_placements_use_the_separate_definition_bank() {
        let foreground_definitions = [definition([0, 0, 0, 0])];
        let mut background_definitions = vec![Map16Tile::default(); 0x1001];
        background_definitions[0x1000] = definition([1, 1, 1, 1]);
        let tiles = [solid(1), solid(2)];
        let placements = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
            definition_index: 0x1000,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Background,
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
            definitions: &foreground_definitions,
            background_definitions: &background_definitions,
            tiles: &tiles,
            palette: &palette(),
        })
        .unwrap();

        assert_eq!(canvas.get(0, 0).unwrap().green, 255);
        assert_eq!(canvas.get(0, 0).unwrap().red, 0);
    }

    #[test]
    fn foreground_definition_bit_fourteen_does_not_imply_an_outer_flip() {
        let mut definitions = vec![Map16Tile::default(); 0x4002];
        definitions[1] = definition([0, 0, 0, 0]);
        definitions[0x4001] = definition([1, 1, 1, 1]);
        let tiles = [solid(1), solid(2)];
        let placements = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0x4001,
            definition_index: 0x4001,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
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
            background_definitions: &[],
            tiles: &tiles,
            palette: &palette(),
        })
        .unwrap();

        assert_eq!(canvas.get(0, 0).unwrap().green, 255);
        assert_eq!(canvas.get(0, 0).unwrap().red, 0);
    }

    #[test]
    fn averaged_map16_pixels_match_lunar_magics_channel_flooring() {
        let definitions = [definition([0, 1, 1, 1])];
        let tiles = [solid(1), solid(0)];
        let placements = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
            definition_index: 0,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
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
            background_definitions: &[],
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
            definition_index: 0,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
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
            background_definitions: &[],
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
    fn drawing_layers_into_an_existing_canvas_preserves_prior_pixels_and_order() {
        let definitions = [definition([0, 0, 0, 0]), definition([1, 1, 1, 1])];
        let tiles = [solid(1), solid(2)];
        let back = [NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
            definition_index: 0,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
            composition: NativeMap16Composition::Opaque,
        }];
        let front = [NativeMap16Placement {
            definition_index: 1,
            word: 1,
            ..back[0]
        }];
        let back_layers: [&[NativeMap16Placement]; 1] = [&back];
        let front_layers: [&[NativeMap16Placement]; 1] = [&front];
        let routing = [NativeMap16PaletteRouting::Direct];
        let palette = palette();
        let request = |layers| NativeLevelRasterRequest {
            width: 24,
            height: 16,
            camera_x: 0,
            camera_y: 0,
            backdrop: Rgba::default(),
            layers,
            definitions: &definitions,
            background_definitions: &[],
            tiles: &tiles,
            palette: &palette,
        };
        let prior = Rgba {
            red: 7,
            green: 9,
            blue: 11,
            alpha: 255,
        };
        let mut canvas = Canvas::from_pixels(24, 16, vec![prior; 24 * 16]).unwrap();
        draw_native_level_layers_with_layer_palette_routing(
            &mut canvas,
            request(&back_layers),
            &routing,
        )
        .unwrap();
        draw_native_level_layers_with_layer_palette_routing(
            &mut canvas,
            request(&front_layers),
            &routing,
        )
        .unwrap();

        assert_eq!(canvas.get(0, 0).unwrap().green, 255);
        assert_eq!(canvas.get(20, 0), Some(prior));
    }

    #[test]
    fn whole_layer_addition_saturates_and_halves_the_averaged_source() {
        let definitions = [definition([0, 0, 0, 0])];
        let tiles = [solid(1)];
        let mut placement = NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
            definition_index: 0,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
            composition: NativeMap16Composition::Opaque,
        };
        let palette = palette();
        let backdrop = Rgba {
            red: 10,
            green: 20,
            blue: 30,
            alpha: 255,
        };
        let routing = [NativeMap16PaletteRouting::Direct];
        let additive = [true];

        let draw = |placement: &NativeMap16Placement| {
            let layer = [*placement];
            let layers: [&[NativeMap16Placement]; 1] = [&layer];
            let mut canvas = Canvas::from_pixels(16, 16, vec![backdrop; 256]).unwrap();
            draw_native_level_layers_with_layer_palette_routing_and_addition(
                &mut canvas,
                NativeLevelRasterRequest {
                    width: 16,
                    height: 16,
                    camera_x: 0,
                    camera_y: 0,
                    backdrop,
                    layers: &layers,
                    definitions: &definitions,
                    background_definitions: &[],
                    tiles: &tiles,
                    palette: &palette,
                },
                &routing,
                &additive,
            )
            .unwrap();
            canvas.get(0, 0).unwrap()
        };

        assert_eq!(draw(&placement).red, 255);
        placement.composition = NativeMap16Composition::Average;
        assert_eq!(draw(&placement).red, 137);
    }

    #[test]
    fn whole_layer_addition_requires_one_flag_per_layer() {
        let mut canvas = Canvas::try_new(1, 1).unwrap();
        let layers: [&[NativeMap16Placement]; 0] = [];
        let palette = palette();
        let error = draw_native_level_layers_with_layer_palette_routing_and_addition(
            &mut canvas,
            NativeLevelRasterRequest {
                width: 1,
                height: 1,
                camera_x: 0,
                camera_y: 0,
                backdrop: Rgba::default(),
                layers: &layers,
                definitions: &[],
                background_definitions: &[],
                tiles: &[],
                palette: &palette,
            },
            &[],
            &[true],
        )
        .unwrap_err();
        assert_eq!(
            error,
            NativeLevelRasterError::InvalidLayerAdditiveLength {
                layers: 0,
                additive: 1,
            }
        );
    }

    #[test]
    fn whole_layer_addition_composes_only_the_final_overwriting_cell() {
        let definitions = [definition([0, 0, 0, 0]), definition([1, 1, 1, 1])];
        let tiles = [solid(1), solid(2)];
        let first = NativeMap16Placement {
            x: 0,
            y: 0,
            word: 0,
            definition_index: 0,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
            composition: NativeMap16Composition::Opaque,
        };
        let last = NativeMap16Placement {
            definition_index: 1,
            word: 1,
            ..first
        };
        let layer = [first, last];
        let layers: [&[NativeMap16Placement]; 1] = [&layer];
        let palette = palette();
        let backdrop = Rgba {
            red: 10,
            green: 20,
            blue: 30,
            alpha: 255,
        };
        let mut canvas = Canvas::from_pixels(16, 16, vec![backdrop; 256]).unwrap();
        draw_native_level_layers_with_layer_palette_routing_and_addition(
            &mut canvas,
            NativeLevelRasterRequest {
                width: 16,
                height: 16,
                camera_x: 0,
                camera_y: 0,
                backdrop,
                layers: &layers,
                definitions: &definitions,
                background_definitions: &[],
                tiles: &tiles,
                palette: &palette,
            },
            &[NativeMap16PaletteRouting::Direct],
            &[true],
        )
        .unwrap();

        let pixel = canvas.get(0, 0).unwrap();
        assert_eq!((pixel.red, pixel.green, pixel.blue), (10, 255, 30));
    }

    #[test]
    fn drawing_layers_rejects_canvas_dimension_disagreement() {
        let mut canvas = Canvas::try_new(2, 2).unwrap();
        let layers: [&[NativeMap16Placement]; 0] = [];
        let palette = palette();
        let error = draw_native_level_layers_with_layer_palette_routing(
            &mut canvas,
            NativeLevelRasterRequest {
                width: 1,
                height: 2,
                camera_x: 0,
                camera_y: 0,
                backdrop: Rgba::default(),
                layers: &layers,
                definitions: &[],
                background_definitions: &[],
                tiles: &[],
                palette: &palette,
            },
            &[],
        )
        .unwrap_err();
        assert_eq!(
            error,
            NativeLevelRasterError::CanvasDimensionsMismatch {
                canvas_width: 2,
                canvas_height: 2,
                request_width: 1,
                request_height: 2,
            }
        );
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
            definition_index: 0,
            outer_x_flip: true,
            outer_y_flip: true,
            definition_bank: NativeMap16DefinitionBank::Foreground,
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
            background_definitions: &[],
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
            definition_index: 0,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
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
            background_definitions: &[],
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
            definition_index: 0,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
            composition: NativeMap16Composition::Opaque,
        }];
        let direct = [NativeMap16Placement {
            x: 1,
            y: 0,
            word: 0,
            definition_index: 0,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
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
                background_definitions: &[],
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
            background_definitions: &[],
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
