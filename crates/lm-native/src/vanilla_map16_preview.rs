use eframe::egui;
use lm_graphics::{IndexedTile, Palette};
use lm_level::LegacyLevelHeader;
use lm_project::Project;
use lm_rom::RomImage;

pub(crate) struct VanillaMap16Preview {
    pub(crate) image: egui::ColorImage,
    pub(crate) graphics_files: [usize; 4],
    pub(crate) sprite_image: egui::ColorImage,
    pub(crate) sprite_tiles: Vec<IndexedTile>,
    pub(crate) palette: Palette,
    pub(crate) foreground_image: egui::ColorImage,
    pub(crate) foreground_tiles: Vec<IndexedTile>,
    pub(crate) layer3_tiles: Vec<IndexedTile>,
    pub(crate) sprite_graphics_files: [usize; 4],
    pub(crate) common_tiles: usize,
    pub(crate) tileset_tiles: usize,
}

const LAYER3_SLOT_BYTES: usize = 0x800;
const LAYER3_SLOT_TILES: usize = 0x80;
const LAYER1_SPRITE_SLOT_TILES: usize = 0x80;
const LAYER1_SPRITE_SLOT_STRIDE: usize = LAYER1_SPRITE_SLOT_TILES;
const LAYER1_SPRITE_GLOBAL_TILES: usize = 4 * LAYER1_SPRITE_SLOT_STRIDE;

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
    let graphics_slots = load_layer1_sprite_graphics_slots(&project, graphics_files)?;
    let graphics = materialize_layer1_sprite_vram(&graphics_slots);
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
    let sprite_graphics = load_layer1_sprite_graphics_slots(&project, sprite_graphics_files)?;
    // The pristine ROM stores ordinary SNES 16-bit tilemap words here. Lunar Magic expands
    // those words into a wider internal descriptor while loading them, but the native renderer
    // consumes `Subtile`'s SNES layout directly. Feeding the widened-and-truncated representation
    // back into this path corrupts palette and flip attributes.
    let definitions = map16.bytes;
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
    let sprite_image = render_sprite_graphics_atlas(&sprite_graphics, &palette);
    let foreground_image = render_foreground_graphics_atlas(&graphics, &palette);
    let layer3_tiles = load_layer3_tiles(&project, usize::from(level))?;
    Ok(VanillaMap16Preview {
        image: egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba),
        foreground_image,
        foreground_tiles: graphics,
        layer3_tiles,
        graphics_files,
        sprite_image,
        sprite_tiles: materialize_layer1_sprite_vram(&sprite_graphics),
        palette,
        sprite_graphics_files,
        common_tiles: map16.common_tiles,
        tileset_tiles: map16.tileset_tiles,
    })
}

pub(crate) fn render_rom_map16_page(
    rom_bytes: Vec<u8>,
    level: u16,
    header: LegacyLevelHeader,
    page: &lm_level::Map16Page,
) -> Result<egui::ColorImage, String> {
    const WIDTH: usize = 16 * 16;
    const HEIGHT: usize = 16 * 16;
    if page.tiles.len() != lm_level::Map16Page::TILE_COUNT {
        return Err(format!(
            "Map16 page contains {} tiles instead of {}",
            page.tiles.len(),
            lm_level::Map16Page::TILE_COUNT
        ));
    }
    let rom = RomImage::from_bytes(rom_bytes).map_err(|error| error.to_string())?;
    let project = Project::new(rom);
    let graphics_files = lm_profile::smw_us_v1_object_tileset_graphics_files(
        &project.rom,
        usize::from(header.object_tileset()),
    )
    .map_err(|error| error.to_string())?;
    let graphics_slots = load_layer1_sprite_graphics_slots(&project, graphics_files)?;
    let graphics = materialize_layer1_sprite_vram(&graphics_slots);
    let palette = lm_profile::compose_smw_us_v1_level_palette(&project, level, header, 0)
        .map_err(|error| error.to_string())?
        .palette;
    let mut rgba = vec![0; WIDTH * HEIGHT * 4];
    for (definition, tile) in page.tiles.iter().enumerate() {
        let definition_x = definition % 16 * 16;
        let definition_y = definition / 16 * 16;
        for (quadrant, word) in [
            tile.top_left.0,
            tile.top_right.0,
            tile.bottom_left.0,
            tile.bottom_right.0,
        ]
        .into_iter()
        .enumerate()
        {
            draw_subtile(
                &mut rgba,
                WIDTH,
                (
                    definition_x + quadrant % 2 * 8,
                    definition_y + quadrant / 2 * 8,
                ),
                graphics.get(usize::from(word & 0x03ff)),
                &palette,
                usize::from((word >> 10) & 7),
                (word & 0x4000 != 0, word & 0x8000 != 0),
            );
        }
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [WIDTH, HEIGHT],
        &rgba,
    ))
}

fn load_layer3_tiles(project: &Project, level: usize) -> Result<Vec<IndexedTile>, String> {
    let settings = lm_profile::load_smw_us_v1_expanded_level_settings(project, level)
        .map_err(|error| error.to_string())?
        .settings;
    let files = [
        usize::from(settings.word(15).map_err(|error| error.to_string())? & 0x0fff),
        usize::from(settings.word(14).map_err(|error| error.to_string())? & 0x0fff),
        usize::from(settings.word(13).map_err(|error| error.to_string())? & 0x0fff),
        usize::from(settings.word(12).map_err(|error| error.to_string())? & 0x0fff),
        0x7f,
        0x7f,
        0x7f,
        0x7f,
    ];
    let mut tiles = Vec::with_capacity(files.len() * LAYER3_SLOT_TILES);
    for file in files {
        if file == 0x7f {
            tiles.extend(
                std::iter::repeat_with(|| IndexedTile::new([0; IndexedTile::PIXEL_COUNT]))
                    .take(LAYER3_SLOT_TILES),
            );
            continue;
        }
        let mut decoded = project
            .load_decompressed_graphics_file(file, lm_profile::smw_us_v1_vanilla_graphics_layout())
            .map_err(|error| error.to_string())?;
        if decoded.len() > LAYER3_SLOT_BYTES {
            return Err(format!(
                "Layer 3 GFX{file:02X} expands to {} bytes, exceeding its {LAYER3_SLOT_BYTES}-byte slot",
                decoded.len()
            ));
        }
        decoded.resize(LAYER3_SLOT_BYTES, 0);
        tiles.extend(
            lm_graphics::decode_planar_tiles(&decoded, 2).map_err(|error| error.to_string())?,
        );
    }
    Ok(tiles)
}

fn load_layer1_sprite_graphics_slots(
    project: &Project,
    files: [usize; 4],
) -> Result<Vec<Vec<IndexedTile>>, String> {
    files
        .into_iter()
        .map(|file| {
            let decoded = project
                .load_decompressed_graphics_file(
                    file,
                    lm_profile::smw_us_v1_vanilla_graphics_layout(),
                )
                .map_err(|error| error.to_string())?;
            let mut tiles = lm_graphics::decode_planar_tiles(&decoded, 3)
                .map_err(|error| format!("cannot decode pristine 3bpp GFX{file:02X}: {error}"))?;
            if tiles.len() > LAYER1_SPRITE_SLOT_TILES {
                return Err(format!(
                    "GFX{file:02X} contains {} tiles, exceeding its {LAYER1_SPRITE_SLOT_TILES}-tile VRAM slot",
                    tiles.len()
                ));
            }
            tiles.resize_with(LAYER1_SPRITE_SLOT_TILES, || {
                IndexedTile::new([0; IndexedTile::PIXEL_COUNT])
            });
            Ok(tiles)
        })
        .collect()
}

fn materialize_layer1_sprite_vram(slots: &[Vec<IndexedTile>]) -> Vec<IndexedTile> {
    let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
    let mut tiles = vec![blank; LAYER1_SPRITE_GLOBAL_TILES];
    for (slot, source) in slots.iter().take(4).enumerate() {
        let start = slot * LAYER1_SPRITE_SLOT_STRIDE;
        let len = source.len().min(LAYER1_SPRITE_SLOT_TILES);
        tiles[start..start + len].clone_from_slice(&source[..len]);
    }
    tiles
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
    const BASE_HEIGHT: usize = FILE_HEIGHT * 2;
    const PALETTE_ROWS: usize = 8;
    const HEIGHT: usize = BASE_HEIGHT * PALETTE_ROWS;
    let mut rgba = vec![0; WIDTH * HEIGHT * 4];
    for palette_row in 0..PALETTE_ROWS {
        for (slot, tiles) in graphics.iter().enumerate().take(4) {
            let slot_x = slot % 2 * FILE_WIDTH;
            let slot_y = palette_row * BASE_HEIGHT + slot / 2 * FILE_HEIGHT;
            for (tile_number, tile) in tiles.iter().enumerate().take(FILE_COLUMNS * FILE_ROWS) {
                let x = slot_x + tile_number % FILE_COLUMNS * 8;
                let y = slot_y + tile_number / FILE_COLUMNS * 8;
                draw_subtile(
                    &mut rgba,
                    WIDTH,
                    (x, y),
                    Some(tile),
                    palette,
                    8 + palette_row,
                    (false, false),
                );
            }
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
    use lm_graphics::{Bgr555, Rgb8};
    use std::{fs, path::PathBuf};

    #[test]
    fn sprite_atlas_materializes_every_encoded_palette_row() {
        let graphics = vec![vec![IndexedTile::new([1; IndexedTile::PIXEL_COUNT])]];
        let mut colors = vec![Bgr555(0); 256];
        colors[8 * 16 + 1] = Bgr555::from_rgb8(Rgb8 {
            red: 255,
            green: 0,
            blue: 0,
        });
        colors[9 * 16 + 1] = Bgr555::from_rgb8(Rgb8 {
            red: 0,
            green: 255,
            blue: 0,
        });
        let image = render_sprite_graphics_atlas(&graphics, &Palette { colors });
        assert_eq!(image.size, [256, 1024]);
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(255, 0, 0));
        assert_eq!(
            image.pixels[128 * image.size[0]],
            egui::Color32::from_rgb(0, 255, 0)
        );
    }

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
        let map16 = lm_profile::load_smw_us_v1_level_map16_base(
            &project.rom,
            usize::from(level.layer1.header.object_tileset()),
        )
        .unwrap()
        .bytes;
        let unavailable_subtiles = map16
            .chunks_exact(2)
            .filter(|word| {
                usize::from(u16::from_le_bytes([word[0], word[1]]) & 0x03ff)
                    >= preview.foreground_tiles.len()
            })
            .count();
        assert_eq!(preview.foreground_tiles.len(), LAYER1_SPRITE_GLOBAL_TILES);
        assert_eq!(preview.sprite_tiles.len(), LAYER1_SPRITE_GLOBAL_TILES);
        assert_eq!(unavailable_subtiles, 0);
        assert_eq!(preview.image.size, [512, 256]);
        assert_eq!(preview.foreground_image.size, [256, 1024]);
        assert_eq!(preview.graphics_files, [0x14, 0x17, 0x1b, 0x08]);
        assert_eq!(preview.sprite_image.size, [256, 1024]);
        assert_eq!(preview.layer3_tiles.len(), 0x400);
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

    #[test]
    fn level_105_palette_matches_lunar_magic_mwl_export() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(bytes) = fs::read(root.join("Super Mario World (USA).sfc")) else {
            return;
        };
        let Ok(mwl_bytes) =
            fs::read(root.join("oracle-work/lm363/pristine-us/levels/Level 105.mwl"))
        else {
            return;
        };
        let project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let level = project
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let actual =
            lm_profile::compose_smw_us_v1_level_palette(&project, 0x105, level.layer1.header, 0)
                .unwrap();
        let mwl = lm_level::MwlFile::decode(&mwl_bytes).unwrap();
        let expected = mwl.palette_section().unwrap();
        let expected_colors = expected.tpl_order_colors();
        let differences = actual
            .palette
            .colors
            .iter()
            .zip(expected_colors)
            .enumerate()
            .filter_map(|(index, (actual, expected))| {
                (actual.0 != expected).then_some((index, actual.0, expected))
            })
            .collect::<Vec<_>>();
        assert!(
            differences.is_empty(),
            "backdrop actual={:04X} expected={:04X}; palette differences: {differences:02X?}",
            actual.backdrop.0,
            expected.backdrop
        );
    }
}
