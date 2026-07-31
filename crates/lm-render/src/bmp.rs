use crate::Canvas;
use std::fmt;

const FILE_HEADER_LEN: usize = 14;
const INFO_HEADER_LEN: usize = 40;
const PIXEL_OFFSET: usize = FILE_HEADER_LEN + INFO_HEADER_LEN;

/// Encodes a bottom-up, uncompressed 24-bit Windows BMP.
///
/// Alpha is intentionally omitted because Lunar Magic's BMP export uses packed BGR pixels.
/// Each stored scanline is padded to a four-byte boundary.
///
/// # Errors
///
/// Returns [`BmpError`] for empty, oversized, inconsistent, or overflowing canvases.
pub fn encode_bmp(canvas: &Canvas) -> Result<Vec<u8>, BmpError> {
    if canvas.width() == 0 || canvas.height() == 0 {
        return Err(BmpError::EmptyCanvas);
    }
    let width = i32::try_from(canvas.width()).map_err(|_| BmpError::DimensionTooLarge)?;
    let height = i32::try_from(canvas.height()).map_err(|_| BmpError::DimensionTooLarge)?;
    let pixel_count = canvas
        .width()
        .checked_mul(canvas.height())
        .ok_or(BmpError::Overflow)?;
    if pixel_count > Canvas::MAX_PIXELS {
        return Err(BmpError::TooManyPixels(pixel_count));
    }
    if canvas.pixels().len() != pixel_count {
        return Err(BmpError::WrongPixelCount {
            expected: pixel_count,
            actual: canvas.pixels().len(),
        });
    }

    let unpadded_row = canvas.width().checked_mul(3).ok_or(BmpError::Overflow)?;
    let row_len = unpadded_row
        .checked_add(3)
        .map(|length| length & !3)
        .ok_or(BmpError::Overflow)?;
    let raster_len = row_len
        .checked_mul(canvas.height())
        .ok_or(BmpError::Overflow)?;
    let file_len = PIXEL_OFFSET
        .checked_add(raster_len)
        .ok_or(BmpError::Overflow)?;
    let file_len_u32 = u32::try_from(file_len).map_err(|_| BmpError::FileTooLarge(file_len))?;
    let raster_len_u32 = u32::try_from(raster_len).map_err(|_| BmpError::FileTooLarge(file_len))?;

    let mut output = Vec::with_capacity(file_len);
    output.extend_from_slice(b"BM");
    output.extend_from_slice(&file_len_u32.to_le_bytes());
    output.extend_from_slice(&[0; 4]);
    output.extend_from_slice(
        &u32::try_from(PIXEL_OFFSET)
            .expect("fixed BMP header length fits u32")
            .to_le_bytes(),
    );
    output.extend_from_slice(
        &u32::try_from(INFO_HEADER_LEN)
            .expect("fixed BMP info-header length fits u32")
            .to_le_bytes(),
    );
    output.extend_from_slice(&width.to_le_bytes());
    output.extend_from_slice(&height.to_le_bytes());
    output.extend_from_slice(&1_u16.to_le_bytes());
    output.extend_from_slice(&24_u16.to_le_bytes());
    output.extend_from_slice(&0_u32.to_le_bytes());
    output.extend_from_slice(&raster_len_u32.to_le_bytes());
    output.extend_from_slice(&0_i32.to_le_bytes());
    output.extend_from_slice(&0_i32.to_le_bytes());
    output.extend_from_slice(&0_u32.to_le_bytes());
    output.extend_from_slice(&0_u32.to_le_bytes());

    let padding = row_len - unpadded_row;
    for row in canvas.pixels().chunks_exact(canvas.width()).rev() {
        for pixel in row {
            output.extend_from_slice(&[pixel.blue, pixel.green, pixel.red]);
        }
        output.extend(std::iter::repeat_n(0, padding));
    }
    debug_assert_eq!(output.len(), file_len);
    Ok(output)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BmpError {
    EmptyCanvas,
    DimensionTooLarge,
    TooManyPixels(usize),
    WrongPixelCount { expected: usize, actual: usize },
    FileTooLarge(usize),
    Overflow,
}

impl fmt::Display for BmpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot encode render BMP: {self:?}")
    }
}

impl std::error::Error for BmpError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rgba;

    #[test]
    fn encodes_bottom_up_bgr_rows_with_four_byte_padding() {
        let canvas = Canvas::from_pixels(
            2,
            2,
            vec![
                Rgba {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 4,
                },
                Rgba {
                    red: 5,
                    green: 6,
                    blue: 7,
                    alpha: 8,
                },
                Rgba {
                    red: 9,
                    green: 10,
                    blue: 11,
                    alpha: 12,
                },
                Rgba {
                    red: 13,
                    green: 14,
                    blue: 15,
                    alpha: 16,
                },
            ],
        )
        .unwrap();
        let bytes = encode_bmp(&canvas).unwrap();
        assert_eq!(&bytes[..2], b"BM");
        assert_eq!(u32::from_le_bytes(bytes[2..6].try_into().unwrap()), 70);
        assert_eq!(u32::from_le_bytes(bytes[10..14].try_into().unwrap()), 54);
        assert_eq!(i32::from_le_bytes(bytes[18..22].try_into().unwrap()), 2);
        assert_eq!(i32::from_le_bytes(bytes[22..26].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(bytes[28..30].try_into().unwrap()), 24);
        assert_eq!(
            &bytes[54..],
            &[11, 10, 9, 15, 14, 13, 0, 0, 3, 2, 1, 7, 6, 5, 0, 0]
        );
    }

    #[test]
    fn rejects_empty_canvas() {
        assert_eq!(
            encode_bmp(&Canvas::try_new(0, 1).unwrap()),
            Err(BmpError::EmptyCanvas)
        );
    }
}
