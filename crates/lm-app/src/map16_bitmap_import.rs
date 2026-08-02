//! Staged cross-domain Map16 bitmap import planning.

use lm_graphics::{
    BitmapImportError, BitmapPaletteColorOptions, BitmapPaletteEntryState,
    BitmapPaletteReductionError, GraphicsFile4bpp, GraphicsOwnership, IndexedBitmapImport,
    IndexedBitmapImportOptions, PaletteEntryOwner, PaletteImportError, PaletteOwnership, Rgba8,
    TransparentPaletteRowImport, allocate_bitmap_palette_rows, reduce_bitmap_palette_with_palette,
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
        let reduced = reduce_bitmap_palette_with_palette(request.pixels, request.palette, &options)
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

/// Decodes a bounded PNG or uncompressed Windows BMP while retaining its source dimensions.
///
/// BMP input accepts the common 40-byte-or-larger DIB header with indexed 1-, 4-, or 8-bit pixels,
/// 24-bit BGR, or 32-bit BGRX pixels, padded rows, and either bottom-up or top-down storage. The
/// unused 32-bit BMP byte is deliberately treated as opaque, matching Windows `BI_RGB` semantics.
///
/// # Errors
///
/// Rejects unknown signatures and the format-specific malformed, oversized, or unsupported forms.
pub fn decode_map16_bitmap_image(
    bytes: &[u8],
) -> Result<DecodedMap16Bitmap, Map16BitmapDecodeError> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return decode_map16_bitmap_png_image(bytes).map_err(Map16BitmapDecodeError::Png);
    }
    if bytes.starts_with(b"BM") {
        return decode_map16_bitmap_bmp_image(bytes).map_err(Map16BitmapDecodeError::Bmp);
    }
    Err(Map16BitmapDecodeError::UnknownFormat)
}

/// Decodes a bounded uncompressed indexed, 24-bit, or 32-bit Windows BMP.
///
/// # Errors
///
/// Rejects oversized/truncated headers and pixel planes, invalid dimensions, non-single-plane
/// images, compressed data, unsupported bit depths, and arithmetic overflow.
pub fn decode_map16_bitmap_bmp_image(
    bytes: &[u8],
) -> Result<DecodedMap16Bitmap, Map16BmpDecodeError> {
    if bytes.len() > MAP16_BITMAP_MAX_PNG_BYTES {
        return Err(Map16BmpDecodeError::InputTooLarge(bytes.len()));
    }
    if !bytes.starts_with(b"BM") || bytes.len() < 54 {
        return Err(Map16BmpDecodeError::Header);
    }
    let pixel_offset =
        usize::try_from(bmp_u32(bytes, 10)?).map_err(|_| Map16BmpDecodeError::Header)?;
    let dib_size = usize::try_from(bmp_u32(bytes, 14)?).map_err(|_| Map16BmpDecodeError::Header)?;
    if dib_size < 40
        || 14_usize
            .checked_add(dib_size)
            .is_none_or(|end| end > bytes.len())
    {
        return Err(Map16BmpDecodeError::Header);
    }
    let width_raw = bmp_i32(bytes, 18)?;
    let height_raw = bmp_i32(bytes, 22)?;
    if width_raw <= 0 || height_raw == 0 || height_raw == i32::MIN {
        return Err(Map16BmpDecodeError::Dimensions {
            width: width_raw,
            height: height_raw,
        });
    }
    let width = usize::try_from(width_raw).map_err(|_| Map16BmpDecodeError::Dimensions {
        width: width_raw,
        height: height_raw,
    })?;
    let height = usize::try_from(height_raw.unsigned_abs()).map_err(|_| {
        Map16BmpDecodeError::Dimensions {
            width: width_raw,
            height: height_raw,
        }
    })?;
    if width > MAP16_BITMAP_MAX_DIMENSION || height > MAP16_BITMAP_MAX_DIMENSION {
        return Err(Map16BmpDecodeError::Dimensions {
            width: width_raw,
            height: height_raw,
        });
    }
    let planes = bmp_u16(bytes, 26)?;
    let bits = bmp_u16(bytes, 28)?;
    let compression = bmp_u32(bytes, 30)?;
    if planes != 1 {
        return Err(Map16BmpDecodeError::Planes(planes));
    }
    if !matches!(bits, 1 | 4 | 8 | 24 | 32) {
        return Err(Map16BmpDecodeError::BitDepth(bits));
    }
    if compression != 0 {
        return Err(Map16BmpDecodeError::Compression(compression));
    }
    let row_bits = width
        .checked_mul(usize::from(bits))
        .ok_or(Map16BmpDecodeError::PixelData)?;
    let row_bytes = row_bits
        .checked_add(7)
        .map(|value| value / 8)
        .ok_or(Map16BmpDecodeError::PixelData)?;
    let stride = row_bytes
        .checked_add(3)
        .map(|value| value & !3)
        .ok_or(Map16BmpDecodeError::PixelData)?;
    let data_len = stride
        .checked_mul(height)
        .ok_or(Map16BmpDecodeError::PixelData)?;
    let pixel_end = pixel_offset
        .checked_add(data_len)
        .ok_or(Map16BmpDecodeError::PixelData)?;
    let palette = if bits <= 8 {
        let declared =
            usize::try_from(bmp_u32(bytes, 46)?).map_err(|_| Map16BmpDecodeError::Palette)?;
        let maximum = 1_usize << bits;
        let entries = if declared == 0 { maximum } else { declared };
        if entries == 0 || entries > maximum {
            return Err(Map16BmpDecodeError::Palette);
        }
        let palette_start = 14_usize
            .checked_add(dib_size)
            .ok_or(Map16BmpDecodeError::Palette)?;
        let palette_end = entries
            .checked_mul(4)
            .and_then(|length| palette_start.checked_add(length))
            .ok_or(Map16BmpDecodeError::Palette)?;
        if palette_end > pixel_offset || palette_end > bytes.len() {
            return Err(Map16BmpDecodeError::Palette);
        }
        Some((palette_start, entries))
    } else {
        None
    };
    if pixel_offset < 14 + dib_size || pixel_end > bytes.len() {
        return Err(Map16BmpDecodeError::PixelData);
    }
    let pixel_count = width
        .checked_mul(height)
        .ok_or(Map16BmpDecodeError::PixelData)?;
    let mut pixels = vec![
        Rgba8 {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        pixel_count
    ];
    for target_row in 0..height {
        let source_row = if height_raw < 0 {
            target_row
        } else {
            height - 1 - target_row
        };
        let source = pixel_offset + source_row * stride;
        for column in 0..width {
            let pixel = if let Some((palette_start, palette_entries)) = palette {
                let index = match bits {
                    1 => (bytes[source + column / 8] >> (7 - column % 8)) & 1,
                    4 => {
                        let packed = bytes[source + column / 2];
                        if column % 2 == 0 {
                            packed >> 4
                        } else {
                            packed & 0x0f
                        }
                    }
                    8 => bytes[source + column],
                    _ => unreachable!("palette exists only for indexed BMP depths"),
                };
                let index = usize::from(index);
                if index >= palette_entries {
                    return Err(Map16BmpDecodeError::Palette);
                }
                let at = palette_start + index * 4;
                Rgba8 {
                    red: bytes[at + 2],
                    green: bytes[at + 1],
                    blue: bytes[at],
                    alpha: 255,
                }
            } else {
                let bytes_per_pixel = usize::from(bits / 8);
                let at = source + column * bytes_per_pixel;
                Rgba8 {
                    red: bytes[at + 2],
                    green: bytes[at + 1],
                    blue: bytes[at],
                    alpha: 255,
                }
            };
            pixels[target_row * width + column] = pixel;
        }
    }
    Ok(DecodedMap16Bitmap {
        width,
        height,
        pixels,
    })
}

fn bmp_u16(bytes: &[u8], offset: usize) -> Result<u16, Map16BmpDecodeError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(Map16BmpDecodeError::Header)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn bmp_u32(bytes: &[u8], offset: usize) -> Result<u32, Map16BmpDecodeError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(Map16BmpDecodeError::Header)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn bmp_i32(bytes: &[u8], offset: usize) -> Result<i32, Map16BmpDecodeError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(Map16BmpDecodeError::Header)?;
    Ok(i32::from_le_bytes([value[0], value[1], value[2], value[3]]))
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16BmpDecodeError {
    InputTooLarge(usize),
    Header,
    Dimensions { width: i32, height: i32 },
    Planes(u16),
    BitDepth(u16),
    Compression(u32),
    Palette,
    PixelData,
}

impl fmt::Display for Map16BmpDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Map16 bitmap BMP decoding failed: {self:?}")
    }
}

impl std::error::Error for Map16BmpDecodeError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16BitmapDecodeError {
    Png(Map16PngDecodeError),
    Bmp(Map16BmpDecodeError),
    UnknownFormat,
}

impl fmt::Display for Map16BitmapDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Map16 bitmap decoding failed: {self:?}")
    }
}

impl std::error::Error for Map16BitmapDecodeError {}

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

    fn test_bmp(
        width: usize,
        height: usize,
        bits: u16,
        top_down: bool,
        pixels: &[Rgba8],
    ) -> Vec<u8> {
        assert_eq!(pixels.len(), width * height);
        let bytes_per_pixel = usize::from(bits / 8);
        let row_bytes = width * bytes_per_pixel;
        let stride = (row_bytes + 3) & !3;
        let pixel_offset = 54_usize;
        let mut bytes = vec![0; pixel_offset + stride * height];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&i32::try_from(width).unwrap().to_le_bytes());
        let signed_height = if top_down {
            -i32::try_from(height).unwrap()
        } else {
            i32::try_from(height).unwrap()
        };
        bytes[22..26].copy_from_slice(&signed_height.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&bits.to_le_bytes());
        bytes[34..38].copy_from_slice(&u32::try_from(stride * height).unwrap().to_le_bytes());
        for stored_row in 0..height {
            let source_row = if top_down {
                stored_row
            } else {
                height - 1 - stored_row
            };
            for column in 0..width {
                let pixel = pixels[source_row * width + column];
                let at = pixel_offset + stored_row * stride + column * bytes_per_pixel;
                bytes[at..at + 3].copy_from_slice(&[pixel.blue, pixel.green, pixel.red]);
                if bits == 32 {
                    bytes[at + 3] = 0x37;
                }
            }
        }
        bytes
    }

    fn test_indexed_bmp(
        width: usize,
        height: usize,
        bits: u16,
        top_down: bool,
        palette: &[Rgba8],
        indices: &[u8],
    ) -> Vec<u8> {
        assert!(matches!(bits, 1 | 4 | 8));
        assert_eq!(indices.len(), width * height);
        let row_bytes = (width * usize::from(bits) + 7) / 8;
        let stride = (row_bytes + 3) & !3;
        let pixel_offset = 54 + palette.len() * 4;
        let mut bytes = vec![0; pixel_offset + stride * height];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&i32::try_from(width).unwrap().to_le_bytes());
        let signed_height = if top_down {
            -i32::try_from(height).unwrap()
        } else {
            i32::try_from(height).unwrap()
        };
        bytes[22..26].copy_from_slice(&signed_height.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&bits.to_le_bytes());
        bytes[34..38].copy_from_slice(&u32::try_from(stride * height).unwrap().to_le_bytes());
        bytes[46..50].copy_from_slice(&u32::try_from(palette.len()).unwrap().to_le_bytes());
        for (index, color) in palette.iter().enumerate() {
            let at = 54 + index * 4;
            bytes[at..at + 4].copy_from_slice(&[color.blue, color.green, color.red, 0x55]);
        }
        for stored_row in 0..height {
            let source_row = if top_down {
                stored_row
            } else {
                height - 1 - stored_row
            };
            let target = pixel_offset + stored_row * stride;
            for column in 0..width {
                let index = indices[source_row * width + column];
                match bits {
                    1 => bytes[target + column / 8] |= index << (7 - column % 8),
                    4 => {
                        bytes[target + column / 2] |=
                            if column % 2 == 0 { index << 4 } else { index }
                    }
                    8 => bytes[target + column] = index,
                    _ => unreachable!(),
                }
            }
        }
        bytes
    }

    #[test]
    fn bmp_decoder_handles_padded_bottom_up_and_top_down_bgrx() {
        let pixels = vec![
            Rgba8 {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 255,
            },
            Rgba8 {
                red: 4,
                green: 5,
                blue: 6,
                alpha: 255,
            },
            Rgba8 {
                red: 7,
                green: 8,
                blue: 9,
                alpha: 255,
            },
            Rgba8 {
                red: 10,
                green: 11,
                blue: 12,
                alpha: 255,
            },
            Rgba8 {
                red: 13,
                green: 14,
                blue: 15,
                alpha: 255,
            },
            Rgba8 {
                red: 16,
                green: 17,
                blue: 18,
                alpha: 255,
            },
        ];
        for (bits, top_down) in [(24, false), (32, true)] {
            let bytes = test_bmp(3, 2, bits, top_down, &pixels);
            let decoded = decode_map16_bitmap_bmp_image(&bytes).unwrap();
            assert_eq!((decoded.width, decoded.height), (3, 2));
            assert_eq!(decoded.pixels, pixels);
            assert_eq!(decode_map16_bitmap_image(&bytes).unwrap(), decoded);
        }
    }

    #[test]
    fn bmp_decoder_handles_packed_indexed_depths_and_palette_padding() {
        let palette = [
            Rgba8 {
                red: 3,
                green: 2,
                blue: 1,
                alpha: 255,
            },
            Rgba8 {
                red: 6,
                green: 5,
                blue: 4,
                alpha: 255,
            },
            Rgba8 {
                red: 9,
                green: 8,
                blue: 7,
                alpha: 255,
            },
            Rgba8 {
                red: 12,
                green: 11,
                blue: 10,
                alpha: 255,
            },
        ];
        for (bits, width, top_down, indices) in [
            (1, 9, false, vec![0, 1, 0, 1, 1, 0, 1, 0, 1]),
            (4, 5, true, vec![0, 1, 2, 3, 1]),
            (8, 3, false, vec![3, 2, 1]),
        ] {
            let palette_len = if bits == 1 { 2 } else { palette.len() };
            let mut bytes =
                test_indexed_bmp(width, 1, bits, top_down, &palette[..palette_len], &indices);
            if bits == 1 {
                bytes[46..50].copy_from_slice(&0_u32.to_le_bytes());
            }
            let decoded = decode_map16_bitmap_bmp_image(&bytes).unwrap();
            assert_eq!((decoded.width, decoded.height), (width, 1));
            assert_eq!(
                decoded.pixels,
                indices
                    .iter()
                    .map(|index| palette[usize::from(*index)])
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn bmp_decoder_rejects_invalid_palette_shape_and_indices() {
        let palette = [
            Rgba8 {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 255,
            },
            Rgba8 {
                red: 4,
                green: 5,
                blue: 6,
                alpha: 255,
            },
        ];
        let mut excessive = test_indexed_bmp(1, 1, 4, false, &palette, &[0]);
        excessive[46..50].copy_from_slice(&17_u32.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&excessive),
            Err(Map16BmpDecodeError::Palette)
        );

        let out_of_range = test_indexed_bmp(1, 1, 4, false, &palette, &[3]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&out_of_range),
            Err(Map16BmpDecodeError::Palette)
        );
    }

    #[test]
    fn bmp_decoder_rejects_unsupported_and_truncated_forms() {
        let pixel = Rgba8 {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 255,
        };
        let valid = test_bmp(1, 1, 24, false, &[pixel]);

        let mut compressed = valid.clone();
        compressed[30..34].copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&compressed),
            Err(Map16BmpDecodeError::Compression(1))
        );
        let mut unsupported_depth = valid.clone();
        unsupported_depth[28..30].copy_from_slice(&16_u16.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&unsupported_depth),
            Err(Map16BmpDecodeError::BitDepth(16))
        );
        assert_eq!(
            decode_map16_bitmap_bmp_image(&valid[..valid.len() - 1]),
            Err(Map16BmpDecodeError::PixelData)
        );
        assert_eq!(
            decode_map16_bitmap_image(b"not an image"),
            Err(Map16BitmapDecodeError::UnknownFormat)
        );
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
