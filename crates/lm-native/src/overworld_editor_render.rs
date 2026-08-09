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
}

#[derive(Clone, Debug)]
pub(crate) struct OverworldExAnimationPreview {
    pub(crate) tick: usize,
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

    let mut palette = overworld.data.palette.clone();
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
    for tick in 0..preview.tick {
        apply_overworld_phase(
            records,
            u8::try_from(tick & 7).expect("three-bit overworld animation phase"),
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
        };
        (overworld, assets)
    }

    #[test]
    fn overworld_preview_builds_all_first_frames_then_interleaves_graphics_and_palette() {
        let (overworld, assets) = preview_fixture();
        let preview = |tick| OverworldExAnimationPreview {
            tick,
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
        assert_eq!(palette.colors[5], Bgr555(0x001f));

        let (_, palette) =
            materialize_overworld_exanimation(&overworld, &assets, &preview(2)).unwrap();
        assert_eq!(palette.colors[5], Bgr555(0x03e0));
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
}
