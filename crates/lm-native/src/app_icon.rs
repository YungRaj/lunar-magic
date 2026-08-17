use eframe::egui;
use std::{io::Cursor, sync::Arc};

// Generated from standard sprite $6E using the graphics and palette in the pristine
// Super Mario World (USA) ROM. Keeping the generated pixels in the binary means the
// application icon is available before a user opens a ROM.
const ORIGINAL_MOON_PNG_BASE64: &str = concat!(
    "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",
);

pub(crate) fn original_moon() -> Arc<egui::IconData> {
    let png = decode_base64(ORIGINAL_MOON_PNG_BASE64).expect("embedded Moon icon is valid base64");
    let decoder = png::Decoder::new(Cursor::new(png));
    let mut reader = decoder
        .read_info()
        .expect("embedded Moon icon has a valid PNG header");
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut pixels)
        .expect("embedded Moon icon PNG decodes");
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    pixels.truncate(info.buffer_size());
    Arc::new(egui::IconData {
        rgba: pixels,
        width: info.width,
        height: info.height,
    })
}

fn decode_base64(source: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(source.len() / 4 * 3);
    for chunk in source.as_bytes().chunks_exact(4) {
        let values = [
            base64_value(chunk[0])?,
            base64_value(chunk[1])?,
            base64_value(chunk[2])?,
            base64_value(chunk[3])?,
        ];
        if values[0] == 64 || values[1] == 64 {
            return None;
        }
        let [a, b, c, d] = values;
        output.push((a << 2) | (b >> 4));
        if c != 64 {
            output.push((b << 4) | (c >> 2));
            if d != 64 {
                output.push((c << 6) | d);
            }
        }
    }
    Some(output)
}

const fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        b'=' => Some(64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_original_moon_is_a_transparent_32_pixel_icon() {
        let icon = super::original_moon();
        assert_eq!((icon.width, icon.height), (32, 32));
        assert_eq!(icon.rgba.len(), 32 * 32 * 4);
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 255));
    }
}
