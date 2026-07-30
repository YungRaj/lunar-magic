use crate::args::{PngMap16ImportCommand, RgbaMap16ImportCommand};
use crate::oracle_input::read_bounded;

pub fn execute(command: &PngMap16ImportCommand) -> Result<(), Box<dyn std::error::Error>> {
    let pixels = lm_app::decode_map16_bitmap_png(&read_bounded(
        &command.png,
        lm_app::MAP16_BITMAP_MAX_PNG_BYTES,
    )?)?;
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

#[cfg(test)]
mod tests {
    use lm_graphics::Rgba8;

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
        let rgb = encode(256, 256, ::png::ColorType::Rgb, &[7, 8, 9].repeat(65_536));
        assert_eq!(
            lm_app::decode_map16_bitmap_png(&rgb).unwrap()[0],
            Rgba8 {
                red: 7,
                green: 8,
                blue: 9,
                alpha: 255
            }
        );
        let rgba = encode(
            256,
            256,
            ::png::ColorType::Rgba,
            &[1, 2, 3, 0].repeat(65_536),
        );
        assert_eq!(lm_app::decode_map16_bitmap_png(&rgba).unwrap()[0].alpha, 0);
        let gray_pixels = vec![42; 65_536];
        let gray = encode(256, 256, ::png::ColorType::Grayscale, &gray_pixels);
        assert_eq!(lm_app::decode_map16_bitmap_png(&gray).unwrap()[0].red, 42);
    }

    #[test]
    fn rejects_wrong_dimensions_and_corrupt_input() {
        let wrong = encode(8, 8, ::png::ColorType::Rgba, &[0; 8 * 8 * 4]);
        assert!(lm_app::decode_map16_bitmap_png(&wrong).is_err());
        assert!(lm_app::decode_map16_bitmap_png(b"not a png").is_err());
    }
}
