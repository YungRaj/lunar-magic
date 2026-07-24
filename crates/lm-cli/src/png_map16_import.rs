use crate::args::{PngMap16ImportCommand, RgbaMap16ImportCommand};
use crate::oracle_input::read_bounded;
use lm_graphics::Rgba8;
use std::io::{self, Cursor};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 256;
const MAX_DECODE_BYTES: usize = 4 * 1024 * 1024;
const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

pub fn execute(command: &PngMap16ImportCommand) -> Result<(), Box<dyn std::error::Error>> {
    let pixels = decode(&read_bounded(&command.png, MAX_INPUT_BYTES)?)?;
    crate::rgba_map16_import::execute_pixels(&as_rgba_command(command), &pixels)
}

fn as_rgba_command(command: &PngMap16ImportCommand) -> RgbaMap16ImportCommand {
    RgbaMap16ImportCommand {
        rgba: command.png.clone(),
        palette: command.palette.clone(),
        palette_access: command.palette_access.clone(),
        graphics: command.graphics.clone(),
        occupancy: command.occupancy.clone(),
        palette_row: command.palette_row,
        acts_like: command.acts_like,
        source_page: command.source_page,
        palette_output: command.palette_output.clone(),
        graphics_output: command.graphics_output.clone(),
        occupancy_output: command.occupancy_output.clone(),
        page_output: command.page_output.clone(),
    }
}

fn decode(bytes: &[u8]) -> Result<Vec<Rgba8>, Box<dyn std::error::Error>> {
    let mut decoder = ::png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(::png::Transformations::EXPAND | ::png::Transformations::STRIP_16);
    decoder.set_limits(::png::Limits {
        bytes: MAX_DECODE_BYTES,
    });
    let mut reader = decoder.read_info()?;
    if reader.info().width != WIDTH || reader.info().height != HEIGHT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "PNG Map16 page must be {WIDTH}x{HEIGHT}, got {}x{}",
                reader.info().width,
                reader.info().height
            ),
        )
        .into());
    }
    let mut output = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut output)?;
    let bytes = &output[..info.buffer_size()];
    let pixels = match info.color_type {
        ::png::ColorType::Rgba => bytes
            .chunks_exact(4)
            .map(|pixel| Rgba8 {
                red: pixel[0],
                green: pixel[1],
                blue: pixel[2],
                alpha: pixel[3],
            })
            .collect(),
        ::png::ColorType::Rgb => bytes
            .chunks_exact(3)
            .map(|pixel| Rgba8 {
                red: pixel[0],
                green: pixel[1],
                blue: pixel[2],
                alpha: 255,
            })
            .collect(),
        ::png::ColorType::Grayscale => bytes
            .iter()
            .map(|value| Rgba8 {
                red: *value,
                green: *value,
                blue: *value,
                alpha: 255,
            })
            .collect(),
        ::png::ColorType::GrayscaleAlpha => bytes
            .chunks_exact(2)
            .map(|pixel| Rgba8 {
                red: pixel[0],
                green: pixel[0],
                blue: pixel[0],
                alpha: pixel[1],
            })
            .collect(),
        ::png::ColorType::Indexed => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PNG indexed data was not expanded",
            )
            .into());
        }
    };
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(width: u32, height: u32, color: ::png::ColorType, bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        {
            let mut encoder = ::png::Encoder::new(&mut output, width, height);
            encoder.set_color(color);
            encoder.set_depth(::png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(bytes).unwrap();
        }
        output
    }

    #[test]
    fn decodes_rgb_rgba_and_grayscale_into_one_pixel_model() {
        let rgb = encode(
            WIDTH,
            HEIGHT,
            ::png::ColorType::Rgb,
            &[7, 8, 9].repeat(65_536),
        );
        assert_eq!(
            decode(&rgb).unwrap()[0],
            Rgba8 {
                red: 7,
                green: 8,
                blue: 9,
                alpha: 255
            }
        );
        let rgba = encode(
            WIDTH,
            HEIGHT,
            ::png::ColorType::Rgba,
            &[1, 2, 3, 0].repeat(65_536),
        );
        assert_eq!(decode(&rgba).unwrap()[0].alpha, 0);
        let gray_pixels = vec![42; 65_536];
        let gray = encode(WIDTH, HEIGHT, ::png::ColorType::Grayscale, &gray_pixels);
        assert_eq!(decode(&gray).unwrap()[0].red, 42);
    }

    #[test]
    fn rejects_wrong_dimensions_and_corrupt_input() {
        let wrong = encode(8, 8, ::png::ColorType::Rgba, &[0; 8 * 8 * 4]);
        assert!(decode(&wrong).is_err());
        assert!(decode(b"not a png").is_err());
    }
}
