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
    /// The five animation switches selected independently for each of the seven maps.
    pub(crate) animation_options: [OverworldAnimationOptions; 7],
    /// Whether Lunar Magic's marker-gated four-feature option table is installed.
    pub(crate) animation_options_runtime_installed: bool,
    /// Whether the active revision profile authenticates the fixed SMW-US option operands.
    pub(crate) animation_options_layout_supported: bool,
    /// Unconsumed low bit of the original lightning byte, retained for lossless save.
    pub(crate) animation_lightning_unused_low_bit: bool,
    /// Lunar Magic's ROM-global ExAnimation set, resolved through the installed runtime.
    pub(crate) global_animation: Option<lm_graphics::CompactExAnimation>,
}

/// Lossless semantic view of Lunar Magic's per-map animation controls.
///
/// Four controls share the inverted high-nibble representation used by the level editor. The
/// original lightning control is stored separately as a seven-map bit mask, so it must not be
/// folded into that byte.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OverworldAnimationOptions {
    pub(crate) features: lm_graphics::ExAnimationFeatureOptions,
    pub(crate) original_lightning: bool,
}

impl OverworldAnimationOptions {
    pub(crate) const VANILLA_ENABLED: Self = Self {
        features: lm_graphics::ExAnimationFeatureOptions::decode(0),
        original_lightning: false,
    };

    pub(crate) const fn decode(feature_byte: u8, lightning_enabled: bool) -> Self {
        Self {
            features: lm_graphics::ExAnimationFeatureOptions::decode(feature_byte),
            original_lightning: lightning_enabled,
        }
    }
}

pub(crate) const fn vanilla_overworld_animation_options() -> [OverworldAnimationOptions; 7] {
    decode_overworld_animation_options([0; 7], 0xf7)
}

/// Decodes the exact two-source representation used by Lunar Magic 3.63.
///
/// `feature_bytes` are the seven inverted high-nibble option bytes loaded by the editor. The
/// lightning routine tests bit 7 and shifts its separate byte left after each map; a clear bit
/// enables lightning for that map. Bit zero is not consumed because there are only seven maps.
pub(crate) const fn decode_overworld_animation_options(
    feature_bytes: [u8; 7],
    lightning_disable_mask: u8,
) -> [OverworldAnimationOptions; 7] {
    let mut result = [OverworldAnimationOptions::VANILLA_ENABLED; 7];
    let mut index = 0;
    while index < result.len() {
        result[index] = OverworldAnimationOptions::decode(
            feature_bytes[index],
            lightning_disable_mask & (0x80 >> index) == 0,
        );
        index += 1;
    }
    result
}

pub(crate) fn encode_overworld_animation_options(
    options: [OverworldAnimationOptions; 7],
    lightning_unused_low_bit: bool,
) -> ([u8; 7], u8) {
    let mut feature_bytes = [0; 7];
    let mut lightning_disable_mask = 0_u8;
    let mut index = 0;
    while index < options.len() {
        feature_bytes[index] = options[index].features.encode();
        if !options[index].original_lightning {
            lightning_disable_mask |= 0x80 >> index;
        }
        index += 1;
    }
    if lightning_unused_low_bit {
        lightning_disable_mask |= 1;
    }
    (feature_bytes, lightning_disable_mask)
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum OverworldAnimationDomain {
    #[default]
    Local,
    Global,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OverworldAnimationOwner {
    pub(crate) domain: OverworldAnimationDomain,
    pub(crate) record: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OverworldAnimationOwnership {
    pub(crate) graphics: Vec<Option<OverworldAnimationOwner>>,
    pub(crate) palette: Vec<Option<OverworldAnimationOwner>>,
}

pub(crate) fn ctrl_shift_animation_navigation(
    modifiers: egui::Modifiers,
    owner: Option<OverworldAnimationOwner>,
) -> Option<OverworldAnimationOwner> {
    (modifiers.ctrl && modifiers.shift)
        .then_some(owner)
        .flatten()
}

/// Reconstructs the destination-attribution tables used by Lunar Magic's Ctrl+Shift navigation.
///
/// Records are visited in the same order as the preview painter: local slots first, then global
/// slots, with later records replacing earlier attribution for overlapping destinations. Disabled
/// domains contribute no owner, matching the per-map animation switches.
pub(crate) fn overworld_animation_ownership(
    local: &lm_graphics::CompactExAnimation,
    global: Option<&lm_graphics::CompactExAnimation>,
    options: OverworldAnimationOptions,
    graphics_len: usize,
    palette_len: usize,
) -> OverworldAnimationOwnership {
    let mut ownership = OverworldAnimationOwnership {
        graphics: vec![None; graphics_len],
        palette: vec![None; palette_len],
    };
    if options
        .features
        .enabled(lm_graphics::ExAnimationFeature::LevelExAnimation)
    {
        attribute_animation_records(
            &mut ownership,
            &local.records[..local.records.len().min(32)],
            OverworldAnimationDomain::Local,
        );
    }
    if options
        .features
        .enabled(lm_graphics::ExAnimationFeature::GlobalExAnimation)
        && let Some(global) = global
    {
        attribute_animation_records(
            &mut ownership,
            &global.records[..global.records.len().min(32)],
            OverworldAnimationDomain::Global,
        );
    }
    ownership
}

fn attribute_animation_records(
    ownership: &mut OverworldAnimationOwnership,
    records: &[lm_graphics::ExAnimationRecord],
    domain: OverworldAnimationDomain,
) {
    // Attribution depends only on the destination span. A large blank source keeps address
    // validation independent of the currently loaded GFX files while reusing the exact transfer
    // semantics used by the renderer.
    static BLANK_SOURCE: std::sync::OnceLock<Vec<lm_graphics::IndexedTile>> =
        std::sync::OnceLock::new();
    let source = BLANK_SOURCE.get_or_init(|| {
        let blank = lm_graphics::IndexedTile::new([0; lm_graphics::IndexedTile::PIXEL_COUNT]);
        vec![blank; 0x4000]
    });
    let palette = vec![lm_graphics::Bgr555(0); ownership.palette.len().max(0x100)];
    for (record_index, record) in records.iter().enumerate() {
        let owner = Some(OverworldAnimationOwner {
            domain,
            record: record_index,
        });
        let second_bank = (8..=0x0f).contains(&record.trigger())
            || lm_graphics::exanimation_trigger_has_second_bank(record.trigger());
        if record.kind() < 0x13 {
            let Ok(address) = lm_graphics::resolve_exanimation_graphics_address_with_banking(
                record,
                0,
                lm_graphics::ExAnimationGraphicsAddressContext {
                    two_bpp_enabled: (0x0f..=0x12).contains(&record.kind()),
                    relative_source_base_tile: 0,
                    relative_source_limit_bytes: 0x8000,
                },
                second_bank,
            ) else {
                continue;
            };
            let Ok(source_tile) = usize::try_from(address.source_tile) else {
                continue;
            };
            let Ok(overrides) = lm_graphics::materialize_exanimation_graphics_transfer_with_banking(
                record,
                0,
                source,
                source_tile,
                address.destination_tile,
                address.two_bpp_destination,
                second_bank,
            ) else {
                continue;
            };
            for entry in overrides {
                if let Ok(index) = usize::try_from(entry.tile_index)
                    && let Some(slot) = ownership.graphics.get_mut(index)
                {
                    *slot = owner;
                }
            }
        } else if record.kind() <= 0x1b
            && let Ok(lm_graphics::ExAnimationPaletteTransfer::Palette(overrides)) =
                lm_graphics::materialize_exanimation_palette_transfer_with_banking(
                    record,
                    0,
                    &palette,
                    0,
                    false,
                    second_bank,
                )
        {
            for entry in overrides {
                if let Ok(index) = usize::try_from(entry.color_index)
                    && let Some(slot) = ownership.palette.get_mut(index)
                {
                    *slot = owner;
                }
            }
        }
    }
}

pub(crate) fn render_layer_texture(
    context: &egui::Context,
    layer: &lm_overworld::OverworldLayer,
    layer1: &lm_overworld::OverworldLayer,
    layer1_map16: &lm_level::Map16SetFile,
    palette: &lm_graphics::Palette,
    assets: &OverworldAssets,
    state: lm_app::EmulatorRuntimeState,
) -> Result<egui::TextureHandle, String> {
    let background =
        lm_render::render_smw_overworld_layer2_tilemap(layer, &assets.graphics, palette)
            .map_err(|error| error.to_string())?;
    // Map16 definition 0 is the transparent Layer-1 cell. Its vanilla subtiles refer to the
    // game's separate blank character-base region, which is not part of the portable GFX table.
    // Normalize only definition 0: $122 is also used by real overworld geometry elsewhere.
    let blank_tile = assets
        .graphics
        .graphics
        .tiles
        .iter()
        .position(|tile| tile.pixels().iter().all(|&pixel| pixel == 0))
        .ok_or("vanilla overworld graphics contain no transparent tile")?;
    let blank_tile =
        u16::try_from(blank_tile).map_err(|_| "transparent overworld tile exceeds SNES range")?;
    let mut rendered_map16 = layer1_map16.clone();
    if let Some(definition) = rendered_map16
        .set
        .pages
        .first_mut()
        .and_then(|page| page.tiles.first_mut())
    {
        for subtile in [
            &mut definition.top_left,
            &mut definition.top_right,
            &mut definition.bottom_left,
            &mut definition.bottom_right,
        ] {
            subtile.0 = (subtile.0 & !0x03ff) | blank_tile;
        }
    }
    // The pristine Layer-1 table also retains the event-reveal staging cells. They are not part
    // of the zero-event-state map shown in game, but remain intact in the editable workspace.
    let mut visible_layer1 = layer1.clone();
    for tile in &mut visible_layer1.tiles {
        if (0x97..=0xbb).contains(tile) {
            *tile = 0;
        }
    }
    let foreground = lm_render::render_portable_overworld_layer(
        1,
        &visible_layer1,
        &rendered_map16,
        &assets.graphics,
        palette,
    )
    .map_err(|error| error.to_string())?;
    let mut pixels = background.pixels().to_vec();
    for y in 0..foreground.height() {
        for x in 0..foreground.width() {
            if let Some(pixel) = foreground.get(x, y)
                && pixel.alpha != 0
            {
                let destination_x = x;
                if destination_x < background.width() && y < background.height() {
                    pixels[y * background.width() + destination_x] = pixel;
                }
            }
        }
    }
    let composed = lm_render::Canvas::from_pixels(background.width(), background.height(), pixels)
        .map_err(|error| error.to_string())?;
    // SMW displays maps in a 224x160 playfield at (16, 40), surrounded by Layer 3/HUD.
    // Materialize that authentic game viewport here so the primary editor canvas has the same
    // geometry as the player's view instead of exposing the raw 512x512 submap sheet.
    let border = lm_render::Rgba {
        red: 198,
        green: 181,
        blue: 165,
        alpha: 255,
    };
    let mut frame_pixels = vec![border; 256 * 224];
    let plane_x = if state.overworld_submap == 0 { 0 } else { 512 };
    for y in 0..160 {
        for x in 0..224 {
            // The recovered camera origins include the game's 16-pixel left and 40-pixel top
            // screen insets. Add those screen coordinates before sampling the 224x160 playfield;
            // otherwise wrapped submaps expose the opposite edge of the shared sheet.
            let source_x = plane_x + ((usize::from(state.camera_x) + 16 + x) & 0x1ff);
            let source_y = (usize::from(state.camera_y) + 40 + y) & 0x1ff;
            if let Some(pixel) = composed.get(source_x, source_y) {
                frame_pixels[(40 + y) * 256 + 16 + x] = pixel;
            }
        }
    }
    let frame = lm_render::Canvas::from_pixels(256, 224, frame_pixels)
        .map_err(|error| error.to_string())?;
    texture_from_canvas(context, "native-main-overworld-composed", &frame)
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

pub(crate) fn render_exanimation_graphics_texture(
    context: &egui::Context,
    overworld: &CompleteOverworldFile,
    assets: &OverworldAssets,
    preview: &OverworldExAnimationPreview,
) -> Result<egui::TextureHandle, String> {
    let (graphics, palette) = materialize_overworld_exanimation(overworld, assets, preview)?;
    render_layer2_graphics_texture(context, &graphics, &palette, 0)
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
    native_custom_sprites: Option<&lm_overworld::NativeCustomOverworldSpriteTable>,
    completed_reveals: usize,
) -> Result<egui::TextureHandle, String> {
    render_texture_with_preview(
        context,
        overworld,
        assets,
        native_appearances,
        native_custom_sprites,
        completed_reveals,
        None,
    )
}

pub(crate) fn render_texture_with_preview(
    context: &egui::Context,
    overworld: &CompleteOverworldFile,
    assets: &OverworldAssets,
    native_appearances: Option<&lm_render::NativeOverworldAppearancePair>,
    native_custom_sprites: Option<&lm_overworld::NativeCustomOverworldSpriteTable>,
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
        let placements = native_overworld_sprite_placements(overworld, native_custom_sprites);
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

fn native_overworld_sprite_placements(
    overworld: &CompleteOverworldFile,
    native_custom_sprites: Option<&lm_overworld::NativeCustomOverworldSpriteTable>,
) -> Vec<lm_render::NativeOverworldSpritePlacement> {
    let mut placements = overworld
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
    if let Some(custom) = native_custom_sprites {
        placements.extend(custom.maps.iter().enumerate().flat_map(|(map, sprites)| {
            sprites
                .iter()
                .map(move |sprite| lm_render::NativeOverworldSpritePlacement {
                    id: u16::from(sprite.id),
                    x: i32::from(sprite.x) + if map == 0 { 0 } else { 512 },
                    y: i32::from(sprite.y),
                    submap: u8::try_from(map).unwrap_or_default(),
                })
        }));
    }
    placements
}

pub(crate) fn native_custom_sprite_hit_test(
    native: Option<&lm_render::NativeOverworldAppearancePair>,
    custom: &lm_overworld::NativeCustomOverworldSpriteTable,
    map: usize,
    point: (usize, usize),
) -> Option<usize> {
    let records = custom.maps.get(map)?;
    let canvas_x = if map == 0 { 0 } else { 512 };
    let submap = u8::try_from(map).ok()?;
    let placements = records
        .iter()
        .map(|sprite| lm_render::NativeOverworldSpritePlacement {
            id: u16::from(sprite.id),
            x: i32::from(sprite.x) + canvas_x,
            y: i32::from(sprite.y),
            submap,
        })
        .collect::<Vec<_>>();
    let mut rendered = vec![false; records.len()];
    if let Some(native) = native {
        let elements = lm_render::resolve_native_overworld_sprite_elements(
            &placements,
            &native.definitions,
            lm_render::lunar_magic_builtin_overworld_sprite_map16(),
            &native.sprite_map16,
        );
        for element in &elements {
            let index = match element {
                lm_render::ResolvedNativeOverworldSpriteElement::Tile { sprite_index, .. }
                | lm_render::ResolvedNativeOverworldSpriteElement::Label { sprite_index, .. }
                | lm_render::ResolvedNativeOverworldSpriteElement::EditorTextDefinition {
                    sprite_index,
                    ..
                }
                | lm_render::ResolvedNativeOverworldSpriteElement::UnresolvedMap16 {
                    sprite_index,
                    ..
                } => *sprite_index,
            };
            if let Some(rendered) = rendered.get_mut(index) {
                *rendered = true;
            }
        }
        let point = (i32::try_from(point.0).ok()?, i32::try_from(point.1).ok()?);
        if let Some(index) =
            lm_render::hit_test_resolved_native_overworld_sprite_elements(&elements, point)
        {
            return Some(index);
        }
    }
    let point = (u16::try_from(point.0).ok()?, u16::try_from(point.1).ok()?);
    records.iter().enumerate().rposition(|(index, sprite)| {
        let x = sprite
            .x
            .saturating_add(u16::try_from(canvas_x).unwrap_or_default());
        !rendered[index]
            && point.0 >= x
            && point.1 >= sprite.y
            && point.0 < x.saturating_add(8)
            && point.1 < sprite.y.saturating_add(8)
    })
}

pub(crate) fn native_custom_sprite_indices_in_rect(
    native: Option<&lm_render::NativeOverworldAppearancePair>,
    custom: &lm_overworld::NativeCustomOverworldSpriteTable,
    map: usize,
    rect: (usize, usize, usize, usize),
) -> std::collections::BTreeSet<usize> {
    let Some(records) = custom.maps.get(map) else {
        return std::collections::BTreeSet::default();
    };
    let canvas_x = if map == 0 { 0 } else { 512 };
    let Ok(submap) = u8::try_from(map) else {
        return std::collections::BTreeSet::default();
    };
    let placements = records
        .iter()
        .map(|sprite| lm_render::NativeOverworldSpritePlacement {
            id: u16::from(sprite.id),
            x: i32::from(sprite.x) + canvas_x,
            y: i32::from(sprite.y),
            submap,
        })
        .collect::<Vec<_>>();
    let mut rendered = vec![false; records.len()];
    let mut selected = std::collections::BTreeSet::new();
    if let Some(native) = native {
        let elements = lm_render::resolve_native_overworld_sprite_elements(
            &placements,
            &native.definitions,
            lm_render::lunar_magic_builtin_overworld_sprite_map16(),
            &native.sprite_map16,
        );
        for element in &elements {
            let index = native_overworld_element_sprite_index(element);
            if let Some(rendered) = rendered.get_mut(index) {
                *rendered = true;
            }
        }
        let rect = (
            i32::try_from(rect.0).unwrap_or(i32::MAX),
            i32::try_from(rect.1).unwrap_or(i32::MAX),
            i32::try_from(rect.2).unwrap_or(i32::MAX),
            i32::try_from(rect.3).unwrap_or(i32::MAX),
        );
        selected.extend(
            lm_render::resolved_native_overworld_sprite_elements_intersecting_rect(&elements, rect),
        );
    }
    let canvas_x = u16::try_from(canvas_x).unwrap_or_default();
    for (index, sprite) in records.iter().enumerate() {
        if rendered[index] {
            continue;
        }
        let x = usize::from(sprite.x.saturating_add(canvas_x));
        let y = usize::from(sprite.y);
        if x < rect.2 && y < rect.3 && x.saturating_add(8) > rect.0 && y.saturating_add(8) > rect.1
        {
            selected.insert(index);
        }
    }
    selected
}

fn native_overworld_element_sprite_index(
    element: &lm_render::ResolvedNativeOverworldSpriteElement,
) -> usize {
    match element {
        lm_render::ResolvedNativeOverworldSpriteElement::Tile { sprite_index, .. }
        | lm_render::ResolvedNativeOverworldSpriteElement::Label { sprite_index, .. }
        | lm_render::ResolvedNativeOverworldSpriteElement::EditorTextDefinition {
            sprite_index,
            ..
        }
        | lm_render::ResolvedNativeOverworldSpriteElement::UnresolvedMap16 {
            sprite_index, ..
        } => *sprite_index,
    }
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
    let options = assets.animation_options[submap];
    if options
        .features
        .enabled(lm_graphics::ExAnimationFeature::VanillaAnimation)
    {
        apply_builtin_overworld_animation(
            &mut cache,
            &assets.built_in_animation_addresses,
            runtime_substeps,
        )?;
    }
    apply_builtin_overworld_palette_animation(
        &mut palette,
        assets.built_in_level_dot_palette.as_ref(),
        assets.built_in_lightning.as_ref(),
        options,
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

    let local_records = if options
        .features
        .enabled(lm_graphics::ExAnimationFeature::LevelExAnimation)
    {
        &overworld.data.animation.records[..overworld.data.animation.records.len().min(32)]
    } else {
        &[]
    };
    let global_animation = options
        .features
        .enabled(lm_graphics::ExAnimationFeature::GlobalExAnimation)
        .then_some(assets.global_animation.as_ref())
        .flatten();
    let global_records = global_animation
        .map(|animation| &animation.records[..animation.records.len().min(32)])
        .unwrap_or_default();
    let global_relative_base = global_animation
        .map(|animation| RELATIVE_BASES[usize::from(animation.setting & 3)])
        .unwrap_or(relative_base);
    let mut global_triggers = global_animation
        .map(exanimation_trigger_preview_state)
        .unwrap_or_default();
    global_triggers.overworld_event_manual = triggers.overworld_event_manual;
    let mut local_state = lm_graphics::ExAnimationPreviewState::new(local_records.len());
    let mut global_state = lm_graphics::ExAnimationPreviewState::new(global_records.len());
    // Lunar Magic constructs a complete first-frame cache before showing the map.  Subsequent
    // updates retain the native eight-way slot interleave.
    for phase in 0..8_u8 {
        apply_overworld_phase(
            local_records,
            phase,
            &mut local_state,
            &mut triggers,
            &mut cache,
            &mut palette,
            relative_base,
        )?;
    }
    for phase in 0..8_u8 {
        apply_overworld_phase(
            global_records,
            phase,
            &mut global_state,
            &mut global_triggers,
            &mut cache,
            &mut palette,
            global_relative_base,
        )?;
    }
    for substep in 0..runtime_substeps {
        apply_overworld_phase(
            local_records,
            u8::try_from(substep & 7).expect("three-bit overworld animation phase"),
            &mut local_state,
            &mut triggers,
            &mut cache,
            &mut palette,
            relative_base,
        )?;
        apply_overworld_phase(
            global_records,
            u8::try_from(substep & 7).expect("three-bit overworld animation phase"),
            &mut global_state,
            &mut global_triggers,
            &mut cache,
            &mut palette,
            global_relative_base,
        )?;
    }
    let mut graphics = assets.graphics.clone();
    let len = graphics.graphics.tiles.len();
    graphics.graphics.tiles.clone_from_slice(&cache[..len]);
    Ok((graphics, palette))
}

fn exanimation_trigger_preview_state(
    animation: &lm_graphics::CompactExAnimation,
) -> lm_graphics::ExAnimationTriggerPreviewState {
    let mut triggers = lm_graphics::ExAnimationTriggerPreviewState::default();
    for index in 0..16 {
        if animation.trigger_mask & (1 << index) != 0 {
            triggers.manual_frames[index] = animation.trigger_values[index];
            triggers.custom[index] = animation.trigger_values[index] != 0;
        }
    }
    triggers
}

fn apply_builtin_overworld_palette_animation(
    palette: &mut lm_graphics::Palette,
    level_dot_colors: Option<&[[lm_graphics::Bgr555; 8]; 2]>,
    lightning: Option<&BuiltInOverworldLightning>,
    options: OverworldAnimationOptions,
    runtime_substeps: usize,
) -> Result<(), String> {
    const LEVEL_DOT_TARGETS: [usize; 2] = [0x6d, 0x7d];
    const LIGHTNING_TARGET: usize = 0x47;
    const LIGHTNING_SOURCE_BASE: usize = 0x28;

    if options
        .features
        .enabled(lm_graphics::ExAnimationFeature::PaletteAnimation)
        && let Some(colors) = level_dot_colors
    {
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

    if options.original_lightning
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
    use lm_overworld::{
        EventRevealTable, NativeCustomOverworldSprite, NativeCustomOverworldSpriteTable,
        NativeOverworldSpriteAppearance, NativeOverworldSpriteDisplay,
        NativeOverworldSpriteSidecar, OverworldLayer,
    };
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
            animation_options: vanilla_overworld_animation_options(),
            animation_options_runtime_installed: false,
            animation_options_layout_supported: false,
            animation_lightning_unused_low_bit: true,
            global_animation: None,
        };
        (overworld, assets)
    }

    #[test]
    fn native_custom_sprite_placements_route_every_map_to_its_canvas_plane() {
        let (overworld, _) = preview_fixture();
        let custom = NativeCustomOverworldSpriteTable {
            maps: std::array::from_fn(|map| {
                vec![NativeCustomOverworldSprite {
                    id: u8::try_from(map).unwrap(),
                    x: 24,
                    y: 40,
                    screen: 0,
                    extra: Vec::new(),
                }]
            }),
        };

        let placements = native_overworld_sprite_placements(&overworld, Some(&custom));
        assert_eq!(placements.len(), 7);
        assert_eq!((placements[0].x, placements[0].y), (24, 40));
        for (map, placement) in placements.iter().enumerate().skip(1) {
            assert_eq!((placement.x, placement.y), (536, 40));
            assert_eq!(usize::from(placement.submap), map);
        }
    }

    #[test]
    fn native_custom_sprite_hit_test_uses_rendered_label_geometry_on_the_selected_map() {
        let custom = NativeCustomOverworldSpriteTable {
            maps: std::array::from_fn(|map| {
                (map == 1)
                    .then(|| {
                        vec![NativeCustomOverworldSprite {
                            id: 3,
                            x: 48,
                            y: 24,
                            screen: 0,
                            extra: Vec::new(),
                        }]
                    })
                    .unwrap_or_default()
            }),
        };
        let native = lm_render::NativeOverworldAppearancePair {
            definitions: NativeOverworldSpriteSidecar {
                tooltips: Default::default(),
                appearances: std::collections::BTreeMap::from([(
                    3,
                    NativeOverworldSpriteAppearance {
                        shadow: false,
                        display: NativeOverworldSpriteDisplay::Label {
                            x: -16,
                            y: 8,
                            text: "Warp".into(),
                        },
                    },
                )]),
                graphics_ranges: Vec::new(),
                palette_ranges: Vec::new(),
            },
            sprite_map16: lm_level::S16OvSidecar::decode(&[]).unwrap(),
        };

        assert_eq!(
            native_custom_sprite_hit_test(Some(&native), &custom, 1, (544, 32)),
            Some(0)
        );
        assert_eq!(
            native_custom_sprite_hit_test(Some(&native), &custom, 1, (546, 35)),
            Some(0)
        );
        assert_eq!(
            native_custom_sprite_hit_test(Some(&native), &custom, 1, (560, 24)),
            None
        );
        assert_eq!(
            native_custom_sprite_hit_test(Some(&native), &custom, 0, (544, 32)),
            None
        );
        assert_eq!(
            native_custom_sprite_hit_test(Some(&native), &custom, 1, (48, 24)),
            None
        );
        assert_eq!(
            native_custom_sprite_indices_in_rect(Some(&native), &custom, 1, (544, 32, 545, 33)),
            std::collections::BTreeSet::from([0])
        );
        assert!(
            native_custom_sprite_indices_in_rect(Some(&native), &custom, 1, (560, 24, 568, 32))
                .is_empty(),
            "a rendered label must not also expose an invisible anchor target"
        );
        assert!(
            native_custom_sprite_indices_in_rect(Some(&native), &custom, 0, (544, 32, 545, 33))
                .is_empty()
        );
        assert_eq!(
            native_custom_sprite_indices_in_rect(None, &custom, 1, (560, 24, 568, 32)),
            std::collections::BTreeSet::from([0]),
            "sprites without drawable appearance data retain the anchor fallback"
        );
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
            vanilla_overworld_animation_options()[0],
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
            vanilla_overworld_animation_options()[4],
            504,
        )
        .unwrap();
        assert_eq!(palette.colors[0x6d], Bgr555(0x1000));
        assert_eq!(palette.colors[0x7d], Bgr555(0x2000));
        assert_eq!(palette.colors[0x47], Bgr555(0x2f));
    }

    #[test]
    fn every_overworld_feature_byte_round_trips_with_recovered_inverted_semantics() {
        for packed in 0_u8..=u8::MAX {
            let options = OverworldAnimationOptions::decode(packed, packed & 1 != 0);
            assert_eq!(options.features.encode(), packed);
            assert_eq!(options.original_lightning, packed & 1 != 0);
        }
        let options = OverworldAnimationOptions::decode(0xf0, true);
        for feature in [
            lm_graphics::ExAnimationFeature::LevelExAnimation,
            lm_graphics::ExAnimationFeature::GlobalExAnimation,
            lm_graphics::ExAnimationFeature::VanillaAnimation,
            lm_graphics::ExAnimationFeature::PaletteAnimation,
        ] {
            assert!(!options.features.enabled(feature));
        }
    }

    #[test]
    fn every_lightning_mask_decodes_in_the_original_high_bit_shift_order() {
        for mask in 0_u8..=u8::MAX {
            let options = decode_overworld_animation_options([0; 7], mask);
            for (submap, option) in options.into_iter().enumerate() {
                assert_eq!(option.original_lightning, mask & (0x80 >> submap) == 0);
            }
            let (features, encoded_mask) =
                encode_overworld_animation_options(options, mask & 1 != 0);
            assert_eq!(features, [0; 7]);
            assert_eq!(encoded_mask, mask);
        }
        let vanilla = vanilla_overworld_animation_options();
        assert_eq!(
            vanilla.map(|option| option.original_lightning),
            [false, false, false, false, true, false, false]
        );
    }

    #[test]
    fn per_map_options_gate_dot_lightning_and_local_records_independently() {
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
            OverworldAnimationOptions::decode(0x80, true),
            504,
        )
        .unwrap();
        assert_eq!(palette.colors[0x6d], Bgr555(0x6d));
        assert_eq!(palette.colors[0x7d], Bgr555(0x7d));
        assert_eq!(palette.colors[0x47], Bgr555(0x2f));

        let (overworld, mut assets) = preview_fixture();
        assets.animation_options[0] = OverworldAnimationOptions::decode(0x10, false);
        let (graphics, palette) = materialize_overworld_exanimation(
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
        assert_eq!(graphics.graphics.tiles[0].pixels(), &[0; 64]);
        assert_eq!(palette.colors[5], Bgr555(0));
    }

    #[test]
    fn enabled_global_overworld_records_run_after_local_records_and_obey_the_map_gate() {
        let (overworld, mut assets) = preview_fixture();
        assets.global_animation = Some(CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![
                ExAnimationRecord::new(0x13, 0, 0, 5, false, &[0x00, 0x7c], false).unwrap(),
            ],
        });
        let preview = OverworldExAnimationPreview {
            tick: 0,
            substeps_per_tick: 4,
            triggers: lm_graphics::ExAnimationTriggerPreviewState::default(),
            events_passed: vec![false; 256],
        };

        let (_, palette) =
            materialize_overworld_exanimation(&overworld, &assets, &preview).unwrap();
        assert_eq!(palette.colors[5], Bgr555(0x7c00));

        assets.animation_options[0] = OverworldAnimationOptions::decode(0x20, false);
        let (_, palette) =
            materialize_overworld_exanimation(&overworld, &assets, &preview).unwrap();
        assert_eq!(palette.colors[5], Bgr555(0x001f));
    }

    #[test]
    fn destination_ownership_matches_domain_gates_slot_limits_and_last_writer_precedence() {
        let palette_record = |destination| {
            ExAnimationRecord::new(0x13, 0, 0, destination, false, &[0x1f, 0x00], false).unwrap()
        };
        let graphics_record = |destination| {
            ExAnimationRecord::new(1, 0, 0, destination, false, &[0x00, 0x00], false).unwrap()
        };
        let mut local_records = vec![palette_record(5), graphics_record(0x70)];
        local_records.extend((2..33).map(|_| palette_record(9)));
        let local = CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: local_records,
        };
        let global = CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![palette_record(5), graphics_record(0x70), palette_record(10)],
        };

        let ownership = overworld_animation_ownership(
            &local,
            Some(&global),
            OverworldAnimationOptions::VANILLA_ENABLED,
            64,
            32,
        );
        assert_eq!(
            ownership.palette[5],
            Some(OverworldAnimationOwner {
                domain: OverworldAnimationDomain::Global,
                record: 0,
            })
        );
        assert_eq!(
            ownership.graphics[7],
            Some(OverworldAnimationOwner {
                domain: OverworldAnimationDomain::Global,
                record: 1,
            })
        );
        assert_eq!(ownership.palette[10].unwrap().record, 2);
        // Slot 32 is outside the native 32-record overworld window and cannot own color 9.
        assert_eq!(ownership.palette[9].unwrap().record, 31);

        let local_only = overworld_animation_ownership(
            &local,
            Some(&global),
            OverworldAnimationOptions::decode(0x20, false),
            64,
            32,
        );
        assert_eq!(
            local_only.palette[5],
            Some(OverworldAnimationOwner {
                domain: OverworldAnimationDomain::Local,
                record: 0,
            })
        );
        assert_eq!(
            local_only.graphics[7],
            Some(OverworldAnimationOwner {
                domain: OverworldAnimationDomain::Local,
                record: 1,
            })
        );

        let disabled = overworld_animation_ownership(
            &local,
            Some(&global),
            OverworldAnimationOptions::decode(0x30, false),
            64,
            32,
        );
        assert!(disabled.graphics.iter().all(Option::is_none));
        assert!(disabled.palette.iter().all(Option::is_none));
    }

    #[test]
    fn ownership_navigation_requires_ctrl_shift_accepts_alt_and_rejects_unowned_destinations() {
        let owner = Some(OverworldAnimationOwner {
            domain: OverworldAnimationDomain::Local,
            record: 7,
        });
        assert_eq!(
            ctrl_shift_animation_navigation(
                egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    alt: true,
                    ..Default::default()
                },
                owner,
            ),
            owner
        );
        assert_eq!(
            ctrl_shift_animation_navigation(
                egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
                owner,
            ),
            None
        );
        assert_eq!(
            ctrl_shift_animation_navigation(
                egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Default::default()
                },
                None,
            ),
            None
        );
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
