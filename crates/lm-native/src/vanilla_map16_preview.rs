use eframe::egui;
use lm_graphics::IndexedTile;
use lm_project::Project;
use lm_rom::RomImage;

pub(crate) struct VanillaMap16Preview {
    pub(crate) image: egui::ColorImage,
    pub(crate) graphics_files: [usize; 4],
    pub(crate) common_tiles: usize,
    pub(crate) tileset_tiles: usize,
}

pub(crate) fn render(rom_bytes: Vec<u8>, tileset: u8) -> Result<VanillaMap16Preview, String> {
    let rom = RomImage::from_bytes(rom_bytes).map_err(|error| error.to_string())?;
    let project = Project::new(rom);
    let graphics_files =
        lm_profile::smw_us_v1_object_tileset_graphics_files(&project.rom, usize::from(tileset))
            .map_err(|error| error.to_string())?;
    let mut graphics = Vec::new();
    for file in graphics_files {
        graphics.extend(
            project
                .load_graphics_file(file, lm_profile::smw_us_v1_vanilla_graphics_layout())
                .map_err(|error| error.to_string())?
                .tiles,
        );
    }
    let map16 = lm_profile::load_smw_us_v1_level_map16_base(&project.rom, usize::from(tileset))
        .map_err(|error| error.to_string())?;
    let definitions = map16.editor_graphics_bytes();
    let width = 32 * 16;
    let height = 16 * 16;
    let mut rgba = vec![0; width * height * 4];
    for definition in 0..lm_profile::SMW_US_V1_MAP16_BASE_TILE_COUNT {
        let definition_x = definition % 32 * 16;
        let definition_y = definition / 32 * 16;
        for quadrant in 0..4 {
            let word_offset = definition * lm_profile::SMW_US_V1_MAP16_TILE_BYTES + quadrant * 2;
            let word = u16::from_le_bytes([definitions[word_offset], definitions[word_offset + 1]]);
            let tile_number = usize::from(word & 0x03ff);
            let quadrant_x = quadrant % 2 * 8;
            let quadrant_y = quadrant / 2 * 8;
            draw_subtile(
                &mut rgba,
                width,
                definition_x + quadrant_x,
                definition_y + quadrant_y,
                graphics.get(tile_number),
                word & 0x4000 != 0,
                word & 0x8000 != 0,
            );
        }
    }
    Ok(VanillaMap16Preview {
        image: egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba),
        graphics_files,
        common_tiles: map16.common_tiles,
        tileset_tiles: map16.tileset_tiles,
    })
}

fn draw_subtile(
    rgba: &mut [u8],
    canvas_width: usize,
    target_x: usize,
    target_y: usize,
    tile: Option<&IndexedTile>,
    x_flip: bool,
    y_flip: bool,
) {
    for y in 0..8 {
        for x in 0..8 {
            let source_x = if x_flip { 7 - x } else { x };
            let source_y = if y_flip { 7 - y } else { y };
            let color = tile
                .and_then(|tile| tile.pixel(source_x, source_y))
                .map_or([0xff, 0x20, 0x80, 0xff], grayscale);
            let output = ((target_y + y) * canvas_width + target_x + x) * 4;
            rgba[output..output + 4].copy_from_slice(&color);
        }
    }
}

fn grayscale(index: u8) -> [u8; 4] {
    if index == 0 {
        return [12, 12, 18, 255];
    }
    let intensity = 32_u8.saturating_add(index.saturating_mul(14));
    [intensity, intensity, intensity, 255]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn renders_real_pristine_tileset_when_reference_rom_is_available() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("Super Mario World (USA).sfc");
        let Ok(bytes) = fs::read(path) else {
            return;
        };
        let preview = render(bytes, 0).unwrap();
        assert_eq!(preview.image.size, [512, 256]);
        assert_eq!(preview.graphics_files, [0x14, 0x17, 0x19, 0x15]);
        assert_eq!(preview.common_tiles + preview.tileset_tiles, 512);
    }
}
