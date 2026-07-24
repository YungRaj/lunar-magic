use crate::{GraphicsFileError, IndexedTile};
use std::fmt;

pub const SNES_4BPP_TILE_LEN: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanarGraphicsError {
    InvalidBitsPerPixel(u8),
    InvalidTileLength {
        actual: usize,
        expected: usize,
    },
    PartialTile {
        actual: usize,
        bytes_per_tile: usize,
    },
    SizeOverflow {
        tiles: usize,
        bits_per_pixel: u8,
    },
    PixelOutOfRange {
        tile: usize,
        pixel: usize,
        value: u8,
        maximum: u8,
    },
}

impl fmt::Display for PlanarGraphicsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid SNES planar graphics: {self:?}")
    }
}

impl std::error::Error for PlanarGraphicsError {}

/// Decodes one SNES planar 8×8 tile at any recovered Lunar Magic depth from 1 through 8 bpp.
///
/// Plane pairs use the native interleaved 16-byte SNES layout. At odd depths, the final unpaired
/// plane occupies eight contiguous row bytes, matching Lunar Magic's generic decoder.
///
/// # Errors
///
/// Returns [`PlanarGraphicsError`] for a depth outside 1–8 or a non-exact tile length.
pub fn decode_planar_tile(
    bytes: &[u8],
    bits_per_pixel: u8,
) -> Result<IndexedTile, PlanarGraphicsError> {
    let bytes_per_tile = bytes_per_tile(bits_per_pixel)?;
    if bytes.len() != bytes_per_tile {
        return Err(PlanarGraphicsError::InvalidTileLength {
            actual: bytes.len(),
            expected: bytes_per_tile,
        });
    }
    Ok(decode_valid_planar_tile(bytes, bits_per_pixel))
}

/// Decodes a complete sequence of equal-depth SNES planar tiles.
///
/// # Errors
///
/// Returns [`PlanarGraphicsError`] for an invalid depth or trailing partial tile.
pub fn decode_planar_tiles(
    bytes: &[u8],
    bits_per_pixel: u8,
) -> Result<Vec<IndexedTile>, PlanarGraphicsError> {
    let bytes_per_tile = bytes_per_tile(bits_per_pixel)?;
    if bytes.len() % bytes_per_tile != 0 {
        return Err(PlanarGraphicsError::PartialTile {
            actual: bytes.len(),
            bytes_per_tile,
        });
    }
    Ok(bytes
        .chunks_exact(bytes_per_tile)
        .map(|tile| decode_valid_planar_tile(tile, bits_per_pixel))
        .collect())
}

/// Encodes one indexed tile without truncating pixels that exceed its requested bit depth.
///
/// # Errors
///
/// Returns [`PlanarGraphicsError`] for an invalid depth or the first unrepresentable pixel.
pub fn encode_planar_tile(
    tile: &IndexedTile,
    bits_per_pixel: u8,
) -> Result<Vec<u8>, PlanarGraphicsError> {
    encode_planar_tiles(std::slice::from_ref(tile), bits_per_pixel)
}

/// Encodes a complete equal-depth tile sequence with checked aggregate sizing.
///
/// # Errors
///
/// Returns [`PlanarGraphicsError`] for invalid depth, size overflow, or an unrepresentable pixel.
pub fn encode_planar_tiles(
    tiles: &[IndexedTile],
    bits_per_pixel: u8,
) -> Result<Vec<u8>, PlanarGraphicsError> {
    let bytes_per_tile = bytes_per_tile(bits_per_pixel)?;
    let encoded_len =
        tiles
            .len()
            .checked_mul(bytes_per_tile)
            .ok_or(PlanarGraphicsError::SizeOverflow {
                tiles: tiles.len(),
                bits_per_pixel,
            })?;
    let maximum = u8::MAX >> (8 - bits_per_pixel);
    let mut output = Vec::with_capacity(encoded_len);
    for (tile_index, tile) in tiles.iter().enumerate() {
        let mut encoded = vec![0; bytes_per_tile];
        for (pixel, value) in tile.pixels().iter().copied().enumerate() {
            if value > maximum {
                return Err(PlanarGraphicsError::PixelOutOfRange {
                    tile: tile_index,
                    pixel,
                    value,
                    maximum,
                });
            }
            let row = pixel / 8;
            let mask = 1 << (7 - pixel % 8);
            for plane in 0..bits_per_pixel {
                if value & (1 << plane) != 0 {
                    encoded[plane_byte_index(plane, bits_per_pixel, row)] |= mask;
                }
            }
        }
        output.extend_from_slice(&encoded);
    }
    Ok(output)
}

fn bytes_per_tile(bits_per_pixel: u8) -> Result<usize, PlanarGraphicsError> {
    if !(1..=8).contains(&bits_per_pixel) {
        return Err(PlanarGraphicsError::InvalidBitsPerPixel(bits_per_pixel));
    }
    Ok(usize::from(bits_per_pixel) * 8)
}

fn decode_valid_planar_tile(bytes: &[u8], bits_per_pixel: u8) -> IndexedTile {
    let pixels = std::array::from_fn(|pixel| {
        let row = pixel / 8;
        let mask = 1 << (7 - pixel % 8);
        (0..bits_per_pixel).fold(0, |color, plane| {
            color
                | u8::from(bytes[plane_byte_index(plane, bits_per_pixel, row)] & mask != 0) << plane
        })
    });
    IndexedTile::new(pixels)
}

fn plane_byte_index(plane: u8, bits_per_pixel: u8, row: usize) -> usize {
    let pair_base = usize::from(plane / 2) * 16;
    if plane + 1 == bits_per_pixel && bits_per_pixel % 2 != 0 {
        pair_base + row
    } else {
        pair_base + row * 2 + usize::from(plane & 1)
    }
}

/// Decodes one SNES 4bpp planar tile.
///
/// # Errors
///
/// Returns the supplied length unless exactly 32 bytes are provided.
pub fn decode_4bpp_tile(bytes: &[u8]) -> Result<IndexedTile, usize> {
    if bytes.len() != SNES_4BPP_TILE_LEN {
        return Err(bytes.len());
    }
    decode_planar_tile(bytes, 4).map_err(|_| bytes.len())
}

/// Encodes one SNES 4bpp planar tile without discarding high pixel bits.
///
/// # Errors
///
/// Returns [`GraphicsFileError::PixelOutOfRange`] when a pixel is not representable in 4bpp.
pub fn encode_4bpp_tile(tile: &IndexedTile) -> Result<[u8; SNES_4BPP_TILE_LEN], GraphicsFileError> {
    let bytes = encode_planar_tile(tile, 4).map_err(|error| match error {
        PlanarGraphicsError::PixelOutOfRange {
            tile, pixel, value, ..
        } => GraphicsFileError::PixelOutOfRange { tile, pixel, value },
        _ => GraphicsFileError::SizeOverflow { tiles: 1 },
    })?;
    bytes
        .try_into()
        .map_err(|_| GraphicsFileError::SizeOverflow { tiles: 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_color_round_trips() {
        let pixels = std::array::from_fn(|index| index.to_le_bytes()[0] & 0x0f);
        let tile = IndexedTile::new(pixels);
        assert_eq!(
            decode_4bpp_tile(&encode_4bpp_tile(&tile).unwrap()).unwrap(),
            tile
        );
    }

    #[test]
    fn encoding_rejects_pixels_that_do_not_fit_four_bits() {
        let tile = IndexedTile::new([16; IndexedTile::PIXEL_COUNT]);
        assert_eq!(
            encode_4bpp_tile(&tile),
            Err(GraphicsFileError::PixelOutOfRange {
                tile: 0,
                pixel: 0,
                value: 16,
            })
        );
    }

    #[test]
    fn every_recovered_depth_round_trips_single_and_multiple_tiles() {
        for bits_per_pixel in 1..=8 {
            let mask = u8::MAX >> (8 - bits_per_pixel);
            let first =
                IndexedTile::new(std::array::from_fn(|pixel| pixel.to_le_bytes()[0] & mask));
            let second = IndexedTile::new(std::array::from_fn(|pixel| {
                pixel.to_le_bytes()[0].wrapping_mul(13) & mask
            }));
            let encoded =
                encode_planar_tiles(&[first.clone(), second.clone()], bits_per_pixel).unwrap();
            assert_eq!(encoded.len(), usize::from(bits_per_pixel) * 16);
            assert_eq!(
                decode_planar_tiles(&encoded, bits_per_pixel).unwrap(),
                [first.clone(), second]
            );
            assert_eq!(
                decode_planar_tile(
                    &encode_planar_tile(&first, bits_per_pixel).unwrap(),
                    bits_per_pixel
                )
                .unwrap(),
                first
            );
        }
    }

    #[test]
    fn three_bpp_uses_interleaved_pair_then_contiguous_odd_plane() {
        let tile = IndexedTile::new(std::array::from_fn(|pixel| {
            u8::try_from(pixel % 8).unwrap()
        }));
        let mut expected = Vec::new();
        for _ in 0..8 {
            expected.extend_from_slice(&[0x55, 0x33]);
        }
        expected.extend_from_slice(&[0x0f; 8]);
        assert_eq!(encode_planar_tile(&tile, 3).unwrap(), expected);
        assert_eq!(decode_planar_tile(&expected, 3).unwrap(), tile);
    }

    #[test]
    fn invalid_depth_lengths_and_pixels_are_rejected_without_truncation() {
        assert_eq!(
            decode_planar_tile(&[], 0),
            Err(PlanarGraphicsError::InvalidBitsPerPixel(0))
        );
        assert_eq!(
            decode_planar_tile(&[0; 72], 9),
            Err(PlanarGraphicsError::InvalidBitsPerPixel(9))
        );
        assert_eq!(
            decode_planar_tile(&[0; 23], 3),
            Err(PlanarGraphicsError::InvalidTileLength {
                actual: 23,
                expected: 24,
            })
        );
        assert_eq!(
            decode_planar_tiles(&[0; 25], 3),
            Err(PlanarGraphicsError::PartialTile {
                actual: 25,
                bytes_per_tile: 24,
            })
        );
        assert_eq!(
            encode_planar_tile(&IndexedTile::new([4; 64]), 2),
            Err(PlanarGraphicsError::PixelOutOfRange {
                tile: 0,
                pixel: 0,
                value: 4,
                maximum: 3,
            })
        );
    }
}
