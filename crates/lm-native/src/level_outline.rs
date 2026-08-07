use eframe::egui;

const ATLAS_BASE64: &str = include_str!("../assets/lunar-magic-level-outline-atlas.png.b64");
const GLYPH_COUNT: usize = 0x71;

pub(crate) fn atlas_image() -> Result<egui::ColorImage, String> {
    let bytes = decode_base64(ATLAS_BASE64)?;
    let decoder = png::Decoder::new(bytes.as_slice());
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let output = reader
        .next_frame(&mut buffer)
        .map_err(|error| error.to_string())?;
    if output.width != 16 * GLYPH_COUNT as u32 || output.height != 16 {
        return Err("Lunar Magic outline atlas has unexpected dimensions".into());
    }
    let mut pixels = Vec::with_capacity((output.width * output.height) as usize);
    for rgb in buffer[..output.buffer_size()].chunks_exact(3) {
        pixels.push(if rgb == [255, 0, 255] {
            egui::Color32::TRANSPARENT
        } else {
            egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
        });
    }
    Ok(egui::ColorImage {
        size: [output.width as usize, output.height as usize],
        pixels,
    })
}

pub(crate) fn glyph_for_tile(
    tile: u16,
    object_tileset: u8,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
    surface: bool,
    line_guide: bool,
) -> Option<u8> {
    let root = acts_like_root(tile & 0x7fff, custom_map16)?;
    let line = line_guide_glyph(root);
    if surface {
        surface_glyph(root, object_tileset).or(line_guide.then_some(line).flatten())
    } else if line_guide {
        line
    } else {
        None
    }
}

fn acts_like_root(
    mut tile: u16,
    custom_map16: Option<&lm_app::NativeMap16SidecarDocument>,
) -> Option<u16> {
    for _ in 0..0x8000 {
        if tile <= 0x1ff {
            return Some(tile);
        }
        let lm_app::NativeMap16SidecarDocument::M16(sidecar) = custom_map16? else {
            return None;
        };
        tile = sidecar.tile(usize::from(tile))?.acts_like & 0x7fff;
    }
    None
}

fn line_guide_glyph(root: u16) -> Option<u8> {
    match root {
        0x76..=0x93 => Some((root - 0x25) as u8),
        // The pristine editor's animation-trigger flag is clear, selecting $95/$62.
        0x95 => Some(0x62),
        0x96..=0x99 => Some(0x6f),
        _ => None,
    }
}

fn surface_glyph(root: u16, object_tileset: u8) -> Option<u8> {
    let mut table = [0_u8; 0x200];
    install_surface_table(&mut table);
    match object_tileset {
        0 | 7 => {
            table[0x1c7] = table[0x1aa];
            table[0x1c4] = table[0x1aa];
            table[0x1c6] = table[0x1af];
            table[0x1c5] = table[0x1af];
            table[0x1ee] = table[0x1e2];
            table[0x1ec] = table[0x1e2];
            table[0x1ef] = table[0x1e4];
            table[0x1ed] = table[0x1e4];
        }
        1 => {
            table[0x159..=0x15c].fill(0x4b);
            table[0x166..=0x169].fill(0x4b);
        }
        2 | 8 => {
            table[0x10c] = 0x4d;
            table[0x10d] = 0x4e;
        }
        3 | 0x0e => {
            table[0x159..=0x15b].fill(0x4c);
            for (tile, glyph) in [
                (0x1d2, 0x3e),
                (0x1d3, 0x3f),
                (0x1d4, 0x41),
                (0x1d5, 0x42),
                (0x1d6, 0x44),
                (0x1d7, 0x46),
            ] {
                table[tile] = glyph;
            }
        }
        5 | 0x0d => table[0x159..=0x15b].fill(0x4b),
        _ => {}
    }
    let glyph = table[usize::from(root)];
    (glyph != 0).then_some(glyph)
}

fn install_surface_table(table: &mut [u8; 0x200]) {
    const PAIRS: &[(usize, u8)] = &[
        (0x172, 6),
        (0x171, 6),
        (0x181, 9),
        (0x180, 9),
        (0x189, 11),
        (0x188, 11),
        (0x187, 11),
        (0x18e, 12),
        (0x18d, 12),
        (0x18c, 12),
        (0x186, 15),
        (0x185, 15),
        (0x18b, 16),
        (0x18a, 16),
        (0x190, 17),
        (0x18f, 17),
        (0x195, 18),
        (0x194, 18),
        (0x1d2, 19),
        (0x198, 19),
        (0x197, 19),
        (0x196, 19),
        (0x1df, 21),
        (0x1de, 21),
        (0x19a, 22),
        (0x199, 22),
        (0x170, 1),
        (0x16f, 1),
        (0x16e, 1),
        (0x175, 2),
        (0x174, 2),
        (0x173, 2),
        (0x17a, 3),
        (0x179, 3),
        (0x178, 3),
        (0x17f, 4),
        (0x17e, 4),
        (0x17d, 4),
        (0x1d9, 5),
        (0x1d8, 5),
        (0x177, 7),
        (0x176, 7),
        (0x17c, 8),
        (0x17b, 8),
        (0x184, 10),
        (0x183, 10),
        (0x182, 10),
        (0x193, 13),
        (0x192, 13),
        (0x191, 13),
        (0x1dd, 14),
        (0x1dc, 14),
        (0x1d3, 20),
        (0x19d, 20),
        (0x19c, 20),
        (0x19b, 20),
        (0x19f, 23),
        (0x19e, 23),
        (0x1d4, 24),
        (0x1a2, 24),
        (0x1a1, 24),
        (0x1a0, 24),
        (0x1d5, 25),
        (0x1a7, 25),
        (0x1a6, 25),
        (0x1a5, 25),
        (0x1e1, 26),
        (0x1e0, 26),
        (0x1a4, 27),
        (0x1a3, 27),
        (0x1a9, 28),
        (0x1a8, 28),
        (0x1f8, 30),
        (0x1f7, 30),
        (0x1e9, 30),
        (0x1e3, 30),
        (0x1e2, 30),
        (0x1ae, 31),
        (0x1ad, 31),
        (0x1d6, 29),
        (0x1ac, 29),
        (0x1ab, 29),
        (0x1aa, 29),
        (0x1d7, 32),
        (0x1b1, 32),
        (0x1b0, 32),
        (0x1af, 32),
        (0x1fa, 33),
        (0x1f9, 33),
        (0x1ea, 33),
        (0x1e5, 33),
        (0x1e4, 33),
        (0x1b3, 34),
        (0x1b2, 34),
        (0x1b4, 29),
        (0x1eb, 35),
        (0x1b5, 32),
        (0x1ca, 36),
        (0x1cb, 37),
        (0x1f1, 38),
        (0x1cc, 39),
        (0x1cd, 40),
        (0x1f2, 41),
        (0x1b6, 42),
        (0x1b7, 43),
        (0x1b9, 4),
        (0x1b8, 4),
        (0x1bb, 10),
        (0x1ba, 10),
        (0x1bd, 20),
        (0x1bc, 20),
        (0x1bf, 24),
        (0x1be, 24),
        (0x1c1, 29),
        (0x1c0, 29),
        (0x1c3, 32),
        (0x1c2, 32),
        (0x1c4, 44),
        (0x1ec, 45),
        (0x1c5, 46),
        (0x1ed, 47),
        (0x1c6, 48),
        (0x1c7, 49),
        (0x1ee, 50),
        (0x1c8, 51),
        (0x1c9, 52),
        (0x1ef, 53),
        (0x1ce, 54),
        (0x1f3, 55),
        (0x1cf, 56),
        (0x1f4, 57),
        (0x1d0, 58),
        (0x1f5, 59),
        (0x1d1, 60),
        (0x1f6, 61),
        (0x005, 0x48),
        (0x1ff, 0x48),
        (0x1fb, 0x40),
        (0x1fc, 0x43),
        (0x1fd, 0x45),
        (0x1fe, 0x47),
        (0x004, 0x4f),
        (0x12f, 0x4b),
    ];
    for &(tile, glyph) in PAIRS {
        table[tile] = glyph;
    }
    table[0x100..=0x110].fill(0x49);
    table[0x111..=0x12e].fill(0x4a);
    table[0x130..=0x16d].fill(0x4a);
    table[0x0ec..=0x0fb].fill(0x4a);
}

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut chunk = [0_u8; 4];
    let mut count = 0;
    for byte in value.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        chunk[count] = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 64,
            _ => return Err("invalid base64 in outline atlas".into()),
        };
        count += 1;
        if count == 4 {
            output.push((chunk[0] << 2) | (chunk[1] >> 4));
            if chunk[2] != 64 {
                output.push((chunk[1] << 4) | (chunk[2] >> 2));
            }
            if chunk[3] != 64 {
                output.push((chunk[2] << 6) | chunk[3]);
            }
            count = 0;
        }
    }
    (count == 0)
        .then_some(output)
        .ok_or_else(|| "truncated base64 outline atlas".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_outline_atlas_and_vanilla_tables_are_stable() {
        let image = atlas_image().unwrap();
        assert_eq!(image.size, [1808, 16]);
        assert_eq!(surface_glyph(0x170, 0), Some(1));
        assert_eq!(surface_glyph(0x100, 0), Some(0x49));
        assert_eq!(surface_glyph(0x1d2, 3), Some(0x3e));
        assert_eq!(line_guide_glyph(0x76), Some(0x51));
        assert_eq!(line_guide_glyph(0x95), Some(0x62));
        assert_eq!(line_guide_glyph(0x99), Some(0x6f));
    }
}
