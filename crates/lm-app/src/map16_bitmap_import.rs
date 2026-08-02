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

/// Decodes a bounded PNG or Windows BMP while retaining its source dimensions.
///
/// BMP input accepts the 12-byte core header, the 64-byte OS/2 2.x header, and common Windows
/// 40-byte-or-larger DIB headers. Supported pixels include indexed 1-/2-/4-/8-bit, 16-bit RGB,
/// 24-bit BGR, 32-bit BGRX, validated 16-/32-bit RGB/alpha bitfields, RLE4/RLE8, OS/2 RLE24,
/// and embedded JPEG/PNG payloads. The unused 32-bit `BI_RGB` byte remains opaque.
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

/// Decodes a bounded indexed, RGB, bitfield, RLE, or embedded-image Windows BMP.
///
/// # Errors
///
/// Rejects oversized/truncated headers and pixel planes, invalid dimensions, non-single-plane
/// images, unsupported compression/depth combinations, and arithmetic overflow.
pub fn decode_map16_bitmap_bmp_image(
    bytes: &[u8],
) -> Result<DecodedMap16Bitmap, Map16BmpDecodeError> {
    if bytes.len() > MAP16_BITMAP_MAX_PNG_BYTES {
        return Err(Map16BmpDecodeError::InputTooLarge(bytes.len()));
    }
    if !bytes.starts_with(b"BM") || bytes.len() < 18 {
        return Err(Map16BmpDecodeError::Header);
    }
    let pixel_offset =
        usize::try_from(bmp_u32(bytes, 10)?).map_err(|_| Map16BmpDecodeError::Header)?;
    let dib_size = usize::try_from(bmp_u32(bytes, 14)?).map_err(|_| Map16BmpDecodeError::Header)?;
    if dib_size == 12 {
        return decode_core_bmp_image(bytes, pixel_offset);
    }
    if dib_size == 64 {
        return decode_os2_v2_bmp_image(bytes, pixel_offset);
    }
    if bytes.len() < 54 {
        return Err(Map16BmpDecodeError::Header);
    }
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
    if !(matches!(bits, 1 | 2 | 4 | 8 | 16 | 24 | 32) || matches!(compression, 4 | 5) && bits == 0)
    {
        return Err(Map16BmpDecodeError::BitDepth(bits));
    }
    let compression_supported = match compression {
        0 => true,
        1 => bits == 8,
        2 => bits == 4,
        3 => matches!(bits, 16 | 32),
        4 | 5 => bits == 0,
        6 => matches!(bits, 16 | 32),
        _ => false,
    };
    if !compression_supported {
        return Err(Map16BmpDecodeError::Compression(compression));
    }
    if matches!(compression, 1 | 2) && height_raw < 0 {
        return Err(Map16BmpDecodeError::Rle);
    }
    if matches!(compression, 4 | 5) && height_raw < 0 {
        return Err(Map16BmpDecodeError::Compression(compression));
    }
    if matches!(compression, 4 | 5) {
        return decode_embedded_bmp_image(bytes, pixel_offset, compression, width, height);
    }
    let channel_masks = bmp_channel_masks(bytes, dib_size, pixel_offset, bits, compression)?;
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
    if pixel_offset < 14 + dib_size {
        return Err(Map16BmpDecodeError::PixelData);
    }
    let pixel_count = width
        .checked_mul(height)
        .ok_or(Map16BmpDecodeError::PixelData)?;
    if matches!(compression, 1 | 2) {
        let (palette_start, palette_entries) = palette.ok_or(Map16BmpDecodeError::Palette)?;
        let declared_size =
            usize::try_from(bmp_u32(bytes, 34)?).map_err(|_| Map16BmpDecodeError::Rle)?;
        let stream_end = if declared_size == 0 {
            bytes.len()
        } else {
            pixel_offset
                .checked_add(declared_size)
                .ok_or(Map16BmpDecodeError::Rle)?
        };
        let stream = bytes
            .get(pixel_offset..stream_end)
            .ok_or(Map16BmpDecodeError::Rle)?;
        let indices = decode_bmp_rle(stream, width, height, bits, palette_entries)?;
        let pixels = indices
            .into_iter()
            .map(|index| {
                let at = palette_start + usize::from(index) * 4;
                Rgba8 {
                    red: bytes[at + 2],
                    green: bytes[at + 1],
                    blue: bytes[at],
                    alpha: 255,
                }
            })
            .collect();
        return Ok(DecodedMap16Bitmap {
            width,
            height,
            pixels,
        });
    }
    if pixel_end > bytes.len() {
        return Err(Map16BmpDecodeError::PixelData);
    }
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
                    2 => (bytes[source + column / 4] >> (6 - (column % 4) * 2)) & 3,
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
            } else if let Some(masks) = channel_masks {
                let bytes_per_pixel = usize::from(bits / 8);
                let at = source + column * bytes_per_pixel;
                let packed = if bits == 16 {
                    u32::from(bmp_u16(bytes, at).map_err(|_| Map16BmpDecodeError::PixelData)?)
                } else {
                    bmp_u32(bytes, at).map_err(|_| Map16BmpDecodeError::PixelData)?
                };
                Rgba8 {
                    red: bmp_masked_channel(packed, masks.rgb[0]),
                    green: bmp_masked_channel(packed, masks.rgb[1]),
                    blue: bmp_masked_channel(packed, masks.rgb[2]),
                    alpha: masks
                        .alpha
                        .map_or(255, |mask| bmp_masked_channel(packed, mask)),
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

fn decode_core_bmp_image(
    bytes: &[u8],
    pixel_offset: usize,
) -> Result<DecodedMap16Bitmap, Map16BmpDecodeError> {
    if bytes.len() < 26 {
        return Err(Map16BmpDecodeError::Header);
    }
    let width = usize::from(bmp_u16(bytes, 18)?);
    let height = usize::from(bmp_u16(bytes, 20)?);
    if width == 0
        || height == 0
        || width > MAP16_BITMAP_MAX_DIMENSION
        || height > MAP16_BITMAP_MAX_DIMENSION
    {
        return Err(Map16BmpDecodeError::Dimensions {
            width: i32::try_from(width).unwrap_or(i32::MAX),
            height: i32::try_from(height).unwrap_or(i32::MAX),
        });
    }
    let planes = bmp_u16(bytes, 22)?;
    let bits = bmp_u16(bytes, 24)?;
    if planes != 1 {
        return Err(Map16BmpDecodeError::Planes(planes));
    }
    if !matches!(bits, 1 | 4 | 8 | 24) {
        return Err(Map16BmpDecodeError::BitDepth(bits));
    }
    let palette = if bits <= 8 {
        let entries = 1_usize << bits;
        let start = 26_usize;
        let end = entries
            .checked_mul(3)
            .and_then(|length| start.checked_add(length))
            .ok_or(Map16BmpDecodeError::Palette)?;
        if end > pixel_offset || end > bytes.len() {
            return Err(Map16BmpDecodeError::Palette);
        }
        Some((start, entries))
    } else {
        if pixel_offset < 26 {
            return Err(Map16BmpDecodeError::PixelData);
        }
        None
    };
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
    let pixel_end = stride
        .checked_mul(height)
        .and_then(|length| pixel_offset.checked_add(length))
        .ok_or(Map16BmpDecodeError::PixelData)?;
    if pixel_end > bytes.len() {
        return Err(Map16BmpDecodeError::PixelData);
    }
    let pixel_count = width
        .checked_mul(height)
        .ok_or(Map16BmpDecodeError::PixelData)?;
    let mut pixels = Vec::with_capacity(pixel_count);
    for target_row in 0..height {
        let source = pixel_offset + (height - 1 - target_row) * stride;
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
                    _ => unreachable!("core palette exists only for indexed depths"),
                };
                let index = usize::from(index);
                if index >= palette_entries {
                    return Err(Map16BmpDecodeError::Palette);
                }
                let at = palette_start + index * 3;
                Rgba8 {
                    red: bytes[at + 2],
                    green: bytes[at + 1],
                    blue: bytes[at],
                    alpha: 255,
                }
            } else {
                let at = source + column * 3;
                Rgba8 {
                    red: bytes[at + 2],
                    green: bytes[at + 1],
                    blue: bytes[at],
                    alpha: 255,
                }
            };
            pixels.push(pixel);
        }
    }
    Ok(DecodedMap16Bitmap {
        width,
        height,
        pixels,
    })
}

fn decode_os2_v2_bmp_image(
    bytes: &[u8],
    pixel_offset: usize,
) -> Result<DecodedMap16Bitmap, Map16BmpDecodeError> {
    if bytes.len() < 78 {
        return Err(Map16BmpDecodeError::Header);
    }
    let width_raw = bmp_u32(bytes, 18)?;
    let height_raw = bmp_u32(bytes, 22)?;
    let width = usize::try_from(width_raw).map_err(|_| Map16BmpDecodeError::Dimensions {
        width: i32::MAX,
        height: i32::try_from(height_raw).unwrap_or(i32::MAX),
    })?;
    let height = usize::try_from(height_raw).map_err(|_| Map16BmpDecodeError::Dimensions {
        width: i32::try_from(width_raw).unwrap_or(i32::MAX),
        height: i32::MAX,
    })?;
    if width == 0
        || height == 0
        || width > MAP16_BITMAP_MAX_DIMENSION
        || height > MAP16_BITMAP_MAX_DIMENSION
    {
        return Err(Map16BmpDecodeError::Dimensions {
            width: i32::try_from(width_raw).unwrap_or(i32::MAX),
            height: i32::try_from(height_raw).unwrap_or(i32::MAX),
        });
    }
    let planes = bmp_u16(bytes, 26)?;
    let bits = bmp_u16(bytes, 28)?;
    let compression = bmp_u32(bytes, 30)?;
    if planes != 1 {
        return Err(Map16BmpDecodeError::Planes(planes));
    }
    if !matches!(bits, 1 | 2 | 4 | 8 | 24) {
        return Err(Map16BmpDecodeError::BitDepth(bits));
    }
    let compression_supported = match compression {
        0 => true,
        1 => bits == 8,
        2 => bits == 4,
        3 => bits == 1,
        4 => bits == 24,
        _ => false,
    };
    if !compression_supported {
        return Err(Map16BmpDecodeError::Compression(compression));
    }
    let palette = if bits <= 8 {
        let maximum = 1_usize << bits;
        let declared =
            usize::try_from(bmp_u32(bytes, 46)?).map_err(|_| Map16BmpDecodeError::Palette)?;
        let entries = if declared == 0 { maximum } else { declared };
        if entries == 0 || entries > maximum {
            return Err(Map16BmpDecodeError::Palette);
        }
        let start = 78_usize;
        let end = entries
            .checked_mul(4)
            .and_then(|length| start.checked_add(length))
            .ok_or(Map16BmpDecodeError::Palette)?;
        if end > pixel_offset || end > bytes.len() {
            return Err(Map16BmpDecodeError::Palette);
        }
        Some((start, entries))
    } else {
        if pixel_offset < 78 {
            return Err(Map16BmpDecodeError::PixelData);
        }
        None
    };
    let pixel_count = width
        .checked_mul(height)
        .ok_or(Map16BmpDecodeError::PixelData)?;
    if compression == 3 {
        let (palette_start, palette_entries) = palette.ok_or(Map16BmpDecodeError::Palette)?;
        if palette_entries != 2 {
            return Err(Map16BmpDecodeError::Palette);
        }
        let declared_size =
            usize::try_from(bmp_u32(bytes, 34)?).map_err(|_| Map16BmpDecodeError::Rle)?;
        let stream_end = if declared_size == 0 {
            bytes.len()
        } else {
            pixel_offset
                .checked_add(declared_size)
                .ok_or(Map16BmpDecodeError::Rle)?
        };
        let stream = bytes
            .get(pixel_offset..stream_end)
            .ok_or(Map16BmpDecodeError::Rle)?;
        let palette = [
            Rgba8 {
                red: bytes[palette_start + 2],
                green: bytes[palette_start + 1],
                blue: bytes[palette_start],
                alpha: 255,
            },
            Rgba8 {
                red: bytes[palette_start + 6],
                green: bytes[palette_start + 5],
                blue: bytes[palette_start + 4],
                alpha: 255,
            },
        ];
        return Ok(DecodedMap16Bitmap {
            width,
            height,
            pixels: decode_os2_bmp_huffman1d(stream, width, height, palette)?,
        });
    }
    if compression == 4 {
        let declared_size =
            usize::try_from(bmp_u32(bytes, 34)?).map_err(|_| Map16BmpDecodeError::Rle)?;
        let stream_end = if declared_size == 0 {
            bytes.len()
        } else {
            pixel_offset
                .checked_add(declared_size)
                .ok_or(Map16BmpDecodeError::Rle)?
        };
        let stream = bytes
            .get(pixel_offset..stream_end)
            .ok_or(Map16BmpDecodeError::Rle)?;
        return Ok(DecodedMap16Bitmap {
            width,
            height,
            pixels: decode_os2_bmp_rle24(stream, width, height)?,
        });
    }
    if matches!(compression, 1 | 2) {
        let (palette_start, palette_entries) = palette.ok_or(Map16BmpDecodeError::Palette)?;
        let declared_size =
            usize::try_from(bmp_u32(bytes, 34)?).map_err(|_| Map16BmpDecodeError::Rle)?;
        let stream_end = if declared_size == 0 {
            bytes.len()
        } else {
            pixel_offset
                .checked_add(declared_size)
                .ok_or(Map16BmpDecodeError::Rle)?
        };
        let stream = bytes
            .get(pixel_offset..stream_end)
            .ok_or(Map16BmpDecodeError::Rle)?;
        let indices = decode_bmp_rle(stream, width, height, bits, palette_entries)?;
        let pixels = indices
            .into_iter()
            .map(|index| {
                let at = palette_start + usize::from(index) * 4;
                Rgba8 {
                    red: bytes[at + 2],
                    green: bytes[at + 1],
                    blue: bytes[at],
                    alpha: 255,
                }
            })
            .collect();
        return Ok(DecodedMap16Bitmap {
            width,
            height,
            pixels,
        });
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
    let pixel_end = stride
        .checked_mul(height)
        .and_then(|length| pixel_offset.checked_add(length))
        .ok_or(Map16BmpDecodeError::PixelData)?;
    if pixel_end > bytes.len() {
        return Err(Map16BmpDecodeError::PixelData);
    }
    let mut pixels = Vec::with_capacity(pixel_count);
    for target_row in 0..height {
        let source = pixel_offset + (height - 1 - target_row) * stride;
        for column in 0..width {
            let pixel = if let Some((palette_start, palette_entries)) = palette {
                let index = match bits {
                    1 => (bytes[source + column / 8] >> (7 - column % 8)) & 1,
                    2 => (bytes[source + column / 4] >> (6 - (column % 4) * 2)) & 3,
                    4 => {
                        let packed = bytes[source + column / 2];
                        if column % 2 == 0 {
                            packed >> 4
                        } else {
                            packed & 0x0f
                        }
                    }
                    8 => bytes[source + column],
                    _ => unreachable!("OS/2 palette exists only for indexed depths"),
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
                let at = source + column * 3;
                Rgba8 {
                    red: bytes[at + 2],
                    green: bytes[at + 1],
                    blue: bytes[at],
                    alpha: 255,
                }
            };
            pixels.push(pixel);
        }
    }
    Ok(DecodedMap16Bitmap {
        width,
        height,
        pixels,
    })
}

fn decode_embedded_bmp_image(
    bytes: &[u8],
    pixel_offset: usize,
    compression: u32,
    expected_width: usize,
    expected_height: usize,
) -> Result<DecodedMap16Bitmap, Map16BmpDecodeError> {
    let declared_size =
        usize::try_from(bmp_u32(bytes, 34)?).map_err(|_| Map16BmpDecodeError::PixelData)?;
    if declared_size == 0 {
        return Err(Map16BmpDecodeError::PixelData);
    }
    let payload_end = pixel_offset
        .checked_add(declared_size)
        .ok_or(Map16BmpDecodeError::PixelData)?;
    let payload = bytes
        .get(pixel_offset..payload_end)
        .ok_or(Map16BmpDecodeError::PixelData)?;
    let decoded = if compression == 5 {
        decode_map16_bitmap_png_image(payload)
            .map_err(|error| Map16BmpDecodeError::Embedded(error.to_string()))?
    } else {
        decode_map16_bitmap_jpeg_image(payload)?
    };
    if decoded.width != expected_width || decoded.height != expected_height {
        return Err(Map16BmpDecodeError::EmbeddedDimensions {
            header_width: expected_width,
            header_height: expected_height,
            image_width: decoded.width,
            image_height: decoded.height,
        });
    }
    Ok(decoded)
}

fn decode_map16_bitmap_jpeg_image(bytes: &[u8]) -> Result<DecodedMap16Bitmap, Map16BmpDecodeError> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    decoder.set_max_decoding_buffer_size(MAX_PNG_DECODE_BYTES);
    let decoded_bytes = decoder
        .decode()
        .map_err(|error| Map16BmpDecodeError::Embedded(error.to_string()))?;
    let info = decoder
        .info()
        .ok_or_else(|| Map16BmpDecodeError::Embedded("JPEG metadata is missing".into()))?;
    let width = usize::from(info.width);
    let height = usize::from(info.height);
    if width == 0
        || height == 0
        || width > MAP16_BITMAP_MAX_DIMENSION
        || height > MAP16_BITMAP_MAX_DIMENSION
    {
        return Err(Map16BmpDecodeError::EmbeddedDimensions {
            header_width: width,
            header_height: height,
            image_width: width,
            image_height: height,
        });
    }
    let pixels: Vec<Rgba8> = match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => decoded_bytes
            .into_iter()
            .map(|value| Rgba8 {
                red: value,
                green: value,
                blue: value,
                alpha: 255,
            })
            .collect(),
        jpeg_decoder::PixelFormat::RGB24 => decoded_bytes
            .chunks_exact(3)
            .map(|pixel| Rgba8 {
                red: pixel[0],
                green: pixel[1],
                blue: pixel[2],
                alpha: 255,
            })
            .collect(),
        jpeg_decoder::PixelFormat::L16 => decoded_bytes
            .chunks_exact(2)
            .map(|pixel| {
                let value = pixel[0];
                Rgba8 {
                    red: value,
                    green: value,
                    blue: value,
                    alpha: 255,
                }
            })
            .collect(),
        jpeg_decoder::PixelFormat::CMYK32 => decoded_bytes
            .chunks_exact(4)
            .map(|pixel| {
                let black = u16::from(pixel[3]);
                let convert = |channel: u8| {
                    u8::try_from((u16::from(255 - channel) * (255 - black) + 127) / 255)
                        .expect("a converted CMYK channel is at most 255")
                };
                Rgba8 {
                    red: convert(pixel[0]),
                    green: convert(pixel[1]),
                    blue: convert(pixel[2]),
                    alpha: 255,
                }
            })
            .collect(),
    };
    if pixels.len() != width * height {
        return Err(Map16BmpDecodeError::PixelData);
    }
    Ok(DecodedMap16Bitmap {
        width,
        height,
        pixels,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BmpChannelMasks {
    rgb: [u32; 3],
    alpha: Option<u32>,
}

fn bmp_channel_masks(
    bytes: &[u8],
    dib_size: usize,
    pixel_offset: usize,
    bits: u16,
    compression: u32,
) -> Result<Option<BmpChannelMasks>, Map16BmpDecodeError> {
    if !matches!(compression, 3 | 6) {
        return Ok((bits == 16).then_some(BmpChannelMasks {
            rgb: [0x7c00, 0x03e0, 0x001f],
            alpha: None,
        }));
    }
    if dib_size != 40 && dib_size < 52 {
        return Err(Map16BmpDecodeError::BitMasks);
    }
    let rgb = [
        bmp_u32(bytes, 54).map_err(|_| Map16BmpDecodeError::BitMasks)?,
        bmp_u32(bytes, 58).map_err(|_| Map16BmpDecodeError::BitMasks)?,
        bmp_u32(bytes, 62).map_err(|_| Map16BmpDecodeError::BitMasks)?,
    ];
    let alpha_raw = if dib_size == 40 {
        (compression == 6)
            .then(|| bmp_u32(bytes, 66).map_err(|_| Map16BmpDecodeError::BitMasks))
            .transpose()?
    } else if dib_size >= 56 {
        Some(bmp_u32(bytes, 66).map_err(|_| Map16BmpDecodeError::BitMasks)?)
    } else {
        None
    };
    let alpha = alpha_raw.filter(|mask| *mask != 0);
    if compression == 6 && alpha.is_none() {
        return Err(Map16BmpDecodeError::BitMasks);
    }
    let mask_end = if dib_size == 40 {
        if compression == 6 { 70 } else { 66 }
    } else {
        14 + dib_size
    };
    let pixel_bits = if bits == 32 {
        u32::MAX
    } else {
        (1_u32 << bits) - 1
    };
    if pixel_offset < mask_end
        || rgb
            .iter()
            .any(|mask| *mask == 0 || *mask & !pixel_bits != 0 || !bmp_mask_is_contiguous(*mask))
        || rgb[0] & rgb[1] != 0
        || rgb[0] & rgb[2] != 0
        || rgb[1] & rgb[2] != 0
        || alpha.is_some_and(|mask| {
            mask & !pixel_bits != 0
                || !bmp_mask_is_contiguous(mask)
                || rgb.iter().any(|rgb_mask| mask & rgb_mask != 0)
        })
    {
        return Err(Map16BmpDecodeError::BitMasks);
    }
    Ok(Some(BmpChannelMasks { rgb, alpha }))
}

fn decode_bmp_rle(
    stream: &[u8],
    width: usize,
    height: usize,
    bits: u16,
    palette_entries: usize,
) -> Result<Vec<u8>, Map16BmpDecodeError> {
    let mut indices = vec![0; width * height];
    let mut cursor = 0_usize;
    let mut x = 0_usize;
    let mut y = 0_usize;
    let mut ended = false;
    while cursor < stream.len() {
        let pair = stream
            .get(cursor..cursor + 2)
            .ok_or(Map16BmpDecodeError::Rle)?;
        cursor += 2;
        let count = usize::from(pair[0]);
        let value = pair[1];
        if count != 0 {
            bmp_rle_extent(x, y, count, width, height)?;
            for offset in 0..count {
                let index = if bits == 8 || offset % 2 == 0 {
                    if bits == 8 { value } else { value >> 4 }
                } else {
                    value & 0x0f
                };
                bmp_write_rle_index(
                    &mut indices,
                    width,
                    height,
                    x + offset,
                    y,
                    index,
                    palette_entries,
                )?;
            }
            x += count;
            continue;
        }
        match value {
            0 => {
                x = 0;
                y = y.checked_add(1).ok_or(Map16BmpDecodeError::Rle)?;
                if y > height {
                    return Err(Map16BmpDecodeError::Rle);
                }
            }
            1 => {
                ended = true;
                break;
            }
            2 => {
                let delta = stream
                    .get(cursor..cursor + 2)
                    .ok_or(Map16BmpDecodeError::Rle)?;
                cursor += 2;
                x = x
                    .checked_add(usize::from(delta[0]))
                    .ok_or(Map16BmpDecodeError::Rle)?;
                y = y
                    .checked_add(usize::from(delta[1]))
                    .ok_or(Map16BmpDecodeError::Rle)?;
                if x > width || y >= height {
                    return Err(Map16BmpDecodeError::Rle);
                }
            }
            absolute => {
                let count = usize::from(absolute);
                bmp_rle_extent(x, y, count, width, height)?;
                let packed_len = if bits == 8 { count } else { count.div_ceil(2) };
                let stored_len = (packed_len + 1) & !1;
                let packed = stream
                    .get(cursor..cursor + stored_len)
                    .ok_or(Map16BmpDecodeError::Rle)?;
                for offset in 0..count {
                    let index = if bits == 8 {
                        packed[offset]
                    } else if offset % 2 == 0 {
                        packed[offset / 2] >> 4
                    } else {
                        packed[offset / 2] & 0x0f
                    };
                    bmp_write_rle_index(
                        &mut indices,
                        width,
                        height,
                        x + offset,
                        y,
                        index,
                        palette_entries,
                    )?;
                }
                cursor += stored_len;
                x += count;
            }
        }
    }
    if !ended {
        return Err(Map16BmpDecodeError::Rle);
    }
    Ok(indices)
}

fn decode_os2_bmp_rle24(
    stream: &[u8],
    width: usize,
    height: usize,
) -> Result<Vec<Rgba8>, Map16BmpDecodeError> {
    let pixel_count = width.checked_mul(height).ok_or(Map16BmpDecodeError::Rle)?;
    let mut pixels = vec![
        Rgba8 {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        pixel_count
    ];
    let mut cursor = 0_usize;
    let mut x = 0_usize;
    let mut y = 0_usize;
    let mut ended = false;
    while cursor < stream.len() {
        let count = usize::from(*stream.get(cursor).ok_or(Map16BmpDecodeError::Rle)?);
        cursor += 1;
        if count != 0 {
            let color = stream
                .get(cursor..cursor + 3)
                .ok_or(Map16BmpDecodeError::Rle)?;
            cursor += 3;
            bmp_rle_extent(x, y, count, width, height)?;
            let pixel = Rgba8 {
                red: color[2],
                green: color[1],
                blue: color[0],
                alpha: 255,
            };
            for column in x..x + count {
                bmp_write_rle24_pixel(&mut pixels, width, height, column, y, pixel)?;
            }
            x += count;
            continue;
        }
        let command = *stream.get(cursor).ok_or(Map16BmpDecodeError::Rle)?;
        cursor += 1;
        match command {
            0 => {
                x = 0;
                y = y.checked_add(1).ok_or(Map16BmpDecodeError::Rle)?;
                if y > height {
                    return Err(Map16BmpDecodeError::Rle);
                }
            }
            1 => {
                ended = true;
                break;
            }
            2 => {
                let delta = stream
                    .get(cursor..cursor + 2)
                    .ok_or(Map16BmpDecodeError::Rle)?;
                cursor += 2;
                x = x
                    .checked_add(usize::from(delta[0]))
                    .ok_or(Map16BmpDecodeError::Rle)?;
                y = y
                    .checked_add(usize::from(delta[1]))
                    .ok_or(Map16BmpDecodeError::Rle)?;
                if x > width || y >= height {
                    return Err(Map16BmpDecodeError::Rle);
                }
            }
            absolute => {
                let count = usize::from(absolute);
                bmp_rle_extent(x, y, count, width, height)?;
                let packed_len = count.checked_mul(3).ok_or(Map16BmpDecodeError::Rle)?;
                let stored_len = packed_len
                    .checked_add(1)
                    .map(|length| length & !1)
                    .ok_or(Map16BmpDecodeError::Rle)?;
                let packed = stream
                    .get(cursor..cursor + stored_len)
                    .ok_or(Map16BmpDecodeError::Rle)?;
                for offset in 0..count {
                    let at = offset * 3;
                    bmp_write_rle24_pixel(
                        &mut pixels,
                        width,
                        height,
                        x + offset,
                        y,
                        Rgba8 {
                            red: packed[at + 2],
                            green: packed[at + 1],
                            blue: packed[at],
                            alpha: 255,
                        },
                    )?;
                }
                cursor += stored_len;
                x += count;
            }
        }
    }
    if !ended {
        return Err(Map16BmpDecodeError::Rle);
    }
    Ok(pixels)
}

fn decode_os2_bmp_huffman1d(
    stream: &[u8],
    width: usize,
    height: usize,
    palette: [Rgba8; 2],
) -> Result<Vec<Rgba8>, Map16BmpDecodeError> {
    let width_u16 = u16::try_from(width).map_err(|_| Map16BmpDecodeError::Rle)?;
    let mut rows = Vec::with_capacity(height);
    let mut valid = true;
    let decoded = fax::decoder::decode_g3(stream.iter().copied(), |transitions| {
        if rows.len() >= height
            || transitions.iter().enumerate().any(|(index, position)| {
                *position > width_u16 || index > 0 && transitions[index - 1] >= *position
            })
        {
            valid = false;
            return;
        }
        rows.push(
            fax::decoder::pels(transitions, width_u16)
                .map(|color| match color {
                    fax::Color::White => palette[0],
                    fax::Color::Black => palette[1],
                })
                .collect::<Vec<_>>(),
        );
    });
    if decoded.is_none() || !valid || rows.len() != height {
        return Err(Map16BmpDecodeError::Rle);
    }
    let mut pixels = Vec::with_capacity(width.checked_mul(height).ok_or(Map16BmpDecodeError::Rle)?);
    for row in rows.into_iter().rev() {
        if row.len() != width {
            return Err(Map16BmpDecodeError::Rle);
        }
        pixels.extend(row);
    }
    Ok(pixels)
}

fn bmp_write_rle24_pixel(
    pixels: &mut [Rgba8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    pixel: Rgba8,
) -> Result<(), Map16BmpDecodeError> {
    if x >= width || y >= height {
        return Err(Map16BmpDecodeError::Rle);
    }
    let target_y = height - 1 - y;
    pixels[target_y * width + x] = pixel;
    Ok(())
}

fn bmp_rle_extent(
    x: usize,
    y: usize,
    count: usize,
    width: usize,
    height: usize,
) -> Result<(), Map16BmpDecodeError> {
    if y >= height || x.checked_add(count).is_none_or(|end| end > width) {
        return Err(Map16BmpDecodeError::Rle);
    }
    Ok(())
}

fn bmp_write_rle_index(
    indices: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    index: u8,
    palette_entries: usize,
) -> Result<(), Map16BmpDecodeError> {
    if usize::from(index) >= palette_entries {
        return Err(Map16BmpDecodeError::Palette);
    }
    let target_y = height - 1 - y;
    indices[target_y * width + x] = index;
    Ok(())
}

const fn bmp_mask_is_contiguous(mask: u32) -> bool {
    let normalized = mask >> mask.trailing_zeros();
    normalized & normalized.wrapping_add(1) == 0
}

fn bmp_masked_channel(pixel: u32, mask: u32) -> u8 {
    let shift = mask.trailing_zeros();
    let maximum = mask >> shift;
    let value = (pixel & mask) >> shift;
    u8::try_from((u64::from(value) * 255 + u64::from(maximum / 2)) / u64::from(maximum))
        .expect("a scaled BMP channel is at most 255")
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
    Dimensions {
        width: i32,
        height: i32,
    },
    Planes(u16),
    BitDepth(u16),
    Compression(u32),
    Palette,
    BitMasks,
    Rle,
    Embedded(String),
    EmbeddedDimensions {
        header_width: usize,
        header_height: usize,
        image_width: usize,
        image_height: usize,
    },
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

    fn test_core_indexed_bmp(
        width: usize,
        height: usize,
        bits: u16,
        palette: &[Rgba8],
        indices: &[u8],
    ) -> Vec<u8> {
        assert!(matches!(bits, 1 | 4 | 8));
        assert_eq!(palette.len(), 1_usize << bits);
        assert_eq!(indices.len(), width * height);
        let row_bytes = (width * usize::from(bits) + 7) / 8;
        let stride = (row_bytes + 3) & !3;
        let pixel_offset = 26 + palette.len() * 3;
        let mut bytes = vec![0; pixel_offset + stride * height];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&12_u32.to_le_bytes());
        bytes[18..20].copy_from_slice(&u16::try_from(width).unwrap().to_le_bytes());
        bytes[20..22].copy_from_slice(&u16::try_from(height).unwrap().to_le_bytes());
        bytes[22..24].copy_from_slice(&1_u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&bits.to_le_bytes());
        for (index, color) in palette.iter().enumerate() {
            let at = 26 + index * 3;
            bytes[at..at + 3].copy_from_slice(&[color.blue, color.green, color.red]);
        }
        for stored_row in 0..height {
            let source_row = height - 1 - stored_row;
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

    fn test_core_24_bmp(width: usize, height: usize, pixels: &[Rgba8]) -> Vec<u8> {
        assert_eq!(pixels.len(), width * height);
        let stride = (width * 3 + 3) & !3;
        let pixel_offset = 26_usize;
        let mut bytes = vec![0; pixel_offset + stride * height];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&12_u32.to_le_bytes());
        bytes[18..20].copy_from_slice(&u16::try_from(width).unwrap().to_le_bytes());
        bytes[20..22].copy_from_slice(&u16::try_from(height).unwrap().to_le_bytes());
        bytes[22..24].copy_from_slice(&1_u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&24_u16.to_le_bytes());
        for stored_row in 0..height {
            let source_row = height - 1 - stored_row;
            for column in 0..width {
                let pixel = pixels[source_row * width + column];
                let at = pixel_offset + stored_row * stride + column * 3;
                bytes[at..at + 3].copy_from_slice(&[pixel.blue, pixel.green, pixel.red]);
            }
        }
        bytes
    }

    fn test_os2_v2_bmp(
        width: usize,
        height: usize,
        bits: u16,
        compression: u32,
        palette: &[Rgba8],
        pixel_data: &[u8],
    ) -> Vec<u8> {
        let pixel_offset = 78 + palette.len() * 4;
        let mut bytes = vec![0; pixel_offset];
        bytes.extend_from_slice(pixel_data);
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&64_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&u32::try_from(width).unwrap().to_le_bytes());
        bytes[22..26].copy_from_slice(&u32::try_from(height).unwrap().to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&bits.to_le_bytes());
        bytes[30..34].copy_from_slice(&compression.to_le_bytes());
        bytes[34..38].copy_from_slice(&u32::try_from(pixel_data.len()).unwrap().to_le_bytes());
        bytes[46..50].copy_from_slice(&u32::try_from(palette.len()).unwrap().to_le_bytes());
        for (index, color) in palette.iter().enumerate() {
            let at = 78 + index * 4;
            bytes[at..at + 4].copy_from_slice(&[color.blue, color.green, color.red, 0]);
        }
        bytes
    }

    fn test_group3_bits(bits: &[u8]) -> Vec<u8> {
        bits.chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0_u8, |byte, (index, bit)| byte | (*bit << (7 - index)))
            })
            .collect()
    }

    fn test_group3_two_rows() -> Vec<u8> {
        const EOL: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let mut bits = Vec::new();
        bits.extend_from_slice(&EOL);
        bits.extend_from_slice(&[1, 0, 0, 1, 1]); // stored bottom: white run 8
        bits.extend_from_slice(&EOL);
        bits.extend_from_slice(&[1, 0, 1, 1]); // stored top: white run 4
        bits.extend_from_slice(&[0, 1, 1]); // then black run 4
        bits.extend_from_slice(&EOL);
        for _ in 0..5 {
            bits.extend_from_slice(&EOL);
        }
        test_group3_bits(&bits)
    }

    fn test_embedded_bmp(width: usize, height: usize, compression: u32, payload: &[u8]) -> Vec<u8> {
        let pixel_offset = 54_usize;
        let mut bytes = vec![0; pixel_offset];
        bytes.extend_from_slice(payload);
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&i32::try_from(width).unwrap().to_le_bytes());
        bytes[22..26].copy_from_slice(&i32::try_from(height).unwrap().to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&0_u16.to_le_bytes());
        bytes[30..34].copy_from_slice(&compression.to_le_bytes());
        bytes[34..38].copy_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
        bytes
    }

    #[test]
    fn bmp_decoder_handles_embedded_png_and_jpeg_payloads() {
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        }
        assert_eq!(
            decode_map16_bitmap_bmp_image(&test_embedded_bmp(2, 1, 5, &png)).unwrap(),
            DecodedMap16Bitmap {
                width: 2,
                height: 1,
                pixels: vec![
                    Rgba8 {
                        red: 1,
                        green: 2,
                        blue: 3,
                        alpha: 4,
                    },
                    Rgba8 {
                        red: 5,
                        green: 6,
                        blue: 7,
                        alpha: 8,
                    },
                ],
            }
        );

        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 100)
            .encode(&[12, 34, 56], 1, 1, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        let decoded = decode_map16_bitmap_bmp_image(&test_embedded_bmp(1, 1, 4, &jpeg)).unwrap();
        assert_eq!((decoded.width, decoded.height), (1, 1));
        assert_eq!(decoded.pixels[0].alpha, 255);
        assert!(decoded.pixels[0].red.abs_diff(12) <= 2);
        assert!(decoded.pixels[0].green.abs_diff(34) <= 2);
        assert!(decoded.pixels[0].blue.abs_diff(56) <= 2);
    }

    #[test]
    fn bmp_decoder_rejects_embedded_shape_and_payload_framing_errors() {
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[1, 2, 3]).unwrap();
        }
        assert!(matches!(
            decode_map16_bitmap_bmp_image(&test_embedded_bmp(2, 1, 5, &png)),
            Err(Map16BmpDecodeError::EmbeddedDimensions { .. })
        ));
        let mut truncated = test_embedded_bmp(1, 1, 5, &png);
        truncated.pop();
        assert_eq!(
            decode_map16_bitmap_bmp_image(&truncated),
            Err(Map16BmpDecodeError::PixelData)
        );
        let mut zero_size = test_embedded_bmp(1, 1, 5, &png);
        zero_size[34..38].copy_from_slice(&0_u32.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&zero_size),
            Err(Map16BmpDecodeError::PixelData)
        );
        let mut top_down = test_embedded_bmp(1, 1, 5, &png);
        top_down[22..26].copy_from_slice(&(-1_i32).to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&top_down),
            Err(Map16BmpDecodeError::Compression(5))
        );
    }

    fn test_indexed_bmp(
        width: usize,
        height: usize,
        bits: u16,
        top_down: bool,
        palette: &[Rgba8],
        indices: &[u8],
    ) -> Vec<u8> {
        assert!(matches!(bits, 1 | 2 | 4 | 8));
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
                    2 => bytes[target + column / 4] |= index << (6 - (column % 4) * 2),
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

    fn test_bitfield_bmp(
        bits: u16,
        compression: u32,
        masks: [u32; 3],
        packed_pixels: &[u32],
    ) -> Vec<u8> {
        assert!(matches!(bits, 16 | 32));
        let bytes_per_pixel = usize::from(bits / 8);
        let row_bytes = packed_pixels.len() * bytes_per_pixel;
        let stride = (row_bytes + 3) & !3;
        let pixel_offset = if compression == 3 { 66 } else { 54 };
        let mut bytes = vec![0; pixel_offset + stride];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&i32::try_from(packed_pixels.len()).unwrap().to_le_bytes());
        bytes[22..26].copy_from_slice(&1_i32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&bits.to_le_bytes());
        bytes[30..34].copy_from_slice(&compression.to_le_bytes());
        bytes[34..38].copy_from_slice(&u32::try_from(stride).unwrap().to_le_bytes());
        if compression == 3 {
            for (index, mask) in masks.iter().enumerate() {
                let at = 54 + index * 4;
                bytes[at..at + 4].copy_from_slice(&mask.to_le_bytes());
            }
        }
        for (column, pixel) in packed_pixels.iter().enumerate() {
            let at = pixel_offset + column * bytes_per_pixel;
            bytes[at..at + bytes_per_pixel]
                .copy_from_slice(&pixel.to_le_bytes()[..bytes_per_pixel]);
        }
        bytes
    }

    fn test_alpha_bitfield_bmp(
        dib_size: usize,
        bits: u16,
        compression: u32,
        masks: [u32; 4],
        packed_pixels: &[u32],
    ) -> Vec<u8> {
        assert!(matches!(dib_size, 40 | 56 | 108 | 124));
        assert!(matches!(bits, 16 | 32));
        let mask_bytes = if dib_size == 40 { 16 } else { 0 };
        let pixel_offset = 14 + dib_size + mask_bytes;
        let bytes_per_pixel = usize::from(bits / 8);
        let stride = (packed_pixels.len() * bytes_per_pixel + 3) & !3;
        let mut bytes = vec![0; pixel_offset + stride];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&u32::try_from(dib_size).unwrap().to_le_bytes());
        bytes[18..22].copy_from_slice(&i32::try_from(packed_pixels.len()).unwrap().to_le_bytes());
        bytes[22..26].copy_from_slice(&1_i32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&bits.to_le_bytes());
        bytes[30..34].copy_from_slice(&compression.to_le_bytes());
        bytes[34..38].copy_from_slice(&u32::try_from(stride).unwrap().to_le_bytes());
        for (index, mask) in masks.iter().enumerate() {
            let at = 54 + index * 4;
            bytes[at..at + 4].copy_from_slice(&mask.to_le_bytes());
        }
        for (column, pixel) in packed_pixels.iter().enumerate() {
            let at = pixel_offset + column * bytes_per_pixel;
            bytes[at..at + bytes_per_pixel]
                .copy_from_slice(&pixel.to_le_bytes()[..bytes_per_pixel]);
        }
        bytes
    }

    fn test_rle_bmp(
        width: usize,
        height: usize,
        bits: u16,
        palette: &[Rgba8],
        stream: &[u8],
    ) -> Vec<u8> {
        assert!(matches!(bits, 4 | 8));
        let pixel_offset = 54 + palette.len() * 4;
        let mut bytes = vec![0; pixel_offset + stream.len()];
        let file_len = u32::try_from(bytes.len()).unwrap();
        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&file_len.to_le_bytes());
        bytes[10..14].copy_from_slice(&u32::try_from(pixel_offset).unwrap().to_le_bytes());
        bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&i32::try_from(width).unwrap().to_le_bytes());
        bytes[22..26].copy_from_slice(&i32::try_from(height).unwrap().to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&bits.to_le_bytes());
        bytes[30..34].copy_from_slice(&(if bits == 8 { 1_u32 } else { 2 }).to_le_bytes());
        bytes[34..38].copy_from_slice(&u32::try_from(stream.len()).unwrap().to_le_bytes());
        bytes[46..50].copy_from_slice(&u32::try_from(palette.len()).unwrap().to_le_bytes());
        for (index, color) in palette.iter().enumerate() {
            let at = 54 + index * 4;
            bytes[at..at + 4].copy_from_slice(&[color.blue, color.green, color.red, 0]);
        }
        bytes[pixel_offset..].copy_from_slice(stream);
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
    fn bmp_decoder_handles_core_header_rgb_triples_and_bottom_up_rows() {
        for bits in [1, 4, 8] {
            let palette = (0..1_usize << bits)
                .map(|index| Rgba8 {
                    red: u8::try_from(index).unwrap(),
                    green: u8::try_from(index * 3 % 256).unwrap(),
                    blue: u8::try_from(index * 7 % 256).unwrap(),
                    alpha: 255,
                })
                .collect::<Vec<_>>();
            let indices = if bits == 1 {
                vec![0, 1, 0, 1, 0, 1]
            } else {
                vec![0, 1, 2, 3, 4, 5]
            };
            let decoded = decode_map16_bitmap_bmp_image(&test_core_indexed_bmp(
                3, 2, bits, &palette, &indices,
            ))
            .unwrap();
            assert_eq!((decoded.width, decoded.height), (3, 2));
            assert_eq!(
                decoded.pixels,
                indices
                    .into_iter()
                    .map(|index| palette[usize::from(index)])
                    .collect::<Vec<_>>()
            );
        }

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
        ];
        assert_eq!(
            decode_map16_bitmap_bmp_image(&test_core_24_bmp(2, 2, &pixels))
                .unwrap()
                .pixels,
            pixels
        );
    }

    #[test]
    fn bmp_decoder_handles_os2_v2_uncompressed_and_indexed_rle_images() {
        let palette = vec![
            Rgba8 {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            Rgba8 {
                red: 0x12,
                green: 0x34,
                blue: 0x56,
                alpha: 255,
            },
            Rgba8 {
                red: 0x78,
                green: 0x9a,
                blue: 0xbc,
                alpha: 255,
            },
            Rgba8 {
                red: 0xde,
                green: 0xf0,
                blue: 0x12,
                alpha: 255,
            },
        ];
        let indexed = test_os2_v2_bmp(2, 2, 8, 0, &palette, &[1, 0, 0, 0, 0, 1, 0, 0]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&indexed).unwrap().pixels,
            vec![palette[0], palette[1], palette[1], palette[0]]
        );
        let packed_2bpp = test_os2_v2_bmp(5, 2, 2, 0, &palette, &[0xe4, 0xc0, 0, 0, 0x1b, 0, 0, 0]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&packed_2bpp).unwrap().pixels,
            vec![
                palette[0], palette[1], palette[2], palette[3], palette[0], palette[3], palette[2],
                palette[1], palette[0], palette[3],
            ]
        );

        let direct = test_os2_v2_bmp(
            1,
            2,
            24,
            0,
            &[],
            &[0x33, 0x22, 0x11, 0, 0x66, 0x55, 0x44, 0],
        );
        assert_eq!(
            decode_map16_bitmap_bmp_image(&direct).unwrap().pixels,
            vec![
                Rgba8 {
                    red: 0x44,
                    green: 0x55,
                    blue: 0x66,
                    alpha: 255,
                },
                Rgba8 {
                    red: 0x11,
                    green: 0x22,
                    blue: 0x33,
                    alpha: 255,
                },
            ]
        );

        let rle8 = test_os2_v2_bmp(2, 1, 8, 1, &palette, &[2, 1, 0, 1]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&rle8).unwrap().pixels,
            vec![palette[1], palette[1]]
        );
        let rle4 = test_os2_v2_bmp(2, 1, 4, 2, &palette, &[2, 0x10, 0, 1]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&rle4).unwrap().pixels,
            vec![palette[1], palette[0]]
        );
    }

    #[test]
    fn bmp_decoder_handles_os2_v2_rle24_runs_absolute_delta_and_padding() {
        let stream = [
            2, 0, 0, 255, // two red pixels
            0, 3, 0, 255, 0, 255, 0, 0, 255, 255, 255, 0, // three literal pixels + pad
            0, 0, // end bottom row
            0, 2, 1, 0, // skip one pixel on the top row
            2, 0, 255, 255, // two yellow pixels
            0, 1, // end bitmap
        ];
        let decoded =
            decode_map16_bitmap_bmp_image(&test_os2_v2_bmp(5, 2, 24, 4, &[], &stream)).unwrap();
        let black = Rgba8 {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        assert_eq!(
            decoded.pixels,
            vec![
                black,
                Rgba8 {
                    red: 255,
                    green: 255,
                    blue: 0,
                    alpha: 255,
                },
                Rgba8 {
                    red: 255,
                    green: 255,
                    blue: 0,
                    alpha: 255,
                },
                black,
                black,
                Rgba8 {
                    red: 255,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                },
                Rgba8 {
                    red: 255,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                },
                Rgba8 {
                    red: 0,
                    green: 255,
                    blue: 0,
                    alpha: 255,
                },
                Rgba8 {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                },
                Rgba8 {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                },
            ]
        );
    }

    #[test]
    fn bmp_decoder_handles_os2_v2_huffman1d_rows_palette_and_orientation() {
        let palette = [
            Rgba8 {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 255,
            },
            Rgba8 {
                red: 0x12,
                green: 0x34,
                blue: 0x56,
                alpha: 255,
            },
        ];
        let decoded = decode_map16_bitmap_bmp_image(&test_os2_v2_bmp(
            8,
            2,
            1,
            3,
            &palette,
            &test_group3_two_rows(),
        ))
        .unwrap();
        assert_eq!(
            decoded.pixels,
            [palette[0]; 4]
                .into_iter()
                .chain([palette[1]; 4])
                .chain([palette[0]; 8])
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn bmp_decoder_rejects_malformed_and_incompatible_os2_v2_images() {
        let palette = vec![Rgba8 {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 255,
        }];
        let valid = test_os2_v2_bmp(1, 1, 8, 0, &palette, &[0, 0, 0, 0]);

        let mut short_header = valid.clone();
        short_header.truncate(77);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&short_header),
            Err(Map16BmpDecodeError::Header)
        );
        let mut zero_height = valid.clone();
        zero_height[22..26].copy_from_slice(&0_u32.to_le_bytes());
        assert!(matches!(
            decode_map16_bitmap_bmp_image(&zero_height),
            Err(Map16BmpDecodeError::Dimensions { .. })
        ));
        let mut bad_planes = valid.clone();
        bad_planes[26..28].copy_from_slice(&2_u16.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&bad_planes),
            Err(Map16BmpDecodeError::Planes(2))
        );
        let huffman_palette = [palette[0], palette[0]];
        let huffman = test_os2_v2_bmp(8, 2, 1, 3, &huffman_palette, &test_group3_two_rows());
        let mut wrong_rows = huffman.clone();
        wrong_rows[22..26].copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&wrong_rows),
            Err(Map16BmpDecodeError::Rle)
        );
        let mut overwide = huffman.clone();
        overwide[18..22].copy_from_slice(&4_u32.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&overwide),
            Err(Map16BmpDecodeError::Rle)
        );
        let mut truncated_huffman = huffman;
        truncated_huffman.truncate(truncated_huffman.len() - 2);
        truncated_huffman[34..38].copy_from_slice(
            &u32::try_from(test_group3_two_rows().len() - 2)
                .unwrap()
                .to_le_bytes(),
        );
        assert_eq!(
            decode_map16_bitmap_bmp_image(&truncated_huffman),
            Err(Map16BmpDecodeError::Rle)
        );
        let huffman_short_palette =
            test_os2_v2_bmp(8, 2, 1, 3, &huffman_palette[..1], &test_group3_two_rows());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&huffman_short_palette),
            Err(Map16BmpDecodeError::Palette)
        );
        let rle24 = test_os2_v2_bmp(1, 1, 24, 4, &[], &[1, 2, 3]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&rle24),
            Err(Map16BmpDecodeError::Rle)
        );
        let mut palette_overlap = valid.clone();
        palette_overlap[10..14].copy_from_slice(&78_u32.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&palette_overlap),
            Err(Map16BmpDecodeError::Palette)
        );
        let mut truncated_pixels = valid;
        truncated_pixels.pop();
        assert_eq!(
            decode_map16_bitmap_bmp_image(&truncated_pixels),
            Err(Map16BmpDecodeError::PixelData)
        );

        for malformed in [
            vec![0, 3, 1, 2, 3, 0, 1],
            vec![0, 3, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0],
            vec![2, 1, 2, 3, 0, 1],
            vec![0, 2, 2, 0, 0, 1],
            vec![1, 1, 2, 3],
        ] {
            assert_eq!(
                decode_map16_bitmap_bmp_image(&test_os2_v2_bmp(1, 1, 24, 4, &[], &malformed,)),
                Err(Map16BmpDecodeError::Rle)
            );
        }
    }

    #[test]
    fn bmp_decoder_rejects_malformed_core_headers_palettes_and_pixels() {
        let palette = vec![
            Rgba8 {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            };
            2
        ];
        let valid = test_core_indexed_bmp(1, 1, 1, &palette, &[0]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&valid[..25]),
            Err(Map16BmpDecodeError::Header)
        );
        for (offset, value, expected) in [
            (
                18,
                0_u16,
                Map16BmpDecodeError::Dimensions {
                    width: 0,
                    height: 1,
                },
            ),
            (22, 2, Map16BmpDecodeError::Planes(2)),
            (24, 2, Map16BmpDecodeError::BitDepth(2)),
            (24, 16, Map16BmpDecodeError::BitDepth(16)),
        ] {
            let mut malformed = valid.clone();
            malformed[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
            assert_eq!(decode_map16_bitmap_bmp_image(&malformed), Err(expected));
        }
        let mut overlapping_palette = valid.clone();
        overlapping_palette[10..14].copy_from_slice(&26_u32.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&overlapping_palette),
            Err(Map16BmpDecodeError::Palette)
        );
        let mut truncated_pixels = valid;
        truncated_pixels.pop();
        assert_eq!(
            decode_map16_bitmap_bmp_image(&truncated_pixels),
            Err(Map16BmpDecodeError::PixelData)
        );
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
            (2, 7, false, vec![0, 1, 2, 3, 3, 2, 1]),
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
        let out_of_range_2bpp = test_indexed_bmp(1, 1, 2, false, &palette, &[3]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&out_of_range_2bpp),
            Err(Map16BmpDecodeError::Palette)
        );
    }

    #[test]
    fn bmp_decoder_handles_default_rgb555_and_explicit_bitfields() {
        let rgb555 = test_bitfield_bmp(16, 0, [0; 3], &[0x7c00, 0x03e0, 0x001f]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&rgb555).unwrap().pixels,
            [
                Rgba8 {
                    red: 255,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                },
                Rgba8 {
                    red: 0,
                    green: 255,
                    blue: 0,
                    alpha: 255,
                },
                Rgba8 {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                },
            ]
        );

        let rgb565 = test_bitfield_bmp(16, 3, [0xf800, 0x07e0, 0x001f], &[0xf800, 0x07e0, 0x001f]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&rgb565).unwrap().pixels,
            decode_map16_bitmap_bmp_image(&rgb555).unwrap().pixels
        );

        let swapped_32 = test_bitfield_bmp(
            32,
            3,
            [0x0000_00ff, 0x0000_ff00, 0x00ff_0000],
            &[0x0033_2211],
        );
        assert_eq!(
            decode_map16_bitmap_bmp_image(&swapped_32).unwrap().pixels,
            [Rgba8 {
                red: 0x11,
                green: 0x22,
                blue: 0x33,
                alpha: 255,
            }]
        );
    }

    #[test]
    fn bmp_decoder_preserves_valid_v3_v4_v5_and_external_alpha_bitfields() {
        let masks = [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000];
        let packed = [0x0012_3456, 0x8078_9abc, 0xffde_f012];
        let expected = vec![
            Rgba8 {
                red: 0x12,
                green: 0x34,
                blue: 0x56,
                alpha: 0,
            },
            Rgba8 {
                red: 0x78,
                green: 0x9a,
                blue: 0xbc,
                alpha: 0x80,
            },
            Rgba8 {
                red: 0xde,
                green: 0xf0,
                blue: 0x12,
                alpha: 0xff,
            },
        ];
        for (dib_size, compression) in [(40, 6), (56, 3), (108, 3), (124, 3)] {
            assert_eq!(
                decode_map16_bitmap_bmp_image(&test_alpha_bitfield_bmp(
                    dib_size,
                    32,
                    compression,
                    masks,
                    &packed,
                ))
                .unwrap()
                .pixels,
                expected,
            );
        }

        let without_alpha =
            test_alpha_bitfield_bmp(56, 32, 3, [masks[0], masks[1], masks[2], 0], &packed);
        assert!(
            decode_map16_bitmap_bmp_image(&without_alpha)
                .unwrap()
                .pixels
                .iter()
                .all(|pixel| pixel.alpha == 255)
        );

        let rgba_1555 = test_alpha_bitfield_bmp(
            40,
            16,
            6,
            [0x7c00, 0x03e0, 0x001f, 0x8000],
            &[0x7c00, 0x83e0],
        );
        assert_eq!(
            decode_map16_bitmap_bmp_image(&rgba_1555).unwrap().pixels,
            [
                Rgba8 {
                    red: 255,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                },
                Rgba8 {
                    red: 0,
                    green: 255,
                    blue: 0,
                    alpha: 255,
                },
            ]
        );
    }

    #[test]
    fn bmp_decoder_rejects_missing_overlapping_and_noncontiguous_alpha_masks() {
        let rgb = [0x00ff_0000, 0x0000_ff00, 0x0000_00ff];
        for alpha in [0, rgb[0], 0xa000_0000] {
            let bytes = test_alpha_bitfield_bmp(40, 32, 6, [rgb[0], rgb[1], rgb[2], alpha], &[0]);
            assert_eq!(
                decode_map16_bitmap_bmp_image(&bytes),
                Err(Map16BmpDecodeError::BitMasks)
            );
        }
        let invalid_depth = test_bitfield_bmp(16, 0, [0; 3], &[0]);
        let mut invalid_depth = invalid_depth;
        invalid_depth[28..30].copy_from_slice(&24_u16.to_le_bytes());
        invalid_depth[30..34].copy_from_slice(&6_u32.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&invalid_depth),
            Err(Map16BmpDecodeError::Compression(6))
        );
    }

    #[test]
    fn bmp_decoder_rejects_overlapping_noncontiguous_and_out_of_range_masks() {
        for masks in [
            [0xf800, 0xf800, 0x001f],
            [0xa800, 0x07e0, 0x001f],
            [0x1_f000, 0x07e0, 0x001f],
        ] {
            let bytes = test_bitfield_bmp(16, 3, masks, &[0]);
            assert_eq!(
                decode_map16_bitmap_bmp_image(&bytes),
                Err(Map16BmpDecodeError::BitMasks)
            );
        }
        let mut masks_overlap_pixels = test_bitfield_bmp(16, 3, [0xf800, 0x07e0, 0x001f], &[0]);
        masks_overlap_pixels[10..14].copy_from_slice(&54_u32.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&masks_overlap_pixels),
            Err(Map16BmpDecodeError::BitMasks)
        );
    }

    #[test]
    fn bmp_decoder_handles_rle8_runs_absolute_delta_and_padding() {
        let palette = [
            Rgba8 {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            Rgba8 {
                red: 10,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            Rgba8 {
                red: 0,
                green: 20,
                blue: 0,
                alpha: 255,
            },
            Rgba8 {
                red: 0,
                green: 0,
                blue: 30,
                alpha: 255,
            },
        ];
        let stream = [
            2, 1, 0, 4, 2, 3, 2, 3, 0, 0, // bottom: encoded + absolute
            0, 2, 1, 0, 4, 2, 0, 0, // middle: delta + encoded
            0, 5, 3, 2, 1, 2, 3, 0, 1, 1, // top: padded absolute + encoded
            0, 1,
        ];
        let decoded =
            decode_map16_bitmap_bmp_image(&test_rle_bmp(6, 3, 8, &palette, &stream)).unwrap();
        let expected_indices: [u8; 18] = [
            3, 2, 1, 2, 3, 1, // top
            0, 2, 2, 2, 2, 0, // middle
            1, 1, 2, 3, 2, 3, // bottom
        ];
        assert_eq!(
            decoded.pixels,
            expected_indices.map(|index| palette[usize::from(index)])
        );
    }

    #[test]
    fn bmp_decoder_handles_rle4_alternating_and_absolute_nibbles() {
        let palette = [
            Rgba8 {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
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
        ];
        let stream = [
            4, 0x12, 0, 3, 0x30, 0x30, 0, 0, // bottom
            0, 7, 0x01, 0x23, 0x21, 0x00, 0, 1, // top
        ];
        let decoded =
            decode_map16_bitmap_bmp_image(&test_rle_bmp(7, 2, 4, &palette, &stream)).unwrap();
        let expected_indices: [u8; 14] = [0, 1, 2, 3, 2, 1, 0, 1, 2, 1, 2, 3, 0, 3];
        assert_eq!(
            decoded.pixels,
            expected_indices.map(|index| palette[usize::from(index)])
        );
    }

    #[test]
    fn bmp_decoder_rejects_malformed_rle_commands_and_orientation() {
        let palette = [
            Rgba8 {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            Rgba8 {
                red: 1,
                green: 1,
                blue: 1,
                alpha: 255,
            },
        ];
        for stream in [
            &[1, 1][..],
            &[3, 1, 0, 1][..],
            &[0, 2, 3, 0, 0, 1][..],
            &[0, 3, 1, 0, 0, 1][..],
        ] {
            assert_eq!(
                decode_map16_bitmap_bmp_image(&test_rle_bmp(2, 1, 8, &palette, stream)),
                Err(Map16BmpDecodeError::Rle)
            );
        }

        let invalid_palette_index = test_rle_bmp(1, 1, 8, &palette, &[1, 2, 0, 1]);
        assert_eq!(
            decode_map16_bitmap_bmp_image(&invalid_palette_index),
            Err(Map16BmpDecodeError::Palette)
        );

        let mut top_down = test_rle_bmp(1, 1, 8, &palette, &[1, 1, 0, 1]);
        top_down[22..26].copy_from_slice(&(-1_i32).to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&top_down),
            Err(Map16BmpDecodeError::Rle)
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
        unsupported_depth[28..30].copy_from_slice(&12_u16.to_le_bytes());
        assert_eq!(
            decode_map16_bitmap_bmp_image(&unsupported_depth),
            Err(Map16BmpDecodeError::BitDepth(12))
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
