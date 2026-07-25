use eframe::egui;
use lm_graphics::{IndexedTile, Palette};
use lm_level::LegacyLevelHeader;
use lm_project::Project;
use lm_rom::RomImage;

pub(crate) struct VanillaMap16Preview {
    pub(crate) image: egui::ColorImage,
    pub(crate) graphics_files: [usize; 4],
    pub(crate) sprite_image: egui::ColorImage,
    pub(crate) foreground_image: egui::ColorImage,
    pub(crate) sprite_graphics_files: [usize; 4],
    pub(crate) common_tiles: usize,
    pub(crate) tileset_tiles: usize,
}

pub(crate) fn render(
    rom_bytes: Vec<u8>,
    level: u16,
    header: LegacyLevelHeader,
) -> Result<VanillaMap16Preview, String> {
    let rom = RomImage::from_bytes(rom_bytes).map_err(|error| error.to_string())?;
    let project = Project::new(rom);
    let tileset = header.object_tileset();
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
    let palette = lm_profile::compose_smw_us_v1_level_palette(&project, level, header, 0)
        .map_err(|error| error.to_string())?
        .palette;
    let sprite_graphics_files = lm_profile::smw_us_v1_sprite_tileset_graphics_files(
        &project.rom,
        usize::from(header.sprite_tileset()),
    )
    .map_err(|error| error.to_string())?;
    let mut sprite_graphics = Vec::new();
    for file in sprite_graphics_files {
        sprite_graphics.push(
            project
                .load_graphics_file(file, lm_profile::smw_us_v1_vanilla_graphics_layout())
                .map_err(|error| error.to_string())?
                .tiles,
        );
    }
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
                (definition_x + quadrant_x, definition_y + quadrant_y),
                graphics.get(tile_number),
                &palette,
                usize::from((word >> 10) & 7),
                (word & 0x4000 != 0, word & 0x8000 != 0),
            );
        }
    }
    Ok(VanillaMap16Preview {
        image: egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba),
        foreground_image: render_foreground_graphics_atlas(&graphics, &palette),
        graphics_files,
        sprite_image: render_sprite_graphics_atlas(&sprite_graphics, &palette),
        sprite_graphics_files,
        common_tiles: map16.common_tiles,
        tileset_tiles: map16.tileset_tiles,
    })
}

fn render_foreground_graphics_atlas(
    graphics: &[IndexedTile],
    palette: &Palette,
) -> egui::ColorImage {
    const COLUMNS: usize = 32;
    const TILE_ROWS: usize = 16;
    const PALETTE_ROWS: usize = 8;
    const WIDTH: usize = COLUMNS * 8;
    const HEIGHT: usize = TILE_ROWS * PALETTE_ROWS * 8;
    let mut rgba = vec![0; WIDTH * HEIGHT * 4];
    for palette_row in 0..PALETTE_ROWS {
        for (tile_number, tile) in graphics.iter().enumerate().take(COLUMNS * TILE_ROWS) {
            let x = tile_number % COLUMNS * 8;
            let y = (palette_row * TILE_ROWS + tile_number / COLUMNS) * 8;
            draw_subtile(
                &mut rgba,
                WIDTH,
                (x, y),
                Some(tile),
                palette,
                palette_row,
                (false, false),
            );
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([WIDTH, HEIGHT], &rgba)
}

fn render_sprite_graphics_atlas(
    graphics: &[Vec<IndexedTile>],
    palette: &Palette,
) -> egui::ColorImage {
    const FILE_COLUMNS: usize = 16;
    const FILE_ROWS: usize = 8;
    const FILE_WIDTH: usize = FILE_COLUMNS * 8;
    const FILE_HEIGHT: usize = FILE_ROWS * 8;
    const WIDTH: usize = FILE_WIDTH * 2;
    const HEIGHT: usize = FILE_HEIGHT * 2;
    let mut rgba = vec![0; WIDTH * HEIGHT * 4];
    for (slot, tiles) in graphics.iter().enumerate().take(4) {
        let slot_x = slot % 2 * FILE_WIDTH;
        let slot_y = slot / 2 * FILE_HEIGHT;
        for (tile_number, tile) in tiles.iter().enumerate().take(FILE_COLUMNS * FILE_ROWS) {
            let x = slot_x + tile_number % FILE_COLUMNS * 8;
            let y = slot_y + tile_number / FILE_COLUMNS * 8;
            draw_subtile(
                &mut rgba,
                WIDTH,
                (x, y),
                Some(tile),
                palette,
                8,
                (false, false),
            );
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([WIDTH, HEIGHT], &rgba)
}

fn draw_subtile(
    rgba: &mut [u8],
    canvas_width: usize,
    target: (usize, usize),
    tile: Option<&IndexedTile>,
    palette: &Palette,
    palette_row: usize,
    flips: (bool, bool),
) {
    let (target_x, target_y) = target;
    let (x_flip, y_flip) = flips;
    for y in 0..8 {
        for x in 0..8 {
            let source_x = if x_flip { 7 - x } else { x };
            let source_y = if y_flip { 7 - y } else { y };
            let color = tile
                .and_then(|tile| tile.pixel(source_x, source_y))
                .and_then(|index| palette_color(palette, palette_row, index))
                .unwrap_or([0xff, 0x20, 0x80, 0xff]);
            let output = ((target_y + y) * canvas_width + target_x + x) * 4;
            rgba[output..output + 4].copy_from_slice(&color);
        }
    }
}

fn palette_color(palette: &Palette, palette_row: usize, index: u8) -> Option<[u8; 4]> {
    if index == 0 {
        return Some([12, 12, 18, 255]);
    }
    let color = palette
        .colors
        .get(palette_row * Palette::COLORS_PER_ROW + usize::from(index))?
        .to_rgb8();
    Some([color.red, color.green, color.blue, 255])
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
        let project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let level = project
            .load_level_slot(
                0,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let preview = render(bytes, 0, level.layer1.header).unwrap();
        assert_eq!(preview.image.size, [512, 256]);
        assert_eq!(preview.foreground_image.size, [256, 1024]);
        assert_eq!(preview.graphics_files, [0x14, 0x17, 0x1b, 0x08]);
        assert_eq!(preview.sprite_image.size, [256, 128]);
        assert_eq!(
            preview.sprite_graphics_files,
            lm_profile::smw_us_v1_sprite_tileset_graphics_files(
                &project.rom,
                usize::from(level.layer1.header.sprite_tileset()),
            )
            .unwrap()
        );
        assert_eq!(preview.common_tiles + preview.tileset_tiles, 512);
    }
}
