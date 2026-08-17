use eframe::egui;
use lm_app::RevisionProfile;
use lm_graphics::{IndexedTile, Palette};
use lm_level::LegacyLevelHeader;
use lm_project::Project;
use lm_rom::RomImage;

pub(crate) struct VanillaMap16Preview {
    pub(crate) image: egui::ColorImage,
    pub(crate) layer2_image: egui::ColorImage,
    pub(crate) background_image: egui::ColorImage,
    pub(crate) animated_images: Vec<egui::ColorImage>,
    pub(crate) block_contents_images: Vec<egui::ColorImage>,
    pub(crate) animated_layer2_images: Vec<egui::ColorImage>,
    pub(crate) animated_background_images: Vec<egui::ColorImage>,
    pub(crate) animated_background_plane_images: Vec<egui::ColorImage>,
    pub(crate) graphics_files: [usize; 4],
    pub(crate) background_graphics_files: [usize; 4],
    pub(crate) sprite_image: egui::ColorImage,
    pub(crate) animated_sprite_images: Vec<egui::ColorImage>,
    pub(crate) entrance_image: egui::ColorImage,
    pub(crate) sprite_tiles: Vec<IndexedTile>,
    pub(crate) animated_sprite_tiles: Vec<IndexedTile>,
    pub(crate) palette: Palette,
    pub(crate) backdrop: lm_graphics::Bgr555,
    pub(crate) foreground_image: egui::ColorImage,
    pub(crate) foreground_tiles: Vec<IndexedTile>,
    pub(crate) layer3_tiles: Vec<IndexedTile>,
    pub(crate) layer3_low_image: Option<egui::ColorImage>,
    pub(crate) layer3_high_image: Option<egui::ColorImage>,
    pub(crate) layer3_position: Option<(i16, i16)>,
    pub(crate) layer3_editor_row_offset: Option<i16>,
    pub(crate) layer3_between_background_and_foreground: bool,
    pub(crate) sprite_graphics_files: [usize; 4],
    pub(crate) common_tiles: usize,
    pub(crate) tileset_tiles: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VanillaAnimationViewState {
    pub(crate) blue_pow_active: bool,
    pub(crate) silver_pow_active: bool,
    pub(crate) conditional: lm_render::LunarMagicConditionalViewState,
    pub(crate) two_bpp_mode: u8,
    pub(crate) layer3_16x16_mode: u8,
    pub(crate) gfx_display_override: GfxDisplayOverride,
}

impl Default for VanillaAnimationViewState {
    fn default() -> Self {
        Self {
            blue_pow_active: false,
            silver_pow_active: false,
            conditional: lm_render::LunarMagicConditionalViewState::default(),
            two_bpp_mode: 0,
            layer3_16x16_mode: 0,
            gfx_display_override: GfxDisplayOverride::default(),
        }
    }
}

/// Lunar Magic's session-only `GFX Display Override` values.
///
/// Each `$7F` entry delegates to the level's real assignment. Other values select an explicit
/// GFX/ExGFX file for editor display only; these values are deliberately never written to ROM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GfxDisplayOverride {
    pub(crate) layer_1_2: [u16; 8],
    pub(crate) layer_3: [u16; 8],
}

impl Default for GfxDisplayOverride {
    fn default() -> Self {
        Self {
            layer_1_2: [0x7f; 8],
            layer_3: [0x7f; 8],
        }
    }
}

fn apply_display_override<const N: usize>(
    mut files: [usize; N],
    overrides: &[u16; 8],
) -> [usize; N] {
    for (file, &override_file) in files.iter_mut().zip(overrides) {
        if override_file != 0x7f {
            *file = usize::from(override_file);
        }
    }
    files
}

fn layer_1_2_display_files(real: [usize; 4], overrides: &[u16; 8]) -> [usize; 8] {
    let mut files = [0x7f; 8];
    files[..real.len()].copy_from_slice(&real);
    apply_display_override(files, overrides)
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
            let word = tilemap[source_index];
            let tile = usize::from(word & 0x3fff);
            if tile >= 512 {
                continue;
            }
            let source_x = tile % TILES * TILE;
            let source_y = tile / TILES * TILE;
            let x_flip = word & 0x4000 != 0;
            let y_flip = word & 0x8000 != 0;
            for target_y in 0..TILE {
                let source_pixel_y = if y_flip {
                    TILE - 1 - target_y
                } else {
                    target_y
                };
                for target_x in 0..TILE {
                    let source_pixel_x = if x_flip {
                        TILE - 1 - target_x
                    } else {
                        target_x
                    };
                    image.pixels[(y * TILE + target_y) * EXTENT + x * TILE + target_x] = atlas
                        .pixels
                        [(source_y + source_pixel_y) * atlas.size[0] + source_x + source_pixel_x];
                }
            }
        }
    }
    Ok(image)
}

pub(crate) fn compose_native_map16_bank_plane(
    atlas: &egui::ColorImage,
    tilemap: &[u16],
) -> Result<egui::ColorImage, String> {
    const TILE: usize = 16;
    const COLUMNS: usize = 32;
    const DEFINITIONS: usize = 0x1000;
    const EXTENT: usize = TILE * COLUMNS;
    if atlas.size != [EXTENT, DEFINITIONS / COLUMNS * TILE] {
        return Err(format!(
            "background Map16 bank atlas is {}×{} instead of {EXTENT}×{}",
            atlas.size[0],
            atlas.size[1],
            DEFINITIONS / COLUMNS * TILE
        ));
    }
    if tilemap.len() != COLUMNS * COLUMNS {
        return Err(format!(
            "native Layer 2 tilemap has {} words instead of {}",
            tilemap.len(),
            COLUMNS * COLUMNS
        ));
    }
    let mut image = egui::ColorImage::new([EXTENT, EXTENT], egui::Color32::TRANSPARENT);
    for y in 0..COLUMNS {
        for x in 0..COLUMNS {
            let source_index = lm_level::native_layer2_tilemap_index(x, y)
                .expect("bounded native Layer 2 coordinate");
            let word = tilemap[source_index];
            let definition = usize::from(word & 0x0fff);
            let source_x = definition % COLUMNS * TILE;
            let source_y = definition / COLUMNS * TILE;
            let x_flip = word & 0x4000 != 0;
            let y_flip = word & 0x8000 != 0;
            for target_y in 0..TILE {
                let source_pixel_y = if y_flip {
                    TILE - 1 - target_y
                } else {
                    target_y
                };
                for target_x in 0..TILE {
                    let source_pixel_x = if x_flip {
                        TILE - 1 - target_x
                    } else {
                        target_x
                    };
                    image.pixels[(y * TILE + target_y) * EXTENT + x * TILE + target_x] = atlas
                        .pixels
                        [(source_y + source_pixel_y) * atlas.size[0] + source_x + source_pixel_x];
                }
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
const LAYER1_DISPLAY_TILES: usize = 8 * LAYER1_SPRITE_SLOT_STRIDE;
const GFX33_DECODED_TILE_PADDING: usize = 0x30;

pub(crate) const INTERNAL_GRAPHICS_CACHE_TILES: usize = 0x4000;
const INTERNAL_FOREGROUND_START: usize = 0x0000;
const INTERNAL_SPRITE_START: usize = 0x0400;
const INTERNAL_GFX33_START: usize = 0x0600;
const INTERNAL_GFX33_TILES: usize = 0x0180;
const INTERNAL_AUXILIARY_ANIMATION_START: usize = 0x0780;
const INTERNAL_AUXILIARY_ANIMATION_TILES: usize = 0x0100;
const INTERNAL_GFX32_START: usize = 0x0900;
const INTERNAL_GFX32_TILES: usize = 0x02e8;
const INTERNAL_EXANIMATION_START: usize = 0x0c00;
const INTERNAL_EXANIMATION_TILES: usize = 0x1000;
const INTERNAL_LAYER3_START: usize = 0x1c00;
const INTERNAL_LAYER3_TILES: usize = 0x0400;
const INTERNAL_EXTERNAL_SPRITE_START: usize = 0x2000;
const INTERNAL_EXTERNAL_SPRITE_TILES: usize = 0x2000;

/// Lunar Magic's complete decoded `$006204B0` graphics workspace.
///
/// The ordinary editor exposes only pages `$00-$05`. Its diagnostic Ctrl+Shift+PageDown command
/// raises the page limit to `$3F`, revealing the remaining banks represented here. Installed
/// ExAnimation and external-file banks are deliberately left for their owning loaders; this
/// constructor covers the exact pristine-ROM population rather than fabricating those sources.
pub(crate) struct VanillaInternalGraphicsCache {
    pub(crate) tiles: Vec<IndexedTile>,
}

pub(crate) fn load_pristine_internal_graphics_cache(
    rom_bytes: Vec<u8>,
    level: u16,
    header: LegacyLevelHeader,
    special_world_passed: bool,
) -> Result<VanillaInternalGraphicsCache, String> {
    load_pristine_internal_graphics_cache_with_berry_conversion(
        rom_bytes,
        level,
        header,
        special_world_passed,
        true,
    )
}

pub(crate) fn load_pristine_internal_graphics_cache_with_berry_conversion(
    rom_bytes: Vec<u8>,
    level: u16,
    header: LegacyLevelHeader,
    special_world_passed: bool,
    convert_berry_gfx_tile: bool,
) -> Result<VanillaInternalGraphicsCache, String> {
    let rom = RomImage::from_bytes(rom_bytes).map_err(|error| error.to_string())?;
    let project = Project::new(rom);
    let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
    let mut cache = vec![blank.clone(); INTERNAL_GRAPHICS_CACHE_TILES];

    let foreground_files = lm_profile::smw_us_v1_object_tileset_graphics_files(
        &project.rom,
        usize::from(header.object_tileset()),
    )
    .map_err(|error| error.to_string())?;
    let foreground_slots =
        load_layer1_sprite_graphics_slots(&project, foreground_files, convert_berry_gfx_tile)?;
    let mut foreground = materialize_layer1_sprite_vram(&foreground_slots);
    apply_vanilla_common_animation_frame(&project, &mut foreground, 0, header.object_tileset())?;
    cache[INTERNAL_FOREGROUND_START..INTERNAL_FOREGROUND_START + foreground.len()]
        .clone_from_slice(&foreground);

    let mut sprite_files = lm_profile::smw_us_v1_sprite_tileset_graphics_files(
        &project.rom,
        usize::from(header.sprite_tileset()),
    )
    .map_err(|error| error.to_string())?;
    if special_world_passed {
        sprite_files[1] = 0x31;
    }
    let sprite_slots =
        load_layer1_sprite_graphics_slots(&project, sprite_files, convert_berry_gfx_tile)?;
    let sprites = materialize_layer1_sprite_vram(&sprite_slots);
    cache[INTERNAL_SPRITE_START..INTERNAL_SPRITE_START + sprites.len()].clone_from_slice(&sprites);

    let gfx33 = load_smw_us_v1_special_graphics_file(&project, true)?;
    let gfx33 = lm_graphics::decode_planar_tiles(&gfx33, 3)
        .map_err(|error| format!("cannot decode pristine internal GFX33: {error}"))?;
    if gfx33.len() < INTERNAL_GFX33_TILES {
        return Err(format!(
            "pristine internal GFX33 has {} tiles instead of at least {INTERNAL_GFX33_TILES}",
            gfx33.len()
        ));
    }
    cache[INTERNAL_GFX33_START..INTERNAL_GFX33_START + INTERNAL_GFX33_TILES]
        .clone_from_slice(&gfx33[..INTERNAL_GFX33_TILES]);

    // The ordinary pristine path clears the selected auxiliary-animation slot and identifies it
    // as file `$7F`; expanded settings may populate this range through a separate owned loader.
    debug_assert!(
        cache[INTERNAL_AUXILIARY_ANIMATION_START
            ..INTERNAL_AUXILIARY_ANIMATION_START + INTERNAL_AUXILIARY_ANIMATION_TILES]
            .iter()
            .all(|tile| tile.pixels().iter().all(|&pixel| pixel == 0))
    );

    let gfx32 = load_smw_us_v1_special_graphics_file(&project, false)?;
    let gfx32 = lm_graphics::decode_planar_tiles(&gfx32, 4)
        .map_err(|error| format!("cannot decode pristine internal GFX32: {error}"))?;
    if gfx32.len() < INTERNAL_GFX32_TILES {
        return Err(format!(
            "pristine internal GFX32 has {} tiles instead of at least {INTERNAL_GFX32_TILES}",
            gfx32.len()
        ));
    }
    cache[INTERNAL_GFX32_START..INTERNAL_GFX32_START + INTERNAL_GFX32_TILES]
        .clone_from_slice(&gfx32[..INTERNAL_GFX32_TILES]);

    // Pristine SMW has no four `$8000`-byte ExAnimation graphics allocations. Their decoded
    // `$C00-$1BFF` destinations therefore retain the zero-filled initialization.
    debug_assert!(
        cache[INTERNAL_EXANIMATION_START..INTERNAL_EXANIMATION_START + INTERNAL_EXANIMATION_TILES]
            .iter()
            .all(|tile| tile.pixels().iter().all(|&pixel| pixel == 0))
    );

    let layer3 = load_layer3_tiles(
        &project,
        usize::from(level),
        lm_profile::smw_us_v1_vanilla_graphics_layout(),
    )?;
    if layer3.len() != INTERNAL_LAYER3_TILES {
        return Err(format!(
            "pristine internal Layer 3 cache has {} tiles instead of {INTERNAL_LAYER3_TILES}",
            layer3.len()
        ));
    }
    cache[INTERNAL_LAYER3_START..INTERNAL_LAYER3_START + INTERNAL_LAYER3_TILES]
        .clone_from_slice(&layer3);

    // ExSpriteGFX00-07 are external project files, not ROM resources. With no external directory
    // supplied, Lunar Magic zeroes all eight `$8000`-byte banks before their optional reads.
    debug_assert!(
        cache[INTERNAL_EXTERNAL_SPRITE_START
            ..INTERNAL_EXTERNAL_SPRITE_START + INTERNAL_EXTERNAL_SPRITE_TILES]
            .iter()
            .all(|tile| tile.pixels().iter().all(|&pixel| pixel == 0))
    );

    Ok(VanillaInternalGraphicsCache { tiles: cache })
}

/// Materializes the installed editor's level-dependent portion of Lunar Magic's complete decoded
/// graphics workspace through the active revision profile.
///
/// Unlike the pristine constructor, this resolves an enabled six-slot Super GFX Bypass and the
/// installed graphics pointer table. Optional external project assets populate their recovered
/// banks; no unrelated ROM bytes are guessed.
pub(crate) fn load_profiled_internal_graphics_cache(
    image: RomImage,
    profile: &RevisionProfile,
    level: u16,
    special_world_passed: bool,
    external_sprite_assets: Option<&lm_graphics::ExternalSpriteAssets>,
) -> Result<VanillaInternalGraphicsCache, String> {
    load_profiled_internal_graphics_cache_with_berry_conversion(
        image,
        profile,
        level,
        special_world_passed,
        external_sprite_assets,
        true,
    )
}

pub(crate) fn load_profiled_internal_graphics_cache_with_berry_conversion(
    image: RomImage,
    profile: &RevisionProfile,
    level: u16,
    special_world_passed: bool,
    external_sprite_assets: Option<&lm_graphics::ExternalSpriteAssets>,
    convert_berry_gfx_tile: bool,
) -> Result<VanillaInternalGraphicsCache, String> {
    let project = Project::new(image.clone());
    let level_layout = profile
        .level_layout_for_rom(&image)
        .map_err(|error| error.to_string())?;
    let loaded_level = project
        .load_level_slot(usize::from(level), level_layout, &profile.sprite_lengths)
        .map_err(|error| format!("cannot load level {level:03X} graphics header: {error}"))?;
    let header = loaded_level.layer1.header;
    let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
    let mut cache = vec![blank.clone(); INTERNAL_GRAPHICS_CACHE_TILES];

    let expanded = profile
        .expanded_settings
        .map(|layout| {
            project
                .load_expanded_level_settings(usize::from(level), layout)
                .map_err(|error| {
                    format!("cannot load level {level:03X} expanded graphics settings: {error}")
                })
        })
        .transpose()?;
    let bypass = expanded
        .as_ref()
        .map(lm_level::ExpandedLevelHeader::from)
        .map(|header| header.super_graphics_bypass())
        .filter(|selection| selection.enabled);
    let (foreground_files, mut sprite_files) = if let Some(selection) = bypass {
        (
            selection
                .foreground_background
                .into_iter()
                .map(usize::from)
                .collect::<Vec<_>>(),
            selection.sprites.map(usize::from).to_vec(),
        )
    } else {
        if profile.game != lm_rom::SupportedGame::SuperMarioWorld
            || profile.region != lm_rom::Region::NorthAmerica
            || profile.revision != 0
        {
            return Err(format!(
                "legacy graphics assignment tables are not recovered for profile {}",
                profile.name
            ));
        }
        (
            lm_profile::smw_us_v1_object_tileset_graphics_files(
                &image,
                usize::from(header.object_tileset()),
            )
            .map_err(|error| error.to_string())?
            .to_vec(),
            lm_profile::smw_us_v1_sprite_tileset_graphics_files(
                &image,
                usize::from(header.sprite_tileset()),
            )
            .map_err(|error| error.to_string())?
            .to_vec(),
        )
    };
    if special_world_passed && sprite_files.len() >= 2 {
        sprite_files[1] = 0x31;
    }
    let mut foreground = load_profiled_graphics_slots(
        &project,
        profile.graphics,
        &foreground_files,
        convert_berry_gfx_tile,
    )?;
    foreground.resize_with(6 * LAYER1_SPRITE_SLOT_TILES, || blank.clone());
    let mut sprites = load_profiled_graphics_slots(
        &project,
        profile.graphics,
        &sprite_files,
        convert_berry_gfx_tile,
    )?;
    sprites.resize_with(4 * LAYER1_SPRITE_SLOT_TILES, || blank.clone());
    let foreground_len = foreground.len().min(INTERNAL_SPRITE_START);
    cache[..foreground_len].clone_from_slice(&foreground[..foreground_len]);
    let sprite_len = sprites
        .len()
        .min(INTERNAL_GFX33_START - INTERNAL_SPRITE_START);
    cache[INTERNAL_SPRITE_START..INTERNAL_SPRITE_START + sprite_len]
        .clone_from_slice(&sprites[..sprite_len]);

    let (gfx33, gfx32) = load_profiled_internal_special_graphics(&project, profile.graphics)?;
    let gfx33_len = gfx33.len().min(INTERNAL_GFX33_TILES);
    cache[INTERNAL_GFX33_START..INTERNAL_GFX33_START + gfx33_len]
        .clone_from_slice(&gfx33[..gfx33_len]);
    let gfx32_len = gfx32.len().min(INTERNAL_GFX32_TILES);
    cache[INTERNAL_GFX32_START..INTERNAL_GFX32_START + gfx32_len]
        .clone_from_slice(&gfx32[..gfx32_len]);

    if let Some(file) = profiled_auxiliary_graphics_file(
        expanded.as_ref().map(lm_level::ExpandedLevelHeader::from),
        &loaded_level.layer1.objects,
    ) {
        let auxiliary = load_optional_profiled_graphics_file(&project, profile.graphics, file)?;
        let count = auxiliary.len().min(INTERNAL_AUXILIARY_ANIMATION_TILES);
        cache[INTERNAL_AUXILIARY_ANIMATION_START..INTERNAL_AUXILIARY_ANIMATION_START + count]
            .clone_from_slice(&auxiliary[..count]);
    }

    let layer3 = if let Some(settings) = expanded.as_ref() {
        load_layer3_tiles_from_settings(&project, settings, profile.graphics)?
    } else {
        load_layer3_tiles(&project, usize::from(level), profile.graphics)?
    };
    if layer3.len() != INTERNAL_LAYER3_TILES {
        return Err(format!(
            "profiled internal Layer 3 cache has {} tiles instead of {INTERNAL_LAYER3_TILES}",
            layer3.len()
        ));
    }
    cache[INTERNAL_LAYER3_START..INTERNAL_LAYER3_START + INTERNAL_LAYER3_TILES]
        .clone_from_slice(&layer3);

    populate_profiled_exanimation_source_banks(&project, profile.graphics, &mut cache)?;
    if let Some(assets) = external_sprite_assets {
        for index in INTERNAL_EXTERNAL_SPRITE_START
            ..INTERNAL_EXTERNAL_SPRITE_START + INTERNAL_EXTERNAL_SPRITE_TILES
        {
            let global = u16::try_from(index).expect("internal cache index fits u16");
            if let Some(tile) = assets.graphics_tile(global) {
                cache[index] = tile.clone();
            }
        }
    }
    Ok(VanillaInternalGraphicsCache { tiles: cache })
}

fn profiled_auxiliary_graphics_file(
    expanded: Option<lm_level::ExpandedLevelHeader>,
    objects: &lm_level::ObjectStream,
) -> Option<u16> {
    if let Some(header) = expanded.filter(|header| header.fields[0] & 0x8000 != 0) {
        return Some(header.fields[0] & 0x0fff);
    }
    objects
        .records
        .iter()
        .filter(|record| record.command_id() == 0x25)
        .map(lm_level::ObjectRecord::parameter)
        .last()
        .filter(|file| *file != 0)
        .map(|file| u16::from(file - 1))
}

fn load_optional_profiled_graphics_file(
    project: &Project,
    layout: lm_project::GraphicsRomLayout,
    file: u16,
) -> Result<Vec<IndexedTile>, String> {
    let entries = layout
        .split_pointer_planes
        .map_or(layout.pointers.entries, |planes| planes.entries);
    if usize::from(file) >= entries {
        return Ok(Vec::new());
    }
    let pointer = layout
        .read_pointer(project, usize::from(file))
        .map_err(|error| format!("cannot resolve auxiliary GFX{file:02X}: {error}"))?;
    if pointer.get() == 0 || pointer.get() == 0x00ff_ffff {
        return Ok(Vec::new());
    }
    project
        .load_super_graphics_file(file, layout)
        .map(|graphics| graphics.tiles)
        .map_err(|error| format!("cannot load auxiliary GFX{file:02X}: {error}"))
}

fn populate_profiled_exanimation_source_banks(
    project: &Project,
    layout: lm_project::GraphicsRomLayout,
    cache: &mut [IndexedTile],
) -> Result<(), String> {
    let entries = layout
        .split_pointer_planes
        .map_or(layout.pointers.entries, |planes| planes.entries);
    for (bank, file) in (0x60..=0x63).enumerate() {
        if file >= entries {
            continue;
        }
        let pointer = layout
            .read_pointer(project, file)
            .map_err(|error| format!("cannot resolve ExAnimation source GFX{file:02X}: {error}"))?;
        if pointer.get() == 0 || pointer.get() == 0x00ff_ffff {
            continue;
        }
        let tiles = project
            .load_super_graphics_file(
                u16::try_from(file).expect("ExAnimation source file fits u16"),
                layout,
            )
            .map_err(|error| format!("cannot load ExAnimation source GFX{file:02X}: {error}"))?
            .tiles;
        copy_exanimation_source_bank(bank, &tiles, cache)?;
    }
    Ok(())
}

fn copy_exanimation_source_bank(
    bank: usize,
    tiles: &[IndexedTile],
    cache: &mut [IndexedTile],
) -> Result<(), String> {
    if bank >= 4 {
        return Err(format!("ExAnimation source bank {bank} is outside 0..3"));
    }
    let start = INTERNAL_EXANIMATION_START + bank * 0x400;
    let count = tiles.len().min(0x400);
    let end = start
        .checked_add(count)
        .ok_or_else(|| "ExAnimation source-bank range overflow".to_owned())?;
    let cache_len = cache.len();
    let destination = cache.get_mut(start..end).ok_or_else(|| {
        format!(
            "internal cache has {cache_len:X} tiles; ExAnimation bank {bank} requires {start:X}..{end:X}"
        )
    })?;
    destination.clone_from_slice(&tiles[..count]);
    Ok(())
}

fn load_profiled_graphics_slots(
    project: &Project,
    layout: lm_project::GraphicsRomLayout,
    files: &[usize],
    convert_berry_gfx_tile: bool,
) -> Result<Vec<IndexedTile>, String> {
    let mut tiles = Vec::with_capacity(files.len() * LAYER1_SPRITE_SLOT_TILES);
    for (slot, file) in files.iter().copied().enumerate() {
        let file_number = file;
        let file = u16::try_from(file_number)
            .map_err(|_| format!("graphics slot {slot} file {file:X} exceeds $FFFF"))?;
        let mut loaded = project
            .load_super_graphics_file(file, layout)
            .map_err(|error| {
                format!("cannot load graphics slot {slot} file GFX{file:02X}: {error}")
            })?
            .tiles;
        if convert_berry_gfx_tile && matches!(file_number, 0x01 | 0x17 | 0x31) {
            synthesize_berry_tile_high_plane(&mut loaded);
        }
        loaded.resize_with(LAYER1_SPRITE_SLOT_TILES, || {
            IndexedTile::new([0; IndexedTile::PIXEL_COUNT])
        });
        tiles.extend(loaded);
    }
    Ok(tiles)
}

fn load_profiled_internal_special_graphics(
    project: &Project,
    layout: lm_project::GraphicsRomLayout,
) -> Result<(Vec<IndexedTile>, Vec<IndexedTile>), String> {
    let entries = layout
        .split_pointer_planes
        .map_or(layout.pointers.entries, |planes| planes.entries);
    let (gfx33_file, gfx32_file, gfx33_layout, gfx32_layout) = if entries > 0x33 {
        (0x33, 0x32, layout, layout)
    } else {
        let special = lm_profile::smw_us_v1_special_graphics_layouts(&project.rom)
            .map_err(|error| format!("cannot resolve profiled special graphics: {error}"))?;
        (0, 0, special.gfx33, special.gfx32)
    };
    let gfx33 = project
        .load_decompressed_graphics_file(gfx33_file, gfx33_layout)
        .map_err(|error| format!("cannot load profiled internal GFX33: {error}"))?;
    let gfx33 = lm_graphics::decode_planar_tiles(&gfx33, 3)
        .map_err(|error| format!("cannot decode profiled internal GFX33: {error}"))?;
    let gfx32 = project
        .load_decompressed_graphics_file(gfx32_file, gfx32_layout)
        .map_err(|error| format!("cannot load profiled internal GFX32: {error}"))?;
    let gfx32 = lm_graphics::decode_planar_tiles(&gfx32, 4)
        .map_err(|error| format!("cannot decode profiled internal GFX32: {error}"))?;
    Ok((gfx33, gfx32))
}

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
    special_world_passed: bool,
) -> Result<VanillaMap16Preview, String> {
    render_with_editor_palette_phase(
        rom_bytes,
        level,
        header,
        game_runtime,
        special_world_passed,
        requested_vanilla_editor_palette_phase(),
    )
}

pub(crate) fn render_with_animation_view_state(
    rom_bytes: Vec<u8>,
    level: u16,
    header: LegacyLevelHeader,
    game_runtime: bool,
    special_world_passed: bool,
    animation_view_state: VanillaAnimationViewState,
) -> Result<VanillaMap16Preview, String> {
    render_with_animation_view_state_and_background_bank(
        rom_bytes,
        level,
        header,
        game_runtime,
        special_world_passed,
        animation_view_state,
        0,
        None,
    )
}

pub(crate) fn render_with_animation_view_state_and_background_bank(
    rom_bytes: Vec<u8>,
    level: u16,
    header: LegacyLevelHeader,
    game_runtime: bool,
    special_world_passed: bool,
    animation_view_state: VanillaAnimationViewState,
    background_bank: u8,
    background_tilemap: Option<Vec<u16>>,
) -> Result<VanillaMap16Preview, String> {
    render_with_animation_view_state_background_bank_and_berry_conversion(
        rom_bytes,
        level,
        header,
        game_runtime,
        special_world_passed,
        animation_view_state,
        background_bank,
        background_tilemap,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_with_animation_view_state_background_bank_and_berry_conversion(
    rom_bytes: Vec<u8>,
    level: u16,
    header: LegacyLevelHeader,
    game_runtime: bool,
    special_world_passed: bool,
    animation_view_state: VanillaAnimationViewState,
    background_bank: u8,
    background_tilemap: Option<Vec<u16>>,
    convert_berry_gfx_tile: bool,
) -> Result<VanillaMap16Preview, String> {
    render_with_editor_palette_phase_and_animation_view_state(
        rom_bytes,
        level,
        header,
        game_runtime,
        special_world_passed,
        requested_vanilla_editor_palette_phase(),
        animation_view_state,
        background_bank,
        background_tilemap,
        convert_berry_gfx_tile,
    )
}

pub(crate) fn render_with_editor_palette_phase(
    rom_bytes: Vec<u8>,
    level: u16,
    header: LegacyLevelHeader,
    game_runtime: bool,
    special_world_passed: bool,
    editor_palette_phase: usize,
) -> Result<VanillaMap16Preview, String> {
    render_with_editor_palette_phase_and_animation_view_state(
        rom_bytes,
        level,
        header,
        game_runtime,
        special_world_passed,
        editor_palette_phase,
        VanillaAnimationViewState::default(),
        0,
        None,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_with_editor_palette_phase_and_animation_view_state(
    rom_bytes: Vec<u8>,
    level: u16,
    header: LegacyLevelHeader,
    game_runtime: bool,
    special_world_passed: bool,
    editor_palette_phase: usize,
    animation_view_state: VanillaAnimationViewState,
    background_bank: u8,
    background_tilemap: Option<Vec<u16>>,
    convert_berry_gfx_tile: bool,
) -> Result<VanillaMap16Preview, String> {
    if animation_view_state.two_bpp_mode > 2 {
        return Err(format!(
            "2bpp view mode {} is outside 0..=2",
            animation_view_state.two_bpp_mode
        ));
    }
    if animation_view_state.layer3_16x16_mode > 2 {
        return Err(format!(
            "Layer 3 16x16 mode {} is outside 0..=2",
            animation_view_state.layer3_16x16_mode
        ));
    }
    if editor_palette_phase >= 8 {
        return Err(format!(
            "vanilla editor palette phase {editor_palette_phase} is outside 0..8"
        ));
    }
    if background_bank >= 8 {
        return Err(format!(
            "background Map16 bank {background_bank} is outside 0..8"
        ));
    }
    let rom = RomImage::from_bytes(rom_bytes).map_err(|error| error.to_string())?;
    let project = Project::new(rom);
    let tileset = header.object_tileset();
    let graphics_files =
        lm_profile::smw_us_v1_object_tileset_graphics_files(&project.rom, usize::from(tileset))
            .map_err(|error| error.to_string())?;
    let display_graphics_files = layer_1_2_display_files(
        graphics_files,
        &animation_view_state.gfx_display_override.layer_1_2,
    );
    let graphics_slots = load_layer1_sprite_graphics_slots(
        &project,
        display_graphics_files,
        convert_berry_gfx_tile,
    )?;
    let mut base_foreground_graphics = materialize_layer1_sprite_vram(&graphics_slots);
    apply_lunar_magic_two_bpp_view(
        &mut base_foreground_graphics,
        animation_view_state.two_bpp_mode,
    );
    let background_display_graphics_files = layer_1_2_display_files(
        game_graphics_files(level, header, graphics_files),
        &animation_view_state.gfx_display_override.layer_1_2,
    );
    let background_graphics_slots = load_layer1_sprite_graphics_slots(
        &project,
        background_display_graphics_files,
        convert_berry_gfx_tile,
    )?;
    let mut base_background_graphics = materialize_layer1_sprite_vram(&background_graphics_slots);
    apply_lunar_magic_two_bpp_view(
        &mut base_background_graphics,
        animation_view_state.two_bpp_mode,
    );
    let map16 = lm_profile::load_smw_us_v1_level_map16_base(&project.rom, usize::from(tileset))
        .map_err(|error| error.to_string())?;
    let background_map16 = lm_profile::load_smw_us_v1_background_map16(&project.rom)
        .map_err(|error| error.to_string())?;
    let needs_complete_background = background_tilemap.as_deref().is_some_and(|tilemap| {
        background_bank != 0 || tilemap.iter().any(|word| word & 0x0fff >= 0x200)
    });
    let background_bank_map16 = needs_complete_background
        .then(|| {
            let complete = lm_profile::load_smw_us_v1_secondary_map16(&project)
                .map_err(|error| error.to_string())?;
            let bank_words = complete
                .definitions
                .chunks_exact(0x4000)
                .nth(usize::from(background_bank))
                .ok_or_else(|| format!("background Map16 bank {background_bank} is unavailable"))?;
            Ok::<_, String>(
                bank_words
                    .iter()
                    .flat_map(|word| word.to_le_bytes())
                    .collect::<Vec<_>>(),
            )
        })
        .transpose()?;
    let palette_header = if game_runtime {
        game_palette_header(level, header)
    } else {
        header
    };
    let composed_palette =
        lm_profile::compose_smw_us_v1_level_palette(&project, level, palette_header, 0)
            .map_err(|error| error.to_string())?;
    let backdrop = composed_palette.backdrop;
    let mut palette = composed_palette.palette;
    if !game_runtime {
        apply_vanilla_editor_palette_animation(&mut palette, editor_palette_phase);
    }
    if animation_view_state.two_bpp_mode != 0 {
        apply_lunar_magic_two_bpp_palette_rows(&mut palette);
    }
    let mut sprite_graphics_files = lm_profile::smw_us_v1_sprite_tileset_graphics_files(
        &project.rom,
        usize::from(header.sprite_tileset()),
    )
    .map_err(|error| error.to_string())?;
    if special_world_passed {
        // LoadSpecialWorldGraphicsFile @ 00464890 materializes GFX31 into Lunar Magic's SP2
        // working slot whenever the non-persistent editor-view flag at $00E278DF is enabled.
        sprite_graphics_files[1] = 0x31;
    }
    let sprite_graphics =
        load_layer1_sprite_graphics_slots(&project, sprite_graphics_files, convert_berry_gfx_tile)?;
    // The pristine ROM stores ordinary SNES 16-bit tilemap words here. Lunar Magic expands
    // those words into a wider internal descriptor while loading them, but the native renderer
    // consumes `Subtile`'s SNES layout directly. Feeding the widened-and-truncated representation
    // back into this path corrupts palette and flip attributes.
    let mut animated_foreground_graphics = Vec::with_capacity(8);
    let mut animated_images = Vec::with_capacity(32);
    let mut block_contents_images = Vec::with_capacity(8);
    let mut animated_layer2_images = Vec::with_capacity(32);
    let mut animated_background_images = Vec::with_capacity(8);
    let mut animated_background_plane_images = Vec::with_capacity(8);
    for phase in 0..8 {
        let mut foreground_graphics = base_foreground_graphics.clone();
        apply_vanilla_common_animation_frame_with_view_state(
            &project,
            &mut foreground_graphics,
            phase,
            tileset,
            animation_view_state,
        )?;
        let mut background_graphics = base_background_graphics.clone();
        apply_vanilla_common_animation_frame_with_view_state(
            &project,
            &mut background_graphics,
            phase,
            tileset,
            animation_view_state,
        )?;
        for screen_variant in 0..4 {
            let screen_map16 = map16_definitions_for_phase(&map16.bytes, screen_variant);
            animated_images.push(render_map16_definition_atlas(
                &screen_map16,
                &foreground_graphics,
                &palette,
            ));
            // Conditional-object visibility is applied per source object by the editor
            // compositor. Baking it into either shared atlas remaps ordinary Map16 cells that
            // merely share those numeric IDs, corrupting foregrounds and backgrounds alike.
            animated_layer2_images.push(render_layer2_map16_definition_atlas(
                &screen_map16,
                &foreground_graphics,
                &palette,
                tileset,
            ));
        }
        block_contents_images.push(render_default_m16_overlay_atlas(
            &foreground_graphics,
            &palette,
        ));
        let mut background_image =
            render_map16_definition_atlas(&background_map16, &background_graphics, &palette);
        if lm_profile::smw_us_v1_level_mode(header.level_mode()).background_half_color {
            apply_black_half_color(&mut background_image);
        }
        if let Some(tilemap) = background_tilemap.as_deref() {
            let plane = if let Some(definitions) = background_bank_map16.as_deref() {
                let mut plane = render_background_map16_bank_plane(
                    definitions,
                    &background_graphics,
                    &palette,
                    tilemap,
                )?;
                if lm_profile::smw_us_v1_level_mode(header.level_mode()).background_half_color {
                    apply_black_half_color(&mut plane);
                }
                plane
            } else {
                compose_native_map16_plane(&background_image, tilemap)?
            };
            animated_background_plane_images.push(plane);
        }
        animated_background_images.push(background_image);
        animated_foreground_graphics.push(foreground_graphics);
    }
    let foreground_graphics = animated_foreground_graphics.remove(0);
    let image = animated_images[0].clone();
    let layer2_image = animated_layer2_images[0].clone();
    let background_image = animated_background_images[0].clone();
    let sprite_tiles = materialize_layer1_sprite_vram(&sprite_graphics);
    // A sprite tile word with bit $200 set does not address an animated copy of SP1-SP4.
    // Lunar Magic adds the sprite display's $400-tile cache base, so page 2 resolves to decoded
    // cache $600-$7FF. LoadAnimationAndPlayerGraphicsCaches @ 0045B360 materializes GFX33 at
    // cache $600, independently of the ordinary level sprite slots at cache $400-$5FF.
    let animated_sprite_slots = load_vanilla_sprite_display_page(&project)?;
    let animated_sprite_image = render_sprite_graphics_atlas(&animated_sprite_slots, &palette);
    let animated_sprite_tiles = materialize_layer1_sprite_vram(&animated_sprite_slots);
    let animated_sprite_images = vec![animated_sprite_image; 4];
    let sprite_image = render_sprite_graphics_atlas(&sprite_graphics, &palette);
    let entrance_image = render_default_entrance_marker(&project, &palette)?;
    let foreground_image = render_foreground_graphics_atlas(&foreground_graphics, &palette);
    let layer3_tiles = load_layer3_tiles_with_override(
        &project,
        usize::from(level),
        lm_profile::smw_us_v1_vanilla_graphics_layout(),
        &animation_view_state.gfx_display_override.layer_3,
    )?;
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
    let layer3_editor_row_offset = layer3.as_ref().and_then(|layer3| {
        vanilla_layer3_editor_row_offset(layer3.behavior, header.object_tileset())
    });
    // The live source-order array for Layer 3 smash levels is background (0), Layer 3 (2),
    // foreground (1). Both Layer 3 priority classes are composited before foreground.
    let layer3_between_background_and_foreground = layer3
        .as_ref()
        .is_some_and(|layer3| vanilla_layer3_between_background_and_foreground(layer3.behavior));
    let (layer3_low_image, layer3_high_image) = layer3.as_ref().map_or((None, None), |layer3| {
        let additive = vanilla_layer3_additive(
            header.level_mode(),
            layer3.behavior,
            header.object_tileset(),
        );
        let (low, high) = render_layer3_planes_with_mode(
            &layer3.tilemap,
            &layer3_tiles,
            &palette,
            additive,
            animation_view_state.layer3_16x16_mode,
        );
        (Some(low), Some(high))
    });
    Ok(VanillaMap16Preview {
        image,
        layer2_image,
        background_image,
        animated_images,
        block_contents_images,
        animated_layer2_images,
        animated_background_images,
        animated_background_plane_images,
        foreground_image,
        foreground_tiles: foreground_graphics,
        layer3_tiles,
        layer3_low_image,
        layer3_high_image,
        layer3_position,
        layer3_editor_row_offset,
        layer3_between_background_and_foreground,
        graphics_files: display_graphics_files[..4]
            .try_into()
            .expect("four display slots"),
        background_graphics_files: background_display_graphics_files[..4]
            .try_into()
            .expect("four display slots"),
        sprite_image,
        animated_sprite_images,
        entrance_image,
        sprite_tiles,
        animated_sprite_tiles,
        palette,
        backdrop,
        sprite_graphics_files,
        common_tiles: map16.common_tiles,
        tileset_tiles: map16.tileset_tiles,
    })
}

fn render_default_m16_overlay_atlas(
    graphics: &[IndexedTile],
    palette: &Palette,
) -> egui::ColorImage {
    // LoadEmbeddedLevelEditorLookupResources @ $00498D90 locks PE resource type 500, ID 502;
    // ValidateAndInitializeOpenedRom @ $0047BE10 copies all 0x2000 bytes into the active `.m16`
    // bank before an optional ROM-adjacent sidecar replaces it. Preserve that authenticated bank
    // verbatim so editor-only definitions such as $219/$21A are available to Block Contents.
    const DEFINITIONS: &[u8; 0x2000] = include_bytes!("assets/lm363-default-m16.bin");
    const COLUMNS: usize = 32;
    const ROWS: usize = 32;
    const TILE: usize = 16;
    let width = COLUMNS * TILE;
    let height = ROWS * TILE;
    let mut rgba = vec![0; width * height * 4];
    for definition in 0..COLUMNS * ROWS {
        let definition_x = definition % COLUMNS * TILE;
        let definition_y = definition / COLUMNS * TILE;
        for quadrant in 0..4 {
            let offset = definition * 8 + quadrant * 2;
            let word = u16::from_le_bytes([DEFINITIONS[offset], DEFINITIONS[offset + 1]]);
            let (quadrant_x, quadrant_y) = map16_quadrant_offset(quadrant);
            draw_subtile(
                &mut rgba,
                width,
                (definition_x + quadrant_x, definition_y + quadrant_y),
                graphics.get(usize::from(word & 0x03ff)),
                palette,
                usize::from(word >> 10 & 7),
                (word & 0x4000 != 0, word & 0x8000 != 0),
            );
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba)
}

fn requested_vanilla_editor_palette_phase() -> usize {
    std::env::var("LM_NATIVE_PALETTE_PHASE")
        .ok()
        .and_then(|phase| phase.parse::<usize>().ok())
        .filter(|&phase| phase < 8)
        // Preserve Lunar Magic's previously authenticated yellow frame when
        // no deterministic audit phase was requested.
        .unwrap_or(2)
}

pub(crate) fn apply_vanilla_editor_palette_animation(palette: &mut Palette, phase: usize) {
    // AdvanceExAnimationFrames @ 0045AAC0 applies the built-in palette record before Lunar
    // Magic builds its editor cache. The pristine-ROM sequence begins at logical PC $00360C
    // and oscillates through eight Dragon Coin colors before repeating.
    const DRAGON_COIN_COLORS: [lm_graphics::Bgr555; 8] = [
        lm_graphics::Bgr555(0x02df),
        lm_graphics::Bgr555(0x035f),
        lm_graphics::Bgr555(0x27ff),
        lm_graphics::Bgr555(0x5fff),
        lm_graphics::Bgr555(0x73ff),
        lm_graphics::Bgr555(0x5fff),
        lm_graphics::Bgr555(0x27ff),
        lm_graphics::Bgr555(0x035f),
    ];
    if let Some(color) = palette.colors.get_mut(0x64) {
        *color = DRAGON_COIN_COLORS[phase & 7];
    }
}

pub(crate) const fn vanilla_layer3_editor_row_offset(
    behavior: lm_profile::SmwUsV1Layer3Behavior,
    object_tileset: u8,
) -> Option<i16> {
    match behavior {
        lm_profile::SmwUsV1Layer3Behavior::LowTide => Some(-2),
        lm_profile::SmwUsV1Layer3Behavior::HighTide => Some(-8),
        lm_profile::SmwUsV1Layer3Behavior::Static { code: 0x80 } => Some(1),
        lm_profile::SmwUsV1Layer3Behavior::Static { code: 0x81 }
            if matches!(object_tileset, 1 | 3 | 9 | 0x0d) =>
        {
            Some(0)
        }
        lm_profile::SmwUsV1Layer3Behavior::Static { .. } => None,
    }
}

pub(crate) const fn vanilla_layer3_between_background_and_foreground(
    behavior: lm_profile::SmwUsV1Layer3Behavior,
) -> bool {
    matches!(
        behavior,
        lm_profile::SmwUsV1Layer3Behavior::Static { code: 0x80 }
    )
}

pub(crate) const fn vanilla_layer3_additive(
    level_mode: u8,
    behavior: lm_profile::SmwUsV1Layer3Behavior,
    object_tileset: u8,
) -> bool {
    level_mode == 0x0e
        || (object_tileset == 9
            && matches!(
                behavior,
                lm_profile::SmwUsV1Layer3Behavior::Static { code: 0x81 }
            ))
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
    render_entrance_marker(project, palette, 0)
}

pub(crate) fn render_entrance_marker(
    project: &Project,
    palette: &Palette,
    action: u8,
) -> Result<egui::ColorImage, String> {
    // The full-level main/midway path emits only the configured player pose; `$217` must not
    // accompany action 6 in this compositor.
    let mut image = render_entrance_marker_with_helpers(project, palette, action, false)?;
    add_action_six_left_boundary(&mut image, palette, action);
    Ok(image)
}

pub(crate) fn render_secondary_entrance_marker(
    project: &Project,
    palette: &Palette,
    action: u8,
) -> Result<egui::ColorImage, String> {
    render_entrance_marker(project, palette, action)
}

fn add_action_six_left_boundary(image: &mut egui::ColorImage, palette: &Palette, action: u8) {
    if action == 6 {
        // `$306/$316` is emitted through RenderEditorTileAtOffset. Its signed three-pixel
        // boundary fragment lands immediately left of the nominal +3 X offset; preserving that
        // fragment is observable where the secondary-entrance label pointer crosses the pose.
        const LEFT_BOUNDARY: &[(usize, usize, u8)] = &[
            (15, 16, 2),
            (14, 17, 2),
            (15, 17, 1),
            (14, 18, 2),
            (15, 18, 1),
            (15, 19, 2),
            (15, 22, 2),
            (14, 23, 2),
            (15, 23, 3),
            (14, 24, 2),
            (15, 24, 3),
            (13, 25, 2),
            (14, 25, 3),
            (15, 25, 3),
            (13, 26, 2),
            (14, 26, 3),
            (15, 26, 3),
            (13, 27, 2),
            (14, 27, 5),
            (15, 27, 3),
            (14, 28, 2),
            (15, 28, 2),
        ];
        let width = image.size[0];
        for &(x, y, palette_index) in LEFT_BOUNDARY {
            if let Some([red, green, blue, alpha]) = palette_color(palette, 8, palette_index) {
                image.pixels[y * width + x] =
                    egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha);
            }
        }
    }
}

fn render_entrance_marker_with_helpers(
    project: &Project,
    palette: &Palette,
    action: u8,
    include_action_helper: bool,
) -> Result<egui::ColorImage, String> {
    // Horizontal action-0 path in `RenderConfiguredLevelEntrance` @ 004CC660. Lunar Magic places
    // editor-only Map16 $300 at Y+2 and $310 at Y+18. These are their live sidecar definitions.
    let (width, height, parts): (usize, usize, &[([u16; 4], usize, usize)]) = match action {
        // Actions 1 and 2 use their own `$303/$313` and `$302/$312` GFX32 definitions. These
        // values are the live Lunar Magic 3.63 sidecar entries for an untouched SMW ROM.
        1 => (
            16,
            32,
            &[
                ([0x00e0, 0x00f0, 0x00e1, 0x00f1], 0, 0),
                ([0x0000, 0x0010, 0x0001, 0x0011], 0, 16),
            ],
        ),
        2 => (
            16,
            32,
            &[
                ([0x40e1, 0x40f1, 0x40e0, 0x40f0], 0, 0),
                ([0x4001, 0x4011, 0x4000, 0x4010], 0, 16),
            ],
        ),
        // Action 3 uses editor definitions $308/$318. Ghidra's case sets the horizontal
        // entrance offset to eight pixels before emitting both halves.
        3 => (
            24,
            32,
            &[
                ([0x0108, 0x0118, 0x0109, 0x0119], 8, 0),
                ([0x000e, 0x001e, 0x000f, 0x001f], 8, 16),
            ],
        ),
        // Action 4 shares the `$308/$318` pose but starts three pixels above the
        // entrance anchor. Shift the image origin down five pixels so its signed call offsets are
        // retained. The separate `$11A` overlay comes from the ordinary sprite cache, not GFX32,
        // and is therefore composited by the full-level renderer.
        4 => (
            24,
            34,
            &[
                ([0x0108, 0x0118, 0x0109, 0x0119], 8, 2),
                ([0x000e, 0x001e, 0x000f, 0x001f], 8, 18),
            ],
        ),
        // Case 5 in `RenderConfiguredLevelEntrance` draws the swimming/rope entrance pose from
        // `$304/$314`, adds `$117` fourteen pixels to the left, and overlays `$216` two pixels
        // below the lower cell. The image origin is shifted right by 14 so the caller can retain
        // Lunar Magic's signed placement offset.
        5 => (
            48,
            34,
            &[
                ([0x40e1, 0x40f1, 0x40e0, 0x40f0], 16, 0),
                ([0x4029, 0x4039, 0x4028, 0x4038], 16, 16),
            ],
        ),
        // Action 6 emits definitions $306/$316 at (+3,+3)/(+3,+19), followed by $217
        // at (-13,+19) for this secondary entrance pose. Shift the image origin by 13 pixels;
        // the full-level compositor applies the inverse anchor adjustment.
        6 if include_action_helper => (
            48,
            32,
            &[
                ([0x40e1, 0x40f1, 0x40e0, 0x40f0], 16, 0),
                ([0x402b, 0x403b, 0x402a, 0x403a], 16, 16),
                ([0x0019, 0x0019, 0x601a, 0x601b], 0, 16),
            ],
        ),
        6 => (
            48,
            32,
            &[
                ([0x40e1, 0x40f1, 0x40e0, 0x40f0], 16, 0),
                ([0x402b, 0x403b, 0x402a, 0x403a], 16, 16),
            ],
        ),
        _ => (
            16,
            32,
            &[
                ([0x40e1, 0x40f1, 0x40e0, 0x40f0], 0, 0),
                ([0x4005, 0x4015, 0x4004, 0x4014], 0, 16),
            ],
        ),
    };
    let player_bytes = load_smw_us_v1_special_graphics_file(project, false)?;
    let player_tiles = lm_graphics::decode_planar_tiles(&player_bytes, 4)
        .map_err(|error| format!("cannot decode pristine entrance GFX32: {error}"))?;
    let mut rgba = vec![0; width * height * 4];
    for &(definition, part_x, part_y) in parts {
        for (quadrant, word) in definition.into_iter().enumerate() {
            let tile_index = usize::from(word & 0x03ff);
            let tile = player_tiles
                .get(tile_index)
                .ok_or_else(|| format!("entrance subtile ${tile_index:03X} is unavailable"))?;
            let (x, y) = map16_quadrant_offset(quadrant);
            draw_subtile_over(
                &mut rgba,
                width,
                (part_x + x, part_y + y),
                Some(tile),
                palette,
                8 + usize::from(word >> 10 & 7),
                (word & 0x4000 != 0, word & 0x8000 != 0),
            );
        }
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width, height],
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

#[cfg(test)]
fn render_layer3_planes(
    tilemap: &[u16],
    graphics: &[IndexedTile],
    palette: &Palette,
    additive: bool,
) -> (egui::ColorImage, egui::ColorImage) {
    render_layer3_planes_with_mode(tilemap, graphics, palette, additive, 0)
}

fn render_layer3_planes_with_mode(
    tilemap: &[u16],
    graphics: &[IndexedTile],
    palette: &Palette,
    additive: bool,
    mode: u8,
) -> (egui::ColorImage, egui::ColorImage) {
    let tiles = if mode == 1 {
        32
    } else {
        lm_profile::SMW_US_V1_LAYER3_TILEMAP_SIDE
    };
    const TILE_PIXELS: usize = IndexedTile::WIDTH;
    let cell_pixels = if mode == 0 {
        TILE_PIXELS
    } else {
        TILE_PIXELS * 2
    };
    let extent = tiles * cell_pixels;
    let mut low = egui::ColorImage::new([extent, extent], egui::Color32::TRANSPARENT);
    let mut high = egui::ColorImage::new([extent, extent], egui::Color32::TRANSPARENT);
    for tile_y in 0..tiles {
        for tile_x in 0..tiles {
            let position = tile_y * lm_profile::SMW_US_V1_LAYER3_TILEMAP_SIDE + tile_x;
            let Some(&word) = tilemap.get(position) else {
                continue;
            };
            // The stripe decoder fills untouched BG3 cells with SMW's canonical
            // blank word.  Its tile number is not universally blank in the
            // level-specific graphics set, so treat the sentinel itself as empty.
            if word == 0x38fc {
                continue;
            }
            let palette_number = usize::from((word >> 10) & 7);
            let x_flip = word & 0x4000 != 0;
            let y_flip = word & 0x8000 != 0;
            for y in 0..cell_pixels {
                for x in 0..cell_pixels {
                    let source_x = if x_flip { cell_pixels - 1 - x } else { x };
                    let source_y = if y_flip { cell_pixels - 1 - y } else { y };
                    let subtile = (source_y / TILE_PIXELS) * 0x10 + source_x / TILE_PIXELS;
                    let tile_number =
                        usize::from(word & 0x03ff) + if mode == 0 { 0 } else { subtile };
                    let Some(tile) = graphics.get(tile_number & 0x03ff) else {
                        continue;
                    };
                    let Some(index) = tile.pixel(source_x % TILE_PIXELS, source_y % TILE_PIXELS)
                    else {
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
                    target.pixels[(tile_y * cell_pixels + y) * extent + tile_x * cell_pixels + x] =
                        if additive {
                            egui::Color32::from_rgb_additive(color.red, color.green, color.blue)
                        } else {
                            egui::Color32::from_rgb(color.red, color.green, color.blue)
                        };
                }
            }
        }
    }
    (low, high)
}

pub(crate) fn apply_vanilla_common_animation_frame(
    project: &Project,
    graphics: &mut [IndexedTile],
    phase: usize,
    tileset: u8,
) -> Result<(), String> {
    apply_vanilla_common_animation_frame_with_view_state(
        project,
        graphics,
        phase,
        tileset,
        VanillaAnimationViewState::default(),
    )
}

fn apply_vanilla_common_animation_frame_with_view_state(
    project: &Project,
    graphics: &mut [IndexedTile],
    phase: usize,
    tileset: u8,
    view_state: VanillaAnimationViewState,
) -> Result<(), String> {
    if phase >= 8 {
        return Err(format!(
            "vanilla common animation phase {phase} is outside 0..8"
        ));
    }
    apply_vanilla_common_animation_phases_with_view_state(
        project,
        graphics,
        &vanilla_common_animation_phases(phase),
        tileset,
        0,
        false,
        view_state,
    )
}

pub(crate) fn apply_vanilla_common_animation_frame_with_tiles(
    project: &Project,
    graphics: &mut [IndexedTile],
    phase: usize,
    tileset: u8,
    gfx33_tiles: &[IndexedTile],
    gfx32_tiles: &[IndexedTile],
) -> Result<(), String> {
    if phase >= 8 {
        return Err(format!(
            "vanilla common animation phase {phase} is outside 0..8"
        ));
    }
    apply_vanilla_common_animation_phases_with_tiles(
        project,
        graphics,
        &vanilla_common_animation_phases(phase),
        tileset,
        0,
        false,
        VanillaAnimationViewState::default(),
        gfx33_tiles,
        gfx32_tiles,
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
    gfx33_decoded_tile_bias: usize,
    column_major_destinations: bool,
) -> Result<(), String> {
    apply_vanilla_common_animation_phases_with_view_state(
        project,
        graphics,
        phases,
        tileset,
        gfx33_decoded_tile_bias,
        column_major_destinations,
        VanillaAnimationViewState::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_vanilla_common_animation_phases_with_view_state(
    project: &Project,
    graphics: &mut [IndexedTile],
    phases: &[u8; 19],
    tileset: u8,
    gfx33_decoded_tile_bias: usize,
    column_major_destinations: bool,
    view_state: VanillaAnimationViewState,
) -> Result<(), String> {
    let (gfx33_tiles, gfx32_tiles) = load_vanilla_special_animation_tiles(project)?;
    apply_vanilla_common_animation_phases_with_tiles(
        project,
        graphics,
        phases,
        tileset,
        gfx33_decoded_tile_bias,
        column_major_destinations,
        view_state,
        &gfx33_tiles,
        &gfx32_tiles,
    )
}

fn load_vanilla_special_animation_tiles(
    project: &Project,
) -> Result<(Vec<IndexedTile>, Vec<IndexedTile>), String> {
    let decoded_gfx33 = load_smw_us_v1_special_graphics_file(project, true)?;
    let mut gfx33_tiles = lm_graphics::decode_planar_tiles(&decoded_gfx33, 3)
        .map_err(|error| format!("cannot decode pristine animated GFX33: {error}"))?;
    gfx33_tiles.resize_with(gfx33_tiles.len() + GFX33_DECODED_TILE_PADDING, || {
        IndexedTile::new([0; IndexedTile::PIXEL_COUNT])
    });
    let decoded_gfx32 = load_smw_us_v1_special_graphics_file(project, false)?;
    let gfx32_tiles = lm_graphics::decode_planar_tiles(&decoded_gfx32, 4)
        .map_err(|error| format!("cannot decode pristine player/animation GFX32: {error}"))?;
    Ok((gfx33_tiles, gfx32_tiles))
}

fn vanilla_animation_source_index(
    animation_index: usize,
    tileset: u8,
    view_state: VanillaAnimationViewState,
) -> usize {
    // LoadExAnimationFormatState @ $004596F0 reads the pristine tables beginning at logical ROM
    // PCs $02B96B and $02B97D (raw $02BB6B/$02BB7D with a copier header). Only trigger entries
    // 6..=13 are consumed because those are the
    // mode-one groups. The editor initializes Invisible POW Objects and On/Off Switch On to one.
    const MODE: [u8; 24] = [
        0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 0, 0, 0, 0, 0,
    ];
    const TRIGGER_6_TO_13: [u8; 8] = [0, 0, 0, 1, 0, 2, 2, 0];
    const TILESET_OFFSETS: [usize; 14] = [0, 5, 10, 15, 20, 20, 25, 20, 10, 20, 0, 5, 0, 20];
    const REPLACEMENT_BASE: usize = 0x26;
    match MODE.get(animation_index).copied().unwrap_or_default() {
        2 => {
            animation_index
                + TILESET_OFFSETS
                    .get(usize::from(tileset))
                    .copied()
                    .unwrap_or_default()
        }
        1 => {
            let trigger = TRIGGER_6_TO_13[animation_index - 6];
            let replacement = match trigger {
                0 if matches!(animation_index, 6 | 7 | 10) => {
                    view_state.conditional.invisible_pow_objects || view_state.blue_pow_active
                }
                0 => view_state.blue_pow_active,
                1 => view_state.silver_pow_active,
                2 => !view_state.conditional.on_off_switch_on,
                _ => false,
            };
            if replacement {
                REPLACEMENT_BASE + animation_index
            } else {
                animation_index
            }
        }
        _ => animation_index,
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_vanilla_common_animation_phases_with_tiles(
    project: &Project,
    graphics: &mut [IndexedTile],
    phases: &[u8; 19],
    tileset: u8,
    gfx33_decoded_tile_bias: usize,
    column_major_destinations: bool,
    view_state: VanillaAnimationViewState,
    gfx33_tiles: &[IndexedTile],
    gfx32_tiles: &[IndexedTile],
) -> Result<(), String> {
    const VRAM_DESTINATIONS: [usize; 24] = [
        0x600, 0x640, 0x680, 0x740, 0xea0, 0x800, 0x500, 0x540, 0x580, 0x5c0, 0x780, 0x7c0, 0xda0,
        0x6c0, 0x700, 0x4c0, 0x440, 0x480, 0x400, 0, 0, 0, 0, 0,
    ];
    const FRAME_TABLE_OFFSET: usize = 0x2_b999;
    const GFX32_SOURCE_BASE: usize = 0x2000;
    const GFX33_SOURCE_BASE: usize = 0x7d00;
    // RenderVanillaAnimationGroupFrame @ $0049DA10 indexes the decoded cache
    // through a $600-tile base. Relative to the raw $7D00 GFX33 source range,
    // the cache's GFX33 pixels begin after a $18-tile lead-in.
    const SOURCE_LIMIT: usize = 0xc800;
    const SNES_4BPP_TILE_BYTES: usize = 32;
    const TILES_PER_COPY: usize = 4;

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
        let source_index = vanilla_animation_source_index(animation_index, tileset, view_state);
        let table_word = source_index * 4 + phase;
        let table_word = legacy_animation_table_word(&project.rom, table_word)?;
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
            let source = (source_address - GFX33_SOURCE_BASE) / SNES_4BPP_TILE_BYTES
                + gfx33_decoded_tile_bias;
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
        } else if column_major_destinations {
            let destinations = [
                destination,
                destination + 0x10,
                destination + 1,
                destination + 0x11,
            ];
            let source_offsets = [0, 0x10, 1, 0x11];
            for (index, (target, source_offset)) in
                destinations.into_iter().zip(source_offsets).enumerate()
            {
                let source = if (GFX32_SOURCE_BASE..GFX33_SOURCE_BASE).contains(&source_address) {
                    let start = (source_address - GFX32_SOURCE_BASE) / SNES_4BPP_TILE_BYTES;
                    gfx32_tiles.get(start + source_offset)
                } else if (GFX33_SOURCE_BASE..SOURCE_LIMIT).contains(&source_address) {
                    let start = (source_address - GFX33_SOURCE_BASE) / SNES_4BPP_TILE_BYTES
                        + gfx33_decoded_tile_bias;
                    gfx33_tiles.get(start + source_offset)
                } else {
                    blank_tiles.get(index)
                }
                .ok_or_else(|| {
                    format!(
                        "column-major animation source offset {source_offset:X} is outside its decoded cache"
                    )
                })?;
                graphics
                    .get_mut(target)
                    .ok_or_else(|| {
                        format!(
                            "sprite animation destination tile {target:X} is outside {graphics_len} tiles"
                        )
                    })?
                    .clone_from(source);
            }
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

fn legacy_animation_table_word(rom: &lm_rom::RomImage, table_word: usize) -> Result<usize, String> {
    const FRAME_TABLE_OFFSET: usize = 0x2_b999;
    const PLACEHOLDER_START: usize = 136;
    const REPLACEMENT_START: usize = 56;
    const REPLACEMENT_WORDS: usize = 4;
    const PLACEHOLDER_MARKER: u16 = 0x9500;

    // Lunar Magic's legacy-format loader patches the four switch-block animation pointers at
    // runtime. LoadExAnimationFormatState @ 00459B80 checks the first $9500 placeholder at
    // DAT_00852B50, then copies the quartet at DAT_00852AB0 over it. Preserve that conditional:
    // modified ROMs with a real destination table must retain their own words.
    let marker_offset = FRAME_TABLE_OFFSET + PLACEHOLDER_START * size_of::<u16>();
    let marker = rom
        .logical_bytes()
        .get(marker_offset..marker_offset + size_of::<u16>())
        .ok_or_else(|| "legacy animation placeholder marker is outside the ROM".to_owned())?;
    if u16::from_le_bytes([marker[0], marker[1]]) == PLACEHOLDER_MARKER
        && (PLACEHOLDER_START..PLACEHOLDER_START + REPLACEMENT_WORDS).contains(&table_word)
    {
        Ok(REPLACEMENT_START + table_word - PLACEHOLDER_START)
    } else {
        Ok(table_word)
    }
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
    let mut raw = VanillaAnimationViewState::default();
    raw.conditional.invisible_pow_objects = false;
    raw.conditional.other_invisible_objects = false;
    render_map16_definition_atlas_with_view_state(definitions, graphics, palette, raw)
}

fn render_background_map16_bank_plane(
    definitions: &[u8],
    graphics: &[IndexedTile],
    palette: &Palette,
    tilemap: &[u16],
) -> Result<egui::ColorImage, String> {
    const TILE: usize = 16;
    const COLUMNS: usize = 32;
    const EXTENT: usize = COLUMNS * TILE;
    if definitions.len() != 0x1000 * lm_profile::SMW_US_V1_MAP16_TILE_BYTES {
        return Err(format!(
            "background Map16 bank has {} bytes instead of {}",
            definitions.len(),
            0x1000 * lm_profile::SMW_US_V1_MAP16_TILE_BYTES
        ));
    }
    if tilemap.len() != COLUMNS * COLUMNS {
        return Err(format!(
            "native Layer 2 tilemap has {} words instead of {}",
            tilemap.len(),
            COLUMNS * COLUMNS
        ));
    }
    let mut rgba = vec![0; EXTENT * EXTENT * 4];
    for y in 0..COLUMNS {
        for x in 0..COLUMNS {
            let source_index = lm_level::native_layer2_tilemap_index(x, y)
                .expect("bounded native Layer 2 coordinate");
            let tilemap_word = tilemap[source_index];
            let definition = usize::from(tilemap_word & 0x0fff);
            let tile_x_flip = tilemap_word & 0x4000 != 0;
            let tile_y_flip = tilemap_word & 0x8000 != 0;
            for quadrant in 0..4 {
                let word_offset =
                    definition * lm_profile::SMW_US_V1_MAP16_TILE_BYTES + quadrant * 2;
                let word =
                    u16::from_le_bytes([definitions[word_offset], definitions[word_offset + 1]]);
                let (mut quadrant_x, mut quadrant_y) = map16_quadrant_offset(quadrant);
                if tile_x_flip {
                    quadrant_x = TILE - 8 - quadrant_x;
                }
                if tile_y_flip {
                    quadrant_y = TILE - 8 - quadrant_y;
                }
                draw_subtile(
                    &mut rgba,
                    EXTENT,
                    (x * TILE + quadrant_x, y * TILE + quadrant_y),
                    graphics.get(usize::from(word & 0x03ff)),
                    palette,
                    usize::from((word >> 10) & 7),
                    (
                        (word & 0x4000 != 0) ^ tile_x_flip,
                        (word & 0x8000 != 0) ^ tile_y_flip,
                    ),
                );
            }
        }
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [EXTENT, EXTENT],
        &rgba,
    ))
}

fn render_map16_definition_atlas_with_view_state(
    definitions: &[u8],
    graphics: &[IndexedTile],
    palette: &Palette,
    view_state: VanillaAnimationViewState,
) -> egui::ColorImage {
    render_map16_definition_atlas_with_layer2_palette(
        definitions,
        graphics,
        palette,
        false,
        view_state,
    )
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
    let mut raw_layer2 = VanillaAnimationViewState::default();
    raw_layer2.conditional.invisible_pow_objects = false;
    raw_layer2.conditional.other_invisible_objects = false;
    render_layer2_map16_definition_atlas_with_view_state(
        definitions,
        graphics,
        palette,
        tileset,
        raw_layer2,
    )
}

fn render_layer2_map16_definition_atlas_with_view_state(
    definitions: &[u8],
    graphics: &[IndexedTile],
    palette: &Palette,
    tileset: u8,
    view_state: VanillaAnimationViewState,
) -> egui::ColorImage {
    render_map16_definition_atlas_with_layer2_palette(
        definitions,
        graphics,
        palette,
        tileset == 3,
        view_state,
    )
}

fn render_map16_definition_atlas_with_layer2_palette(
    definitions: &[u8],
    graphics: &[IndexedTile],
    palette: &Palette,
    shift_low_palette_rows: bool,
    view_state: VanillaAnimationViewState,
) -> egui::ColorImage {
    let width = 32 * 16;
    let height = 16 * 16;
    let mut rgba = vec![0; width * height * 4];
    for definition in 0..lm_profile::SMW_US_V1_MAP16_BASE_TILE_COUNT {
        let definition_x = definition % 32 * 16;
        let definition_y = definition / 32 * 16;
        if view_state.conditional.other_invisible_objects && (0x6f..=0x72).contains(&definition) {
            draw_other_invisible_object_overlay(
                &mut rgba,
                width,
                definition_x,
                definition_y,
                definition - 0x6f,
            );
            continue;
        }
        let (source_definition, half_blend) = map16_conditional_view_definition(
            definition,
            view_state.conditional,
            view_state.blue_pow_active,
        );
        for quadrant in 0..4 {
            let word_offset =
                source_definition * lm_profile::SMW_US_V1_MAP16_TILE_BYTES + quadrant * 2;
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
        if half_blend {
            for y in definition_y..definition_y + 16 {
                for x in definition_x..definition_x + 16 {
                    let alpha = (y * width + x) * 4 + 3;
                    if rgba[alpha] != 0 {
                        rgba[alpha] = 128;
                    }
                }
            }
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba)
}

fn draw_other_invisible_object_overlay(
    rgba: &mut [u8],
    width: usize,
    target_x: usize,
    target_y: usize,
    overlay: usize,
) {
    // PE resource type 500, ID 501 is a 64×16 24bpp strip. Lunar Magic indexes its four
    // 16×16 cells for Map16 $06F-$072, treats blue as transparent, and half-averages every
    // remaining pixel over the editor surface. These rows are the exact top-down resource cells.
    const COLORS: [[u8; 3]; 7] = [
        [0x00, 0x00, 0x00],
        [0xf8, 0xf8, 0xf8],
        [0x00, 0x00, 0xff],
        [0xff, 0x00, 0x00],
        [0x00, 0x78, 0x00],
        [0x00, 0xf8, 0x00],
        [0x00, 0xb8, 0x00],
    ];
    const ROWS: [[&str; 16]; 4] = [
        [
            "2222200000022222",
            "2220000330100222",
            "2201033330111022",
            "2011600330111102",
            "2016510330511102",
            "0465110330555640",
            "0465110330551140",
            "0165110330511110",
            "0116610330611110",
            "0114440330441140",
            "0144000000004440",
            "2000110110110002",
            "2201110110111022",
            "2201111111111022",
            "2220111111110222",
            "2222000000002222",
        ],
        [
            "2222200000022222",
            "2220003333000222",
            "2201033003301022",
            "2011600503301102",
            "2016511103301102",
            "0465111033055640",
            "0465110330551140",
            "0165103301511110",
            "0116033000011110",
            "0114033333301140",
            "0144000000004440",
            "2000110110110002",
            "2201110110111022",
            "2201111111111022",
            "2220111111110222",
            "2222000000002222",
        ],
        [
            "2222200000022222",
            "2220003333000222",
            "2201033003301022",
            "2011600503301102",
            "2016511003301102",
            "0465110333055640",
            "0465111003301140",
            "0165100103301110",
            "0116033003301110",
            "0114403333041140",
            "0144000000004440",
            "2000110110110002",
            "2201110110111022",
            "2201111111111022",
            "2220111111110222",
            "2222000000002222",
        ],
        [
            "2222200000022222",
            "2220040330100222",
            "2201103330111022",
            "2011603330111102",
            "2016033330511102",
            "0465033330555640",
            "0460330330551140",
            "0160333333011110",
            "0116000330611110",
            "0114440330441140",
            "0144000000004440",
            "2000110110110002",
            "2201110110111022",
            "2201111111111022",
            "2220111111110222",
            "2222000000002222",
        ],
    ];
    for (y, row) in ROWS[overlay].iter().enumerate() {
        for (x, value) in row.bytes().enumerate() {
            let color_index = usize::from(value - b'0');
            if color_index == 2 {
                continue;
            }
            let output = ((target_y + y) * width + target_x + x) * 4;
            rgba[output..output + 3].copy_from_slice(&COLORS[color_index]);
            rgba[output + 3] = 128;
        }
    }
}

const fn map16_conditional_view_definition(
    definition: usize,
    view_state: lm_render::LunarMagicConditionalViewState,
    blue_pow_active: bool,
) -> (usize, bool) {
    if view_state.other_invisible_objects {
        match definition {
            0x21 | 0x22 => return (0x114, true),
            0x23 => return (0x113, true),
            0x24 => return (0x115, true),
            _ => {}
        }
    }
    if view_state.invisible_pow_objects && !blue_pow_active && matches!(definition, 0x27..=0x2a) {
        return (definition, true);
    }
    (definition, false)
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
    let graphics_slots = load_layer1_sprite_graphics_slots(&project, graphics_files, true)?;
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

pub(crate) fn load_layer3_tiles(
    project: &Project,
    level: usize,
    graphics_layout: lm_project::GraphicsRomLayout,
) -> Result<Vec<IndexedTile>, String> {
    load_layer3_tiles_with_override(project, level, graphics_layout, &[0x7f; 8])
}

pub(crate) fn load_layer3_tiles_with_override(
    project: &Project,
    level: usize,
    graphics_layout: lm_project::GraphicsRomLayout,
    overrides: &[u16; 8],
) -> Result<Vec<IndexedTile>, String> {
    let settings = lm_profile::load_smw_us_v1_expanded_level_settings(project, level)
        .map_err(|error| error.to_string())?
        .settings;
    load_layer3_tiles_from_settings_with_override(project, &settings, graphics_layout, overrides)
}

fn load_layer3_tiles_from_settings(
    project: &Project,
    settings: &lm_level::ExpandedLevelSettingsRecord,
    graphics_layout: lm_project::GraphicsRomLayout,
) -> Result<Vec<IndexedTile>, String> {
    load_layer3_tiles_from_settings_with_override(project, settings, graphics_layout, &[0x7f; 8])
}

fn load_layer3_tiles_from_settings_with_override(
    project: &Project,
    settings: &lm_level::ExpandedLevelSettingsRecord,
    graphics_layout: lm_project::GraphicsRomLayout,
    overrides: &[u16; 8],
) -> Result<Vec<IndexedTile>, String> {
    let files = apply_display_override(
        [
            usize::from(settings.word(15).map_err(|error| error.to_string())? & 0x0fff),
            usize::from(settings.word(14).map_err(|error| error.to_string())? & 0x0fff),
            usize::from(settings.word(13).map_err(|error| error.to_string())? & 0x0fff),
            usize::from(settings.word(12).map_err(|error| error.to_string())? & 0x0fff),
            0x7f,
            0x7f,
            0x7f,
            0x7f,
        ],
        overrides,
    );
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
            .load_decompressed_graphics_file(file, graphics_layout)
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

fn load_layer1_sprite_graphics_slots<const N: usize>(
    project: &Project,
    files: [usize; N],
    convert_berry_gfx_tile: bool,
) -> Result<Vec<Vec<IndexedTile>>, String> {
    files
        .into_iter()
        .map(|file| {
            if file == 0x7f {
                return Ok(vec![
                    IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
                    LAYER1_SPRITE_SLOT_TILES
                ]);
            }
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
            if convert_berry_gfx_tile && matches!(file, 0x01 | 0x17 | 0x31) {
                synthesize_berry_tile_high_plane(&mut tiles);
            }
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

fn synthesize_berry_tile_high_plane(tiles: &mut [IndexedTile]) {
    if [0usize, 1, 0x10, 0x11].into_iter().any(|tile_index| {
        tiles
            .get(tile_index)
            .is_some_and(|tile| tile.pixels().iter().any(|pixel| pixel & 0x08 != 0))
    }) {
        return;
    }
    for tile_index in [0usize, 1, 0x10, 0x11] {
        let Some(tile) = tiles.get_mut(tile_index) else {
            return;
        };
        let converted = std::array::from_fn(|index| {
            let pixel = tile.pixels()[index];
            if pixel & 0x07 == 0 {
                pixel
            } else {
                pixel | 0x08
            }
        });
        *tile = IndexedTile::new(converted);
    }
}

const fn vanilla_graphics_bitplanes(decoded_len: usize) -> Option<u8> {
    match decoded_len {
        0x800 => Some(2),
        0x600 | 0xc00 => Some(3),
        0x1000 => Some(4),
        _ => None,
    }
}

fn materialize_layer1_sprite_vram(slots: &[Vec<IndexedTile>]) -> Vec<IndexedTile> {
    let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
    let mut tiles = vec![blank; slots.len().max(4) * LAYER1_SPRITE_SLOT_STRIDE];
    for (slot, source) in slots.iter().enumerate() {
        let start = slot * LAYER1_SPRITE_SLOT_STRIDE;
        let len = source.len().min(LAYER1_SPRITE_SLOT_TILES);
        tiles[start..start + len].clone_from_slice(&source[..len]);
    }
    tiles
}

/// Reinterprets Lunar Magic's foreground/background working buffer through the two diagnostic
/// 2bpp layouts selected by `$26B0`. Mode 1 decodes the first `$4000` raw bytes contiguously;
/// mode 2 decodes `$80` tiles from each of six `$1000` source bands. The ordinary 4bpp decode is
/// retained behind every overwritten destination, matching `DecodeLoadedLevelGraphicsCaches`.
fn apply_lunar_magic_two_bpp_view(tiles: &mut Vec<IndexedTile>, mode: u8) {
    if mode == 0 {
        return;
    }
    let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
    let source = tiles.clone();
    tiles.resize_with(0x600, || blank.clone());
    let split = |tile: &IndexedTile, high: bool| {
        IndexedTile::new(std::array::from_fn(|index| {
            let pixel = tile.pixels()[index];
            if high { pixel >> 2 } else { pixel & 3 }
        }))
    };
    match mode {
        1 => {
            for source_index in 0..0x200 {
                let tile = source.get(source_index).unwrap_or(&blank);
                tiles[source_index * 2] = split(tile, false);
                tiles[source_index * 2 + 1] = split(tile, true);
            }
        }
        2 => {
            for band in 0..6 {
                for within in 0..0x40 {
                    let source_index = band * 0x80 + within;
                    let tile = source.get(source_index).unwrap_or(&blank);
                    let destination = band * 0x80 + within * 2;
                    tiles[destination] = split(tile, false);
                    tiles[destination + 1] = split(tile, true);
                }
            }
        }
        _ => unreachable!("2bpp view mode is validated before materialization"),
    }
}

/// Map16 rendering divides the encoded three-bit palette by four in 2bpp mode and uses the
/// foreground reduced-color half beginning at CGRAM row 2.
fn apply_lunar_magic_two_bpp_palette_rows(palette: &mut Palette) {
    let original = palette.colors.clone();
    for encoded_row in 0..8 {
        let source_row = 2 + encoded_row / 4;
        let source = source_row * Palette::COLORS_PER_ROW;
        let destination = encoded_row * Palette::COLORS_PER_ROW;
        if source + Palette::COLORS_PER_ROW <= original.len()
            && destination + Palette::COLORS_PER_ROW <= palette.colors.len()
        {
            palette.colors[destination..destination + Palette::COLORS_PER_ROW]
                .clone_from_slice(&original[source..source + Palette::COLORS_PER_ROW]);
        }
    }
}

fn load_vanilla_sprite_display_page(project: &Project) -> Result<Vec<Vec<IndexedTile>>, String> {
    let decoded = load_smw_us_v1_special_graphics_file(project, true)?;
    let mut tiles = lm_graphics::decode_planar_tiles(&decoded, 3)
        .map_err(|error| format!("cannot decode pristine sprite-display GFX33: {error}"))?;
    let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
    tiles.resize_with(4 * LAYER1_SPRITE_SLOT_TILES, || blank.clone());
    Ok(tiles
        .chunks_exact(LAYER1_SPRITE_SLOT_TILES)
        .take(4)
        .map(<[IndexedTile]>::to_vec)
        .collect())
}

fn load_smw_us_v1_special_graphics_file(project: &Project, gfx33: bool) -> Result<Vec<u8>, String> {
    let layouts = lm_profile::smw_us_v1_special_graphics_layouts(&project.rom)
        .map_err(|error| error.to_string())?;
    project
        .load_decompressed_graphics_file(0, if gfx33 { layouts.gfx33 } else { layouts.gfx32 })
        .map_err(|error| error.to_string())
}

pub(crate) fn materialize_sprite_display_tiles(
    mut gfx33_tiles: Vec<IndexedTile>,
) -> Vec<IndexedTile> {
    let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
    gfx33_tiles.resize_with(4 * LAYER1_SPRITE_SLOT_TILES, || blank.clone());
    let slots = gfx33_tiles
        .chunks_exact(LAYER1_SPRITE_SLOT_TILES)
        .take(4)
        .map(<[IndexedTile]>::to_vec)
        .collect::<Vec<_>>();
    materialize_layer1_sprite_vram(&slots)
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

    fn tile_is_blank(tile: &IndexedTile) -> bool {
        tile.pixels().iter().all(|&pixel| pixel == 0)
    }

    #[test]
    fn embedded_application_icon_matches_original_moon_sprite() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let level = project
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let preview =
            render_with_editor_palette_phase(bytes, 0x105, level.layer1.header, false, false, 0)
                .unwrap();
        let parts = lm_render::render_lunar_magic_standard_sprite(0x6e, false).unwrap();
        let min_x = parts.iter().map(|part| part.x).min().unwrap();
        let min_y = parts.iter().map(|part| part.y).min().unwrap();
        let mut rgba = vec![0; 32 * 32 * 4];
        for part in parts {
            for (quadrant, word) in part.subtiles.into_iter().enumerate() {
                let x = usize::try_from(part.x - min_x).unwrap() + quadrant / 2 * 8;
                let y = usize::try_from(part.y - min_y).unwrap() + quadrant % 2 * 8;
                let subtile = lm_level::Subtile(word);
                draw_subtile(
                    &mut rgba,
                    32,
                    (x, y),
                    preview.sprite_tiles.get(usize::from(word & 0x01ff)),
                    &preview.palette,
                    8 + usize::from(subtile.palette()),
                    (subtile.x_flip(), subtile.y_flip()),
                );
            }
        }
        let icon = crate::app_icon::original_moon();
        assert_eq!((icon.width, icon.height), (32, 32));
        assert_eq!(icon.rgba, rgba);
    }

    #[test]
    fn berry_high_plane_conversion_matches_the_recovered_all_or_nothing_rule() {
        let mut tiles = vec![IndexedTile::new([1; 64]); 0x12];
        synthesize_berry_tile_high_plane(&mut tiles);
        for index in [0usize, 1, 0x10, 0x11] {
            assert!(tiles[index].pixels().iter().all(|pixel| *pixel == 9));
        }
        assert!(tiles[2].pixels().iter().all(|pixel| *pixel == 1));

        let mut guarded = vec![IndexedTile::new([1; 64]); 0x12];
        guarded[1] = IndexedTile::new([8; 64]);
        synthesize_berry_tile_high_plane(&mut guarded);
        assert!(guarded[0].pixels().iter().all(|pixel| *pixel == 1));
        assert!(guarded[1].pixels().iter().all(|pixel| *pixel == 8));
    }

    #[test]
    fn pristine_internal_graphics_cache_materializes_every_recovered_owned_bank() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let level = project
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let cache = load_pristine_internal_graphics_cache(bytes, 0x105, level.layer1.header, false)
            .unwrap();

        assert_eq!(cache.tiles.len(), INTERNAL_GRAPHICS_CACHE_TILES);
        assert!(cache.tiles[..0x200].iter().any(|tile| !tile_is_blank(tile)));
        assert!(cache.tiles[0x200..0x400].iter().all(tile_is_blank));
        assert!(
            cache.tiles[0x400..0x600]
                .iter()
                .any(|tile| !tile_is_blank(tile))
        );

        let gfx33 = load_smw_us_v1_special_graphics_file(&project, true).unwrap();
        let gfx33 = lm_graphics::decode_planar_tiles(&gfx33, 3).unwrap();
        assert_eq!(
            &cache.tiles[INTERNAL_GFX33_START..INTERNAL_GFX33_START + INTERNAL_GFX33_TILES],
            &gfx33[..INTERNAL_GFX33_TILES]
        );
        assert!(
            cache.tiles[INTERNAL_AUXILIARY_ANIMATION_START
                ..INTERNAL_AUXILIARY_ANIMATION_START + INTERNAL_AUXILIARY_ANIMATION_TILES]
                .iter()
                .all(tile_is_blank)
        );
        assert!(
            cache.tiles[0x880..INTERNAL_GFX32_START]
                .iter()
                .all(tile_is_blank)
        );

        let gfx32 = load_smw_us_v1_special_graphics_file(&project, false).unwrap();
        let gfx32 = lm_graphics::decode_planar_tiles(&gfx32, 4).unwrap();
        assert_eq!(
            &cache.tiles[INTERNAL_GFX32_START..INTERNAL_GFX32_START + INTERNAL_GFX32_TILES],
            &gfx32[..INTERNAL_GFX32_TILES]
        );
        assert!(
            cache.tiles[INTERNAL_GFX32_START + INTERNAL_GFX32_TILES..INTERNAL_EXANIMATION_START]
                .iter()
                .all(tile_is_blank)
        );
        assert!(
            cache.tiles[INTERNAL_EXANIMATION_START
                ..INTERNAL_EXANIMATION_START + INTERNAL_EXANIMATION_TILES]
                .iter()
                .all(tile_is_blank)
        );

        let layer3 = load_layer3_tiles(
            &project,
            0x105,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .unwrap();
        assert_eq!(
            &cache.tiles[INTERNAL_LAYER3_START..INTERNAL_LAYER3_START + INTERNAL_LAYER3_TILES],
            layer3.as_slice()
        );
        assert!(
            cache.tiles[INTERNAL_EXTERNAL_SPRITE_START..]
                .iter()
                .all(tile_is_blank)
        );
    }

    #[test]
    fn profiled_internal_graphics_cache_matches_pristine_level_banks() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let project = Project::new(image.clone());
        let level = project
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let expected =
            load_pristine_internal_graphics_cache(bytes, 0x105, level.layer1.header, false)
                .unwrap();
        let mut profile = lm_profile::test_support::profile();
        profile.mapper = lm_rom::Mapper::LoRom;
        profile.level = lm_profile::smw_us_v1_vanilla_level_layout();
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        profile.sprite_lengths = lm_level::SpriteLengthTable::standard();
        profile.expanded_settings = None;

        profile.exanimation_installation = lm_project::InstalledLayout::Absent;
        let actual =
            load_profiled_internal_graphics_cache(image, &profile, 0x105, false, None).unwrap();
        assert_eq!(actual.tiles.len(), INTERNAL_GRAPHICS_CACHE_TILES);
        assert_eq!(
            &actual.tiles[INTERNAL_SPRITE_START..INTERNAL_GFX33_START],
            &expected.tiles[INTERNAL_SPRITE_START..INTERNAL_GFX33_START]
        );
        assert_eq!(
            &actual.tiles[INTERNAL_GFX33_START..INTERNAL_AUXILIARY_ANIMATION_START],
            &expected.tiles[INTERNAL_GFX33_START..INTERNAL_AUXILIARY_ANIMATION_START]
        );
        assert_eq!(
            &actual.tiles[INTERNAL_GFX32_START..INTERNAL_EXANIMATION_START],
            &expected.tiles[INTERNAL_GFX32_START..INTERNAL_EXANIMATION_START]
        );
        assert_eq!(
            &actual.tiles[INTERNAL_LAYER3_START..INTERNAL_EXTERNAL_SPRITE_START],
            &expected.tiles[INTERNAL_LAYER3_START..INTERNAL_EXTERNAL_SPRITE_START]
        );
    }

    #[test]
    fn exanimation_source_files_map_to_four_exact_bounded_banks() {
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let source = (0..0x420)
            .map(|index| IndexedTile::new([u8::try_from(index & 0x0f).unwrap(); 64]))
            .collect::<Vec<_>>();
        let mut cache = vec![blank.clone(); INTERNAL_GRAPHICS_CACHE_TILES];
        for bank in 0..4 {
            copy_exanimation_source_bank(bank, &source, &mut cache).unwrap();
            let start = INTERNAL_EXANIMATION_START + bank * 0x400;
            assert_eq!(&cache[start..start + 0x400], &source[..0x400]);
        }
        assert!(copy_exanimation_source_bank(4, &source, &mut cache).is_err());
    }

    #[test]
    fn auxiliary_cache_file_follows_expanded_header_or_last_legacy_graphics_control() {
        let objects = lm_level::ObjectStream {
            records: vec![
                lm_level::ObjectRecord::new(vec![0x40, 0x50, 0x21]).unwrap(),
                // Music bypass `$26` is not a graphics-file selector.
                lm_level::ObjectRecord::new(vec![0x40, 0x60, 0x43]).unwrap(),
            ],
        };
        assert_eq!(profiled_auxiliary_graphics_file(None, &objects), Some(0x20));

        let mut expanded = lm_level::ExpandedLevelHeader { fields: [0; 16] };
        expanded.fields[0] = 0x8000 | 0x345;
        assert_eq!(
            profiled_auxiliary_graphics_file(Some(expanded), &objects),
            Some(0x345)
        );
        expanded.fields[0] = 0x0345;
        assert_eq!(
            profiled_auxiliary_graphics_file(Some(expanded), &objects),
            Some(0x20)
        );
    }

    #[test]
    fn profiled_cache_places_external_sprite_files_at_all_eight_recovered_bases() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(bytes).unwrap();
        let mut profile = lm_profile::test_support::profile();
        profile.mapper = lm_rom::Mapper::LoRom;
        profile.level = lm_profile::smw_us_v1_vanilla_level_layout();
        profile.graphics = lm_profile::smw_us_v1_vanilla_graphics_layout();
        profile.sprite_lengths = lm_level::SpriteLengthTable::standard();
        profile.expanded_settings = None;
        profile.exanimation_installation = lm_project::InstalledLayout::Absent;
        let mut assets = lm_graphics::ExternalSpriteAssets::default();
        for slot in 0..lm_graphics::EXTERNAL_SPRITE_GRAPHICS_SLOTS {
            let tile = IndexedTile::new([u8::try_from(slot + 1).unwrap(); 64]);
            assets
                .set_graphics_slot(slot, &lm_graphics::encode_4bpp_tile(&tile).unwrap())
                .unwrap();
        }

        let cache =
            load_profiled_internal_graphics_cache(image, &profile, 0x105, false, Some(&assets))
                .unwrap();
        for slot in 0..lm_graphics::EXTERNAL_SPRITE_GRAPHICS_SLOTS {
            let index = INTERNAL_EXTERNAL_SPRITE_START + slot * 0x400;
            assert_eq!(
                cache.tiles[index].pixels(),
                &[u8::try_from(slot + 1).unwrap(); 64]
            );
            assert!(
                cache.tiles[index + 1..index + 0x400]
                    .iter()
                    .all(tile_is_blank)
            );
        }
    }

    #[test]
    fn secondary_action_six_preserves_signed_left_boundary_without_primary_helper() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let level = project
            .load_level_slot(
                0x001,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let palette = lm_profile::compose_smw_us_v1_level_palette(
            &project,
            0x001,
            game_palette_header(0x001, level.layer1.header),
            0,
        )
        .unwrap()
        .palette;
        let marker = render_secondary_entrance_marker(&project, &palette, 6).unwrap();
        assert_eq!(marker.size, [48, 32]);
        let pixel = |x: usize, y: usize| marker.pixels[y * marker.size[0] + x];
        assert_eq!(pixel(15, 16), egui::Color32::BLACK);
        assert_eq!(pixel(15, 17), egui::Color32::WHITE);
        assert_eq!(pixel(15, 27), egui::Color32::from_rgb(140, 90, 24));
        assert_eq!(pixel(14, 27), egui::Color32::from_rgb(255, 222, 115));
        assert_eq!(pixel(12, 25), egui::Color32::TRANSPARENT);
    }

    #[test]
    fn editor_palette_materializes_all_vanilla_dragon_coin_colors() {
        let mut palette = Palette {
            colors: vec![Bgr555(0); 256],
        };
        let expected = [
            Bgr555(0x02df),
            Bgr555(0x035f),
            Bgr555(0x27ff),
            Bgr555(0x5fff),
            Bgr555(0x73ff),
            Bgr555(0x5fff),
            Bgr555(0x27ff),
            Bgr555(0x035f),
        ];
        for (phase, expected) in expected.into_iter().enumerate() {
            apply_vanilla_editor_palette_animation(&mut palette, phase);
            assert_eq!(palette.colors[0x64], expected);
        }
        apply_vanilla_editor_palette_animation(&mut palette, 10);
        assert_eq!(palette.colors[0x64], Bgr555(0x27ff));
        assert_eq!(
            palette.colors[0x64].to_rgb8(),
            Rgb8 {
                red: 255,
                green: 255,
                blue: 74,
            }
        );
    }

    #[test]
    fn explicit_editor_palette_phase_is_independent_of_map16_animation() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let project = Project::new(image);
        let level = project
            .load_level_slot(
                0x010,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let preview =
            render_with_editor_palette_phase(bytes, 0x010, level.layer1.header, false, false, 1)
                .unwrap();
        assert_eq!(preview.palette.colors[0x64], Bgr555(0x035f));
    }

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
        atlas.pixels[16 + 15] = egui::Color32::BLUE;
        atlas.pixels[15 * 512 + 16] = egui::Color32::YELLOW;
        atlas.pixels[15 * 512 + 16 + 15] = egui::Color32::WHITE;
        let tile_two = 2 * 16;
        atlas.pixels[tile_two] = egui::Color32::GREEN;
        let mut tilemap = vec![0; 32 * 32];
        tilemap[lm_level::native_layer2_tilemap_index(0, 0).unwrap()] = 1;
        tilemap[lm_level::native_layer2_tilemap_index(1, 0).unwrap()] = 0x4001;
        tilemap[lm_level::native_layer2_tilemap_index(2, 0).unwrap()] = 0x8001;
        tilemap[lm_level::native_layer2_tilemap_index(3, 0).unwrap()] = 0xc001;
        tilemap[lm_level::native_layer2_tilemap_index(31, 31).unwrap()] = 2;

        let plane = compose_native_map16_plane(&atlas, &tilemap).unwrap();
        assert_eq!(plane.size, [512, 512]);
        assert_eq!(plane.pixels[0], egui::Color32::RED);
        assert_eq!(plane.pixels[16], egui::Color32::BLUE);
        assert_eq!(plane.pixels[32], egui::Color32::YELLOW);
        assert_eq!(plane.pixels[48], egui::Color32::WHITE);
        assert_eq!(
            plane.pixels[(31 * 16) * 512 + 31 * 16],
            egui::Color32::GREEN
        );
        assert_eq!(plane.pixels[64], egui::Color32::TRANSPARENT);
    }

    #[test]
    fn native_background_plane_rejects_inexact_inputs() {
        let atlas = egui::ColorImage::new([511, 256], egui::Color32::TRANSPARENT);
        assert!(compose_native_map16_plane(&atlas, &[0; 1024]).is_err());
        let atlas = egui::ColorImage::new([512, 256], egui::Color32::TRANSPARENT);
        assert!(compose_native_map16_plane(&atlas, &[0; 1023]).is_err());
    }

    #[test]
    fn native_background_bank_plane_renders_all_4096_indexes_and_flips() {
        let mut atlas = egui::ColorImage::new([512, 2048], egui::Color32::TRANSPARENT);
        let definition = 0x0fedusize;
        let source_x = definition % 32 * 16;
        let source_y = definition / 32 * 16;
        atlas.pixels[source_y * 512 + source_x] = egui::Color32::RED;
        atlas.pixels[(source_y + 15) * 512 + source_x + 15] = egui::Color32::BLUE;
        let mut tilemap = vec![0; 1024];
        tilemap[0] = definition as u16;
        tilemap[1] = definition as u16 | 0xc000;
        let plane = compose_native_map16_bank_plane(&atlas, &tilemap).unwrap();
        assert_eq!(plane.pixels[0], egui::Color32::RED);
        assert_eq!(plane.pixels[16], egui::Color32::BLUE);
    }

    #[test]
    fn active_background_bank_raster_reads_high_definition_indexes_directly() {
        let mut definitions = vec![0; 0x1000 * 8];
        let definition = 0x0fedusize;
        for quadrant in 0..4 {
            let offset = definition * 8 + quadrant * 2;
            definitions[offset..offset + 2].copy_from_slice(&1u16.to_le_bytes());
        }
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let solid = IndexedTile::new([1; IndexedTile::PIXEL_COUNT]);
        let mut colors = vec![lm_graphics::Bgr555(0); 256];
        colors[1] = lm_graphics::Bgr555::from_rgb8(lm_graphics::Rgb8 {
            red: 255,
            green: 0,
            blue: 0,
        });
        let palette = Palette { colors };
        let mut tilemap = vec![0; 1024];
        tilemap[0] = definition as u16;
        let plane =
            render_background_map16_bank_plane(&definitions, &[blank, solid], &palette, &tilemap)
                .unwrap();
        assert_eq!(plane.size, [512, 512]);
        assert_eq!(plane.pixels[0], egui::Color32::RED);
        assert_eq!(plane.pixels[15 * 512 + 15], egui::Color32::RED);
        assert_eq!(plane.pixels[16], egui::Color32::TRANSPARENT);
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
    fn layer3_16x16_modes_use_recovered_metatile_quadrants_and_plane_extents() {
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let mut graphics = vec![blank; 0x400];
        graphics[0x10] = IndexedTile::new([1; IndexedTile::PIXEL_COUNT]);
        graphics[0x11] = IndexedTile::new([2; IndexedTile::PIXEL_COUNT]);
        graphics[0x20] = IndexedTile::new([3; IndexedTile::PIXEL_COUNT]);
        graphics[0x21] = IndexedTile::new([1; IndexedTile::PIXEL_COUNT]);
        let mut colors = vec![Bgr555(0); 256];
        colors[1] = Bgr555::from_rgb8(Rgb8 {
            red: 255,
            green: 0,
            blue: 0,
        });
        colors[2] = Bgr555::from_rgb8(Rgb8 {
            red: 0,
            green: 255,
            blue: 0,
        });
        colors[3] = Bgr555::from_rgb8(Rgb8 {
            red: 0,
            green: 0,
            blue: 255,
        });
        let palette = Palette { colors };
        let mut tilemap = vec![0x38fc; lm_profile::SMW_US_V1_LAYER3_TILEMAP_WORDS];
        tilemap[0] = 0x10;

        let (mode_1, _) = render_layer3_planes_with_mode(&tilemap, &graphics, &palette, false, 1);
        assert_eq!(mode_1.size, [512, 512]);
        assert_eq!(mode_1.pixels[0], egui::Color32::RED);
        assert_eq!(mode_1.pixels[8], egui::Color32::GREEN);
        assert_eq!(mode_1.pixels[8 * 512], egui::Color32::BLUE);
        assert_eq!(mode_1.pixels[8 * 512 + 8], egui::Color32::RED);

        let (mode_2, _) = render_layer3_planes_with_mode(&tilemap, &graphics, &palette, false, 2);
        assert_eq!(mode_2.size, [1024, 1024]);
        assert_eq!(mode_2.pixels[8], egui::Color32::GREEN);
        assert_eq!(mode_2.pixels[8 * 1024], egui::Color32::BLUE);
    }

    #[test]
    fn static_layer3_editor_offsets_follow_lunar_magics_mode_state() {
        use lm_profile::SmwUsV1Layer3Behavior::{HighTide, LowTide, Static};

        assert_eq!(vanilla_layer3_editor_row_offset(LowTide, 0), Some(-2));
        assert_eq!(vanilla_layer3_editor_row_offset(HighTide, 0), Some(-8));
        assert_eq!(
            vanilla_layer3_editor_row_offset(Static { code: 0x80 }, 0),
            Some(1)
        );
        assert_eq!(
            vanilla_layer3_editor_row_offset(Static { code: 0x81 }, 1),
            Some(0)
        );
        assert_eq!(
            vanilla_layer3_editor_row_offset(Static { code: 0x81 }, 3),
            Some(0)
        );
        assert_eq!(
            vanilla_layer3_editor_row_offset(Static { code: 0x81 }, 0x0d),
            Some(0)
        );
        assert_eq!(
            vanilla_layer3_editor_row_offset(Static { code: 0x81 }, 9),
            Some(0)
        );
        assert_eq!(
            vanilla_layer3_editor_row_offset(Static { code: 0x82 }, 1),
            None
        );
        assert!(vanilla_layer3_between_background_and_foreground(Static {
            code: 0x80
        }));
        assert!(!vanilla_layer3_between_background_and_foreground(Static {
            code: 0x81
        }));
        assert!(vanilla_layer3_additive(0, Static { code: 0x81 }, 9));
        assert!(vanilla_layer3_additive(0x0e, Static { code: 0x82 }, 1));
        assert!(!vanilla_layer3_additive(0, Static { code: 0x81 }, 3));
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
    fn vanilla_pow_animation_sources_follow_lunar_magics_trigger_table() {
        let ordinary = VanillaAnimationViewState::default();
        assert_eq!(
            (6..=13)
                .map(|index| vanilla_animation_source_index(index, 0, ordinary))
                .collect::<Vec<_>>(),
            [0x2c, 0x2d, 8, 9, 0x30, 11, 12, 13]
        );

        let blue = VanillaAnimationViewState {
            blue_pow_active: true,
            ..ordinary
        };
        assert_eq!(
            (6..=13)
                .map(|index| vanilla_animation_source_index(index, 0, blue))
                .collect::<Vec<_>>(),
            [0x2c, 0x2d, 0x2e, 9, 0x30, 11, 12, 0x33]
        );

        let silver = VanillaAnimationViewState {
            silver_pow_active: true,
            ..ordinary
        };
        assert_eq!(vanilla_animation_source_index(9, 0, silver), 0x2f);
        let mut invisible_off = ordinary;
        invisible_off.conditional.invisible_pow_objects = false;
        assert_eq!(vanilla_animation_source_index(6, 0, invisible_off), 6);
        let mut on_off_clear = ordinary;
        on_off_clear.conditional.on_off_switch_on = false;
        assert_eq!(vanilla_animation_source_index(11, 0, on_off_clear), 0x31);
        assert_eq!(vanilla_animation_source_index(14, 6, ordinary), 39);
    }

    #[test]
    fn invisible_object_views_select_and_half_blend_the_recovered_map16_definitions() {
        let ordinary = lm_render::LunarMagicConditionalViewState::default();
        assert_eq!(
            map16_conditional_view_definition(0x21, ordinary, false),
            (0x114, true)
        );
        assert_eq!(
            map16_conditional_view_definition(0x24, ordinary, false),
            (0x115, true)
        );
        assert_eq!(
            map16_conditional_view_definition(0x27, ordinary, false),
            (0x27, true)
        );
        assert_eq!(
            map16_conditional_view_definition(0x27, ordinary, true),
            (0x27, false)
        );
        assert_eq!(
            map16_conditional_view_definition(
                0x21,
                lm_render::LunarMagicConditionalViewState {
                    other_invisible_objects: false,
                    ..ordinary
                },
                false,
            ),
            (0x21, false)
        );
    }

    #[test]
    fn embedded_default_m16_bank_retains_editor_only_block_content_definitions() {
        const DEFINITIONS: &[u8; 0x2000] = include_bytes!("assets/lm363-default-m16.bin");
        let words = |tile: usize| &DEFINITIONS[tile * 8..tile * 8 + 8];
        assert_eq!(
            words(0x104),
            [0x27, 0x54, 0x37, 0x54, 0x26, 0x54, 0x36, 0x54]
        );
        assert_eq!(
            words(0x219),
            [0x49, 0x50, 0x59, 0x50, 0x48, 0x50, 0x58, 0x50]
        );
        assert_eq!(
            words(0x21a),
            [0xea, 0x00, 0xea, 0x80, 0xea, 0x00, 0xea, 0x80]
        );
    }

    #[test]
    fn other_invisible_overlays_match_the_embedded_resource_color_histogram() {
        let mut rgba = vec![0; 64 * 16 * 4];
        for overlay in 0..4 {
            draw_other_invisible_object_overlay(&mut rgba, 64, overlay * 16, 0, overlay);
        }
        let mut histogram = std::collections::BTreeMap::new();
        let mut transparent = 0;
        for pixel in rgba.chunks_exact(4) {
            if pixel[3] == 0 {
                transparent += 1;
            } else {
                assert_eq!(pixel[3], 128);
                *histogram.entry([pixel[0], pixel[1], pixel[2]]).or_insert(0) += 1;
            }
        }
        assert_eq!(transparent, 192);
        assert_eq!(
            histogram,
            std::collections::BTreeMap::from([
                ([0x00, 0x00, 0x00], 334),
                ([0x00, 0x78, 0x00], 55),
                ([0x00, 0xb8, 0x00], 31),
                ([0x00, 0xf8, 0x00], 35),
                ([0xf8, 0xf8, 0xf8], 274),
                ([0xff, 0x00, 0x00], 103),
            ])
        );
    }

    #[test]
    fn pristine_animation_mode_and_trigger_bytes_match_the_recovered_rom_tables() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        assert_eq!(
            &bytes[0x2_b96b..0x2_b983],
            &[
                0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 0, 0, 0, 0, 0
            ]
        );
        assert_eq!(&bytes[0x2_b983..0x2_b98b], &[0, 0, 0, 1, 0, 2, 2, 0]);
    }

    #[test]
    fn pow_view_states_materialize_distinct_authenticated_vram_groups() {
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let project = Project::new(image);
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let mut ordinary = vec![blank.clone(); 0x800];
        apply_vanilla_common_animation_frame_with_view_state(
            &project,
            &mut ordinary,
            0,
            0,
            VanillaAnimationViewState::default(),
        )
        .unwrap();

        let mut blue = vec![blank.clone(); 0x800];
        apply_vanilla_common_animation_frame_with_view_state(
            &project,
            &mut blue,
            0,
            0,
            VanillaAnimationViewState {
                blue_pow_active: true,
                silver_pow_active: false,
                ..VanillaAnimationViewState::default()
            },
        )
        .unwrap();
        assert_ne!(&ordinary[0x58..0x5c], &blue[0x58..0x5c]);

        let mut silver = vec![blank; 0x800];
        apply_vanilla_common_animation_frame_with_view_state(
            &project,
            &mut silver,
            0,
            0,
            VanillaAnimationViewState {
                blue_pow_active: false,
                silver_pow_active: true,
                ..VanillaAnimationViewState::default()
            },
        )
        .unwrap();
        assert_ne!(&ordinary[0x5c..0x60], &silver[0x5c..0x60]);
    }

    #[test]
    fn legacy_switch_animation_placeholder_uses_lunar_magics_runtime_quartet() {
        const FRAME_TABLE_OFFSET: usize = 0x2_b999;
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        assert_eq!(legacy_animation_table_word(&image, 135).unwrap(), 135);
        assert_eq!(legacy_animation_table_word(&image, 136).unwrap(), 56);
        assert_eq!(legacy_animation_table_word(&image, 139).unwrap(), 59);
        assert_eq!(legacy_animation_table_word(&image, 140).unwrap(), 140);

        let mut modified = image.logical_bytes().to_vec();
        modified[FRAME_TABLE_OFFSET + 136 * 2..FRAME_TABLE_OFFSET + 136 * 2 + 2]
            .copy_from_slice(&0xA500_u16.to_le_bytes());
        let modified = RomImage::from_bytes(modified).unwrap();
        assert_eq!(legacy_animation_table_word(&modified, 136).unwrap(), 136);
        assert_eq!(legacy_animation_table_word(&modified, 139).unwrap(), 139);
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
        let preview = render(bytes, 0, level.layer1.header, true, false).unwrap();
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
        assert_eq!(preview.foreground_tiles.len(), LAYER1_DISPLAY_TILES);
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
        assert_eq!(preview.animated_sprite_images.len(), 4);
        assert_eq!(
            preview.animated_sprite_images[0],
            preview.animated_sprite_images[1]
        );
        let sprite_display_page = load_vanilla_sprite_display_page(&project).unwrap();
        assert_eq!(sprite_display_page.len(), 4);
        assert_eq!(sprite_display_page[0][0x48], animated[0x48]);
        assert_eq!(sprite_display_page[2][0], animated[0x100]);
        assert!(
            sprite_display_page[3]
                .iter()
                .all(|tile| tile.pixels().iter().all(|pixel| *pixel == 0)),
            "GFX33's unused fourth 128-tile display slot must be transparent"
        );
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
    fn special_world_view_materializes_gfx31_in_the_sp2_working_slot() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let level = project
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let preview = render(bytes, 0x105, level.layer1.header, false, true).unwrap();
        let special = project
            .load_decompressed_graphics_file(0x31, lm_profile::smw_us_v1_vanilla_graphics_layout())
            .unwrap();
        let mut special = lm_graphics::decode_planar_tiles(&special, 3).unwrap();
        special.resize_with(128, || IndexedTile::new([0; IndexedTile::PIXEL_COUNT]));
        let unconverted = special.clone();
        synthesize_berry_tile_high_plane(&mut special);

        assert_eq!(preview.sprite_graphics_files[1], 0x31);
        assert_eq!(&preview.sprite_tiles[128..256], special.as_slice());

        let disabled = render_with_animation_view_state_background_bank_and_berry_conversion(
            project.rom.as_file_bytes().to_vec(),
            0x105,
            level.layer1.header,
            false,
            true,
            VanillaAnimationViewState::default(),
            0,
            None,
            false,
        )
        .unwrap();
        assert_eq!(&disabled.sprite_tiles[128..256], unconverted.as_slice());
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
                    usize::from(slot),
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
        let slots = load_layer1_sprite_graphics_slots(&project, files, true).unwrap();
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
                0,
                false,
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
        if std::env::var_os("LM_TRACE_SPRITE_CACHE").is_some() {
            let files = lm_profile::smw_us_v1_sprite_tileset_graphics_files(
                &project.rom,
                usize::from(level.layer1.header.sprite_tileset()),
            )
            .unwrap();
            let sprite_slots = load_layer1_sprite_graphics_slots(&project, files, true).unwrap();
            let sprite_tiles = materialize_layer1_sprite_vram(&sprite_slots);
            for cache_tile_offset in [0, 0x200, 0x400] {
                let differing = sprite_tiles
                    .iter()
                    .enumerate()
                    .filter_map(|(tile, actual)| {
                        let start = (cache_tile_offset + tile) * IndexedTile::PIXEL_COUNT;
                        (actual.pixels().as_slice()
                            != &expected[start..start + IndexedTile::PIXEL_COUNT])
                            .then_some(tile)
                    })
                    .collect::<Vec<_>>();
                eprintln!(
                    "sprite graphics at cache tile ${cache_tile_offset:03X}: {} / {} matching; differing {differing:02X?}",
                    sprite_tiles.len() - differing.len(),
                    sprite_tiles.len(),
                );
            }
        }
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
        let actual = load_layer3_tiles(
            &project,
            slot,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
        )
        .unwrap();
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
        let editor = render(bytes.clone(), 1, raw, false, false).unwrap();
        let runtime = render(bytes, 1, raw, true, false).unwrap();
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
        let preview = render(bytes, 1, header, true, false).unwrap();
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

#[cfg(test)]
mod two_bpp_tests {
    use super::*;

    #[test]
    fn two_bpp_view_modes_match_recovered_contiguous_and_banded_decodes() {
        let source = (0..0x600)
            .map(|tile| {
                IndexedTile::new(std::array::from_fn(|pixel| {
                    ((tile / 64 + pixel) & 0x0f).to_le_bytes()[0]
                }))
            })
            .collect::<Vec<_>>();
        let mut contiguous = source.clone();
        apply_lunar_magic_two_bpp_view(&mut contiguous, 1);
        assert_eq!(contiguous.len(), 0x600);
        assert_eq!(contiguous[0].pixels()[7], source[0].pixels()[7] & 3);
        assert_eq!(contiguous[1].pixels()[7], source[0].pixels()[7] >> 2);
        assert_eq!(
            contiguous[0x3ff].pixels(),
            source[0x1ff].pixels().map(|p| p >> 2).as_slice()
        );

        let mut banded = source.clone();
        apply_lunar_magic_two_bpp_view(&mut banded, 2);
        assert_eq!(banded[0x80].pixels()[3], source[0x80].pixels()[3] & 3);
        assert_eq!(banded[0x81].pixels()[3], source[0x80].pixels()[3] >> 2);
        assert_ne!(contiguous[0x80], banded[0x80]);
        assert_eq!(banded[0x300], source[0x300]);
    }

    #[test]
    fn two_bpp_palette_routes_four_encoded_rows_to_each_reduced_row() {
        let mut palette = Palette {
            colors: (0..256).map(lm_graphics::Bgr555).collect(),
        };
        apply_lunar_magic_two_bpp_palette_rows(&mut palette);
        for row in 0..8 {
            assert_eq!(
                &palette.colors[row * 16..row * 16 + 16],
                &((2 + row / 4) * 16..(3 + row / 4) * 16)
                    .map(|value| lm_graphics::Bgr555(value as u16))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn pristine_level_105_renders_three_distinct_two_bpp_view_states() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let level = project
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let render_mode = |mode| {
            render_with_animation_view_state(
                bytes.clone(),
                0x105,
                level.layer1.header,
                false,
                false,
                VanillaAnimationViewState {
                    two_bpp_mode: mode,
                    ..VanillaAnimationViewState::default()
                },
            )
            .unwrap()
            .image
        };
        let normal = render_mode(0);
        let contiguous = render_mode(1);
        let banded = render_mode(2);
        assert_ne!(normal, contiguous);
        assert_ne!(normal, banded);
        assert_ne!(contiguous, banded);
    }

    #[test]
    fn gfx_display_override_replaces_only_non_7f_slots() {
        let files = [0x14, 0x17, 0x19, 0x15];
        let mut overrides = [0x7f; 8];
        overrides[1] = 0x123;
        overrides[3] = 0;
        assert_eq!(
            apply_display_override(files, &overrides),
            [0x14, 0x123, 0x19, 0]
        );
    }

    #[test]
    fn gfx_display_override_changes_preview_without_mutating_rom() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
        let project = Project::new(image);
        let level = project
            .load_level_slot(
                0x105,
                lm_profile::smw_us_v1_vanilla_level_layout(),
                &lm_level::SpriteLengthTable::standard(),
            )
            .unwrap();
        let ordinary = render_with_animation_view_state(
            bytes.clone(),
            0x105,
            level.layer1.header,
            false,
            false,
            VanillaAnimationViewState::default(),
        )
        .unwrap();
        let mut state = VanillaAnimationViewState::default();
        state.gfx_display_override.layer_1_2[0] = 0;
        state.gfx_display_override.layer_1_2[7] = 0x28;
        state.gfx_display_override.layer_3[0] = 0x29;
        let overridden = render_with_animation_view_state(
            bytes.clone(),
            0x105,
            level.layer1.header,
            false,
            false,
            state,
        )
        .unwrap();
        assert_eq!(overridden.graphics_files[0], 0);
        assert_ne!(ordinary.image, overridden.image);
        assert_eq!(overridden.foreground_tiles.len(), LAYER1_DISPLAY_TILES);
        assert!(
            overridden.foreground_tiles[7 * LAYER1_SPRITE_SLOT_TILES..]
                .iter()
                .any(|tile| tile.pixels().iter().any(|&pixel| pixel != 0))
        );
        assert_ne!(ordinary.layer3_tiles, overridden.layer3_tiles);
        assert_eq!(bytes, crate::test_support::pristine_smw_us_rom_bytes());
    }
}
