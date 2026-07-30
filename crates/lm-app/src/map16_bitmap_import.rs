//! Staged cross-domain Map16 bitmap import planning.

use lm_graphics::{
    BitmapImportError, BitmapPaletteColorOptions, BitmapPaletteEntryState,
    BitmapPaletteReductionError, GraphicsFile4bpp, GraphicsOwnership, IndexedBitmapImport,
    IndexedBitmapImportOptions, PaletteEntryOwner, PaletteImportError, PaletteOwnership, Rgba8,
    TransparentPaletteRowImport, allocate_bitmap_palette_rows, reduce_bitmap_palette,
};
use lm_level::{Map16Page, Map16Tile, Subtile};
use std::{fmt, io::Cursor};

pub const MAP16_BITMAP_WIDTH: usize = 256;
pub const MAP16_BITMAP_HEIGHT: usize = 256;
pub const MAP16_BITMAP_PIXELS: usize = MAP16_BITMAP_WIDTH * MAP16_BITMAP_HEIGHT;
pub const MAP16_BITMAP_MAX_PNG_BYTES: usize = 16 * 1024 * 1024;
pub const MAP16_BITMAP_MAX_DIMENSION: usize = 4096;
pub const MAP16_BITMAP_MAX_PIXELS: usize = MAP16_BITMAP_MAX_DIMENSION * MAP16_BITMAP_MAX_DIMENSION;
const MAX_PNG_DECODE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy)]
pub struct Map16BitmapImportRequest<'a> {
    pub pixels: &'a [Rgba8],
    pub width: usize,
    pub height: usize,
    pub palette_row: u8,
    pub acts_like: u16,
    pub palette: &'a lm_graphics::Palette,
    pub palette_ownership: &'a PaletteOwnership,
    pub graphics: &'a GraphicsFile4bpp,
    pub graphics_ownership: &'a GraphicsOwnership,
    pub occupied: &'a [bool],
}

/// Conversion choices that affect the staged graphics and Map16 result.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Map16BitmapImportOptions {
    pub graphics: IndexedBitmapImportOptions,
    /// Use Lunar Magic's eight-row bitmap color allocation instead of one selected palette row.
    pub color: Option<BitmapPaletteColorOptions>,
    /// Reuse an earlier identical imported 16×16 definition instead of allocating another slot.
    pub deduplicate_map16: bool,
    pub layer_priority: bool,
}

/// All four semantic products of a Map16 bitmap conversion.
///
/// The native dialog previews this value and the commit boundary consumes that same value. No
/// quantization, tile allocation, or Map16 construction is repeated after user approval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16BitmapImportPlan {
    pub source_pixels: Vec<Rgba8>,
    pub palette: lm_graphics::Palette,
    pub graphics: GraphicsFile4bpp,
    pub occupied: Vec<bool>,
    /// Every imported 16×16 definition in source row-major order.
    ///
    /// This is the authoritative import product. Lunar Magic allocates these definitions into
    /// the global Map16 namespace, so an imported bitmap is not inherently limited to one page.
    pub map16_tiles: Vec<Map16Tile>,
    /// Compatibility view of the top-left 16×16 definitions.
    ///
    /// Existing page-oriented file and CLI workflows consume this view. Native ROM import uses
    /// [`Self::map16_tiles`] so definitions beyond the first page are not discarded.
    pub page: Map16Page,
    pub width_in_map16_tiles: usize,
    pub height_in_map16_tiles: usize,
    pub indexed_pixels: Vec<u8>,
    /// One palette row per imported 8×8 source tile, in row-major order.
    pub tile_palette_rows: Vec<u8>,
    pub generated_colors: usize,
    pub newly_occupied_tiles: usize,
}

/// One decoded row-major RGBA bitmap before Lunar Magic's 16-pixel boundary padding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedMap16Bitmap {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Rgba8>,
}

impl Map16BitmapImportPlan {
    /// Quantizes and materializes one complete 256×256 Map16 page without changing its inputs.
    ///
    /// # Errors
    ///
    /// Rejects wrong pixel counts, unavailable palette rows, protected palette/graphics slots,
    /// invalid ownership maps, exhausted 10-bit graphics space, or malformed Map16 output.
    pub fn prepare(request: Map16BitmapImportRequest<'_>) -> Result<Self, Map16BitmapImportError> {
        let options = Map16BitmapImportOptions {
            graphics: IndexedBitmapImportOptions {
                allocation_end: request.graphics.tiles.len().min(0x400),
                ..IndexedBitmapImportOptions::default()
            },
            ..Map16BitmapImportOptions::default()
        };
        Self::prepare_with_options(request, options)
    }

    /// Quantizes and materializes one page using explicit optimization, allocation, and priority
    /// choices shared by native and headless workflows.
    ///
    /// # Errors
    ///
    /// Returns the same validation and staging errors as [`Self::prepare`].
    pub fn prepare_with_options(
        request: Map16BitmapImportRequest<'_>,
        options: Map16BitmapImportOptions,
    ) -> Result<Self, Map16BitmapImportError> {
        let expected_pixels = request.width.checked_mul(request.height).ok_or(
            Map16BitmapImportError::WrongPixelCount {
                expected: usize::MAX,
                actual: request.pixels.len(),
            },
        )?;
        if request.pixels.len() != expected_pixels {
            return Err(Map16BitmapImportError::WrongPixelCount {
                expected: expected_pixels,
                actual: request.pixels.len(),
            });
        }
        if request.palette_row > 7 {
            return Err(Map16BitmapImportError::PaletteRow(request.palette_row));
        }
        let staged_palette = prepare_bitmap_palette(request, options.color)?;
        let materialized = IndexedBitmapImport::materialize_with_options(
            request.width,
            request.height,
            &staged_palette.indices,
            request.graphics,
            request.graphics_ownership,
            request.occupied,
            options.graphics,
        )
        .map_err(Map16BitmapImportError::Graphics)?;
        let (map16_tiles, page, width_in_map16_tiles, height_in_map16_tiles) = build_map16_tiles(
            &materialized,
            &staged_palette.tile_rows,
            request.acts_like,
            options.layer_priority,
        )?;
        let newly_occupied_tiles = materialized
            .occupied
            .iter()
            .zip(request.occupied)
            .filter(|(after, before)| **after && !**before)
            .count();
        Ok(Self {
            source_pixels: request.pixels.to_vec(),
            palette: staged_palette.palette,
            graphics: materialized.graphics,
            occupied: materialized.occupied,
            map16_tiles,
            page,
            width_in_map16_tiles,
            height_in_map16_tiles,
            indexed_pixels: staged_palette.indices,
            tile_palette_rows: staged_palette.tile_rows,
            generated_colors: staged_palette.generated_colors,
            newly_occupied_tiles,
        })
    }

    /// Materializes the exact converted preview through each staged 8×8 tile's palette row.
    #[must_use]
    pub fn converted_pixels(&self) -> Vec<Rgba8> {
        let width = self.width_in_map16_tiles * 16;
        let tiles_wide = width / 8;
        self.indexed_pixels
            .iter()
            .zip(&self.source_pixels)
            .enumerate()
            .map(|(pixel_offset, (index, source))| {
                if source.alpha == 0 {
                    Rgba8 {
                        red: 0,
                        green: 0,
                        blue: 0,
                        alpha: 0,
                    }
                } else {
                    let x = pixel_offset % width;
                    let y = pixel_offset / width;
                    let tile = (y / 8) * tiles_wide + x / 8;
                    let row = self
                        .tile_palette_rows
                        .get(tile)
                        .copied()
                        .unwrap_or_default();
                    let palette_index = usize::from(row) * lm_graphics::Palette::COLORS_PER_ROW
                        + usize::from(*index);
                    let rgb = self
                        .palette
                        .colors
                        .get(palette_index)
                        .copied()
                        .unwrap_or_default()
                        .to_rgb8();
                    Rgba8 {
                        red: rgb.red,
                        green: rgb.green,
                        blue: rgb.blue,
                        alpha: 255,
                    }
                }
            })
            .collect()
    }
}

struct StagedBitmapPalette {
    palette: lm_graphics::Palette,
    indices: Vec<u8>,
    tile_rows: Vec<u8>,
    generated_colors: usize,
}

fn prepare_bitmap_palette(
    request: Map16BitmapImportRequest<'_>,
    color_options: Option<BitmapPaletteColorOptions>,
) -> Result<StagedBitmapPalette, Map16BitmapImportError> {
    if let Some(mut options) = color_options {
        protect_owned_palette_entries(&mut options, request.palette_ownership)?;
        let reduced = reduce_bitmap_palette(request.pixels, &options)
            .map_err(Map16BitmapImportError::BitmapPalette)?;
        let allocated = allocate_bitmap_palette_rows(
            &reduced,
            request.width,
            request.height,
            request.palette,
            &options,
        )
        .map_err(Map16BitmapImportError::BitmapPalette)?;
        return Ok(StagedBitmapPalette {
            palette: allocated.palette,
            indices: allocated.indices,
            tile_rows: allocated.tile_rows,
            generated_colors: allocated.generated_colors,
        });
    }
    let palette_row = usize::from(request.palette_row);
    let row_start = palette_row * lm_graphics::Palette::COLORS_PER_ROW;
    let preserves_owned = (row_start + 1..row_start + lm_graphics::Palette::COLORS_PER_ROW)
        .any(|index| request.palette_ownership.owner(index) != Some(PaletteEntryOwner::Editable));
    let palette = if preserves_owned {
        TransparentPaletteRowImport::quantize_preserving_owned(
            request.pixels,
            palette_row,
            request.palette,
            request.palette_ownership,
        )
    } else {
        TransparentPaletteRowImport::quantize(
            request.pixels,
            palette_row,
            request.palette,
            request.palette_ownership,
        )
    }
    .map_err(Map16BitmapImportError::Palette)?;
    let tile_count = (request.width / 8)
        .checked_mul(request.height / 8)
        .unwrap_or(0);
    Ok(StagedBitmapPalette {
        palette: palette.palette,
        indices: palette.indices,
        tile_rows: vec![request.palette_row; tile_count],
        generated_colors: palette.generated_colors,
    })
}

fn protect_owned_palette_entries(
    options: &mut BitmapPaletteColorOptions,
    ownership: &PaletteOwnership,
) -> Result<(), Map16BitmapImportError> {
    options
        .validate()
        .map_err(Map16BitmapImportError::BitmapPalette)?;
    for (index, state) in options.entries.iter_mut().enumerate() {
        match ownership.owner(index) {
            Some(PaletteEntryOwner::Editable) => {}
            Some(PaletteEntryOwner::Fixed) => *state = BitmapPaletteEntryState::Reusable,
            Some(PaletteEntryOwner::ExAnimation { .. }) => {
                *state = BitmapPaletteEntryState::Reserved;
            }
            None => {
                return Err(Map16BitmapImportError::PaletteOwnershipShape {
                    expected: options.entries.len(),
                    actual: ownership.len(),
                });
            }
        }
    }
    Ok(())
}

/// Decodes a bounded complete-page PNG into the importer's canonical RGBA model.
///
/// # Errors
///
/// Rejects oversized input, malformed PNG data, dimensions other than 256×256, unsupported
/// post-transform color types, or a decoder output whose pixel count is not canonical.
pub fn decode_map16_bitmap_png(bytes: &[u8]) -> Result<Vec<Rgba8>, Map16PngDecodeError> {
    let bitmap = decode_map16_bitmap_png_image(bytes)?;
    if bitmap.width != MAP16_BITMAP_WIDTH || bitmap.height != MAP16_BITMAP_HEIGHT {
        return Err(Map16PngDecodeError::Dimensions {
            width: bitmap.width,
            height: bitmap.height,
        });
    }
    Ok(bitmap.pixels)
}

/// Decodes a bounded PNG while retaining its source dimensions.
///
/// # Errors
///
/// Rejects oversized input, malformed/empty images, dimensions beyond the importer bound,
/// unsupported post-transform color types, or inconsistent decoded pixel counts.
pub fn decode_map16_bitmap_png_image(
    bytes: &[u8],
) -> Result<DecodedMap16Bitmap, Map16PngDecodeError> {
    if bytes.len() > MAP16_BITMAP_MAX_PNG_BYTES {
        return Err(Map16PngDecodeError::InputTooLarge(bytes.len()));
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    decoder.set_limits(png::Limits {
        bytes: MAX_PNG_DECODE_BYTES,
    });
    let mut reader = decoder
        .read_info()
        .map_err(|error| Map16PngDecodeError::Decode(error.to_string()))?;
    let width = usize::try_from(reader.info().width).unwrap_or(usize::MAX);
    let height = usize::try_from(reader.info().height).unwrap_or(usize::MAX);
    if width == 0
        || height == 0
        || width > MAP16_BITMAP_MAX_DIMENSION
        || height > MAP16_BITMAP_MAX_DIMENSION
    {
        return Err(Map16PngDecodeError::Dimensions { width, height });
    }
    let mut output = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut output)
        .map_err(|error| Map16PngDecodeError::Decode(error.to_string()))?;
    let bytes = &output[..info.buffer_size()];
    let pixels: Vec<Rgba8> = match info.color_type {
        png::ColorType::Rgba => bytes
            .chunks_exact(4)
            .map(|pixel| Rgba8 {
                red: pixel[0],
                green: pixel[1],
                blue: pixel[2],
                alpha: pixel[3],
            })
            .collect(),
        png::ColorType::Rgb => bytes
            .chunks_exact(3)
            .map(|pixel| Rgba8 {
                red: pixel[0],
                green: pixel[1],
                blue: pixel[2],
                alpha: 255,
            })
            .collect(),
        png::ColorType::Grayscale => bytes
            .iter()
            .map(|value| Rgba8 {
                red: *value,
                green: *value,
                blue: *value,
                alpha: 255,
            })
            .collect(),
        png::ColorType::GrayscaleAlpha => bytes
            .chunks_exact(2)
            .map(|pixel| Rgba8 {
                red: pixel[0],
                green: pixel[0],
                blue: pixel[0],
                alpha: pixel[1],
            })
            .collect(),
        png::ColorType::Indexed => return Err(Map16PngDecodeError::UnexpandedIndexed),
    };
    if pixels.len() != width * height {
        return Err(Map16PngDecodeError::PixelCount(pixels.len()));
    }
    Ok(DecodedMap16Bitmap {
        width,
        height,
        pixels,
    })
}

/// Pads a decoded bitmap on its right and bottom edges to whole 16×16 Map16 blocks.
///
/// This matches Lunar Magic's clipboard normalization: original pixels remain at the top-left and
/// every added pixel uses the active background color.
///
/// # Errors
///
/// Rejects malformed source shape or arithmetic overflow.
pub fn pad_map16_bitmap(
    bitmap: &DecodedMap16Bitmap,
    fill: Rgba8,
) -> Result<DecodedMap16Bitmap, Map16PngDecodeError> {
    let expected = bitmap
        .width
        .checked_mul(bitmap.height)
        .ok_or(Map16PngDecodeError::PixelCount(bitmap.pixels.len()))?;
    if expected != bitmap.pixels.len() {
        return Err(Map16PngDecodeError::PixelCount(bitmap.pixels.len()));
    }
    let width = bitmap
        .width
        .checked_add(15)
        .map(|value| value & !15)
        .ok_or(Map16PngDecodeError::Dimensions {
            width: bitmap.width,
            height: bitmap.height,
        })?;
    let height = bitmap
        .height
        .checked_add(15)
        .map(|value| value & !15)
        .ok_or(Map16PngDecodeError::Dimensions {
            width: bitmap.width,
            height: bitmap.height,
        })?;
    let len = width
        .checked_mul(height)
        .ok_or(Map16PngDecodeError::Dimensions { width, height })?;
    let mut pixels = vec![fill; len];
    for row in 0..bitmap.height {
        let source = row * bitmap.width;
        let target = row * width;
        pixels[target..target + bitmap.width]
            .copy_from_slice(&bitmap.pixels[source..source + bitmap.width]);
    }
    Ok(DecodedMap16Bitmap {
        width,
        height,
        pixels,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16PngDecodeError {
    InputTooLarge(usize),
    Decode(String),
    Dimensions { width: usize, height: usize },
    UnexpandedIndexed,
    PixelCount(usize),
}

impl fmt::Display for Map16PngDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Map16 bitmap PNG decoding failed: {self:?}")
    }
}

impl std::error::Error for Map16PngDecodeError {}

fn build_map16_tiles(
    imported: &IndexedBitmapImport,
    palette_rows: &[u8],
    acts_like: u16,
    layer_priority: bool,
) -> Result<(Vec<Map16Tile>, Map16Page, usize, usize), Map16BitmapImportError> {
    if imported.width_in_tiles == 0
        || imported.height_in_tiles == 0
        || imported.width_in_tiles % 2 != 0
        || imported.height_in_tiles % 2 != 0
    {
        return Err(Map16BitmapImportError::WrongMaterializedShape {
            width: imported.width_in_tiles,
            height: imported.height_in_tiles,
        });
    }
    let expected_palette_rows = imported.width_in_tiles * imported.height_in_tiles;
    if palette_rows.len() != expected_palette_rows {
        return Err(Map16BitmapImportError::PaletteRowCount {
            expected: expected_palette_rows,
            actual: palette_rows.len(),
        });
    }
    let width_in_map16_tiles = imported.width_in_tiles / 2;
    let height_in_map16_tiles = imported.height_in_tiles / 2;
    let tile_count = width_in_map16_tiles
        .checked_mul(height_in_map16_tiles)
        .ok_or(Map16BitmapImportError::WrongMaterializedShape {
            width: imported.width_in_tiles,
            height: imported.height_in_tiles,
        })?;
    let mut map16_tiles = Vec::with_capacity(tile_count);
    for tile_y in 0..height_in_map16_tiles {
        for tile_x in 0..width_in_map16_tiles {
            let top_left = tile_y * 2 * imported.width_in_tiles + tile_x * 2;
            map16_tiles.push(Map16Tile {
                top_left: descriptor(
                    imported.placements[top_left],
                    palette_rows[top_left],
                    layer_priority,
                ),
                top_right: descriptor(
                    imported.placements[top_left + 1],
                    palette_rows[top_left + 1],
                    layer_priority,
                ),
                bottom_left: descriptor(
                    imported.placements[top_left + imported.width_in_tiles],
                    palette_rows[top_left + imported.width_in_tiles],
                    layer_priority,
                ),
                bottom_right: descriptor(
                    imported.placements[top_left + imported.width_in_tiles + 1],
                    palette_rows[top_left + imported.width_in_tiles + 1],
                    layer_priority,
                ),
                acts_like,
            });
        }
    }
    let mut page_tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
    for row in 0..height_in_map16_tiles.min(16) {
        let source_start = row * width_in_map16_tiles;
        let copy_len = width_in_map16_tiles.min(16);
        let target_start = row * 16;
        page_tiles[target_start..target_start + copy_len]
            .copy_from_slice(&map16_tiles[source_start..source_start + copy_len]);
    }
    let page = Map16Page::new(page_tiles)
        .map_err(|tiles| Map16BitmapImportError::Map16TileCount(tiles.len()))?;
    Ok((
        map16_tiles,
        page,
        width_in_map16_tiles,
        height_in_map16_tiles,
    ))
}

fn descriptor(
    placement: lm_graphics::ImportedTilePlacement,
    palette_row: u8,
    layer_priority: bool,
) -> Subtile {
    let mut word = placement.tile | (u16::from(palette_row) << 10);
    if layer_priority {
        word |= 0x2000;
    }
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
    BitmapPalette(BitmapPaletteReductionError),
    PaletteOwnershipShape { expected: usize, actual: usize },
    Graphics(BitmapImportError),
    WrongMaterializedShape { width: usize, height: usize },
    PaletteRowCount { expected: usize, actual: usize },
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
    use lm_graphics::{
        Bgr555, BitmapPaletteColorOptions, BitmapPaletteEntryState, GraphicsFile4bpp, IndexedTile,
        Palette,
    };

    fn opaque(red: u8, green: u8, blue: u8) -> Rgba8 {
        Rgba8 {
            red,
            green,
            blue,
            alpha: 255,
        }
    }

    #[test]
    fn lunar_magic_padding_keeps_source_at_top_left_and_fills_right_and_bottom() {
        let source = DecodedMap16Bitmap {
            width: 17,
            height: 15,
            pixels: (0..17 * 15)
                .map(|value| Rgba8 {
                    red: u8::try_from(value % 251).unwrap(),
                    green: 0,
                    blue: 0,
                    alpha: 255,
                })
                .collect(),
        };
        let fill = Rgba8 {
            red: 9,
            green: 8,
            blue: 7,
            alpha: 255,
        };
        let padded = pad_map16_bitmap(&source, fill).unwrap();
        assert_eq!((padded.width, padded.height), (32, 16));
        for row in 0..source.height {
            assert_eq!(
                &padded.pixels[row * padded.width..row * padded.width + source.width],
                &source.pixels[row * source.width..(row + 1) * source.width]
            );
            assert!(
                padded.pixels[row * padded.width + source.width..(row + 1) * padded.width]
                    .iter()
                    .all(|pixel| *pixel == fill)
            );
        }
        assert!(
            padded.pixels[15 * padded.width..]
                .iter()
                .all(|pixel| *pixel == fill)
        );
    }

    #[test]
    fn malformed_bitmap_shape_is_rejected_before_padding() {
        assert!(matches!(
            pad_map16_bitmap(
                &DecodedMap16Bitmap {
                    width: 2,
                    height: 2,
                    pixels: vec![
                        Rgba8 {
                            red: 0,
                            green: 0,
                            blue: 0,
                            alpha: 0,
                        };
                        3
                    ],
                },
                Rgba8 {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                },
            ),
            Err(Map16PngDecodeError::PixelCount(3))
        ));
    }

    #[test]
    fn dimension_preserving_png_decoder_accepts_rectangular_sources() {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 17, 15);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&vec![0x7f; 17 * 15 * 4]).unwrap();
        }
        let decoded = decode_map16_bitmap_png_image(&bytes).unwrap();
        assert_eq!((decoded.width, decoded.height), (17, 15));
        assert_eq!(decoded.pixels.len(), 17 * 15);
        assert!(matches!(
            decode_map16_bitmap_png(&bytes),
            Err(Map16PngDecodeError::Dimensions {
                width: 17,
                height: 15
            })
        ));
    }

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
            width: MAP16_BITMAP_WIDTH,
            height: MAP16_BITMAP_HEIGHT,
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
        assert_eq!(plan.converted_pixels()[0].alpha, 255);
    }

    #[test]
    fn plan_retains_map16_definitions_beyond_one_page_width() {
        let width = 17 * 16;
        let height = 16;
        let pixels = vec![
            Rgba8 {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255,
            };
            width * height
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
            width,
            height,
            palette_row: 2,
            acts_like: 0x130,
            palette: &palette,
            palette_ownership: &PaletteOwnership::editable(palette.colors.len()),
            graphics: &graphics,
            graphics_ownership: &GraphicsOwnership::editable(graphics.tiles.len()),
            occupied: &occupied,
        })
        .unwrap();

        assert_eq!(plan.width_in_map16_tiles, 17);
        assert_eq!(plan.height_in_map16_tiles, 1);
        assert_eq!(plan.map16_tiles.len(), 17);
        assert!(plan.map16_tiles.iter().all(|tile| tile.acts_like == 0x130));
        assert_eq!(
            plan.page.tiles[..16],
            plan.map16_tiles[..16],
            "the compatibility page must expose the top-left 16 definitions"
        );
        assert_eq!(
            plan.map16_tiles[16], plan.map16_tiles[0],
            "the seventeenth definition must remain available outside the page view"
        );
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
                width: MAP16_BITMAP_WIDTH,
                height: MAP16_BITMAP_HEIGHT,
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

    #[test]
    fn explicit_priority_and_allocation_options_change_the_preview_plan() {
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
            tiles: vec![IndexedTile::new([0; 64]); 0x300],
        };
        let occupied = vec![false; 0x300];
        let plan = Map16BitmapImportPlan::prepare_with_options(
            Map16BitmapImportRequest {
                pixels: &pixels,
                width: MAP16_BITMAP_WIDTH,
                height: MAP16_BITMAP_HEIGHT,
                palette_row: 2,
                acts_like: 0,
                palette: &palette,
                palette_ownership: &PaletteOwnership::editable(128),
                graphics: &graphics,
                graphics_ownership: &GraphicsOwnership::editable(0x300),
                occupied: &occupied,
            },
            Map16BitmapImportOptions {
                graphics: IndexedBitmapImportOptions {
                    allocation_start: 0x200,
                    allocation_end: 0x300,
                    ..IndexedBitmapImportOptions::default()
                },
                color: None,
                deduplicate_map16: true,
                layer_priority: true,
            },
        )
        .unwrap();
        assert_eq!(plan.page.tiles[0].top_left.0 & 0x23ff, 0x2200);
        assert!(plan.occupied[0x200]);
        assert!(!plan.occupied[..0x200].iter().any(|occupied| *occupied));
    }

    #[test]
    fn multi_row_import_writes_each_map16_subtile_palette_bits_and_preview() {
        let mut pixels = vec![opaque(0, 0, 0); 16 * 16];
        for y in 0..16 {
            for x in 0..16 {
                pixels[y * 16 + x] = if x < 8 {
                    if (x + y) % 2 == 0 {
                        opaque(255, 0, 0)
                    } else {
                        opaque(0, 255, 0)
                    }
                } else if (x + y) % 2 == 0 {
                    opaque(0, 0, 255)
                } else {
                    opaque(255, 255, 0)
                };
            }
        }
        let palette = Palette {
            colors: vec![Bgr555(0); 128],
        };
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0; 64]); 0x100],
        };
        let mut color = BitmapPaletteColorOptions::lunar_magic_initial();
        color.entries.fill(BitmapPaletteEntryState::Reserved);
        for row in 0..2 {
            color.entries[row * 16] = BitmapPaletteEntryState::Reusable;
            color.entries[row * 16 + 1] = BitmapPaletteEntryState::Free;
            color.entries[row * 16 + 2] = BitmapPaletteEntryState::Free;
        }
        let plan = Map16BitmapImportPlan::prepare_with_options(
            Map16BitmapImportRequest {
                pixels: &pixels,
                width: 16,
                height: 16,
                palette_row: 7,
                acts_like: 0x130,
                palette: &palette,
                palette_ownership: &PaletteOwnership::editable(128),
                graphics: &graphics,
                graphics_ownership: &GraphicsOwnership::editable(0x100),
                occupied: &vec![false; 0x100],
            },
            Map16BitmapImportOptions {
                graphics: IndexedBitmapImportOptions {
                    allocation_end: 0x100,
                    ..IndexedBitmapImportOptions::default()
                },
                color: Some(color),
                deduplicate_map16: true,
                layer_priority: false,
            },
        )
        .unwrap();

        assert_eq!(plan.tile_palette_rows, vec![0, 1, 0, 1]);
        assert_eq!(plan.map16_tiles[0].top_left.0 & 0x1c00, 0);
        assert_eq!(plan.map16_tiles[0].top_right.0 & 0x1c00, 1 << 10);
        assert_eq!(plan.map16_tiles[0].bottom_left.0 & 0x1c00, 0);
        assert_eq!(plan.map16_tiles[0].bottom_right.0 & 0x1c00, 1 << 10);
        let converted = plan.converted_pixels();
        assert_eq!(converted[0], pixels[0]);
        assert_eq!(converted[8], pixels[8]);
    }
}
