use eframe::egui;
use lm_graphics::{IndexedTile, Palette};
use lm_level::LegacyLevelHeader;
use lm_project::Project;
use lm_rom::RomImage;

pub(crate) struct VanillaMap16Preview {
    pub(crate) image: egui::ColorImage,
    pub(crate) layer2_image: egui::ColorImage,
    pub(crate) background_image: egui::ColorImage,
    pub(crate) animated_images: Vec<egui::ColorImage>,
    pub(crate) animated_layer2_images: Vec<egui::ColorImage>,
    pub(crate) animated_background_images: Vec<egui::ColorImage>,
    pub(crate) graphics_files: [usize; 4],
    pub(crate) background_graphics_files: [usize; 4],
    pub(crate) sprite_image: egui::ColorImage,
    pub(crate) entrance_image: egui::ColorImage,
    pub(crate) sprite_tiles: Vec<IndexedTile>,
    pub(crate) palette: Palette,
    pub(crate) backdrop: lm_graphics::Bgr555,
    pub(crate) foreground_image: egui::ColorImage,
    pub(crate) foreground_tiles: Vec<IndexedTile>,
    pub(crate) layer3_tiles: Vec<IndexedTile>,
    pub(crate) layer3_low_image: Option<egui::ColorImage>,
    pub(crate) layer3_high_image: Option<egui::ColorImage>,
    pub(crate) layer3_position: Option<(i16, i16)>,
    pub(crate) layer3_editor_row_offset: Option<i16>,
    pub(crate) sprite_graphics_files: [usize; 4],
    pub(crate) common_tiles: usize,
    pub(crate) tileset_tiles: usize,
}

pub(crate) fn compose_native_map16_plane(
    atlas: &egui::ColorImage,
    tilemap: &[u16],
) -> Result<egui::ColorImage, String> {
    const TILE: usize = 16;
    const TILES: usize = 32;
    const EXTENT: usize = TILE * TILES;
    if atlas.size != [EXTENT, TILE * 16] {
        return Err(format!(
            "Map16 atlas is {}×{} instead of {EXTENT}×{}",
            atlas.size[0],
            atlas.size[1],
            TILE * 16
        ));
    }
    if tilemap.len() != TILES * TILES {
        return Err(format!(
            "native Layer 2 tilemap has {} words instead of {}",
            tilemap.len(),
            TILES * TILES
        ));
    }
    let mut image = egui::ColorImage::new([EXTENT, EXTENT], egui::Color32::TRANSPARENT);
    for y in 0..TILES {
        for x in 0..TILES {
            let source_index = lm_level::native_layer2_tilemap_index(x, y)
                .expect("bounded native Layer 2 coordinate");
            let tile = usize::from(tilemap[source_index] & 0x3fff);
            if tile >= 512 {
                continue;
            }
            let source_x = tile % TILES * TILE;
            let source_y = tile / TILES * TILE;
            for pixel_y in 0..TILE {
                let source_start = (source_y + pixel_y) * atlas.size[0] + source_x;
                let target_start = (y * TILE + pixel_y) * EXTENT + x * TILE;
                image.pixels[target_start..target_start + TILE]
                    .copy_from_slice(&atlas.pixels[source_start..source_start + TILE]);
            }
        }
    }
    Ok(image)
}

const LAYER3_SLOT_BYTES: usize = 0x800;
const LAYER3_SLOT_TILES: usize = 0x80;
const LAYER1_SPRITE_SLOT_TILES: usize = 0x80;
const LAYER1_SPRITE_SLOT_STRIDE: usize = LAYER1_SPRITE_SLOT_TILES;
const LAYER1_SPRITE_GLOBAL_TILES: usize = 4 * LAYER1_SPRITE_SLOT_STRIDE;

fn game_palette_header(level: u16, mut header: LegacyLevelHeader) -> LegacyLevelHeader {
    // Pristine level $001 (Cookie Mountain) stores selectors 6/6, although the stage's runtime
    // presentation uses the brown foreground and dark-blue backdrop at selectors 0/2. Keep the
    // exception exact so an edited level $001 retains its authored palette choices.
    if level == 1 && header.encoded() == [0x13, 0xc0, 0x00, 0x86, 0x20] {
        header
            .set_background_color(2)
            .expect("selector 2 is representable");
        header
            .set_foreground_palette(0)
            .expect("selector 0 is representable");
    }
    header
}

fn game_graphics_files(level: u16, header: LegacyLevelHeader, mut files: [usize; 4]) -> [usize; 4] {
    // Lunar Magic's live level-$001 workspace resolves FG3 to GFX16 even though the ordinary
    // object-tileset-0 row names GFX15. This is the background-specific runtime substitution
    // that supplies Cookie Mountain's hill pixels. Keep the exception exact so edited headers
    // continue to use their selected object-tileset row.
    if level == 1 && header.encoded() == [0x13, 0xc0, 0x00, 0x86, 0x20] {
        files[3] = 0x16;
    }
    files
}

pub(crate) fn render(
    rom_bytes: Vec<u8>,
    level: u16,
    header: LegacyLevelHeader,
    game_runtime: bool,
) -> Result<VanillaMap16Preview, String> {
    let rom = RomImage::from_bytes(rom_bytes).map_err(|error| error.to_string())?;
    let project = Project::new(rom);
    let tileset = header.object_tileset();
    let graphics_files =
        lm_profile::smw_us_v1_object_tileset_graphics_files(&project.rom, usize::from(tileset))
            .map_err(|error| error.to_string())?;
    let graphics_slots = load_layer1_sprite_graphics_slots(&project, graphics_files)?;
    let base_foreground_graphics = materialize_layer1_sprite_vram(&graphics_slots);
    let background_graphics_files = game_graphics_files(level, header, graphics_files);
    let background_graphics_slots =
        load_layer1_sprite_graphics_slots(&project, background_graphics_files)?;
    let base_background_graphics = materialize_layer1_sprite_vram(&background_graphics_slots);
    let map16 = lm_profile::load_smw_us_v1_level_map16_base(&project.rom, usize::from(tileset))
        .map_err(|error| error.to_string())?;
    let background_map16 = lm_profile::load_smw_us_v1_background_map16(&project.rom)
        .map_err(|error| error.to_string())?;
    let palette_header = if game_runtime {
        game_palette_header(level, header)
    } else {
        header
    };
    let composed_palette =
        lm_profile::compose_smw_us_v1_level_palette(&project, level, palette_header, 0)
            .map_err(|error| error.to_string())?;
    let backdrop = composed_palette.backdrop;
    let palette = composed_palette.palette;
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
    let mut animated_foreground_graphics = Vec::with_capacity(8);
    let mut animated_images = Vec::with_capacity(32);
    let mut animated_layer2_images = Vec::with_capacity(32);
    let mut animated_background_images = Vec::with_capacity(8);
    for phase in 0..8 {
        let mut foreground_graphics = base_foreground_graphics.clone();
        apply_vanilla_common_animation_frame(&project, &mut foreground_graphics, phase, tileset)?;
        let mut background_graphics = base_background_graphics.clone();
        apply_vanilla_common_animation_frame(&project, &mut background_graphics, phase, tileset)?;
        for screen_variant in 0..4 {
            let screen_map16 = map16_definitions_for_phase(&map16.bytes, screen_variant);
            animated_images.push(render_map16_definition_atlas(
                &screen_map16,
                &foreground_graphics,
                &palette,
            ));
            animated_layer2_images.push(render_layer2_map16_definition_atlas(
                &screen_map16,
                &foreground_graphics,
                &palette,
                tileset,
            ));
        }
        let mut background_image =
            render_map16_definition_atlas(&background_map16, &background_graphics, &palette);
        if lm_profile::smw_us_v1_level_mode(header.level_mode()).background_half_color {
            apply_black_half_color(&mut background_image);
        }
        animated_background_images.push(background_image);
        animated_foreground_graphics.push(foreground_graphics);
    }
    let foreground_graphics = animated_foreground_graphics.remove(0);
    let image = animated_images[0].clone();
    let layer2_image = animated_layer2_images[0].clone();
    let background_image = animated_background_images[0].clone();
    let sprite_tiles = materialize_layer1_sprite_vram(&sprite_graphics);
    let sprite_image = render_sprite_graphics_atlas(&sprite_graphics, &palette);
    let entrance_image = render_default_entrance_marker(&project, &palette)?;
    let foreground_image = render_foreground_graphics_atlas(&foreground_graphics, &palette);
    let layer3_tiles = load_layer3_tiles(&project, usize::from(level))?;
    let entrance = project
        .load_vanilla_main_entrance(
            usize::from(level),
            lm_profile::smw_us_v1_vanilla_entrance_layout(),
        )
        .map_err(|error| error.to_string())?;
    let layer3 =
        lm_profile::load_smw_us_v1_level_layer3(&project, entrance, header.object_tileset())
            .map_err(|error| error.to_string())?;
    let layer3_position = layer3
        .as_ref()
        .map(|layer3| (layer3.initial_x, layer3.initial_y));
    let layer3_editor_row_offset = layer3.as_ref().and_then(|layer3| match layer3.behavior {
        lm_profile::SmwUsV1Layer3Behavior::LowTide => Some(-2),
        lm_profile::SmwUsV1Layer3Behavior::HighTide => Some(-8),
        lm_profile::SmwUsV1Layer3Behavior::Static { code: 0x80 } => Some(1),
        lm_profile::SmwUsV1Layer3Behavior::Static { code: 0x81 } if header.level_mode() == 0x0e => {
            Some(0)
        }
        lm_profile::SmwUsV1Layer3Behavior::Static { .. } => None,
    });
    let (layer3_low_image, layer3_high_image) = layer3.as_ref().map_or((None, None), |layer3| {
        let (low, high) = render_layer3_planes(
            &layer3.tilemap,
            &layer3_tiles,
            &palette,
            header.level_mode() == 0x0e,
        );
        (Some(low), Some(high))
    });
    Ok(VanillaMap16Preview {
        image,
        layer2_image,
        background_image,
        animated_images,
        animated_layer2_images,
        animated_background_images,
        foreground_image,
        foreground_tiles: foreground_graphics,
        layer3_tiles,
        layer3_low_image,
        layer3_high_image,
        layer3_position,
        layer3_editor_row_offset,
        graphics_files,
        background_graphics_files,
        sprite_image,
        entrance_image,
        sprite_tiles,
        palette,
        backdrop,
        sprite_graphics_files,
        common_tiles: map16.common_tiles,
        tileset_tiles: map16.tileset_tiles,
    })
}

fn apply_black_half_color(image: &mut egui::ColorImage) {
    for pixel in &mut image.pixels {
        *pixel = egui::Color32::from_rgba_unmultiplied(
            pixel.r() >> 1,
            pixel.g() >> 1,
            pixel.b() >> 1,
            pixel.a(),
        );
    }
}

fn render_default_entrance_marker(
    project: &Project,
    palette: &Palette,
) -> Result<egui::ColorImage, String> {
    const WIDTH: usize = 16;
    const HEIGHT: usize = 32;
    // Horizontal action-0 path in `RenderConfiguredLevelEntrance` @ 004CC660. Lunar Magic places
    // editor-only Map16 $300 at Y+2 and $310 at Y+18. These are their live sidecar definitions.
    const PARTS: [([u16; 4], usize); 2] = [
        ([0x40e1, 0x40f1, 0x40e0, 0x40f0], 0),
        ([0x4005, 0x4015, 0x4004, 0x4014], 16),
    ];
    let player_bytes = project
        .load_decompressed_graphics_file(1, lm_profile::smw_us_v1_vanilla_special_graphics_layout())
        .map_err(|error| error.to_string())?;
    let player_tiles = lm_graphics::decode_planar_tiles(&player_bytes, 4)
        .map_err(|error| format!("cannot decode pristine entrance GFX32: {error}"))?;
    let mut rgba = vec![0; WIDTH * HEIGHT * 4];
    for (definition, part_y) in PARTS {
        for (quadrant, word) in definition.into_iter().enumerate() {
            let tile_index = usize::from(word & 0x03ff);
            let tile = player_tiles
                .get(tile_index)
                .ok_or_else(|| format!("entrance subtile ${tile_index:03X} is unavailable"))?;
            let (x, y) = map16_quadrant_offset(quadrant);
            draw_subtile_over(
                &mut rgba,
                WIDTH,
                (x, part_y + y),
                Some(tile),
                palette,
                8 + usize::from(word >> 10 & 7),
                (word & 0x4000 != 0, word & 0x8000 != 0),
            );
        }
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [WIDTH, HEIGHT],
        &rgba,
    ))
}

fn draw_subtile_over(
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
            let Some(index) = tile.and_then(|tile| tile.pixel(source_x, source_y)) else {
                continue;
            };
            if index == 0 {
                continue;
            }
            let Some(color) = palette_color(palette, palette_row, index) else {
                continue;
            };
            let output = ((target_y + y) * canvas_width + target_x + x) * 4;
            rgba[output..output + 4].copy_from_slice(&color);
        }
    }
}

fn render_layer3_planes(
    tilemap: &[u16],
    graphics: &[IndexedTile],
    palette: &Palette,
    additive: bool,
) -> (egui::ColorImage, egui::ColorImage) {
    const TILES: usize = lm_profile::SMW_US_V1_LAYER3_TILEMAP_SIDE;
    const TILE_PIXELS: usize = IndexedTile::WIDTH;
    const EXTENT: usize = TILES * TILE_PIXELS;
    let mut low = egui::ColorImage::new([EXTENT, EXTENT], egui::Color32::TRANSPARENT);
    let mut high = egui::ColorImage::new([EXTENT, EXTENT], egui::Color32::TRANSPARENT);
    for (position, &word) in tilemap.iter().take(TILES * TILES).enumerate() {
        // The stripe decoder fills untouched BG3 cells with SMW's canonical
        // blank word.  Its tile number is not universally blank in the
        // level-specific graphics set, so treat the sentinel itself as empty.
        if word == 0x38fc {
            continue;
        }
        let tile_x = position % TILES;
        let tile_y = position / TILES;
        let Some(tile) = graphics.get(usize::from(word & 0x03ff)) else {
            continue;
        };
        let palette_number = usize::from((word >> 10) & 7);
        let x_flip = word & 0x4000 != 0;
        let y_flip = word & 0x8000 != 0;
        for y in 0..TILE_PIXELS {
            for x in 0..TILE_PIXELS {
                let source_x = if x_flip { TILE_PIXELS - 1 - x } else { x };
                let source_y = if y_flip { TILE_PIXELS - 1 - y } else { y };
                let Some(index) = tile.pixel(source_x, source_y) else {
                    continue;
                };
                if index == 0 {
                    continue;
                }
                // BG3 is 2bpp in SMW's normal level mode: each tile palette selects four
                // consecutive CGRAM colors rather than one sixteen-color 4bpp row.
                let color_index = palette_number * 4 + usize::from(index);
                let Some(color) = palette.colors.get(color_index) else {
                    continue;
                };
                let color = color.to_rgb8();
                let target = if word & 0x2000 == 0 {
                    &mut low
                } else {
                    &mut high
                };
                target.pixels[(tile_y * TILE_PIXELS + y) * EXTENT + tile_x * TILE_PIXELS + x] =
                    if additive {
                        egui::Color32::from_rgb_additive(color.red, color.green, color.blue)
                    } else {
                        egui::Color32::from_rgb(color.red, color.green, color.blue)
                    };
            }
        }
    }
    (low, high)
}

fn apply_vanilla_common_animation_frame(
    project: &Project,
    graphics: &mut [IndexedTile],
    phase: usize,
    tileset: u8,
) -> Result<(), String> {
    if phase >= 8 {
        return Err(format!(
            "vanilla common animation phase {phase} is outside 0..8"
        ));
    }
    apply_vanilla_common_animation_phases(
        project,
        graphics,
        &vanilla_common_animation_phases(phase),
        tileset,
    )
}

fn vanilla_common_animation_phases(timer_phase: usize) -> [u8; 19] {
    // Lunar Magic's AdvanceExAnimationFrames (0045aac0) processes four consecutive
    // vanilla groups at the normal rate, while AdvanceVanillaAnimatedTileGroup
    // (00459c60) advances three counters per group. The cursor wraps across eight
    // groups. Its saved seed at 005e81e8 uses zero for counters 0, 4, 5, and 13 and
    // 0xff for the rest; after the first advance, the latter become frame zero and
    // remain one frame behind. Model a steady-state eight-timer-tick cycle.
    const ZERO_SEEDED_COUNTERS: [usize; 4] = [0, 4, 5, 13];
    let substeps = timer_phase * 4;
    let mut phases = [0_u8; 19];
    let mut counter = 0;
    while counter < phases.len() {
        let group = counter / 3;
        let additional_advances = if substeps <= group {
            0
        } else {
            (substeps - 1 - group) / 8 + 1
        };
        let steady_advances = 4 + additional_advances;
        let zero_seeded = ZERO_SEEDED_COUNTERS.contains(&counter);
        let phase = if zero_seeded {
            steady_advances % 4
        } else {
            (steady_advances - 1) % 4
        };
        phases[counter] = u8::try_from(phase).expect("animation phase is two bits");
        counter += 1;
    }
    phases
}

fn apply_vanilla_common_animation_phases(
    project: &Project,
    graphics: &mut [IndexedTile],
    phases: &[u8; 19],
    tileset: u8,
) -> Result<(), String> {
    const VRAM_DESTINATIONS: [usize; 24] = [
        0x600, 0x640, 0x680, 0x740, 0xea0, 0x800, 0x500, 0x540, 0x580, 0x5c0, 0x780, 0x7c0, 0xda0,
        0x6c0, 0x700, 0x4c0, 0x440, 0x480, 0x400, 0, 0, 0, 0, 0,
    ];
    const MODE: [u8; 24] = [
        0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 0, 0, 0, 0, 0,
    ];
    const TILESET_OFFSETS: [usize; 14] = [0, 5, 10, 15, 20, 20, 25, 20, 10, 20, 0, 5, 0, 20];
    const FRAME_TABLE_OFFSET: usize = 0x2_b999;
    const GFX32_SOURCE_BASE: usize = 0x2000;
    const GFX33_SOURCE_BASE: usize = 0x7d00;
    const SOURCE_LIMIT: usize = 0xc800;
    const SNES_4BPP_TILE_BYTES: usize = 32;
    const TILES_PER_COPY: usize = 4;

    let decoded_gfx33 = project
        .load_decompressed_graphics_file(0, lm_profile::smw_us_v1_vanilla_special_graphics_layout())
        .map_err(|error| error.to_string())?;
    let gfx33_tiles = lm_graphics::decode_planar_tiles(&decoded_gfx33, 3)
        .map_err(|error| format!("cannot decode pristine animated GFX33: {error}"))?;
    let decoded_gfx32 = project
        .load_decompressed_graphics_file(1, lm_profile::smw_us_v1_vanilla_special_graphics_layout())
        .map_err(|error| error.to_string())?;
    let gfx32_tiles = lm_graphics::decode_planar_tiles(&decoded_gfx32, 4)
        .map_err(|error| format!("cannot decode pristine player/animation GFX32: {error}"))?;
    let blank_tiles = [
        IndexedTile::new([0; IndexedTile::PIXEL_COUNT]),
        IndexedTile::new([0; IndexedTile::PIXEL_COUNT]),
        IndexedTile::new([0; IndexedTile::PIXEL_COUNT]),
        IndexedTile::new([0; IndexedTile::PIXEL_COUNT]),
    ];
    let graphics_len = graphics.len();
    for (animation_index, destination_word) in VRAM_DESTINATIONS.into_iter().enumerate() {
        if destination_word == 0 {
            continue;
        }
        let phase = usize::from(phases[animation_index]);
        if phase >= 4 {
            return Err(format!(
                "vanilla animation group {animation_index} phase {phase} is outside 0..4"
            ));
        }
        let source_index = if MODE[animation_index] == 2 {
            animation_index
                + TILESET_OFFSETS
                    .get(usize::from(tileset))
                    .copied()
                    .unwrap_or_default()
        } else if MODE[animation_index] == 1 && matches!(animation_index, 6 | 7 | 10) {
            // Lunar Magic's ordinary editor state enables this control bank
            // (`DAT_005e7b06 == 1`, source-bank selector $26).
            0x26 + animation_index
        } else {
            animation_index
        };
        let table_word = source_index * 4 + phase;
        let table_offset = FRAME_TABLE_OFFSET + table_word * 2;
        let source_bytes = project
            .rom
            .logical_bytes()
            .get(table_offset..table_offset + 2)
            .ok_or_else(|| {
                format!("vanilla animation frame table word {table_word} is outside the ROM")
            })?;
        let source_address = usize::from(u16::from_le_bytes([source_bytes[0], source_bytes[1]]));
        let destination = destination_word / 0x10;
        // Active pristine-ROM backend, Ghidra AdvanceVanillaAnimatedTileGroup @ 00459c60:
        // $2000-$7CFF addresses GFX32; $7D00-$C7FF addresses GFX33; other values are blank.
        let source_tiles = if (GFX32_SOURCE_BASE..GFX33_SOURCE_BASE).contains(&source_address) {
            let source = (source_address - GFX32_SOURCE_BASE) / SNES_4BPP_TILE_BYTES;
            let source_end = source + TILES_PER_COPY;
            gfx32_tiles.get(source..source_end).ok_or_else(|| {
                format!(
                    "decoded GFX32 has {} tiles; frame requires tiles {source}..{source_end}",
                    gfx32_tiles.len()
                )
            })?
        } else if (GFX33_SOURCE_BASE..SOURCE_LIMIT).contains(&source_address) {
            let source = (source_address - GFX33_SOURCE_BASE) / SNES_4BPP_TILE_BYTES;
            let source_end = source + TILES_PER_COPY;
            gfx33_tiles.get(source..source_end).ok_or_else(|| {
                format!(
                    "decoded GFX33 has {} tiles; frame requires tiles {source}..{source_end}",
                    gfx33_tiles.len()
                )
            })?
        } else {
            &blank_tiles
        };
        if animation_index == 5 {
            // Lunar Magic writes this group's latter pair to $90-$91, not $82-$83.
            graphics
                .get_mut(destination..destination + 2)
                .ok_or_else(|| {
                    format!(
                        "foreground VRAM has {graphics_len} tiles; animation requires slots {destination}..{}",
                        destination + 2
                    )
                })?
                .clone_from_slice(&source_tiles[..2]);
            let second_destination = destination + 0x10;
            graphics
                .get_mut(second_destination..second_destination + 2)
                .ok_or_else(|| {
                    format!(
                        "foreground VRAM has {graphics_len} tiles; animation requires slots {second_destination}..{}",
                        second_destination + 2
                    )
                })?
                .clone_from_slice(&source_tiles[2..]);
        } else {
            let destination_end = destination + TILES_PER_COPY;
            graphics
                .get_mut(destination..destination_end)
                .ok_or_else(|| {
                    format!(
                        "foreground VRAM has {graphics_len} tiles; animation requires slots {destination}..{destination_end}"
                    )
                })?
                .clone_from_slice(source_tiles);
        }
    }
    Ok(())
}

fn map16_definitions_for_phase(base: &[u8], phase: usize) -> Vec<u8> {
    // Ghidra RenderMap16TileToPixelBuffer @ 0044EAF0 selects a four-phase alternate
    // definition bank for Map16 $133-$13A. The pristine variants retain tile numbers and
    // flips while selecting palette rows 3, 5, 6, and 7 respectively.
    const PALETTE_ROWS: [u16; 4] = [3, 5, 6, 7];
    let mut definitions = base.to_vec();
    let palette = PALETTE_ROWS[phase & 3] << 10;
    for definition in 0x133..=0x13a {
        let start = definition * lm_profile::SMW_US_V1_MAP16_TILE_BYTES;
        for word in
            definitions[start..start + lm_profile::SMW_US_V1_MAP16_TILE_BYTES].chunks_exact_mut(2)
        {
            let value = u16::from_le_bytes([word[0], word[1]]);
            word.copy_from_slice(&((value & !0x1c00) | palette).to_le_bytes());
        }
    }
    definitions
}

fn render_map16_definition_atlas(
    definitions: &[u8],
    graphics: &[IndexedTile],
    palette: &Palette,
) -> egui::ColorImage {
    render_map16_definition_atlas_with_layer2_palette(definitions, graphics, palette, false)
}

fn render_layer2_map16_definition_atlas(
    definitions: &[u8],
    graphics: &[IndexedTile],
    palette: &Palette,
    tileset: u8,
) -> egui::ColorImage {
    // Ghidra RenderLevelEditorViewportRegion @ 00453c0f sets DAT_00600256 for
    // object-backed Layer 2 when the active object tileset is 3. The Map16
    // renderer then adds four palette rows to subtiles using rows 0..3.
    render_map16_definition_atlas_with_layer2_palette(definitions, graphics, palette, tileset == 3)
}

fn render_map16_definition_atlas_with_layer2_palette(
    definitions: &[u8],
    graphics: &[IndexedTile],
    palette: &Palette,
    shift_low_palette_rows: bool,
) -> egui::ColorImage {
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
            let (quadrant_x, quadrant_y) = map16_quadrant_offset(quadrant);
            let mut palette_number = usize::from((word >> 10) & 7);
            if shift_low_palette_rows && palette_number < 4 {
                palette_number += 4;
            }
            draw_subtile(
                &mut rgba,
                width,
                (definition_x + quadrant_x, definition_y + quadrant_y),
                graphics.get(tile_number),
                palette,
                palette_number,
                (word & 0x4000 != 0, word & 0x8000 != 0),
            );
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba)
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
    let palette = lm_profile::compose_smw_us_v1_level_palette(
        &project,
        level,
        game_palette_header(level, header),
        0,
    )
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
            let (quadrant_x, quadrant_y) = map16_quadrant_offset(quadrant);
            draw_subtile(
                &mut rgba,
                WIDTH,
                (definition_x + quadrant_x, definition_y + quadrant_y),
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

const fn map16_quadrant_offset(quadrant: usize) -> (usize, usize) {
    (quadrant / 2 * 8, quadrant % 2 * 8)
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
            let bitplanes = vanilla_graphics_bitplanes(decoded.len()).ok_or_else(|| {
                format!(
                    "pristine GFX{file:02X} expands to unsupported length {}",
                    decoded.len()
                )
            })?;
            let mut tiles = lm_graphics::decode_planar_tiles(&decoded, bitplanes).map_err(
                |error| {
                    format!(
                        "cannot decode pristine {bitplanes}bpp GFX{file:02X}: {error}"
                    )
                },
            )?;
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

const fn vanilla_graphics_bitplanes(decoded_len: usize) -> Option<u8> {
    match decoded_len {
        0x800 => Some(2),
        0xc00 => Some(3),
        0x1000 => Some(4),
        _ => None,
    }
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
        return Some([0, 0, 0, 0]);
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
    fn animated_pipe_map16_definitions_select_native_palette_phases() {
        let mut base = vec![0_u8; lm_profile::SMW_US_V1_MAP16_BASE_BYTES];
        for definition in 0x133..=0x13a {
            let start = definition * lm_profile::SMW_US_V1_MAP16_TILE_BYTES;
            for (quadrant, word) in base[start..start + 8].chunks_exact_mut(2).enumerate() {
                word.copy_from_slice(&(0xc000 | 0x1400 | quadrant as u16).to_le_bytes());
            }
        }
        for (phase, palette) in [3_u16, 5, 6, 7].into_iter().enumerate() {
            let definitions = map16_definitions_for_phase(&base, phase);
            for definition in 0x133..=0x13a {
                let start = definition * lm_profile::SMW_US_V1_MAP16_TILE_BYTES;
                for (quadrant, word) in definitions[start..start + 8].chunks_exact(2).enumerate() {
                    assert_eq!(
                        u16::from_le_bytes([word[0], word[1]]),
                        0xc000 | (palette << 10) | quadrant as u16
                    );
                }
            }
        }
    }

    #[test]
    fn snes_palette_color_zero_is_transparent_in_editor_atlases() {
        let palette = Palette {
            colors: vec![Bgr555(0x7fff); 256],
        };
        assert_eq!(palette_color(&palette, 0, 0), Some([0, 0, 0, 0]));
    }

    #[test]
    fn pristine_graphics_depth_follows_the_decompressed_slot_size() {
        assert_eq!(vanilla_graphics_bitplanes(0x800), Some(2));
        assert_eq!(vanilla_graphics_bitplanes(0xc00), Some(3));
        assert_eq!(vanilla_graphics_bitplanes(0x1000), Some(4));
        assert_eq!(vanilla_graphics_bitplanes(0), None);
    }

    #[test]
    fn cookie_mountain_uses_the_runtime_background_graphics_substitution() {
        let header = LegacyLevelHeader::decode(&[0x13, 0xc0, 0x00, 0x86, 0x20]).unwrap();
        assert_eq!(
            game_graphics_files(1, header, [0x14, 0x17, 0x19, 0x15]),
            [0x14, 0x17, 0x19, 0x16]
        );
        assert_eq!(
            game_graphics_files(2, header, [0x14, 0x17, 0x19, 0x15]),
            [0x14, 0x17, 0x19, 0x15]
        );
    }

    #[test]
    fn native_map16_quadrants_are_column_major() {
        assert_eq!(
            (0..4).map(map16_quadrant_offset).collect::<Vec<_>>(),
            [(0, 0), (0, 8), (8, 0), (8, 8)]
        );
    }

    #[test]
    fn tileset_three_layer2_objects_shift_low_map16_palette_rows() {
        let mut definitions = vec![0; lm_profile::SMW_US_V1_MAP16_BASE_BYTES];
        for word in definitions[..8].chunks_exact_mut(2) {
            word.copy_from_slice(&(2_u16 << 10).to_le_bytes());
        }
        let graphics = vec![IndexedTile::new([1; IndexedTile::PIXEL_COUNT])];
        let mut colors = vec![Bgr555(0); 256];
        colors[2 * 16 + 1] = Bgr555::from_rgb8(Rgb8 {
            red: 255,
            green: 0,
            blue: 0,
        });
        colors[6 * 16 + 1] = Bgr555::from_rgb8(Rgb8 {
            red: 0,
            green: 255,
            blue: 0,
        });
        let palette = Palette { colors };

        let ordinary = render_layer2_map16_definition_atlas(&definitions, &graphics, &palette, 2);
        let shifted = render_layer2_map16_definition_atlas(&definitions, &graphics, &palette, 3);
        assert_eq!(ordinary.pixels[0], egui::Color32::RED);
        assert_eq!(shifted.pixels[0], egui::Color32::GREEN);
    }

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
    fn native_background_plane_composes_column_major_storage_without_seams() {
        let mut atlas = egui::ColorImage::new([512, 256], egui::Color32::TRANSPARENT);
        atlas.pixels[16] = egui::Color32::RED;
        let tile_two = 2 * 16;
        atlas.pixels[tile_two] = egui::Color32::GREEN;
        let mut tilemap = vec![0; 32 * 32];
        tilemap[lm_level::native_layer2_tilemap_index(0, 0).unwrap()] = 1;
        tilemap[lm_level::native_layer2_tilemap_index(31, 31).unwrap()] = 2;

        let plane = compose_native_map16_plane(&atlas, &tilemap).unwrap();
        assert_eq!(plane.size, [512, 512]);
        assert_eq!(plane.pixels[0], egui::Color32::RED);
        assert_eq!(
            plane.pixels[(31 * 16) * 512 + 31 * 16],
            egui::Color32::GREEN
        );
        assert_eq!(plane.pixels[16], egui::Color32::TRANSPARENT);
    }

    #[test]
    fn native_background_plane_rejects_inexact_inputs() {
        let atlas = egui::ColorImage::new([511, 256], egui::Color32::TRANSPARENT);
        assert!(compose_native_map16_plane(&atlas, &[0; 1024]).is_err());
        let atlas = egui::ColorImage::new([512, 256], egui::Color32::TRANSPARENT);
        assert!(compose_native_map16_plane(&atlas, &[0; 1023]).is_err());
    }

    #[test]
    fn layer3_planes_use_priority_two_bit_palettes_transparency_and_flips() {
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let mut pixels = [0; IndexedTile::PIXEL_COUNT];
        pixels[0] = 1;
        let graphics = vec![blank, IndexedTile::new(pixels)];
        let mut colors = vec![Bgr555(0); 256];
        colors[2 * 4 + 1] = Bgr555::from_rgb8(Rgb8 {
            red: 255,
            green: 0,
            blue: 0,
        });
        let palette = Palette { colors };
        let mut tilemap = vec![0; lm_profile::SMW_US_V1_LAYER3_TILEMAP_WORDS];
        tilemap[0] = 1 | 2 << 10;
        tilemap[1] = 1 | 2 << 10 | 0x2000 | 0x4000;
        tilemap[64] = 1 | 2 << 10 | 0x8000;

        let (low, high) = render_layer3_planes(&tilemap, &graphics, &palette, false);
        assert_eq!(low.size, [512, 512]);
        assert_eq!(high.size, [512, 512]);
        assert_eq!(low.pixels[0], egui::Color32::RED);
        assert_eq!(high.pixels[8 + 7], egui::Color32::RED);
        assert_eq!(low.pixels[15 * 512], egui::Color32::RED);
        assert_eq!(low.pixels[8 + 7], egui::Color32::TRANSPARENT);
        assert_eq!(high.pixels[0], egui::Color32::TRANSPARENT);
        assert_eq!(low.pixels[1], egui::Color32::TRANSPARENT);
    }

    #[test]
    fn vanilla_animation_groups_follow_lunar_magics_rolling_counter_schedule() {
        assert_eq!(
            vanilla_common_animation_phases(0),
            [0, 3, 3, 3, 0, 0, 3, 3, 3, 3, 3, 3, 3, 0, 3, 3, 3, 3, 3]
        );
        assert_eq!(
            vanilla_common_animation_phases(1),
            [1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 3, 0, 3, 3, 3, 3, 3]
        );
        assert_eq!(
            vanilla_common_animation_phases(2),
            [1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn renders_real_pristine_tileset_when_reference_rom_is_available() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let level = project
            .load_level_slot(
                0,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let preview = render(bytes, 0, level.layer1.header, true).unwrap();
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
        let animated = project
            .load_decompressed_graphics_file(
                0,
                lm_profile::smw_us_v1_vanilla_special_graphics_layout(),
            )
            .unwrap();
        let animated = lm_graphics::decode_planar_tiles(&animated, 3).unwrap();
        let player_animation = project
            .load_decompressed_graphics_file(
                1,
                lm_profile::smw_us_v1_vanilla_special_graphics_layout(),
            )
            .unwrap();
        let player_animation = lm_graphics::decode_planar_tiles(&player_animation, 4).unwrap();
        assert_eq!(preview.foreground_tiles[0x60], animated[192]);
        assert_eq!(preview.foreground_tiles[0x6b], animated[203]);
        assert_eq!(
            preview.foreground_tiles[0x6c], animated[204],
            "the common coin group must honor Lunar Magic's seeded phase offset"
        );
        assert_eq!(preview.foreground_tiles[0x6f], animated[207]);
        assert_eq!(preview.foreground_tiles[0x80], player_animation[0x26c]);
        assert_eq!(preview.foreground_tiles[0x81], player_animation[0x26d]);
        assert_eq!(preview.foreground_tiles[0x90], player_animation[0x26e]);
        assert_eq!(preview.foreground_tiles[0x91], player_animation[0x26f]);
        assert_eq!(preview.foreground_tiles[0x50], animated[0xa4]);
        assert_eq!(preview.foreground_tiles[0x54], animated[0xfc]);
        assert_eq!(preview.foreground_tiles[0x78], animated[0xf0]);
        let mut last_phase = preview.foreground_tiles.clone();
        apply_vanilla_common_animation_frame(
            &project,
            &mut last_phase,
            3,
            level.layer1.header.object_tileset(),
        )
        .unwrap();
        assert_ne!(last_phase[0x60], preview.foreground_tiles[0x60]);
        assert_eq!(preview.animated_images.len(), 32);
        assert_eq!(preview.animated_background_images.len(), 8);
        assert_ne!(preview.animated_images[0], preview.animated_images[3]);
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
    fn diagnostic_lunar_magic_decoded_cache_matches_level_graphics_when_requested() {
        let Ok(cache_path) = std::env::var("LM_DECODED_GRAPHICS_CACHE") else {
            return;
        };
        let slot = std::env::var("LM_LEVEL_SLOT")
            .ok()
            .map(|slot| u16::from_str_radix(&slot, 16).unwrap())
            .unwrap_or(0x106);
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let level = project
            .load_level_slot(
                usize::from(slot),
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        if let Ok(tilemap_path) = std::env::var("LM_BACKGROUND_TILEMAP") {
            let live = std::fs::read(tilemap_path).unwrap();
            let layer2 = project
                .load_level_layer2(
                    0x106,
                    level.layer1.header.level_mode(),
                    lm_profile::smw_us_v1_vanilla_layer2_layout(),
                )
                .unwrap();
            let lm_level::NativeLayer2Data::Tilemap(native) = &layer2 else {
                panic!("level $106 Layer 2 is not a tilemap");
            };
            let native_words = native
                .chunks_exact(2)
                .map(|word| u16::from_le_bytes([word[0], word[1]]))
                .collect::<Vec<_>>();
            let live_words = live
                .chunks_exact(2)
                .map(|word| u16::from_le_bytes([word[0], word[1]]))
                .collect::<Vec<_>>();
            let best = (0..32)
                .map(|shift| {
                    let equal = (0..32)
                        .flat_map(|y| (0..32).map(move |x| (x, y)))
                        .filter(|&(x, y)| {
                            let live_index = ((x >> 4) * 31 + y) * 16 + x;
                            let native_index =
                                lm_level::native_layer2_tilemap_index(x, (y + shift) % 32).unwrap();
                            live_words[live_index] == native_words[native_index]
                        })
                        .count();
                    (equal, shift)
                })
                .max()
                .unwrap();
            assert_eq!(
                best,
                (1024, 0),
                "Lunar Magic background tilemap best native row shift is {} with {} / 1024 matching words",
                best.1,
                best.0
            );
        }
        let files = lm_profile::smw_us_v1_object_tileset_graphics_files(
            &project.rom,
            usize::from(level.layer1.header.object_tileset()),
        )
        .unwrap();
        let slots = load_layer1_sprite_graphics_slots(&project, files).unwrap();
        let mut tiles = materialize_layer1_sprite_vram(&slots);
        if let Ok(tile) = std::env::var("LM_TRACE_MAP16_TILE") {
            let tile = usize::from_str_radix(&tile, 16).unwrap();
            let map16 = lm_profile::load_smw_us_v1_level_map16_base(
                &project.rom,
                usize::from(level.layer1.header.object_tileset()),
            )
            .unwrap();
            let start = tile * 8;
            eprintln!(
                "level {slot:03X} Map16 ${tile:03X} words={:04X?}",
                map16.bytes[start..start + 8]
                    .chunks_exact(2)
                    .map(|word| u16::from_le_bytes([word[0], word[1]]))
                    .collect::<Vec<_>>()
            );
        }
        let live_counters = std::env::var("LM_ANIMATION_COUNTERS")
            .ok()
            .map(std::fs::read)
            .transpose()
            .unwrap();
        if let Some(counters) = live_counters.as_ref() {
            let phases: [u8; 19] = counters[..19].try_into().unwrap();
            apply_vanilla_common_animation_phases(
                &project,
                &mut tiles,
                &phases,
                level.layer1.header.object_tileset(),
            )
            .unwrap();
        } else {
            apply_vanilla_common_animation_frame(
                &project,
                &mut tiles,
                0,
                level.layer1.header.object_tileset(),
            )
            .unwrap();
        }
        let expected = std::fs::read(cache_path).unwrap();
        assert!(expected.len() >= tiles.len() * IndexedTile::PIXEL_COUNT);
        if expected.len() >= (0x900 + 0x2e8) * IndexedTile::PIXEL_COUNT {
            let gfx32 = project
                .load_decompressed_graphics_file(
                    1,
                    lm_profile::smw_us_v1_vanilla_special_graphics_layout(),
                )
                .unwrap();
            let gfx32 = lm_graphics::decode_planar_tiles(&gfx32, 4).unwrap();
            let cache_start = 0x900 * IndexedTile::PIXEL_COUNT;
            let gfx32 = &gfx32[..0x2e8];
            let cache_end = cache_start + gfx32.len() * IndexedTile::PIXEL_COUNT;
            let flattened = gfx32
                .iter()
                .flat_map(|tile| tile.pixels().iter().copied())
                .collect::<Vec<_>>();
            assert_eq!(flattened, expected[cache_start..cache_end]);
        }
        if std::env::var_os("LM_TRACE_ENTRANCE_CACHE").is_some() {
            let gfx32 = project
                .load_decompressed_graphics_file(
                    1,
                    lm_profile::smw_us_v1_vanilla_special_graphics_layout(),
                )
                .unwrap();
            let gfx32 = lm_graphics::decode_planar_tiles(&gfx32, 4).unwrap();
            let ordinary = (0..lm_profile::SMW_US_V1_VANILLA_GRAPHICS_FILES)
                .filter_map(|file| {
                    let bytes = project
                        .load_decompressed_graphics_file(
                            file,
                            lm_profile::smw_us_v1_vanilla_graphics_layout(),
                        )
                        .ok()?;
                    let bitplanes = vanilla_graphics_bitplanes(bytes.len())?;
                    Some((
                        file,
                        lm_graphics::decode_planar_tiles(&bytes, bitplanes).ok()?,
                    ))
                })
                .collect::<Vec<_>>();
            for cache_tile in [0x640, 0x641, 0x642, 0x643, 0x650, 0x651, 0x652, 0x653] {
                let start = cache_tile * IndexedTile::PIXEL_COUNT;
                let pixels = &expected[start..start + IndexedTile::PIXEL_COUNT];
                let matches = gfx32
                    .iter()
                    .enumerate()
                    .filter_map(|(source, tile)| {
                        (tile.pixels().as_slice() == pixels).then_some(source)
                    })
                    .collect::<Vec<_>>();
                let ordinary_matches = ordinary
                    .iter()
                    .flat_map(|(file, tiles)| {
                        tiles.iter().enumerate().filter_map(move |(source, tile)| {
                            (tile.pixels().as_slice() == pixels).then_some((*file, source))
                        })
                    })
                    .collect::<Vec<_>>();
                eprintln!(
                    "entrance cache ${cache_tile:03X} matches GFX32 {matches:03X?}, ordinary {ordinary_matches:02X?}"
                );
            }
        }
        let differing = tiles
            .iter()
            .enumerate()
            .filter_map(|(tile, actual)| {
                let start = tile * IndexedTile::PIXEL_COUNT;
                (actual.pixels().as_slice() != &expected[start..start + IndexedTile::PIXEL_COUNT])
                    .then_some(tile)
            })
            .collect::<Vec<_>>();
        eprintln!("Lunar Magic cache mismatch tiles: {differing:02X?}");
        if live_counters.is_some() {
            assert!(
                differing.is_empty(),
                "live decoded cache differs at {differing:02X?}"
            );
        } else {
            assert!(
                differing.len() <= 96,
                "{} of {} tiles differ",
                differing.len(),
                tiles.len()
            );
        }
    }

    #[test]
    fn diagnostic_lunar_magic_level_palette_cache_matches_when_requested() {
        let (Ok(slot), Ok(cache_path)) = (
            std::env::var("LM_LEVEL_SLOT"),
            std::env::var("LM_LEVEL_PALETTE_CACHE"),
        ) else {
            return;
        };
        let slot = u16::from_str_radix(&slot, 16).unwrap();
        let live = std::fs::read(cache_path).unwrap();
        assert_eq!(live.len(), 0x202);
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let level = project
            .load_level_slot(
                usize::from(slot),
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let actual =
            lm_profile::compose_smw_us_v1_level_palette(&project, slot, level.layer1.header, 0)
                .unwrap();
        let live_words = live
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect::<Vec<_>>();
        let differences = (1..256)
            .filter_map(|index| {
                let expected = if index % 16 == 0 {
                    0
                } else {
                    live_words[index]
                };
                (actual.palette.colors[index].0 != expected).then_some((
                    index,
                    actual.palette.colors[index].0,
                    expected,
                ))
            })
            .collect::<Vec<_>>();
        eprintln!(
            "level {slot:03X} palette differences={} backdrop actual={:04X} live={:04X}",
            differences.len(),
            actual.backdrop.0,
            live_words[256],
        );
        for (index, actual, expected) in differences.iter().take(32) {
            eprintln!("{index:02X}: rust={actual:04X} wine={expected:04X}");
        }
        assert_eq!(actual.backdrop.0, live_words[256]);
        assert!(differences.is_empty());
    }

    #[test]
    fn diagnostic_lunar_magic_rgb_palette_matches_when_requested() {
        let (Ok(slot), Ok(cache_path)) = (
            std::env::var("LM_LEVEL_SLOT"),
            std::env::var("LM_LEVEL_RGB_PALETTE_CACHE"),
        ) else {
            return;
        };
        let slot = u16::from_str_radix(&slot, 16).unwrap();
        let project = Project::new(
            RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap(),
        );
        let level = project
            .load_level_slot(
                usize::from(slot),
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let actual =
            lm_profile::compose_smw_us_v1_level_palette(&project, slot, level.layer1.header, 0)
                .unwrap();
        let live = std::fs::read(cache_path).unwrap();
        assert_eq!(live.len(), 256 * 4);
        let differences = actual
            .palette
            .colors
            .iter()
            .zip(live.chunks_exact(4))
            .enumerate()
            .filter_map(|(index, (actual, live))| {
                let rgb = actual.to_rgb8();
                let live = [live[0], live[1], live[2]];
                ([rgb.red, rgb.green, rgb.blue] != live).then_some(index)
            })
            .collect::<Vec<_>>();
        eprintln!("level {slot:03X} RGB palette mismatch entries: {differences:02X?}");
        assert!(differences.is_empty());
    }

    #[test]
    fn diagnostic_lunar_magic_map16_graphics_cache_matches_when_requested() {
        let (Ok(slot), Ok(cache_path)) = (
            std::env::var("LM_LEVEL_SLOT"),
            std::env::var("LM_LEVEL_MAP16_GRAPHICS_CACHE"),
        ) else {
            return;
        };
        let slot = usize::from_str_radix(&slot, 16).unwrap();
        let project = Project::new(
            RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap(),
        );
        let level = project
            .load_level_slot(
                slot,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let actual = lm_profile::load_smw_us_v1_level_map16_base(
            &project.rom,
            usize::from(level.layer1.header.object_tileset()),
        )
        .unwrap();
        let expected = std::fs::read(cache_path).unwrap();
        assert_eq!(expected.len(), actual.bytes.len());
        let differences = actual
            .bytes
            .iter()
            .zip(&expected)
            .filter(|(actual, expected)| actual != expected)
            .count();
        eprintln!(
            "level {slot:03X} Map16 graphics differences={differences} / {} bytes",
            expected.len()
        );
        for (index, (actual, expected)) in actual
            .bytes
            .chunks_exact(2)
            .zip(expected.chunks_exact(2))
            .enumerate()
            .filter(|(_, (actual, expected))| actual != expected)
            .take(16)
        {
            eprintln!(
                "{index:03X}: rust={:04X} wine={:04X}",
                u16::from_le_bytes([actual[0], actual[1]]),
                u16::from_le_bytes([expected[0], expected[1]])
            );
        }
        assert_eq!(differences, 0);
    }

    #[test]
    fn diagnostic_lunar_magic_layer3_graphics_cache_matches_when_requested() {
        let (Ok(slot), Ok(cache_path)) = (
            std::env::var("LM_LEVEL_SLOT"),
            std::env::var("LM_LEVEL_LAYER3_GRAPHICS_CACHE"),
        ) else {
            return;
        };
        let slot = usize::from_str_radix(&slot, 16).unwrap();
        let project = Project::new(
            RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap(),
        );
        let actual = load_layer3_tiles(&project, slot).unwrap();
        let actual = actual
            .iter()
            .flat_map(|tile| tile.pixels().iter().copied())
            .collect::<Vec<_>>();
        let expected = std::fs::read(cache_path).unwrap();
        assert_eq!(expected.len(), actual.len());
        let differing = actual
            .chunks_exact(IndexedTile::PIXEL_COUNT)
            .zip(expected.chunks_exact(IndexedTile::PIXEL_COUNT))
            .enumerate()
            .filter_map(|(tile, (actual, expected))| (actual != expected).then_some(tile))
            .collect::<Vec<_>>();
        eprintln!("level {slot:03X} Layer 3 graphics mismatch tiles: {differing:03X?}");
        assert!(differing.is_empty());
    }

    #[test]
    fn background_half_color_matches_lunar_magics_packed_rgb_shift() {
        let mut image = egui::ColorImage::new([2, 1], egui::Color32::from_rgb(17, 83, 231));
        image.pixels[1] = egui::Color32::TRANSPARENT;
        apply_black_half_color(&mut image);
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(8, 41, 115));
        assert_eq!(image.pixels[1], egui::Color32::TRANSPARENT);
    }

    #[test]
    fn pristine_cookie_mountain_uses_its_runtime_game_palette() {
        let raw = LegacyLevelHeader::decode(&[0x13, 0xc0, 0x00, 0x86, 0x20]).unwrap();
        let game = game_palette_header(1, raw);
        assert_eq!(game.background_color(), 2);
        assert_eq!(game.foreground_palette(), 0);
        assert_eq!(
            game_palette_header(2, raw),
            raw,
            "the compatibility rule is level-specific"
        );
        let mut edited = raw;
        edited.set_foreground_palette(5).unwrap();
        assert_eq!(
            game_palette_header(1, edited),
            edited,
            "authored palette changes must not be replaced"
        );

        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let editor = render(bytes.clone(), 1, raw, false).unwrap();
        let runtime = render(bytes, 1, raw, true).unwrap();
        assert_eq!(
            editor.backdrop.0, 0x7393,
            "Lunar Magic 3.63 live editor DIB uses the authored cyan backdrop"
        );
        assert_eq!(runtime.backdrop.0, 0x5d80);
        assert_ne!(editor.palette, runtime.palette);
    }

    #[test]
    fn cookie_mountain_keeps_foreground_and_background_graphics_slots_distinct() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let header = LegacyLevelHeader::decode(&[0x13, 0xc0, 0x00, 0x86, 0x20]).unwrap();
        let preview = render(bytes, 1, header, true).unwrap();
        assert_eq!(preview.graphics_files, [0x14, 0x17, 0x19, 0x15]);
        assert_eq!(preview.background_graphics_files, [0x14, 0x17, 0x19, 0x16]);
    }

    #[test]
    fn level_105_palette_matches_lunar_magic_mwl_export() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
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
