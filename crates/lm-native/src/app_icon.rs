use eframe::egui;
use std::{io::Cursor, sync::Arc};

// Generated from extended object $18 (Map16 tile $06E) using the graphics and palette in the
// pristine Super Mario World (USA) ROM. Keeping the generated pixels in the binary means the
// application icon is available before a user opens a ROM.
const ORIGINAL_MOON_PNG_BASE64: &str = concat!(
    "iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAAC1ElEQVR4Ae3AA6AkWZbG8f937o3IzKdyS2Oubdu2bdu2bdu2bWmM",
    "npZKr54yMyLu+Xa3anqmhztr1U/8y8y/jnjRIf5l5l9HvOgQ/zID+GkP4UWhhz6d5yJeMMS/zAB+2kN4UeihT+e5iBcM8aIzgJ/2",
    "EP419NCn80zieSFedAbw0x7Cv4Ye+nSeSTwvxL+eAfy0h/CvoYc+nWcSz4b41zOAn/YQ/jX00KfzTOLZEP96BvDTHsK/hR76dJ5J",
    "AOJfzwB+2kP4t9BDn84zCUD82xnAT3sIAHro0wHw0x4CgB76dAD8tIfwQHro03kmAYh/OwP4aQ8BQA99OgB+2kMA0EOfDoCf9hAe",
    "SA99Os8kAPFvZwA/7SEA6KFPB8BPewgAeujTAfDTHsLzo4c+HQDxb2cAP+0hAOihTwfAT3sIAHro0wHw0x7C86OHPh0A8W9nAD/t",
    "IQDooU8HwE97CAB66NMB8NMewvOjhz4dAPFvZwA/7SEA6KFPB8BPewgAeujTAfDTHsLzo4c+HQDxr2cAP+0h/HvooU8HQPzrGcBP",
    "ewj/Hnro0wEQ/zLzAH7aQwDQQ58OgJ/2EF4YPfTpAPhpD+GB9NCnAyD+ZeYB/LSHAKCHPh0AP+0hvDB66NMB8NMewgPpoU8HQDyb",
    "eT78tIfw/OihTwfAT3sIz48e+nQA/LSH8EB66NN5JgGIZzPPh5/2EJ4fPfTpAPhpD+H50UOfDoCf9hAeSA99Os8kAPFsBvDTHsK/",
    "hh76dJ4fP+0hPD966NN5JgGIZzOAn/YQ/jX00Kfz/PhpD+H50UOfzjMJQDwvA/hpD+E/kh76dJ5JPBvieRnAT3sI/5H00KfzTOLZ",
    "EC+YAfy0h/DvoYc+nWcSzwvxghnAT3sI/x566NN5JvG8EP8y8wB+2kN4YfTQp/NcxAuG+JeZB/DTHsILo4c+neciXjDEv5554cSL",
    "DvGvZ1448aLjHwHs47YZNJJSsAAAAABJRU5ErkJggg==",
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
    let (pixels, width, height) = upscale_rgba_nearest(&pixels, info.width, info.height, 8);
    Arc::new(egui::IconData {
        rgba: pixels,
        width,
        height,
    })
}

fn upscale_rgba_nearest(source: &[u8], width: u32, height: u32, scale: u32) -> (Vec<u8>, u32, u32) {
    assert_eq!(source.len(), width as usize * height as usize * 4);
    let output_width = width * scale;
    let output_height = height * scale;
    let mut output = vec![0; output_width as usize * output_height as usize * 4];
    for y in 0..output_height {
        for x in 0..output_width {
            let source_pixel = ((y / scale) * width + x / scale) as usize * 4;
            let output_pixel = (y * output_width + x) as usize * 4;
            output[output_pixel..output_pixel + 4]
                .copy_from_slice(&source[source_pixel..source_pixel + 4]);
        }
    }
    (output, output_width, output_height)
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
    fn embedded_original_moon_is_a_transparent_256_pixel_icon() {
        let icon = super::original_moon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 255));
    }
}
