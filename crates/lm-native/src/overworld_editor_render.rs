use eframe::egui;
use lm_graphics::GraphicsInterchangeFile;
use lm_level::Map16SetFile;
use lm_project::CompleteOverworldFile;

pub(crate) struct OverworldAssets {
    pub(crate) map16: Map16SetFile,
    pub(crate) graphics: GraphicsInterchangeFile,
    pub(crate) native_sprite_graphics_cache: Vec<lm_graphics::IndexedTile>,
    pub(crate) external_sprite_assets: lm_graphics::ExternalSpriteAssets,
    pub(crate) gfx32: Vec<lm_graphics::IndexedTile>,
    pub(crate) gfx33: Vec<lm_graphics::IndexedTile>,
    /// Lunar Magic's three built-in overworld seeds followed by eight frames for eight groups.
    pub(crate) built_in_animation_addresses: Vec<u16>,
    /// The two eight-color vanilla cycles copied to CGRAM $6D and $7D.
    pub(crate) built_in_level_dot_palette: Option<[[lm_graphics::Bgr555; 8]; 2]>,
    /// Vanilla's deterministic lightning scheduler and its two selector tables.
    pub(crate) built_in_lightning: Option<BuiltInOverworldLightning>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuiltInOverworldLightning {
    pub(crate) selectors: [u8; 128],
    pub(crate) delays: [u8; 8],
    pub(crate) initial_colors: [u8; 8],
}

#[derive(Clone, Debug)]
pub(crate) struct OverworldExAnimationPreview {
    pub(crate) tick: usize,
    pub(crate) substeps_per_tick: usize,
    pub(crate) triggers: lm_graphics::ExAnimationTriggerPreviewState,
    pub(crate) events_passed: Vec<bool>,
}

pub(crate) fn render_layer_texture(
    context: &egui::Context,
    layer: &lm_overworld::OverworldLayer,
    palette: &lm_graphics::Palette,
    assets: &OverworldAssets,
) -> Result<egui::TextureHandle, String> {
    let canvas = lm_render::render_smw_overworld_layer2_tilemap(layer, &assets.graphics, palette)
        .map_err(|error| error.to_string())?;
    texture_from_canvas(context, "native-main-overworld-layer2", &canvas)
}

pub(crate) fn render_layer2_graphics_texture(
    context: &egui::Context,
    graphics: &GraphicsInterchangeFile,
    palette: &lm_graphics::Palette,
    palette_row: usize,
) -> Result<egui::TextureHandle, String> {
    let canvas = lm_render::render_portable_graphics(
        graphics,
        &lm_graphics::PaletteInterchangeFile {
            source_palette: 0,
            palette: palette.clone(),
        },
        palette_row,
        16,
    )
    .map_err(|error| error.to_string())?;
    texture_from_canvas(context, "native-overworld-layer2-8x8", &canvas)
}

fn texture_from_canvas(
    context: &egui::Context,
    name: &str,
    canvas: &lm_render::Canvas,
) -> Result<egui::TextureHandle, String> {
    let capacity = canvas
        .pixels()
        .len()
        .checked_mul(4)
        .ok_or("overworld texture byte count overflow")?;
    let mut rgba = Vec::with_capacity(capacity);
    for pixel in canvas.pixels() {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([canvas.width(), canvas.height()], &rgba);
    Ok(context.load_texture(name, image, egui::TextureOptions::NEAREST))
}

pub(crate) fn render_texture(
    context: &egui::Context,
    overworld: &CompleteOverworldFile,
    assets: &OverworldAssets,
    native_appearances: Option<&lm_render::NativeOverworldAppearancePair>,
    completed_reveals: usize,
) -> Result<egui::TextureHandle, String> {
    render_texture_with_preview(
        context,
        overworld,
        assets,
        native_appearances,
        completed_reveals,
        None,
    )
}

pub(crate) fn render_texture_with_preview(
    context: &egui::Context,
    overworld: &CompleteOverworldFile,
    assets: &OverworldAssets,
    native_appearances: Option<&lm_render::NativeOverworldAppearancePair>,
    completed_reveals: usize,
    preview: Option<&OverworldExAnimationPreview>,
) -> Result<egui::TextureHandle, String> {
    let (graphics, palette) = if let Some(preview) = preview {
        materialize_overworld_exanimation(overworld, assets, preview)?
    } else {
        (assets.graphics.clone(), overworld.data.palette.clone())
    };
    let mut rendered = overworld.clone();
    rendered.data.palette = palette;
    let mut canvas = lm_render::render_portable_overworld(
        &rendered,
        &assets.map16,
        &graphics,
        None,
        None,
        completed_reveals,
    )
    .map_err(|error| error.to_string())?;
    if let Some(native) = native_appearances {
        let placements = overworld
            .data
            .sprites
            .iter()
            .map(|sprite| lm_render::NativeOverworldSpritePlacement {
                id: sprite.id,
                x: i32::from(sprite.x),
                y: i32::from(sprite.y),
                submap: sprite.submap.encoded(),
            })
            .collect::<Vec<_>>();
        let elements = lm_render::resolve_native_overworld_sprite_elements(
            &placements,
            &native.definitions,
            lm_render::lunar_magic_builtin_overworld_sprite_map16(),
            &native.sprite_map16,
        );
        lm_render::draw_resolved_native_overworld_sprite_resource_elements(
            &mut canvas,
            &elements,
            &assets.native_sprite_graphics_cache,
            &rendered.data.palette,
            &assets.external_sprite_assets,
        );
    }
    let mut rgba = Vec::with_capacity(canvas.pixels().len() * 4);
    for pixel in canvas.pixels() {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([canvas.width(), canvas.height()], &rgba);
    Ok(context.load_texture("portable-overworld", image, egui::TextureOptions::NEAREST))
}

fn materialize_overworld_exanimation(
    overworld: &CompleteOverworldFile,
    assets: &OverworldAssets,
    preview: &OverworldExAnimationPreview,
) -> Result<(GraphicsInterchangeFile, lm_graphics::Palette), String> {
    const CACHE_TILES: usize = 0x1700;
    const GFX33_CACHE_BASE: usize = 0x600;
    const GFX33_DECODED_BIAS: usize = 0x18;
    const AN2_CACHE_BASE: usize = 0x780;
    const GFX32_CACHE_BASE: usize = 0x900;
    const AN2_NATIVE_BASE: usize = 0x2a00;
    const AN2_NATIVE_STRIDE: usize = 0x100;
    const RELATIVE_BASES: [u32; 4] = [0x0c00, 0x1000, 0x1400, 0x1800];

    let blank = lm_graphics::IndexedTile::new([0; lm_graphics::IndexedTile::PIXEL_COUNT]);
    let mut cache = vec![blank; CACHE_TILES];
    copy_preview_tiles(&mut cache, 0, &assets.graphics.graphics.tiles)?;
    if assets.gfx33.len() > GFX33_DECODED_BIAS {
        copy_preview_tiles(
            &mut cache,
            GFX33_CACHE_BASE,
            &assets.gfx33[GFX33_DECODED_BIAS..],
        )?;
    }
    copy_preview_tiles(&mut cache, GFX32_CACHE_BASE, &assets.gfx32)?;
    let submap = usize::from(overworld.source_slot).min(6);
    let an2_start = AN2_NATIVE_BASE + submap * AN2_NATIVE_STRIDE;
    if let Some(an2) = assets
        .native_sprite_graphics_cache
        .get(an2_start..an2_start + AN2_NATIVE_STRIDE)
    {
        copy_preview_tiles(&mut cache, AN2_CACHE_BASE, an2)?;
    }
    let relative_base = RELATIVE_BASES[usize::from(overworld.data.animation.setting & 3)];
    copy_preview_tiles(
        &mut cache,
        usize::try_from(relative_base).unwrap_or(0),
        &assets.graphics.graphics.tiles,
    )?;

    let runtime_substeps = preview.tick.saturating_mul(preview.substeps_per_tick);
    let mut palette = overworld.data.palette.clone();
    apply_builtin_overworld_animation(
        &mut cache,
        &assets.built_in_animation_addresses,
        runtime_substeps,
    )?;
    apply_builtin_overworld_palette_animation(
        &mut palette,
        assets.built_in_level_dot_palette.as_ref(),
        assets.built_in_lightning.as_ref(),
        usize::from(overworld.source_slot).min(6),
        runtime_substeps,
    )?;
    let mut triggers = preview.triggers.clone();
    triggers.overworld_event_manual = Some(std::array::from_fn(|index| {
        let manual = index + 8;
        overworld.data.animation.trigger_mask & (1 << manual) != 0
            && preview
                .events_passed
                .get(usize::from(overworld.data.animation.trigger_values[manual]))
                .copied()
                .unwrap_or(false)
    }));

    let records = &overworld.data.animation.records;
    let mut state = lm_graphics::ExAnimationPreviewState::new(records.len());
    // Lunar Magic constructs a complete first-frame cache before showing the map.  Subsequent
    // updates retain the native eight-way slot interleave.
    for phase in 0..8_u8 {
        apply_overworld_phase(
            records,
            phase,
            &mut state,
            &mut triggers,
            &mut cache,
            &mut palette,
            relative_base,
        )?;
    }
    for substep in 0..runtime_substeps {
        apply_overworld_phase(
            records,
            u8::try_from(substep & 7).expect("three-bit overworld animation phase"),
            &mut state,
            &mut triggers,
            &mut cache,
            &mut palette,
            relative_base,
        )?;
    }
    let mut graphics = assets.graphics.clone();
    let len = graphics.graphics.tiles.len();
    graphics.graphics.tiles.clone_from_slice(&cache[..len]);
    Ok((graphics, palette))
}

fn apply_builtin_overworld_palette_animation(
    palette: &mut lm_graphics::Palette,
    level_dot_colors: Option<&[[lm_graphics::Bgr555; 8]; 2]>,
    lightning: Option<&BuiltInOverworldLightning>,
    submap: usize,
    runtime_substeps: usize,
) -> Result<(), String> {
    const LEVEL_DOT_TARGETS: [usize; 2] = [0x6d, 0x7d];
    const LIGHTNING_SUBMAP: usize = 4;
    const LIGHTNING_TARGET: usize = 0x47;
    const LIGHTNING_SOURCE_BASE: usize = 0x28;

    if let Some(colors) = level_dot_colors {
        // InitializeOverworldAnimationGraphicsCache refreshes with counter eight. Each timer rate
        // contributes its recovered substep count, and Refresh... uses `(counter >> 2) & 7` as
        // the color phase.
        let phase = (8_usize.wrapping_add(runtime_substeps) >> 2) & 7;
        for (target, cycle) in LEVEL_DOT_TARGETS.into_iter().zip(colors) {
            let palette_len = palette.colors.len();
            *palette.colors.get_mut(target).ok_or_else(|| {
                format!(
                    "overworld palette has {palette_len:X} colors; built-in level-dot animation requires ${target:02X}"
                )
            })? = cycle[phase];
        }
    }

    // Vanilla passes mask $F7 to the game's submap check, enabling lightning only where the
    // corresponding bit is clear: Valley of Bowser (native submap four).
    if submap == LIGHTNING_SUBMAP
        && let Some(lightning) = lightning
        && let Some(color_index) = materialize_builtin_lightning_color(lightning, runtime_substeps)
    {
        let source = LIGHTNING_SOURCE_BASE + usize::from(color_index);
        let palette_len = palette.colors.len();
        let color = *palette.colors.get(source).ok_or_else(|| {
            format!(
                "overworld palette has {palette_len:X} colors; lightning requires source ${source:02X}"
            )
        })?;
        *palette.colors.get_mut(LIGHTNING_TARGET).ok_or_else(|| {
            format!(
                "overworld palette has {palette_len:X} colors; lightning requires target ${LIGHTNING_TARGET:02X}"
            )
        })? = color;
    }
    Ok(())
}

fn materialize_builtin_lightning_color(
    tables: &BuiltInOverworldLightning,
    runtime_substeps: usize,
) -> Option<u8> {
    let substeps = 8_usize.saturating_add(runtime_substeps);
    let mut color_index = 0_u8;
    let mut wait = 0_u8;
    let mut duration = 0_u8;
    let mut displayed = None;
    for frame in 0..substeps {
        let mut frame_color = color_index;
        if color_index == 0 {
            if frame & 1 == 0 {
                continue;
            }
            wait = wait.wrapping_sub(1);
            if wait != 0 {
                continue;
            }
            let selector = usize::from(tables.selectors[(frame >> 1) & 0x7f] & 7);
            wait = tables.delays[selector];
            color_index = tables.initial_colors[selector];
            frame_color = color_index;
            duration = 8;
        }
        duration = duration.wrapping_sub(1);
        if duration & 0x80 != 0 {
            color_index = color_index.wrapping_sub(1);
            duration = 4;
        }
        // AdvanceBuiltInOverworldPaletteAnimation saves the pre-decrement color selector before
        // updating the state and publishes that saved selector into the displayed palette cache.
        displayed = Some(frame_color);
    }
    displayed
}

fn apply_builtin_overworld_animation(
    graphics: &mut [lm_graphics::IndexedTile],
    addresses: &[u16],
    runtime_substeps: usize,
) -> Result<(), String> {
    const ADDRESS_WORDS: usize = 3 + 8 * 8;
    const DESTINATION: usize = 0x75;
    const SOURCE_BASE: usize = 0xad00;
    const SOURCE_LIMIT: usize = 0xc800;
    if addresses.is_empty() {
        return Ok(());
    }
    if addresses.len() != ADDRESS_WORDS {
        return Err(format!(
            "built-in overworld animation table has {} words instead of {ADDRESS_WORDS}",
            addresses.len()
        ));
    }
    let bytes_per_tile = usize::from(addresses[4].wrapping_sub(addresses[3]));
    if !matches!(bytes_per_tile, 0x18 | 0x20) {
        return Err(format!(
            "built-in overworld animation source stride is {bytes_per_tile:X}, expected 18 or 20"
        ));
    }
    let source = graphics.to_vec();
    let blank = lm_graphics::IndexedTile::new([0; lm_graphics::IndexedTile::PIXEL_COUNT]);
    let resolve = |address: u16| -> lm_graphics::IndexedTile {
        let address = usize::from(address);
        if (SOURCE_BASE..SOURCE_LIMIT).contains(&address) {
            source
                .get((address - SOURCE_BASE) / bytes_per_tile)
                .cloned()
                .unwrap_or_else(|| blank.clone())
        } else {
            blank.clone()
        }
    };
    let mut animated = vec![blank.clone(); 11];
    for (destination, address) in addresses[..3].iter().copied().enumerate() {
        animated[destination] = resolve(address);
    }
    // InitializeOverworldAnimationGraphicsCache @ $00543480 copies this fixed source tile into
    // cache slot five before AdvanceOverworldExAnimationFrame constructs the first frame.
    animated[5] = source.get(0x7a).cloned().unwrap_or_else(|| blank.clone());
    for group in 0..8 {
        let first_address = addresses[3 + group * 8];
        if group != 2 || !matches!(first_address, 0xb480 | 0xb700) {
            animated[group + 3] = resolve(first_address);
        }
    }

    // Runtime substep zero advances the first built-in group. Thereafter each eight-way slot
    // interleave advances it again; this works unchanged at all four native timer rates.
    let boundaries = runtime_substeps.saturating_add(7) / 8;
    for boundary in 1..=boundaries {
        rotate_builtin_overworld_seed_tiles(&mut animated);
        for group in 0..8 {
            let first_address = addresses[3 + group * 8];
            if group == 2 && matches!(first_address, 0xb480 | 0xb700) {
                continue;
            }
            let frame = if group < 2 {
                boundary.saturating_add(1) / 2 & 7
            } else {
                boundary & 7
            };
            animated[group + 3] = resolve(addresses[3 + group * 8 + frame]);
        }
    }
    let end = DESTINATION + animated.len();
    let graphics_len = graphics.len();
    graphics
        .get_mut(DESTINATION..end)
        .ok_or_else(|| {
            format!(
                "overworld graphics cache has {graphics_len:X} tiles; built-in animation requires {DESTINATION:X}..{end:X}"
            )
        })?
        .clone_from_slice(&animated);
    Ok(())
}

fn rotate_builtin_overworld_seed_tiles(tiles: &mut [lm_graphics::IndexedTile]) {
    fn pixels(tile: &lm_graphics::IndexedTile) -> [u8; 64] {
        *tile.pixels()
    }
    let mut first = pixels(&tiles[0]);
    for row in 0..4 {
        first[row * 8..row * 8 + 8].rotate_left(1);
    }
    for row in 4..8 {
        first[row * 8..row * 8 + 8].rotate_right(1);
    }
    tiles[0] = lm_graphics::IndexedTile::new(first);

    let mut second = pixels(&tiles[1]);
    for column in 0..8 {
        let bottom = second[7 * 8 + column];
        for row in (1..8).rev() {
            second[row * 8 + column] = second[(row - 1) * 8 + column];
        }
        second[column] = bottom;
    }
    tiles[1] = lm_graphics::IndexedTile::new(second);

    let mut third = pixels(&tiles[2]);
    for row in 0..8 {
        third[row * 8..row * 8 + 8].rotate_left(1);
    }
    for column in 0..8 {
        let bottom = third[7 * 8 + column];
        for row in (1..8).rev() {
            third[row * 8 + column] = third[(row - 1) * 8 + column];
        }
        third[column] = bottom;
    }
    tiles[2] = lm_graphics::IndexedTile::new(third);
}

#[allow(clippy::too_many_arguments)]
fn apply_overworld_phase(
    records: &[lm_graphics::ExAnimationRecord],
    phase: u8,
    state: &mut lm_graphics::ExAnimationPreviewState,
    triggers: &mut lm_graphics::ExAnimationTriggerPreviewState,
    cache: &mut [lm_graphics::IndexedTile],
    palette: &mut lm_graphics::Palette,
    relative_base: u32,
) -> Result<(), String> {
    for selected in state.process_phase(records, phase, true, triggers) {
        let record = &records[selected.record];
        let second_bank = (8..=0x0f).contains(&record.trigger())
            || lm_graphics::exanimation_trigger_has_second_bank(record.trigger());
        if record.kind() < 0x13 {
            let address = lm_graphics::resolve_exanimation_graphics_address_with_banking(
                record,
                selected.frame,
                lm_graphics::ExAnimationGraphicsAddressContext {
                    two_bpp_enabled: (0x0f..=0x12).contains(&record.kind()),
                    relative_source_base_tile: relative_base,
                    relative_source_limit_bytes: 0x8000,
                },
                second_bank,
            )
            .map_err(|error| {
                format!("overworld ExAnimation record {}: {error}", selected.record)
            })?;
            let overrides = lm_graphics::materialize_exanimation_graphics_transfer_with_banking(
                record,
                selected.frame,
                cache,
                usize::try_from(address.source_tile)
                    .map_err(|_| "overworld ExAnimation source tile does not fit this platform")?,
                address.destination_tile,
                address.two_bpp_destination,
                second_bank,
            )
            .map_err(|error| {
                format!("overworld ExAnimation record {}: {error}", selected.record)
            })?;
            for entry in overrides {
                let destination = usize::try_from(entry.tile_index)
                    .map_err(|_| "overworld ExAnimation destination tile does not fit")?;
                if let Some(slot) = cache.get_mut(destination) {
                    *slot = entry.tile;
                }
            }
        } else if record.kind() <= 0x1b {
            let source_color = if record.kind() < 0x18 {
                usize::from(
                    lm_graphics::exanimation_frame_source_word_with_banking(
                        record,
                        selected.frame,
                        second_bank,
                    )
                    .map_err(|error| {
                        format!("overworld ExAnimation record {}: {error}", selected.record)
                    })? & 0xff,
                )
            } else {
                0
            };
            let alternate = selected.frame > u16::from(record.frame_count_minus_one());
            if let lm_graphics::ExAnimationPaletteTransfer::Palette(overrides) =
                lm_graphics::materialize_exanimation_palette_transfer_with_banking(
                    record,
                    selected.frame,
                    &palette.colors,
                    source_color,
                    alternate,
                    second_bank,
                )
                .map_err(|error| {
                    format!("overworld ExAnimation record {}: {error}", selected.record)
                })?
            {
                for entry in overrides {
                    palette.colors[usize::try_from(entry.color_index)
                        .expect("validated overworld palette index fits usize")] = entry.color;
                }
            }
        }
    }
    Ok(())
}

fn copy_preview_tiles(
    cache: &mut [lm_graphics::IndexedTile],
    destination: usize,
    tiles: &[lm_graphics::IndexedTile],
) -> Result<(), String> {
    let end = destination
        .checked_add(tiles.len())
        .ok_or("overworld ExAnimation preview cache overflow")?;
    let cache_len = cache.len();
    let target = cache.get_mut(destination..end).ok_or_else(|| {
        format!("overworld ExAnimation preview cache has {cache_len:X} tiles; copy requires {destination:X}..{end:X}")
    })?;
    target.clone_from_slice(tiles);
    Ok(())
}

pub(crate) fn selected_tile(
    rect: egui::Rect,
    position: egui::Pos2,
    width: usize,
    height: usize,
) -> Option<(usize, usize)> {
    if !rect.contains(position) || width == 0 || height == 0 {
        return None;
    }
    let width_f32 = f32::from(u16::try_from(width).ok()?);
    let height_f32 = f32::from(u16::try_from(height).ok()?);
    let x_position = (position.x - rect.min.x) / rect.width();
    let y_position = (position.y - rect.min.y) / rect.height();
    let x = find_axis(x_position, width, width_f32)?;
    let y = find_axis(y_position, height, height_f32)?;
    Some((x, y))
}

fn find_axis(position: f32, count: usize, count_f32: f32) -> Option<usize> {
    (0..count).find(|index| {
        let end = u16::try_from(index + 1).map_or(1.0, |value| f32::from(value) / count_f32);
        position < end
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, CompactExAnimation, ExAnimationRecord, GraphicsFile4bpp, Palette};
    use lm_level::{Map16Set, Map16SetFile};
    use lm_overworld::{EventRevealTable, OverworldLayer};
    use lm_project::{CompleteOverworldData, CompleteOverworldShape, OverworldLayers};

    #[test]
    fn rectangular_world_hit_test_is_exact() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(64.0, 32.0));
        assert_eq!(
            selected_tile(rect, egui::pos2(63.0, 31.0), 4, 2),
            Some((3, 1))
        );
        assert_eq!(selected_tile(rect, egui::pos2(65.0, 1.0), 4, 2), None);
    }

    fn preview_fixture() -> (CompleteOverworldFile, OverworldAssets) {
        let blank = lm_graphics::IndexedTile::new([0; 64]);
        let graphics_record =
            ExAnimationRecord::new(1, 1, 0, 0, false, &[0x00, 0x7d, 0x20, 0x7d], false).unwrap();
        let palette_record =
            ExAnimationRecord::new(0x13, 1, 0, 5, false, &[0x1f, 0x00, 0xe0, 0x03], false).unwrap();
        let overworld = CompleteOverworldFile {
            source_slot: 0,
            shape: CompleteOverworldShape {
                width: 1,
                height: 1,
                event_reveals: 0,
                endpoints: 0,
                messages: 0,
                sprites: 0,
                sprite_record_len: 0,
                palette_colors: 256,
            },
            data: CompleteOverworldData {
                layers: OverworldLayers {
                    layer1: OverworldLayer::new(1, 1, vec![0]).unwrap(),
                    layer2: OverworldLayer::new(1, 1, vec![0]).unwrap(),
                },
                event_reveals: EventRevealTable::default(),
                endpoints: Vec::new(),
                messages: Vec::new(),
                sprites: Vec::new(),
                palette: Palette {
                    colors: vec![Bgr555(0); 256],
                },
                animation: CompactExAnimation {
                    setting: 0,
                    header_value: 0,
                    trigger_mask: 0,
                    trigger_values: [0; 16],
                    records: vec![graphics_record, palette_record],
                },
            },
        };
        let mut gfx33 = vec![blank.clone(); 0x18];
        gfx33.push(lm_graphics::IndexedTile::new([1; 64]));
        gfx33.push(lm_graphics::IndexedTile::new([2; 64]));
        let assets = OverworldAssets {
            map16: Map16SetFile {
                set: Map16Set::default(),
            },
            graphics: GraphicsInterchangeFile {
                source_slot: 0,
                graphics: GraphicsFile4bpp {
                    tiles: vec![blank.clone(); 0x200],
                },
            },
            native_sprite_graphics_cache: vec![blank; 0x3100],
            external_sprite_assets: lm_graphics::ExternalSpriteAssets::default(),
            gfx32: Vec::new(),
            gfx33,
            built_in_animation_addresses: Vec::new(),
            built_in_level_dot_palette: None,
            built_in_lightning: None,
        };
        (overworld, assets)
    }

    #[test]
    fn overworld_preview_builds_all_first_frames_then_interleaves_graphics_and_palette() {
        let (overworld, assets) = preview_fixture();
        let preview = |tick| OverworldExAnimationPreview {
            tick,
            substeps_per_tick: 4,
            triggers: lm_graphics::ExAnimationTriggerPreviewState::default(),
            events_passed: vec![false; 256],
        };

        let (graphics, palette) =
            materialize_overworld_exanimation(&overworld, &assets, &preview(0)).unwrap();
        assert_eq!(graphics.graphics.tiles[0].pixels(), &[1; 64]);
        assert_eq!(palette.colors[5], Bgr555(0x001f));

        let (graphics, palette) =
            materialize_overworld_exanimation(&overworld, &assets, &preview(1)).unwrap();
        assert_eq!(graphics.graphics.tiles[0].pixels(), &[2; 64]);
        assert_eq!(palette.colors[5], Bgr555(0x03e0));

        let (_, palette) =
            materialize_overworld_exanimation(&overworld, &assets, &preview(2)).unwrap();
        assert_eq!(palette.colors[5], Bgr555(0x03e0));
    }

    #[test]
    fn every_native_timer_rate_materializes_the_same_animation_substep() {
        let (overworld, assets) = preview_fixture();
        let render = |tick, substeps_per_tick| {
            materialize_overworld_exanimation(
                &overworld,
                &assets,
                &OverworldExAnimationPreview {
                    tick,
                    substeps_per_tick,
                    triggers: lm_graphics::ExAnimationTriggerPreviewState::default(),
                    events_passed: vec![false; 256],
                },
            )
            .unwrap()
        };
        let expected = render(1, 8);
        for (tick, substeps_per_tick) in [(1, 8), (2, 4), (4, 2), (8, 1)] {
            let actual = render(tick, substeps_per_tick);
            assert_eq!(actual.0.graphics.tiles, expected.0.graphics.tiles);
            assert_eq!(actual.1, expected.1);
        }
    }

    #[test]
    fn passed_event_state_selects_overworld_event_manual_second_bank() {
        let (mut overworld, mut assets) = preview_fixture();
        overworld.data.animation.trigger_mask = 1 << 8;
        overworld.data.animation.trigger_values[8] = 2;
        overworld.data.animation.records = vec![
            ExAnimationRecord::new(1, 0, 8, 0, false, &[0x00, 0x7d, 0x20, 0x7d], true).unwrap(),
        ];
        assets.gfx33.push(lm_graphics::IndexedTile::new([3; 64]));
        let preview = OverworldExAnimationPreview {
            tick: 0,
            substeps_per_tick: 4,
            triggers: lm_graphics::ExAnimationTriggerPreviewState::default(),
            events_passed: vec![false; 256],
        };

        let (graphics, _) =
            materialize_overworld_exanimation(&overworld, &assets, &preview).unwrap();
        assert_eq!(graphics.graphics.tiles[0].pixels(), &[1; 64]);
        let mut preview = preview;
        preview.events_passed[2] = true;
        let (graphics, _) =
            materialize_overworld_exanimation(&overworld, &assets, &preview).unwrap();
        assert_eq!(graphics.graphics.tiles[0].pixels(), &[2; 64]);
    }

    #[test]
    fn built_in_overworld_tiles_use_rom_table_slow_groups_and_exact_seed_rotations() {
        let tile = |value| lm_graphics::IndexedTile::new([value; 64]);
        let mut graphics = (0..0x200)
            .map(|index| tile(u8::try_from(index & 0xff).unwrap()))
            .collect::<Vec<_>>();
        let first_seed = lm_graphics::IndexedTile::new(std::array::from_fn(|index| {
            u8::try_from(index).unwrap()
        }));
        graphics[0x10] = first_seed;
        let address = |tile: usize| u16::try_from(0xad00 + tile * 0x20).unwrap();
        let mut addresses = vec![address(0x10), address(0x11), address(0x12)];
        for group in 0..8 {
            for frame in 0..8 {
                addresses.push(address(0x40 + group * 8 + frame));
            }
        }

        let mut first = graphics.clone();
        apply_builtin_overworld_animation(&mut first, &addresses, 0).unwrap();
        assert_eq!(first[0x75].pixels(), graphics[0x10].pixels());
        assert_eq!(first[0x78].pixels(), &[0x40; 64]);
        assert_eq!(first[0x7a].pixels(), graphics[0x7a].pixels());

        let mut tick_one = graphics.clone();
        apply_builtin_overworld_animation(&mut tick_one, &addresses, 4).unwrap();
        assert_eq!(&tick_one[0x75].pixels()[..8], &[1, 2, 3, 4, 5, 6, 7, 0]);
        assert_eq!(
            &tick_one[0x75].pixels()[32..40],
            &[39, 32, 33, 34, 35, 36, 37, 38]
        );
        assert_eq!(tick_one[0x78].pixels(), &[0x41; 64]);
        assert_eq!(tick_one[0x7a].pixels(), graphics[0x7a].pixels());

        let mut tick_two = graphics.clone();
        apply_builtin_overworld_animation(&mut tick_two, &addresses, 8).unwrap();
        assert_eq!(tick_two[0x75].pixels(), tick_one[0x75].pixels());
        assert_eq!(tick_two[0x78].pixels(), &[0x41; 64]);

        let mut tick_three = graphics.clone();
        apply_builtin_overworld_animation(&mut tick_three, &addresses, 12).unwrap();
        assert_eq!(tick_three[0x78].pixels(), &[0x41; 64]);
        assert_eq!(tick_three[0x7a].pixels(), graphics[0x7a].pixels());
        assert_eq!(tick_three[0x7b].pixels(), &[0x5a; 64]);
    }

    #[test]
    fn built_in_palette_cycles_level_dots_and_valley_lightning_without_touching_other_submaps() {
        let dot_cycles = [
            std::array::from_fn(|index| Bgr555(0x1000 + index as u16)),
            std::array::from_fn(|index| Bgr555(0x2000 + index as u16)),
        ];
        let lightning = BuiltInOverworldLightning {
            selectors: [0; 128],
            delays: [1; 8],
            initial_colors: [7; 8],
        };
        let mut palette = Palette {
            colors: (0..256).map(|index| Bgr555(index)).collect(),
        };
        apply_builtin_overworld_palette_animation(
            &mut palette,
            Some(&dot_cycles),
            Some(&lightning),
            0,
            0,
        )
        .unwrap();
        assert_eq!(palette.colors[0x6d], Bgr555(0x1002));
        assert_eq!(palette.colors[0x7d], Bgr555(0x2002));
        assert_eq!(palette.colors[0x47], Bgr555(0x47));

        apply_builtin_overworld_palette_animation(
            &mut palette,
            Some(&dot_cycles),
            Some(&lightning),
            4,
            504,
        )
        .unwrap();
        assert_eq!(palette.colors[0x6d], Bgr555(0x1000));
        assert_eq!(palette.colors[0x7d], Bgr555(0x2000));
        assert_eq!(palette.colors[0x47], Bgr555(0x2f));
    }

    #[test]
    fn custom_overworld_palette_record_overrides_a_built_in_destination() {
        let (mut overworld, mut assets) = preview_fixture();
        overworld.data.animation.records =
            vec![ExAnimationRecord::new(0x13, 0, 0, 0x6d, false, &[0x1f, 0x00], false).unwrap()];
        assets.built_in_level_dot_palette = Some([[Bgr555(0x1234); 8], [Bgr555(0x5678); 8]]);
        let (_, palette) = materialize_overworld_exanimation(
            &overworld,
            &assets,
            &OverworldExAnimationPreview {
                tick: 0,
                substeps_per_tick: 4,
                triggers: lm_graphics::ExAnimationTriggerPreviewState::default(),
                events_passed: vec![false; 256],
            },
        )
        .unwrap();
        assert_eq!(palette.colors[0x6d], Bgr555(0x001f));
        assert_eq!(palette.colors[0x7d], Bgr555(0x5678));
    }

    #[test]
    fn lightning_uses_the_exact_wrapping_wait_and_predecrement_color_sequence() {
        let lightning = BuiltInOverworldLightning {
            selectors: [0; 128],
            delays: [1; 8],
            initial_colors: [2; 8],
        };
        assert_eq!(materialize_builtin_lightning_color(&lightning, 500), None);
        assert_eq!(
            materialize_builtin_lightning_color(&lightning, 504),
            Some(2)
        );
        assert_eq!(
            materialize_builtin_lightning_color(&lightning, 508),
            Some(2)
        );
        assert_eq!(
            materialize_builtin_lightning_color(&lightning, 512),
            Some(2)
        );
        assert_eq!(
            materialize_builtin_lightning_color(&lightning, 516),
            Some(1)
        );
    }
}
