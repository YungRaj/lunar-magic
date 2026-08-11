//! Headless, unscaled complete-level rasterization for the pristine SMW-US audit corpus.

use eframe::egui;
use lm_level::{
    Layer3PrioritySelection, LevelLayerSlotSource, LevelScreenExtentMode, NativeLayer2Data,
};
use lm_render::{Canvas, NativeLevelMap16Layout, Rgba};
use lm_rom::RomImage;
use std::sync::OnceLock;

// Lunar Magic's deterministic multiple-image export was captured at this built-in animation
// phase. Levels $001, $014, $0D7, $0E3, and $1DD independently select phase 2 as their exact or
// tied-best native match, so corpus comparisons must not mistake animation for renderer defects.
const NATIVE_EXPORT_MAP16_PHASE: usize = 2;
// The multiple-image export captures the palette counter one tick behind the Map16 counter.
// Native Dragon Coins are RGB FFD600 (palette phase 1), while Map16 phase 2 is independently
// authenticated by levels $001, $014, $0D7, $0E3, and $1DD.
const NATIVE_EXPORT_PALETTE_PHASE: usize = 1;
static PRISTINE_REFERENCED_SECONDARY_EXITS: OnceLock<Result<Vec<bool>, String>> = OnceLock::new();

/// One pristine level's core Lunar Magic editor artwork, without toolkit text overlays.
pub(crate) struct PristineFullLevelRender {
    pub(crate) canvas: Canvas,
    pub(crate) screens: u8,
    pub(crate) vertical: bool,
}

/// Renders one full pristine level at Lunar Magic's native 16-pixel Map16 scale.
///
/// Empty Layer 1 slots return `None`, matching Lunar Magic 3.63's multiple-image exporter.
pub(crate) fn render(
    rom_bytes: Vec<u8>,
    level_number: u16,
    map16_phase: usize,
    sprite_phase: u8,
) -> Result<Option<PristineFullLevelRender>, String> {
    if map16_phase >= 8 || sprite_phase >= 4 {
        return Err("full-level animation phase is out of range".into());
    }
    let image = RomImage::from_bytes(rom_bytes.clone()).map_err(|error| error.to_string())?;
    let project = lm_project::Project::new(image.clone());
    let object_definition_map = lm_profile::load_smw_us_v1_standard_object_definition_map(&image)
        .map_err(|error| error.to_string())?;
    let level = project
        .load_level_slot(
            usize::from(level_number),
            lm_profile::smw_us_v1_vanilla_level_layout(),
            &lm_level::SpriteLengthTable::standard(),
        )
        .map_err(|error| error.to_string())?;
    if level.layer1.objects.records.is_empty() {
        return Ok(None);
    }

    let header = level.layer1.header;
    let mode = lm_profile::smw_us_v1_level_mode(header.level_mode());
    if mode.editor_major_screens == 0 {
        return Err(format!(
            "level {level_number:03X} mode {:02X} has no editor canvas",
            mode.index
        ));
    }
    let screens = lm_level::native_level_screen_count_with_header(
        header,
        &level.layer1.objects,
        &level.sprites,
        LevelScreenExtentMode::Stored,
    )
    .min(mode.editor_major_screens);
    let (width, height, tile_width, tile_height) = if mode.vertical {
        (
            512,
            usize::from(screens) * 256,
            32,
            usize::from(screens) * 16,
        )
    } else {
        (
            usize::from(screens) * 256,
            432,
            usize::from(screens) * 16,
            27,
        )
    };

    let preview = crate::vanilla_map16_preview::render_with_editor_palette_phase(
        rom_bytes,
        level_number,
        header,
        false,
        false,
        NATIVE_EXPORT_PALETTE_PHASE,
    )?;
    let backdrop = preview.backdrop.to_rgb8();
    let mut canvas = Canvas::from_pixels(
        width,
        height,
        vec![
            Rgba {
                red: backdrop.red,
                green: backdrop.green,
                blue: backdrop.blue,
                alpha: 255,
            };
            width * height
        ],
    )
    .map_err(|error| error.to_string())?;

    let layer2_layout =
        lm_profile::smw_us_v1_level_layer2_layout(&image, usize::from(level_number))
            .map_err(|error| error.to_string())?
            .ok_or("pristine level has no Layer 2 layout")?;
    let layer2 = project
        .load_level_layer2(
            usize::from(level_number),
            header.level_mode(),
            layer2_layout,
        )
        .map_err(|error| error.to_string())?;
    let shared_background =
        lm_profile::smw_us_v1_level_uses_shared_background(&image, usize::from(level_number))
            .map_err(|error| error.to_string())?;

    if std::env::var_os("LM_PRISTINE_TRACE_SPRITES").is_some() {
        eprintln!(
            "level={level_number:03X} mode={:02X} object_tileset={:02X} sprite_tileset={:02X} sprite_memory={:02X} shared_background={} layer2={}",
            header.level_mode(),
            header.object_tileset(),
            header.sprite_tileset(),
            level.sprites.header & 0x3f,
            shared_background,
            match &layer2 {
                NativeLayer2Data::Tilemap(_) => "tilemap",
                NativeLayer2Data::Objects(_) => "objects",
            },
        );
        for placement in level.sprites.native_placements() {
            eprintln!(
                "level={level_number:03X} sprite={:02X} first={:02X} packed={:02X} major={:03X} minor={:03X}",
                placement.sprite_number,
                placement.first_byte,
                placement.packed_display_position(),
                placement.major,
                placement.minor,
            );
        }
    }

    let layer3_position = preview.layer3_position.map(|(x, y)| {
        preview
            .layer3_editor_row_offset
            .map_or((x, y), |row| (x, row * 16))
    });
    let painter_slots = lm_level::lunar_magic_level_layer_slots(
        header.level_mode(),
        header.split_layer3_priority(),
        None,
    )
    .ok_or_else(|| {
        format!(
            "level mode {:02X} has no painter slots",
            header.level_mode()
        )
    })?;
    for slot in &painter_slots.slots {
        if !slot.enabled {
            continue;
        }
        match slot.source {
            Some(LevelLayerSlotSource::Layer2) => match &layer2 {
                NativeLayer2Data::Tilemap(bytes) => {
                    let words = bytes
                        .chunks_exact(2)
                        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                        .collect::<Vec<_>>();
                    let atlas = if shared_background {
                        &preview.animated_background_images[map16_phase]
                    } else {
                        &preview.animated_layer2_images[map16_phase * 4]
                    };
                    draw_layer2_tilemap(&mut canvas, atlas, &words, tile_width, tile_height);
                }
                NativeLayer2Data::Objects(objects) => draw_object_stream(
                    &mut canvas,
                    &objects.objects,
                    &preview.animated_layer2_images,
                    map16_phase,
                    mode.vertical,
                    header.object_tileset(),
                    &object_definition_map,
                )?,
            },
            Some(LevelLayerSlotSource::Layer1) => draw_object_stream(
                &mut canvas,
                &level.layer1.objects,
                &preview.animated_images,
                map16_phase,
                mode.vertical,
                header.object_tileset(),
                &object_definition_map,
            )?,
            Some(LevelLayerSlotSource::Layer3) => {
                let Some(position) = layer3_position else {
                    continue;
                };
                match slot.layer3_priority {
                    Layer3PrioritySelection::Both => {
                        if let Some(image) = &preview.layer3_low_image {
                            draw_repeating_layer3(&mut canvas, image, position, mode.vertical);
                        }
                        if let Some(image) = &preview.layer3_high_image {
                            draw_repeating_layer3(&mut canvas, image, position, mode.vertical);
                        }
                    }
                    Layer3PrioritySelection::Low => {
                        if let Some(image) = &preview.layer3_low_image {
                            draw_repeating_layer3(&mut canvas, image, position, mode.vertical);
                        }
                    }
                    Layer3PrioritySelection::High => {
                        if let Some(image) = &preview.layer3_high_image {
                            draw_repeating_layer3(&mut canvas, image, position, mode.vertical);
                        }
                    }
                }
            }
            None => {}
        }
    }

    draw_sprites(
        &mut canvas,
        &level.sprites.native_placements(),
        &preview.sprite_tiles,
        &preview.animated_sprite_tiles,
        &preview.palette,
        mode.vertical,
        header.level_mode(),
        header.sprite_tileset(),
        level.sprites.header & 0x3f,
        sprite_phase,
    );

    let entrance = project
        .load_vanilla_main_entrance(
            usize::from(level_number),
            lm_profile::smw_us_v1_vanilla_entrance_layout(),
        )
        .map_err(|error| error.to_string())?;
    let secondary = project
        .load_secondary_exit_table_detected(lm_profile::smw_us_v1_secondary_exit_locator())
        .map_err(|error| error.to_string())?;
    let referenced_secondary_exits = PRISTINE_REFERENCED_SECONDARY_EXITS
        .get_or_init(|| referenced_secondary_exit_slots(&project))
        .as_ref()
        .map_err(Clone::clone)?;
    if std::env::var_os("LM_PRISTINE_TRACE_SPRITES").is_some() {
        for (index, exit) in secondary.table.entries.iter().enumerate() {
            if pristine_secondary_destination(index, *exit) == level_number {
                eprintln!(
                    "secondary index={index:03X} referenced={} exit={exit:?}",
                    referenced_secondary_exits
                        .get(index)
                        .copied()
                        .unwrap_or(false)
                );
            }
        }
    }

    let mut secondary_labels = Vec::new();
    for (index, exit) in secondary.table.entries.iter().copied().enumerate() {
        if pristine_secondary_destination(index, exit) != level_number
            || exit.x_and_overworld_flags & 0x80 != 0
            || !referenced_secondary_exits
                .get(index)
                .copied()
                .unwrap_or(false)
        {
            continue;
        }
        // Slots targeting levels $000/$100 can be all-zero unused records. Lunar Magic filters
        // that legacy sentinel before drawing; other target levels are intrinsically nonempty.
        if matches!(level_number, 0x000 | 0x100)
            && exit.position_and_method == 0
            && exit.screen == 0
            && exit.x == 0
            && exit.y == 0
            && exit.destination_flags == 0
            && exit.x_and_overworld_flags == 0
            && exit.additional_flags == 0
        {
            continue;
        }
        let (marker, label) =
            crate::vanilla_level_editor::secondary_entrance_marker_and_label_pixels(
                exit,
                mode.vertical,
                mode.alternate_layer_layout,
            );
        let entrance_action = exit.destination_flags & 7;
        let render_action = if entrance_action == 7 {
            4
        } else {
            entrance_action
        };
        let secondary_marker = if matches!(render_action, 1 | 2 | 3 | 4 | 5 | 6) {
            crate::vanilla_map16_preview::render_secondary_entrance_marker(
                &project,
                &preview.palette,
                render_action,
            )?
        } else {
            preview.entrance_image.clone()
        };
        blit_entrance_image(
            &mut canvas,
            &secondary_marker,
            i32::from(marker.0).saturating_sub(if render_action == 5 {
                14
            } else if render_action == 6 {
                13
            } else {
                0
            }),
            match render_action {
                3 => i32::from(marker.1.saturating_add(1)),
                4 => i32::from(marker.1).saturating_sub(5),
                6 => i32::from(marker.1.saturating_add(3)),
                _ => i32::from(marker.1.saturating_add(2)),
            },
            mode.vertical,
        );
        if entrance_action == 7 {
            draw_entrance_action_four_overlay(
                &mut canvas,
                &preview,
                i32::from(marker.0),
                i32::from(marker.1),
            );
        }
        if render_action == 5 {
            draw_entrance_action_five_overlays(
                &mut canvas,
                &preview,
                i32::from(marker.0),
                i32::from(marker.1),
            );
        }
        let label_text = format!("Secondary Entrance #{index:03X}");
        lm_render::draw_lunar_magic_editor_label(
            &mut canvas,
            &label_text,
            i32::from(label.0),
            i32::from(label.1),
        );
        secondary_labels.push((label_text, label));
    }
    let (entrance_x, entrance_y) = if mode.vertical {
        crate::vanilla_level_editor::vertical_primary_entrance_marker_pixels(
            entrance,
            mode.alternate_layer_layout,
        )
    } else {
        crate::vanilla_level_editor::horizontal_primary_entrance_marker_pixels(entrance)
    };
    let main_entrance_action = entrance.vertical_settings >> 3 & 7;
    // Vanilla action 7 is normalized to action 4 when no custom entrance tile list is installed.
    let main_render_action = if main_entrance_action == 7 {
        4
    } else {
        main_entrance_action
    };
    let main_entrance_marker = if matches!(main_render_action, 1 | 2 | 3 | 4 | 5 | 6) {
        crate::vanilla_map16_preview::render_entrance_marker(
            &project,
            &preview.palette,
            main_render_action,
        )?
    } else {
        preview.entrance_image.clone()
    };
    blit_entrance_image(
        &mut canvas,
        &main_entrance_marker,
        i32::from(entrance_x).saturating_sub(if main_render_action == 5 {
            14
        } else if main_render_action == 6 {
            13
        } else {
            0
        }),
        match main_render_action {
            3 => i32::from(entrance_y.saturating_add(1)),
            4 => i32::from(entrance_y).saturating_sub(5),
            6 => i32::from(entrance_y.saturating_add(3)),
            _ => i32::from(entrance_y.saturating_add(2)),
        },
        mode.vertical,
    );
    if main_entrance_action == 7 {
        draw_entrance_action_four_overlay(
            &mut canvas,
            &preview,
            i32::from(entrance_x),
            i32::from(entrance_y),
        );
    }
    if main_render_action == 5 {
        draw_entrance_action_five_overlays(
            &mut canvas,
            &preview,
            i32::from(entrance_x),
            i32::from(entrance_y),
        );
    }
    let (label_x, label_y) = if mode.vertical {
        crate::vanilla_level_editor::vertical_primary_entrance_label_pixels(
            entrance,
            mode.alternate_layer_layout,
        )
    } else {
        crate::vanilla_level_editor::horizontal_primary_entrance_label_pixels(entrance)
    };
    let midway_marker = crate::vanilla_level_editor::midway_entrance_marker_pixels(
        entrance,
        mode.vertical,
        mode.alternate_layer_layout,
    );
    // Lunar Magic decides whether to collapse the vanilla midway node before translating the
    // entrance settings through the horizontal/vertical coordinate tables. Comparing raster
    // positions is wrong for vertical modes whose primary Y page is normalized during loading.
    let entrances_overlap =
        entrance.level_mode_and_screen & 0x1f == entrance.screen_and_method >> 4;
    if std::env::var_os("LM_PRISTINE_TRACE_SPRITES").is_some() {
        eprintln!(
            "entrance raw={entrance:?} primary=({entrance_x},{entrance_y}) label=({label_x},{label_y}) midway=({},{}) overlap={entrances_overlap}",
            midway_marker.0, midway_marker.1,
        );
    }
    lm_render::draw_lunar_magic_editor_label(
        &mut canvas,
        &format!(
            "{}Entrance to level {level_number:02X}",
            if entrances_overlap { ">" } else { "" }
        ),
        i32::from(label_x),
        i32::from(label_y),
    );
    if !entrances_overlap {
        // Vanilla action 7 normalizes to the action-4 pose only for the primary entrance. The
        // separately rendered midway node remains the default pose and receives no `$11A`
        // overlay.
        let midway_render_action = if main_entrance_action == 7 {
            0
        } else {
            main_render_action
        };
        let midway_entrance_marker = if main_entrance_action == 7 {
            &preview.entrance_image
        } else {
            &main_entrance_marker
        };
        let midway_target_x =
            i32::from(midway_marker.0).saturating_sub(if midway_render_action == 5 {
                14
            } else if midway_render_action == 6 {
                13
            } else {
                0
            });
        let midway_target_y = match midway_render_action {
            3 => i32::from(midway_marker.1.saturating_add(1)),
            4 => i32::from(midway_marker.1).saturating_sub(5),
            6 => i32::from(midway_marker.1.saturating_add(3)),
            _ => i32::from(midway_marker.1.saturating_add(2)),
        };
        blit_entrance_image(
            &mut canvas,
            midway_entrance_marker,
            midway_target_x,
            midway_target_y,
            mode.vertical,
        );
        if main_entrance_action == 7 {
            draw_entrance_action_four_sparkle(
                &mut canvas,
                i32::from(midway_marker.0),
                i32::from(midway_marker.1),
            );
        }
        if midway_render_action == 5 {
            draw_entrance_action_five_overlays(
                &mut canvas,
                &preview,
                i32::from(midway_marker.0),
                i32::from(midway_marker.1),
            );
        }
        let midway_label = crate::vanilla_level_editor::midway_entrance_label_pixels(
            entrance,
            mode.vertical,
            mode.alternate_layer_layout,
        );
        lm_render::draw_lunar_magic_editor_label(
            &mut canvas,
            "Midway Entrance",
            i32::from(midway_label.0),
            i32::from(midway_label.1),
        );
        composite_secondary_labels_over_marker(
            &mut canvas,
            midway_entrance_marker,
            midway_target_x,
            midway_target_y,
            &secondary_labels,
        );
    }

    if mode.vertical && entrance.level_mode_and_screen & 0x20 == 0 {
        lm_render::draw_lunar_magic_editor_node_text_lines(
            &mut canvas,
            &["Warning:Turn on vertical", "entrance positioning!!  "],
            32,
            i32::from(entrance_y.saturating_add(20)),
        );
    } else if !mode.vertical && entrance.level_mode_and_screen & 0x20 != 0 {
        lm_render::draw_lunar_magic_editor_node_text_lines(
            &mut canvas,
            &["Warning:Turn off vertical", "entrance positioning!!   "],
            32,
            i32::from(entrance_y.saturating_add(20)),
        );
    }

    Ok(Some(PristineFullLevelRender {
        canvas,
        screens,
        vertical: mode.vertical,
    }))
}

fn referenced_secondary_exit_slots(project: &lm_project::Project) -> Result<Vec<bool>, String> {
    let mut referenced = vec![false; 0x2000];
    for level_number in 0..0x200 {
        let level = project
            .load_level_slot(
                level_number,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .map_err(|error| error.to_string())?;
        for record in &level.layer1.objects.records {
            let Some(exit) = record.screen_exit() else {
                continue;
            };
            // Native screen exits set bit $0200 when the remaining nine destination bits name a
            // secondary-exit slot. Lunar Magic only materializes those referenced vanilla slots
            // into its working table; stale, nonzero ROM records are consequently absent from the
            // full-level export even though a raw six-plane decode can still see them.
            let index = usize::from(
                exit.destination_and_flags & 0x00ff | (exit.destination_and_flags & 0x0200) >> 1,
            );
            referenced[index] = true;
        }
    }
    Ok(referenced)
}

fn pristine_secondary_destination(index: usize, exit: lm_level::SecondaryExit) -> u16 {
    // The original 512-slot format derives the destination-level high bit from the secondary
    // slot's high half when Lunar Magic expands the ROM tables into its in-memory six-plane form.
    // Stale raw records do not consistently retain plane-3 bit 3, so decoding that plane alone
    // misroutes $1xx slots to the corresponding $0xx level.
    if index < 0x200 {
        exit.destination_level & 0x00ff | u16::try_from(index & 0x100).unwrap_or(0)
    } else {
        exit.destination_level
    }
}

fn draw_object_stream(
    canvas: &mut Canvas,
    stream: &lm_level::ObjectStream,
    atlases: &[egui::ColorImage],
    phase: usize,
    vertical: bool,
    object_tileset: u8,
    map: &lm_profile::SmwUsV1StandardObjectDefinitionMap,
) -> Result<(), String> {
    let family = match lm_profile::smw_us_v1_object_family(object_tileset) {
        lm_profile::VanillaObjectFamily::Normal => 0,
        lm_profile::VanillaObjectFamily::Castle => 1,
        lm_profile::VanillaObjectFamily::Rope => 2,
        lm_profile::VanillaObjectFamily::Underground => 3,
        lm_profile::VanillaObjectFamily::GhostHouse => 4,
    };
    let mut definitions = lm_render::StandardObjectDefinitionSet::empty();
    lm_render::install_lunar_magic_shared_extended_objects(&mut definitions)
        .map_err(|error| error.to_string())?;
    lm_render::install_lunar_magic_tileset_extended_objects(&mut definitions, object_tileset)
        .map_err(|error| error.to_string())?;
    lm_render::install_lunar_magic_shared_standard_objects(&mut definitions)
        .map_err(|error| error.to_string())?;
    let layout = NativeLevelMap16Layout {
        width: canvas.width() / 16,
        height: canvas.height() / 16,
        page_stride: 0x1b0,
        base_cell: 0,
        vertical,
    };
    let report = lm_render::render_mapped_standard_object_stream(
        stream,
        &definitions,
        map.family(family).ok_or("missing vanilla object family")?,
        layout,
        0x0025,
    )
    .map_err(|error| error.to_string())?;
    if !report.missing_commands.is_empty() || !report.missing_extended_objects.is_empty() {
        return Err("full-level object stream contains unresolved handlers".into());
    }
    for y in 0..layout.height {
        for x in 0..layout.width {
            let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
            if !report.cache.was_written(index) {
                continue;
            }
            let tile = report.cache.cells()[index];
            if std::env::var_os("LM_PRISTINE_TRACE_MAP16").is_some() {
                let record = report
                    .painted_cells
                    .iter()
                    .rev()
                    .find(|cell| cell.index == index)
                    .map(|cell| cell.record_index);
                let command = record.map(|record| stream.records[record].command_id());
                let handler = command.and_then(|command| map.definition(family, command));
                eprintln!(
                    "map16 x={x:03X} y={y:02X} tile={tile:04X} record={record:?} command={command:?} handler={handler:?}"
                );
            }
            let major = if vertical { y } else { x };
            let variant = (major >> 4) & 3;
            let Some(atlas) = atlases.get(phase * 4 + variant) else {
                return Err("missing animated Map16 atlas".into());
            };
            // `RenderMap16TileToPixelBuffer` replaces the otherwise invisible editor objects
            // `$21-$24` with visible translucent cells whenever object visualization is enabled.
            // `$27-$2A` retain their native index but use the same half-color compositor.
            let (display_tile, half) = lunar_magic_editor_map16_cell(tile);
            blit_map16(
                canvas,
                atlas,
                display_tile,
                x * 16,
                y * 16,
                false,
                false,
                half,
            );
            draw_lunar_magic_numbered_one_up_marker(canvas, tile, x * 16, y * 16);
        }
    }
    Ok(())
}

const fn lunar_magic_editor_map16_cell(tile: u16) -> (u16, bool) {
    match tile {
        0x21 | 0x22 => (0x114, true),
        0x23 => (0x113, true),
        0x24 => (0x115, true),
        0x27..=0x2a => (tile, true),
        _ => (tile, false),
    }
}

// Lunar Magic substitutes these four otherwise transparent vanilla Map16 cells with
// translucent editor-only 1-Up markers.  The six-color source pixels and packed-channel
// averaging below were recovered from two pristine Level $012 placements per marker over
// different backgrounds; both placements resolve to the same source image.
const NUMBERED_ONE_UP_MARKERS: [[[u8; 16]; 16]; 4] = [
    marker_rows([
        ".....KKKKKK.....",
        "...KKKKRRKWKK...",
        "..KWKRRRRKWWWK..",
        ".KWWmKKRRKWWWWK.",
        ".KWmgWKRRKgWWWK.",
        "KdmgWWKRRKgggmdK",
        "KdmgWWKRRKggWWdK",
        "KWmgWWKRRKgWWWWK",
        "KWWmmWKRRKmWWWWK",
        "KWWdddKRRKddWWdK",
        "KWddKKKKKKKKdddK",
        ".KKKWWKWWKWWKKK.",
        "..KWWWKWWKWWWK..",
        "..KWWWWWWWWWWK..",
        "...KWWWWWWWWK...",
        "....KKKKKKKK....",
    ]),
    marker_rows([
        ".....KKKKKK.....",
        "...KKKRRRRKKK...",
        "..KWKRRKKRRKWK..",
        ".KWWmKKgKRRKWWK.",
        ".KWmgWWWKRRKWWK.",
        "KdmgWWWKRRKggmdK",
        "KdmgWWKRRKggWWdK",
        "KWmgWKRRKWgWWWWK",
        "KWWmKRRKKKKWWWWK",
        "KWWdKRRRRRRKWWdK",
        "KWddKKKKKKKKdddK",
        ".KKKWWKWWKWWKKK.",
        "..KWWWKWWKWWWK..",
        "..KWWWWWWWWWWK..",
        "...KWWWWWWWWK...",
        "....KKKKKKKK....",
    ]),
    marker_rows([
        ".....KKKKKK.....",
        "...KKKRRRRKKK...",
        "..KWKRRKKRRKWK..",
        ".KWWmKKgKRRKWWK.",
        ".KWmgWWKKRRKWWK.",
        "KdmgWWKRRRKggmdK",
        "KdmgWWWKKRRKWWdK",
        "KWmgWKKWKRRKWWWK",
        "KWWmKRRKKRRKWWWK",
        "KWWddKRRRRKdWWdK",
        "KWddKKKKKKKKdddK",
        ".KKKWWKWWKWWKKK.",
        "..KWWWKWWKWWWK..",
        "..KWWWWWWWWWWK..",
        "...KWWWWWWWWK...",
        "....KKKKKKKK....",
    ]),
    marker_rows([
        ".....KKKKKK.....",
        "...KKdKRRKWKK...",
        "..KWWKRRRKWWWK..",
        ".KWWmKRRRKWWWWK.",
        ".KWmKRRRRKgWWWK.",
        "KdmgKRRRRKgggmdK",
        "KdmKRRKRRKggWWdK",
        "KWmKRRRRRRKWWWWK",
        "KWWmKKKRRKmWWWWK",
        "KWWdddKRRKddWWdK",
        "KWddKKKKKKKKdddK",
        ".KKKWWKWWKWWKKK.",
        "..KWWWKWWKWWWK..",
        "..KWWWWWWWWWWK..",
        "...KWWWWWWWWK...",
        "....KKKKKKKK....",
    ]),
];

const fn marker_rows(rows: [&str; 16]) -> [[u8; 16]; 16] {
    let mut result = [[0; 16]; 16];
    let mut y = 0;
    while y < 16 {
        let bytes = rows[y].as_bytes();
        let mut x = 0;
        while x < 16 {
            result[y][x] = bytes[x];
            x += 1;
        }
        y += 1;
    }
    result
}

fn draw_lunar_magic_numbered_one_up_marker(
    canvas: &mut Canvas,
    tile: u16,
    target_x: usize,
    target_y: usize,
) {
    let Some(marker) = tile
        .checked_sub(0x6f)
        .filter(|index| *index < 4)
        .map(|index| &NUMBERED_ONE_UP_MARKERS[usize::from(index)])
    else {
        return;
    };
    for (y, row) in marker.iter().enumerate() {
        for (x, code) in row.iter().copied().enumerate() {
            let source = match code {
                b'K' => Rgba {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                },
                b'd' => Rgba {
                    red: 0,
                    green: 120,
                    blue: 0,
                    alpha: 255,
                },
                b'm' => Rgba {
                    red: 0,
                    green: 184,
                    blue: 0,
                    alpha: 255,
                },
                b'g' => Rgba {
                    red: 0,
                    green: 248,
                    blue: 0,
                    alpha: 255,
                },
                b'W' => Rgba {
                    red: 248,
                    green: 248,
                    blue: 248,
                    alpha: 255,
                },
                b'R' => Rgba {
                    red: 254,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                },
                _ => continue,
            };
            let tx = target_x + x;
            let ty = target_y + y;
            let Some(destination) = canvas.get(tx, ty) else {
                continue;
            };
            canvas.set(
                tx,
                ty,
                Rgba {
                    red: (source.red & 0xfe) / 2 + (destination.red & 0xfe) / 2,
                    green: (source.green & 0xfe) / 2 + (destination.green & 0xfe) / 2,
                    blue: (source.blue & 0xfe) / 2 + (destination.blue & 0xfe) / 2,
                    alpha: 255,
                },
            );
        }
    }
}

fn draw_layer2_tilemap(
    canvas: &mut Canvas,
    atlas: &egui::ColorImage,
    words: &[u16],
    columns: usize,
    rows: usize,
) {
    for y in 0..rows {
        for x in 0..columns {
            let Some(index) = lm_level::native_layer2_tilemap_index(x % 32, y % 32) else {
                continue;
            };
            let Some(&word) = words.get(index) else {
                continue;
            };
            blit_map16(
                canvas,
                atlas,
                word & 0x3fff,
                x * 16,
                y * 16,
                word & 0x4000 != 0,
                word & 0x8000 != 0,
                false,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_sprites(
    canvas: &mut Canvas,
    placements: &[lm_level::NativeSpritePlacement],
    ordinary_tiles: &[lm_graphics::IndexedTile],
    animated_tiles: &[lm_graphics::IndexedTile],
    palette: &lm_graphics::Palette,
    vertical: bool,
    level_mode: u8,
    sprite_tileset: u8,
    sprite_memory_index: u8,
    phase: u8,
) {
    let mut sequence_8a = 0_u8;
    for placement in placements {
        let mode = crate::vanilla_level_editor::standard_sprite_preview_mode(
            placement,
            vertical,
            level_mode,
            sprite_tileset,
            sprite_memory_index,
            phase,
            sequence_8a,
        );
        if placement.sprite_number == 0x8a {
            sequence_8a = sequence_8a.saturating_add(1);
        }
        let Some(parts) =
            lm_render::render_lunar_magic_standard_sprite_with_mode(placement.sprite_number, mode)
        else {
            continue;
        };
        let (tile_x, tile_y) = if vertical {
            (placement.minor, placement.major)
        } else {
            (placement.major, placement.minor)
        };
        for part in parts {
            let mut x = i32::from(tile_x) * 16 + i32::from(part.x);
            let mut y = i32::from(tile_y) * 16 + i32::from(part.y);
            // Lunar Magic stores horizontal-level render cells screen-major. A
            // composite part whose origin runs below the full editor canvas is
            // therefore materialized at the top of the following 16-column
            // screen, rather than discarded below the bitmap. Level $0F8's
            // lower $E2 Boo-ring element authenticates this wrap at (1763,433)
            // -> (2019,1).
            if !vertical {
                (x, y) = wrap_horizontal_sprite_part(
                    x,
                    y,
                    i32::try_from(canvas.height()).unwrap_or(i32::MAX),
                );
            }
            if std::env::var_os("LM_PRISTINE_TRACE_SPRITES").is_some() {
                eprintln!(
                    "sprite-part sprite={:02X} definition={:03X} x={x} y={y}",
                    placement.sprite_number, part.definition_index
                );
            }
            lm_render::draw_native_sprite_preview_definition_pages_with_half_color(
                canvas,
                part.subtiles,
                ordinary_tiles,
                animated_tiles,
                palette,
                x,
                y,
                pristine_standard_sprite_half_color(
                    level_mode,
                    placement.sprite_number,
                    part.definition_index,
                ),
            );
            let canvas_height = i32::try_from(canvas.height()).unwrap_or(i32::MAX);
            if !vertical && y < canvas_height && y.saturating_add(16) > canvas_height {
                // AppendEditorRenderTileNode splits a 16x16 definition whose lower edge crosses
                // the horizontal editor canvas. The overflow continues at the top of the next
                // screen column even though the definition origin itself has not wrapped.
                lm_render::draw_native_sprite_preview_definition_pages_with_half_color(
                    canvas,
                    part.subtiles,
                    ordinary_tiles,
                    animated_tiles,
                    palette,
                    x.saturating_add(16 * 16),
                    y.saturating_sub(canvas_height),
                    pristine_standard_sprite_half_color(
                        level_mode,
                        placement.sprite_number,
                        part.definition_index,
                    ),
                );
            }
        }
    }
}

fn wrap_horizontal_sprite_part(mut x: i32, mut y: i32, canvas_height: i32) -> (i32, i32) {
    if canvas_height <= 0 {
        return (x, y);
    }
    while y >= canvas_height {
        y -= canvas_height;
        x += 16 * 16;
    }
    while y < 0 {
        y += canvas_height;
        x -= 16 * 16;
    }
    (x, y)
}

fn draw_entrance_action_four_overlay(
    canvas: &mut Canvas,
    preview: &crate::vanilla_map16_preview::VanillaMap16Preview,
    entrance_x: i32,
    entrance_y: i32,
) {
    // RenderConfiguredLevelEntrance normalizes vanilla action 7 to action 4, then emits editor
    // definition $11A at (+10,-5). `$019` quadrants are transparent in the ordinary sprite cache;
    // `$01C` supplies the small white sparkle visible beside the climbing entrance pose.
    lm_render::draw_native_sprite_preview_definition_pages_with_half_color(
        canvas,
        ENTRANCE_ACTION_FOUR_OVERLAY_SUBTILES,
        &preview.sprite_tiles,
        &preview.animated_sprite_tiles,
        &preview.palette,
        entrance_x + ENTRANCE_ACTION_FOUR_OVERLAY_OFFSET.0,
        entrance_y + ENTRANCE_ACTION_FOUR_OVERLAY_OFFSET.1,
        false,
    );
}

const ENTRANCE_ACTION_FOUR_OVERLAY_SUBTILES: [u16; 4] = [0x0819, 0x0819, 0x081c, 0x0819];
const ENTRANCE_ACTION_FOUR_OVERLAY_OFFSET: (i32, i32) = (10, -5);

fn draw_entrance_action_four_sparkle(canvas: &mut Canvas, entrance_x: i32, entrance_y: i32) {
    // `$01C` contributes only this eight-pixel white diamond after native transparency and the
    // midway action-7 pose normalization.
    const DIAMOND: &[(i32, i32)] = &[
        (1, 0),
        (2, 0),
        (0, 1),
        (3, 1),
        (0, 2),
        (3, 2),
        (1, 3),
        (2, 3),
    ];
    let origin_x = entrance_x + 12;
    let origin_y = entrance_y + 2;
    for &(x, y) in DIAMOND {
        let Some(x) = usize::try_from(origin_x + x).ok() else {
            continue;
        };
        let Some(y) = usize::try_from(origin_y + y).ok() else {
            continue;
        };
        canvas.set(
            x,
            y,
            lm_render::Rgba {
                red: 0xff,
                green: 0xff,
                blue: 0xff,
                alpha: 0xff,
            },
        );
    }
}

fn draw_entrance_action_five_overlays(
    canvas: &mut Canvas,
    preview: &crate::vanilla_map16_preview::VanillaMap16Preview,
    entrance_x: i32,
    entrance_y: i32,
) {
    for &(subtiles, (x, y)) in &ENTRANCE_ACTION_FIVE_OVERLAYS {
        lm_render::draw_native_sprite_preview_definition_pages_with_half_color(
            canvas,
            subtiles,
            &preview.sprite_tiles,
            &preview.animated_sprite_tiles,
            &preview.palette,
            entrance_x + x,
            entrance_y + y,
            false,
        );
    }
}

const ENTRANCE_ACTION_FIVE_OVERLAYS: [([u16; 4], (i32, i32)); 2] = [
    ([0x4019, 0x4019, 0x400c, 0x4019], (-14, 18)),
    ([0x4019, 0x4019, 0x4019, 0x2c5c], (2, 20)),
];

const fn pristine_standard_sprite_half_color(
    level_mode: u8,
    sprite_number: u8,
    definition_index: u16,
) -> bool {
    (level_mode == 0x0c && matches!(sprite_number, 0x38..=0x39))
        || (level_mode == 0x0c && definition_index == 0x1b8)
        || sprite_number == 0x90
        || definition_index & 0x8000 != 0
}

fn blit_map16(
    canvas: &mut Canvas,
    atlas: &egui::ColorImage,
    tile: u16,
    target_x: usize,
    target_y: usize,
    x_flip: bool,
    y_flip: bool,
    half: bool,
) {
    let tile = usize::from(tile);
    let source_x = tile % 32 * 16;
    let source_y = tile / 32 * 16;
    if source_x + 16 > atlas.size[0] || source_y + 16 > atlas.size[1] {
        return;
    }
    for y in 0..16 {
        for x in 0..16 {
            let sx = source_x + if x_flip { 15 - x } else { x };
            let sy = source_y + if y_flip { 15 - y } else { y };
            let source = atlas.pixels[sy * atlas.size[0] + sx];
            if source.a() == 0 {
                continue;
            }
            let tx = target_x + x;
            let ty = target_y + y;
            if tx >= canvas.width() || ty >= canvas.height() {
                continue;
            }
            let mut pixel = Rgba {
                red: source.r(),
                green: source.g(),
                blue: source.b(),
                alpha: 255,
            };
            if half {
                let destination = canvas.get(tx, ty).unwrap_or_default();
                pixel = Rgba {
                    red: (pixel.red & 0xfe) / 2 + (destination.red & 0xfe) / 2,
                    green: (pixel.green & 0xfe) / 2 + (destination.green & 0xfe) / 2,
                    blue: (pixel.blue & 0xfe) / 2 + (destination.blue & 0xfe) / 2,
                    alpha: 255,
                };
            }
            canvas.set(tx, ty, pixel);
        }
    }
}

fn draw_repeating_layer3(
    canvas: &mut Canvas,
    image: &egui::ColorImage,
    position: (i16, i16),
    vertical: bool,
) {
    let width = i32::try_from(canvas.width()).unwrap_or(i32::MAX);
    let height = i32::try_from(canvas.height()).unwrap_or(i32::MAX);
    let x_origins = repeating_origins(position.0, width);
    let y_origins = if vertical {
        repeating_origins(position.1, height)
    } else {
        vec![-i32::from(position.1)]
    };
    for y in y_origins {
        for &x in &x_origins {
            blit_image(canvas, image, x, y);
        }
    }
}

fn repeating_origins(position: i16, extent: i32) -> Vec<i32> {
    let mut origin = -i32::from(position);
    while origin > 0 {
        origin -= 512;
    }
    while origin + 512 <= 0 {
        origin += 512;
    }
    let mut origins = Vec::new();
    while origin < extent {
        origins.push(origin);
        origin += 512;
    }
    origins
}

fn blit_image(canvas: &mut Canvas, image: &egui::ColorImage, target_x: i32, target_y: i32) {
    for y in 0..image.size[1] {
        for x in 0..image.size[0] {
            let tx = target_x.saturating_add(i32::try_from(x).unwrap_or(i32::MAX));
            let ty = target_y.saturating_add(i32::try_from(y).unwrap_or(i32::MAX));
            let (Ok(tx), Ok(ty)) = (usize::try_from(tx), usize::try_from(ty)) else {
                continue;
            };
            if tx >= canvas.width() || ty >= canvas.height() {
                continue;
            }
            let source = image.pixels[y * image.size[0] + x];
            let additive =
                source.a() == 0 && (source.r() != 0 || source.g() != 0 || source.b() != 0);
            if source.a() == 0 && !additive {
                continue;
            }
            let source = Rgba {
                red: source.r(),
                green: source.g(),
                blue: source.b(),
                alpha: 255,
            };
            let output = if additive {
                let destination = canvas.get(tx, ty).unwrap_or_default();
                Rgba {
                    red: destination.red.saturating_add(source.red),
                    green: destination.green.saturating_add(source.green),
                    blue: destination.blue.saturating_add(source.blue),
                    alpha: 255,
                }
            } else {
                source
            };
            canvas.set(tx, ty, output);
        }
    }
}

fn blit_entrance_image(
    canvas: &mut Canvas,
    image: &egui::ColorImage,
    target_x: i32,
    target_y: i32,
    vertical: bool,
) {
    blit_image(canvas, image, target_x, target_y);
    let canvas_height = i32::try_from(canvas.height()).unwrap_or(i32::MAX);
    let image_height = i32::try_from(image.size[1]).unwrap_or(i32::MAX);
    if !vertical
        && target_y < canvas_height
        && target_y.saturating_add(image_height) > canvas_height
    {
        blit_image(
            canvas,
            image,
            target_x.saturating_add(16 * 16),
            target_y.saturating_sub(canvas_height),
        );
    }
}

fn composite_secondary_labels_over_marker(
    canvas: &mut Canvas,
    marker: &egui::ColorImage,
    target_x: i32,
    target_y: i32,
    labels: &[(String, (u16, u16))],
) {
    if labels.is_empty() {
        return;
    }
    let mut composited = canvas.clone();
    for (text, (x, y)) in labels {
        lm_render::draw_lunar_magic_editor_label(
            &mut composited,
            text,
            i32::from(*x),
            i32::from(*y),
        );
    }
    for y in 0..marker.size[1] {
        for x in 0..marker.size[0] {
            let source = marker.pixels[y * marker.size[0] + x];
            if source.a() == 0 && source.r() == 0 && source.g() == 0 && source.b() == 0 {
                continue;
            }
            let output_x = target_x.saturating_add(i32::try_from(x).unwrap_or(i32::MAX));
            let output_y = target_y.saturating_add(i32::try_from(y).unwrap_or(i32::MAX));
            let (Ok(output_x), Ok(output_y)) =
                (usize::try_from(output_x), usize::try_from(output_y))
            else {
                continue;
            };
            if let Some(pixel) = composited.get(output_x, output_y) {
                canvas.set(output_x, output_y, pixel);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pristine_level_105_remains_pixel_exact_with_the_lunar_magic_oracle() {
        let rendered = render(
            crate::test_support::pristine_smw_us_rom_bytes(),
            0x105,
            NATIVE_EXPORT_MAP16_PHASE,
            0,
        )
        .unwrap()
        .expect("pristine level 105 is renderable");
        let png = lm_render::encode_png(&rendered.canvas).unwrap();
        assert_eq!(
            lm_oracle::sha256_hex(&png),
            "42478d82ad450c2995f44e96a8b346090ee0bbffc8e31ff6f1593cc3c81e33fc"
        );
    }

    #[test]
    fn horizontal_sprite_parts_wrap_into_the_following_screen_column() {
        assert_eq!(wrap_horizontal_sprite_part(1763, 433, 432), (2019, 1));
        assert_eq!(wrap_horizontal_sprite_part(512, -1, 432), (256, 431));
        assert_eq!(wrap_horizontal_sprite_part(512, 200, 432), (512, 200));
    }

    #[test]
    fn entrance_action_four_uses_the_authenticated_definition_and_signed_offset() {
        assert_eq!(
            ENTRANCE_ACTION_FOUR_OVERLAY_SUBTILES,
            [0x0819, 0x0819, 0x081c, 0x0819]
        );
        assert_eq!(ENTRANCE_ACTION_FOUR_OVERLAY_OFFSET, (10, -5));
    }

    #[test]
    fn entrance_action_five_uses_live_sidecar_definitions_and_signed_offsets() {
        assert_eq!(
            ENTRANCE_ACTION_FIVE_OVERLAYS,
            [
                ([0x4019, 0x4019, 0x400c, 0x4019], (-14, 18)),
                ([0x4019, 0x4019, 0x4019, 0x2c5c], (2, 20)),
            ]
        );
    }

    #[test]
    fn boo_stream_translucency_is_scoped_to_dark_level_mode() {
        assert!(pristine_standard_sprite_half_color(0x0c, 0x38, 0x38));
        assert!(pristine_standard_sprite_half_color(0x0c, 0x39, 0x48));
        assert!(pristine_standard_sprite_half_color(0x0c, 0xe1, 0x1b8));
        assert!(!pristine_standard_sprite_half_color(0x00, 0x38, 0x38));
        assert!(!pristine_standard_sprite_half_color(0x0c, 0x37, 0x1f));
    }

    #[test]
    fn additive_layer3_pixels_are_composited_instead_of_discarded_as_transparent() {
        let mut canvas = Canvas::from_pixels(
            1,
            1,
            vec![Rgba {
                red: 240,
                green: 20,
                blue: 30,
                alpha: 255,
            }],
        )
        .unwrap();
        let image = egui::ColorImage::new([1, 1], egui::Color32::from_rgb_additive(30, 40, 50));

        blit_image(&mut canvas, &image, 0, 0);

        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 255,
                green: 60,
                blue: 80,
                alpha: 255,
            })
        );
    }

    #[test]
    fn editor_half_color_map16_range_is_not_tileset_scoped() {
        for tile in 0x27..=0x2a {
            assert_eq!(lunar_magic_editor_map16_cell(tile), (tile, true));
        }
        assert_eq!(lunar_magic_editor_map16_cell(0x26), (0x26, false));
        assert_eq!(lunar_magic_editor_map16_cell(0x2b), (0x2b, false));
    }

    #[test]
    fn invisible_editor_objects_use_lunar_magics_translucent_display_cells() {
        assert_eq!(lunar_magic_editor_map16_cell(0x21), (0x114, true));
        assert_eq!(lunar_magic_editor_map16_cell(0x22), (0x114, true));
        assert_eq!(lunar_magic_editor_map16_cell(0x23), (0x113, true));
        assert_eq!(lunar_magic_editor_map16_cell(0x24), (0x115, true));
    }

    #[test]
    fn numbered_one_up_map16_markers_are_translucent_and_have_authenticated_masks() {
        let background = Rgba {
            red: 201,
            green: 101,
            blue: 51,
            alpha: 255,
        };
        for tile in 0x6f..=0x72 {
            let mut canvas = Canvas::from_pixels(16, 16, vec![background; 16 * 16]).unwrap();
            draw_lunar_magic_numbered_one_up_marker(&mut canvas, tile, 0, 0);
            assert_eq!(
                canvas
                    .pixels()
                    .iter()
                    .filter(|pixel| **pixel != background)
                    .count(),
                208
            );
            assert_eq!(canvas.get(0, 0), Some(background));
            // All four marker masks have black at this location. Lunar Magic averages packed
            // even-valued channels instead of using a conventional alpha blend.
            assert_eq!(
                canvas.get(5, 0),
                Some(Rgba {
                    red: 100,
                    green: 50,
                    blue: 25,
                    alpha: 255,
                })
            );
        }
    }

    #[test]
    fn pristine_secondary_overlay_slots_follow_live_screen_exit_references() {
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let project = lm_project::Project::new(image);
        let referenced = referenced_secondary_exit_slots(&project).unwrap();

        // Bit $0200 in a screen-exit destination becomes bit $0100 of the secondary slot index.
        assert!(referenced[0x0bf]);
        assert!(referenced[0x1be]);
        assert!(referenced[0x1c0]);
        assert!(referenced[0x1c1]);

        let secondary = project
            .load_secondary_exit_table_detected(lm_profile::smw_us_v1_secondary_exit_locator())
            .unwrap();
        assert_eq!(
            pristine_secondary_destination(0x1be, secondary.table.entries[0x1be]),
            0x102
        );
        assert_eq!(
            pristine_secondary_destination(0x1c0, secondary.table.entries[0x1c0]),
            0x117
        );
        assert_eq!(
            pristine_secondary_destination(0x1c1, secondary.table.entries[0x1c1]),
            0x11f
        );
    }

    #[test]
    fn pristine_full_render_matches_native_dimensions_and_empty_outcomes() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let horizontal = render(bytes.clone(), 0x001, 0, 0).unwrap().unwrap();
        assert_eq!(horizontal.screens, 20);
        assert!(!horizontal.vertical);
        assert_eq!(
            (horizontal.canvas.width(), horizontal.canvas.height()),
            (5120, 432)
        );

        let vertical = render(bytes.clone(), 0x0f7, 0, 0).unwrap().unwrap();
        assert_eq!(vertical.screens, 26);
        assert!(vertical.vertical);
        assert_eq!(
            (vertical.canvas.width(), vertical.canvas.height()),
            (512, 6656)
        );

        assert!(render(bytes, 0x095, 0, 0).unwrap().is_none());
    }

    #[test]
    fn diagnostic_export_pristine_full_render_corpus_when_requested() {
        use std::fmt::Write as _;

        let Some(output) = std::env::var_os("LM_PRISTINE_FULL_RENDER_DIR") else {
            return;
        };
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&output).unwrap();
        let bytes = std::sync::Arc::new(crate::test_support::pristine_smw_us_rom_bytes());
        let mut manifest = String::from("slot\tstatus\tvertical\tscreens\twidth\theight\tpng\n");
        let mut rows = std::thread::scope(|scope| {
            let handles = (0_u16..8)
                .map(|worker| {
                    let bytes = bytes.clone();
                    let output = output.clone();
                    scope.spawn(move || {
                        let mut rows = Vec::new();
                        for level in (worker..0x200).step_by(8) {
                            let row = match render(
                                bytes.as_ref().clone(),
                                level,
                                NATIVE_EXPORT_MAP16_PHASE,
                                0,
                            )
                            .unwrap_or_else(|error| panic!("level {level:03X}: {error}"))
                            {
                                Some(rendered_level) => {
                                    let name = format!("Level {level:03X}.png");
                                    let png =
                                        lm_render::encode_png(&rendered_level.canvas).unwrap();
                                    std::fs::write(output.join(&name), png).unwrap();
                                    format!(
                                        "{level:03X}\trendered\t{}\t{}\t{}\t{}\t{name}\n",
                                        u8::from(rendered_level.vertical),
                                        rendered_level.screens,
                                        rendered_level.canvas.width(),
                                        rendered_level.canvas.height(),
                                    )
                                }
                                None => format!("{level:03X}\tnative-non-renderable\t\t\t\t\t\n"),
                            };
                            rows.push((level, row));
                        }
                        rows
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        rows.sort_by_key(|(level, _)| *level);
        let rendered = rows
            .iter()
            .filter(|(_, row)| row.contains("\trendered\t"))
            .count();
        let non_renderable = rows.len() - rendered;
        for (_, row) in rows {
            manifest.write_str(&row).unwrap();
        }
        assert_eq!((rendered, non_renderable), (488, 24));
        std::fs::write(output.join("manifest.tsv"), manifest).unwrap();
    }

    #[test]
    fn diagnostic_export_one_pristine_full_render_when_requested() {
        let Ok(specification) = std::env::var("LM_PRISTINE_FULL_RENDER_ONE") else {
            return;
        };
        let (level, output) = specification
            .split_once(':')
            .expect("LM_PRISTINE_FULL_RENDER_ONE must be HEX_LEVEL:OUTPUT.png");
        let level = u16::from_str_radix(level, 16).expect("full-render level must be hexadecimal");
        let rendered = render(
            crate::test_support::pristine_smw_us_rom_bytes(),
            level,
            NATIVE_EXPORT_MAP16_PHASE,
            0,
        )
        .unwrap()
        .expect("requested level must be natively renderable");
        std::fs::write(output, lm_render::encode_png(&rendered.canvas).unwrap()).unwrap();
    }

    #[test]
    fn diagnostic_export_pristine_sprite_pages_when_requested() {
        let Ok(specification) = std::env::var("LM_PRISTINE_SPRITE_PAGES") else {
            return;
        };
        let (level_number, output) = specification
            .split_once(':')
            .expect("LM_PRISTINE_SPRITE_PAGES must be HEX_LEVEL:OUTPUT.png");
        let level_number =
            u16::from_str_radix(level_number, 16).expect("sprite-page level must be hexadecimal");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let project = lm_project::Project::new(image);
        let level = project
            .load_level_slot(
                usize::from(level_number),
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let preview = crate::vanilla_map16_preview::render(
            bytes,
            level_number,
            level.layer1.header,
            false,
            false,
        )
        .unwrap();
        let atlas = &preview.sprite_image;
        let mut canvas = Canvas::try_new(atlas.size[0], atlas.size[1]).unwrap();
        blit_image(&mut canvas, atlas, 0, 0);
        std::fs::write(output, lm_render::encode_png(&canvas).unwrap()).unwrap();
    }

    #[test]
    fn diagnostic_export_pristine_full_render_phases_when_requested() {
        let Ok(specification) = std::env::var("LM_PRISTINE_FULL_RENDER_PHASES") else {
            return;
        };
        let (level, output) = specification
            .split_once(':')
            .expect("LM_PRISTINE_FULL_RENDER_PHASES must be HEX_LEVEL:OUTPUT_DIR");
        let level = u16::from_str_radix(level, 16).expect("full-render level must be hexadecimal");
        let output = std::path::Path::new(output);
        std::fs::create_dir_all(output).unwrap();
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        for phase in 0..8 {
            let rendered = render(
                bytes.clone(),
                level,
                phase,
                u8::try_from(phase / 2).unwrap(),
            )
            .unwrap()
            .expect("requested level must be natively renderable");
            std::fs::write(
                output.join(format!("Level {level:03X} phase {phase}.png")),
                lm_render::encode_png(&rendered.canvas).unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    fn diagnostic_export_pristine_full_render_sprite_phases_when_requested() {
        let Ok(specification) = std::env::var("LM_PRISTINE_FULL_RENDER_SPRITE_PHASES") else {
            return;
        };
        let (level, output) = specification
            .split_once(':')
            .expect("LM_PRISTINE_FULL_RENDER_SPRITE_PHASES must be HEX_LEVEL:OUTPUT_DIR");
        let level = u16::from_str_radix(level, 16).expect("full-render level must be hexadecimal");
        let output = std::path::Path::new(output);
        std::fs::create_dir_all(output).unwrap();
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        for phase in 0..4 {
            let rendered = render(bytes.clone(), level, NATIVE_EXPORT_MAP16_PHASE, phase)
                .unwrap()
                .expect("requested level must be natively renderable");
            std::fs::write(
                output.join(format!("Level {level:03X} sprite phase {phase}.png")),
                lm_render::encode_png(&rendered.canvas).unwrap(),
            )
            .unwrap();
        }
    }
}
