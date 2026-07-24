use crate::{Canvas, Rgba};
use std::fmt;

mod stored_zlib;

use stored_zlib::zlib_stored;
#[cfg(test)]
use stored_zlib::{adler32, stored_zlib_capacity};

/// Encodes a deterministic, non-interlaced, 8-bit RGBA PNG using stored DEFLATE blocks.
///
/// The encoder is intentionally dependency-free and compression-free: golden render artifacts are
/// byte-stable across platforms and Rust toolchains, while remaining ordinary standards-compliant
/// PNG files.
///
/// # Errors
///
/// Returns [`PngError`] for empty, oversized, inconsistent, or overflowing canvases.
pub fn encode_png(canvas: &Canvas) -> Result<Vec<u8>, PngError> {
    if canvas.width() == 0 || canvas.height() == 0 {
        return Err(PngError::EmptyCanvas);
    }
    let width = u32::try_from(canvas.width()).map_err(|_| PngError::DimensionTooLarge)?;
    let height = u32::try_from(canvas.height()).map_err(|_| PngError::DimensionTooLarge)?;
    let pixel_count = canvas
        .width()
        .checked_mul(canvas.height())
        .ok_or(PngError::Overflow)?;
    if pixel_count > Canvas::MAX_PIXELS {
        return Err(PngError::TooManyPixels(pixel_count));
    }
    if canvas.pixels().len() != pixel_count {
        return Err(PngError::WrongPixelCount {
            expected: pixel_count,
            actual: canvas.pixels().len(),
        });
    }

    let row_len = canvas
        .width()
        .checked_mul(4)
        .and_then(|bytes| bytes.checked_add(1))
        .ok_or(PngError::Overflow)?;
    let raster_capacity = row_len
        .checked_mul(canvas.height())
        .ok_or(PngError::Overflow)?;
    let mut raw = Vec::with_capacity(raster_capacity);
    for row in canvas.pixels().chunks_exact(canvas.width()) {
        raw.push(0); // PNG filter method None.
        for pixel in row {
            raw.extend_from_slice(&rgba_bytes(*pixel));
        }
    }

    let mut output = Vec::new();
    output.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    append_chunk(&mut output, *b"IHDR", &ihdr)?;
    append_chunk(&mut output, *b"IDAT", &zlib_stored(&raw)?)?;
    append_chunk(&mut output, *b"IEND", &[])?;
    Ok(output)
}

const fn rgba_bytes(pixel: Rgba) -> [u8; 4] {
    [pixel.red, pixel.green, pixel.blue, pixel.alpha]
}

fn append_chunk(output: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) -> Result<(), PngError> {
    let len = u32::try_from(data.len()).map_err(|_| PngError::ChunkTooLarge(data.len()))?;
    output.extend_from_slice(&len.to_be_bytes());
    output.extend_from_slice(&kind);
    output.extend_from_slice(data);
    let checksum_capacity = data.len().checked_add(4).ok_or(PngError::Overflow)?;
    let mut checksum_input = Vec::with_capacity(checksum_capacity);
    checksum_input.extend_from_slice(&kind);
    checksum_input.extend_from_slice(data);
    output.extend_from_slice(&crc32(&checksum_input).to_be_bytes());
    Ok(())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = crc >> 1 ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PngError {
    EmptyCanvas,
    DimensionTooLarge,
    TooManyPixels(usize),
    WrongPixelCount { expected: usize, actual: usize },
    ChunkTooLarge(usize),
    Overflow,
}

impl fmt::Display for PngError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot encode render PNG: {self:?}")
    }
}

impl std::error::Error for PngError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks(bytes: &[u8]) -> Vec<([u8; 4], &[u8])> {
        let mut cursor = 8;
        let mut result = Vec::new();
        while cursor < bytes.len() {
            let len = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = bytes[cursor + 4..cursor + 8].try_into().unwrap();
            let data = &bytes[cursor + 8..cursor + 8 + len];
            let expected_crc = u32::from_be_bytes(
                bytes[cursor + 8 + len..cursor + 12 + len]
                    .try_into()
                    .unwrap(),
            );
            let mut checksum = kind.to_vec();
            checksum.extend_from_slice(data);
            assert_eq!(crc32(&checksum), expected_crc);
            result.push((kind, data));
            cursor += 12 + len;
        }
        result
    }

    fn decode_stored_zlib(bytes: &[u8]) -> Vec<u8> {
        assert_eq!(&bytes[..2], &[0x78, 0x01]);
        let mut cursor = 2;
        let mut output = Vec::new();
        loop {
            let header = bytes[cursor];
            cursor += 1;
            assert_eq!(header & 0xfe, 0);
            let len = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
            let complement = u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]);
            assert_eq!(!len, complement);
            let len = usize::from(len);
            cursor += 4;
            output.extend_from_slice(&bytes[cursor..cursor + len]);
            cursor += len;
            if header & 1 != 0 {
                break;
            }
        }
        assert_eq!(
            adler32(&output),
            u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap())
        );
        assert_eq!(cursor + 4, bytes.len());
        output
    }

    #[test]
    fn rgba_raster_encodes_as_deterministic_valid_chunks() {
        let canvas = Canvas::from_pixels(
            2,
            1,
            vec![
                Rgba {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 4,
                },
                Rgba {
                    red: 250,
                    green: 251,
                    blue: 252,
                    alpha: 253,
                },
            ],
        )
        .unwrap();
        let first = encode_png(&canvas).unwrap();
        assert_eq!(first, encode_png(&canvas).unwrap());
        assert_eq!(&first[..8], b"\x89PNG\r\n\x1a\n");
        let chunks = chunks(&first);
        assert_eq!(
            chunks.iter().map(|(kind, _)| kind).collect::<Vec<_>>(),
            [b"IHDR", b"IDAT", b"IEND"]
        );
        assert_eq!(&chunks[0].1[..8], &[0, 0, 0, 2, 0, 0, 0, 1]);
        assert_eq!(
            decode_stored_zlib(chunks[1].1),
            [0, 1, 2, 3, 4, 250, 251, 252, 253]
        );
    }

    #[test]
    fn multi_block_output_preserves_scanlines_and_empty_canvas_is_rejected() {
        let canvas = Canvas::from_pixels(20_000, 1, vec![Rgba::default(); 20_000]).unwrap();
        let png = encode_png(&canvas).unwrap();
        let chunks = chunks(&png);
        assert_eq!(decode_stored_zlib(chunks[1].1).len(), 80_001);
        assert_eq!(
            encode_png(&Canvas::try_new(0, 1).unwrap()),
            Err(PngError::EmptyCanvas)
        );
    }

    #[test]
    fn stored_block_capacity_and_final_bits_are_exact_at_boundaries() {
        for length in [0, 1, 65_535, 65_536, 131_070, 131_071] {
            let data = vec![0x5a; length];
            let encoded = zlib_stored(&data).unwrap();
            assert_eq!(encoded.len(), stored_zlib_capacity(length).unwrap());
            assert_eq!(decode_stored_zlib(&encoded), data);
        }
        assert_eq!(stored_zlib_capacity(usize::MAX), Err(PngError::Overflow));
    }
}
