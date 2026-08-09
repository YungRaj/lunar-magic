use super::{
    AppState, MainLayer2Workspace, PendingClose, PendingLoad, PendingOpen, RomOverworldEditor,
    Workspace, egui,
};
use crate::{
    document_loader::{BoundedRead, LoadedDocument},
    level_editor_forms,
};
use lm_app::{PaletteOwnershipFile, RevisionProfileControllers};
use lm_graphics::{Bgr555, GraphicsFile4bpp, GraphicsInterchangeFile, IndexedTile};
use lm_level::Map16SetFile;
use lm_project::Project;
use lm_rom::{RomImage, detect_identity};

const OVERWORLD_GRAPHICS_FILES: [usize; 4] = [0x1c, 0x1d, 0x1e, 0x1f];
const TILES_PER_NATIVE_GRAPHICS_SLOT: usize = 0x80;
const NATIVE_SPRITE_CACHE_TILES: usize = 0x3100;
const NATIVE_BASE_SUBMAP_STRIDE: usize = 0x400;
const NATIVE_SPRITE_SUBMAP_BASE: usize = 0x1c00;
const NATIVE_SPRITE_SUBMAP_STRIDE: usize = 0x200;
const NATIVE_ANIMATED_SUBMAP_BASE: usize = 0x2a00;
const NATIVE_ANIMATED_SUBMAP_STRIDE: usize = 0x100;
const VANILLA_SHARED_PALETTE_TABLE_OFFSET: usize = 0x30a0;
const VANILLA_LEVEL_DOT_CYCLE_OFFSETS: [usize; 2] = [0x56c, 0x57c];
const VANILLA_LIGHTNING_DELAYS_OFFSET: usize = 0x276f8;
const VANILLA_LIGHTNING_INITIAL_COLORS_OFFSET: usize = 0x27700;
const VANILLA_LIGHTNING_SELECTORS_OFFSET: usize = 0x27708;

impl RomOverworldEditor {
    pub(crate) fn handles(app: &AppState) -> bool {
        let Some(identity) = app.project().and_then(|project| project.identity.as_ref()) else {
            return false;
        };
        matches!(app.mode, lm_app::EditorMode::Overworld)
            && identity.game == lm_rom::SupportedGame::SuperMarioWorld
            && identity.region == lm_rom::Region::NorthAmerica
            && identity.revision == 0
            && identity.mapper == lm_rom::Mapper::LoRom
    }

    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
            || self.main_layer2_workspace.is_some()
            || self.pending_open.is_some()
            || self.loader.is_running()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        match app.profiled_controller_snapshot() {
            Ok(profiled) if profiled.snapshot.mode == lm_app::EditorMode::Overworld => {
                self.pending_open = Some(PendingOpen {
                    profiled,
                    slot: "0".into(),
                    rom_path: app.document_path.clone(),
                });
            }
            Ok(_) => self.error = Some("switch to overworld mode before opening the editor".into()),
            Err(profile_error) => match app
                .controller_snapshot()
                .map_err(|error| error.to_string())
                .and_then(decode_main_layer2_workspace)
            {
                Ok(workspace) => {
                    self.main_layer2_workspace = Some(workspace);
                    self.search_start.clear();
                    self.search_end.clear();
                    self.layer = 1;
                    self.x = 0;
                    self.y = 0;
                    self.paint_anchor = None;
                    self.map16_page = 0;
                    self.main_path = Default::default();
                    self.load_main_path_link();
                    self.invalidate();
                }
                Err(native_error) => {
                    self.error = Some(format!(
                        "{profile_error}; built-in playable Layer 2 open also failed: {native_error}"
                    ));
                }
            },
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.manifest_loader.is_running()
            || self.loader.is_running()
            || self.transfer_loader.is_running()
            || self.transfer_persistence.is_running()
        {
            self.error = Some("wait for overworld file work to finish before closing".into());
            return false;
        }
        if self.pending_open.is_some() {
            self.pending_open = None;
            return true;
        }
        let modified = self.workspace.as_ref().is_some_and(|workspace| {
            workspace.controller.is_modified()
                || workspace.assets.animation_options != workspace.baseline_animation_options
        }) || self
            .main_layer2_workspace
            .as_ref()
            .is_some_and(|workspace| {
                workspace.controller.is_modified() || workspace.paths != workspace.original_paths
            });
        if !modified {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Editor
        });
        false
    }

    pub(super) fn open_dialog(&mut self, context: &egui::Context) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new("Open native overworld")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Profile overworld slot (hex)");
                if let Some(pending) = self.pending_open.as_mut() {
                    ui.text_edit_singleline(&mut pending.slot);
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_open = None;
                    }
                    if ui.button("Open").clicked() {
                        self.finish_open();
                    }
                });
            });
    }

    fn finish_open(&mut self) {
        let Some(pending) = self.pending_open.take() else {
            return;
        };
        let slot = match parse_slot(&pending.slot) {
            Ok(slot) => slot,
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending);
                return;
            }
        };
        let Some(path) = crate::dialogs::choose_palette_ownership() else {
            self.pending_open = Some(pending);
            return;
        };
        let mut requests = vec![BoundedRead::new(
            path,
            u64::try_from(PaletteOwnershipFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "palette ownership evidence",
        )];
        if let Some(rom_path) = pending.rom_path.as_ref() {
            requests.push(BoundedRead::optional(
                rom_path.with_extension("sscov"),
                u64::try_from(lm_overworld::SSCOV_MAX_BYTES).unwrap_or(u64::MAX),
                "ROM-adjacent native overworld sprite display definitions",
            ));
            requests.push(BoundedRead::optional(
                rom_path.with_extension("s16ov"),
                u64::try_from(lm_level::S16OvSidecar::CAPACITY).unwrap_or(u64::MAX),
                "ROM-adjacent native overworld Sprite Map16 definitions",
            ));
            requests.extend(crate::ssc_sidecar_editor::external_sprite_requests(
                &rom_path.with_extension("sscov"),
            ));
        }
        match self.loader.start(requests) {
            Ok(()) => {
                self.pending_load = Some(PendingLoad {
                    open: pending,
                    slot,
                });
            }
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending);
            }
        }
    }

    pub(super) fn finish_ownership_load(
        &mut self,
        result: Result<LoadedDocument, String>,
        current_revision: u64,
    ) {
        let Some(pending) = self.pending_load.take() else {
            self.error = Some("overworld ownership load lost its open request".into());
            return;
        };
        match result.and_then(|loaded| decode_loaded(&pending, loaded, current_revision)) {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                self.search_start.clear();
                self.search_end.clear();
                self.x = 0;
                self.y = 0;
                self.paint_anchor = None;
                self.map16_page = 0;
                self.invalidate();
            }
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending.open);
            }
        }
    }

    pub(super) fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged overworld changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(if self.main_layer2_workspace.is_some() {
                    "Playable terrain or route-link changes have not been committed."
                } else {
                    "Overworld payload or per-map animation-option changes have not been committed."
                });
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_close = None;
                    }
                    if ui.button("Discard").clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    pub(super) fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("ROM overworld error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.main_layer2_workspace = None;
        self.pending_open = None;
        self.pending_load = None;
        self.pending_close = None;
        self.transfer_kind = None;
        self.paint_anchor = None;
        self.texture = None;
        self.map16_texture = None;
        self.direct_tile_texture = None;
        self.direct_tile_rendered_palette = None;
        self.invalidate();
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn decode_main_layer2_workspace(
    snapshot: lm_app::ControllerSnapshot,
) -> Result<MainLayer2Workspace, String> {
    if snapshot.mode != lm_app::EditorMode::Overworld {
        return Err("switch to overworld mode before opening the editor".into());
    }
    let controller = lm_app::SmwMainOverworldLayer2Controller::decode(&snapshot)
        .map_err(|error| error.to_string())?;
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let project = Project::new(image.clone());
    let paths = project
        .load_overworld_path_links_detected(lm_profile::smw_us_v1_overworld_path_patch_locator())
        .map_err(|error| format!("could not load gameplay route links: {error}"))?
        .table;
    let mut map16_snapshot = snapshot.clone();
    map16_snapshot.mode = lm_app::EditorMode::Map16;
    let map16 =
        lm_app::SmwMap16Controller::decode(&map16_snapshot).map_err(|error| error.to_string())?;
    let mut tiles =
        Vec::with_capacity(OVERWORLD_GRAPHICS_FILES.len() * TILES_PER_NATIVE_GRAPHICS_SLOT);
    for file_number in OVERWORLD_GRAPHICS_FILES {
        let slot = project
            .load_graphics_file(file_number, lm_profile::smw_us_v1_vanilla_graphics_layout())
            .map_err(|error| format!("could not load overworld GFX{file_number:02X}: {error}"))?
            .tiles;
        append_overworld_graphics_slot(&mut tiles, file_number, slot)?;
    }
    let mut palette = project
        .load_shared_palette(lm_profile::smw_us_v1_shared_palette_layout())
        .map_err(|error| error.to_string())?
        .palette()
        .map_err(|error| error.to_string())?;
    // The legacy `.smwpal` backend retains one non-row tail color. Rendering consumes complete
    // 16-color SNES CGRAM rows, so keep every complete row and leave that auxiliary tail intact in
    // the ROM rather than inventing a palette entry.
    let complete_colors = palette.colors.len() / 16 * 16;
    palette.colors.truncate(complete_colors);
    let (gfx32, gfx33) =
        load_overworld_special_graphics(&project, lm_profile::smw_us_v1_vanilla_graphics_layout())?;
    Ok(MainLayer2Workspace {
        controller,
        original_paths: paths.clone(),
        paths,
        palette,
        assets: crate::overworld_editor_render::OverworldAssets {
            map16: Map16SetFile {
                set: map16.set().clone(),
            },
            graphics: GraphicsInterchangeFile {
                source_slot: u16::try_from(OVERWORLD_GRAPHICS_FILES[0]).unwrap_or_default(),
                graphics: GraphicsFile4bpp { tiles },
            },
            native_sprite_graphics_cache: load_native_sprite_graphics_cache(
                &project,
                lm_profile::smw_us_v1_vanilla_graphics_layout(),
            )?,
            external_sprite_assets: lm_graphics::ExternalSpriteAssets::default(),
            gfx32,
            gfx33,
            built_in_animation_addresses: load_builtin_overworld_animation_addresses(&project)?,
            built_in_level_dot_palette: load_builtin_overworld_level_dot_palette(&project)?,
            built_in_lightning: load_builtin_overworld_lightning(&project)?,
            animation_options: crate::overworld_editor_render::vanilla_overworld_animation_options(
            ),
            animation_options_runtime_installed: false,
            animation_options_layout_supported: true,
            animation_lightning_unused_low_bit: true,
            global_animation: None,
        },
    })
}

fn parse_slot(value: &str) -> Result<u16, String> {
    level_editor_forms::parse_hex_u16(value, "overworld slot")
}

fn decode_loaded(
    pending: &PendingLoad,
    loaded: LoadedDocument,
    current_revision: u64,
) -> Result<Workspace, String> {
    crate::rom_load::ensure_current_revision(
        pending.open.profiled.snapshot.revision,
        current_revision,
        "overworld ownership evidence",
    )?;
    let mut files = loaded.files.into_iter();
    let (_, bytes) = files
        .next()
        .ok_or("overworld ownership loader omitted its required first input")?;
    let ownership = PaletteOwnershipFile::decode(&bytes)
        .map_err(|error| error.to_string())?
        .ownership;
    let profiled = pending.open.profiled.clone();
    let controller = profiled
        .profile
        .decode_overworld(
            &profiled.snapshot,
            usize::from(pending.slot),
            ownership.clone(),
        )
        .map_err(|error| error.to_string())?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let mut assets = decode_overworld_assets(&profiled)?;
    let definitions_path = pending
        .open
        .rom_path
        .as_ref()
        .map(|path| path.with_extension("sscov"));
    let map16_path = pending
        .open
        .rom_path
        .as_ref()
        .map(|path| path.with_extension("s16ov"));
    let (native_files, external_files): (Vec<_>, Vec<_>) = files.partition(|(path, _)| {
        Some(path) == definitions_path.as_ref() || Some(path) == map16_path.as_ref()
    });
    let native_appearances =
        decode_native_appearance_siblings(pending.open.rom_path.as_deref(), native_files)?;
    assets.external_sprite_assets =
        crate::ssc_sidecar_editor::decode_external_sprite_assets(external_files.into_iter())?;
    let baseline_animation_options = assets.animation_options;
    Ok(Workspace {
        controller,
        profiled,
        slot: pending.slot,
        image,
        ownership,
        assets,
        baseline_animation_options,
        native_appearances,
    })
}

fn decode_native_appearance_siblings(
    rom_path: Option<&std::path::Path>,
    files: Vec<(std::path::PathBuf, Vec<u8>)>,
) -> Result<Option<lm_render::NativeOverworldAppearancePair>, String> {
    let Some(rom_path) = rom_path else {
        if files.is_empty() {
            return Ok(None);
        }
        return Err("overworld loader returned native sidecars without a ROM document path".into());
    };
    let definitions_path = rom_path.with_extension("sscov");
    let map16_path = rom_path.with_extension("s16ov");
    let mut definitions = None;
    let mut map16 = None;
    for (path, bytes) in files {
        if path == definitions_path && definitions.is_none() {
            definitions = Some(bytes);
        } else if path == map16_path && map16.is_none() {
            map16 = Some(bytes);
        } else {
            return Err(format!(
                "overworld loader returned an unexpected or duplicate sidecar: {}",
                path.display()
            ));
        }
    }
    if definitions.is_none() && map16.is_none() {
        return Ok(None);
    }
    Ok(Some(lm_render::NativeOverworldAppearancePair {
        definitions: lm_overworld::NativeOverworldSpriteSidecar::decode(
            definitions.as_deref().unwrap_or_default(),
        )
        .map_err(|error| error.to_string())?,
        sprite_map16: lm_level::S16OvSidecar::decode(map16.as_deref().unwrap_or_default())
            .map_err(|error| error.to_string())?,
    }))
}

fn decode_overworld_assets(
    profiled: &lm_app::ProfiledControllerSnapshot,
) -> Result<crate::overworld_editor_render::OverworldAssets, String> {
    let map16 = profiled
        .profile
        .decode_map16(&profiled.snapshot)
        .map_err(|error| error.to_string())?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let project = Project::new(image);
    let mut tiles =
        Vec::with_capacity(OVERWORLD_GRAPHICS_FILES.len() * TILES_PER_NATIVE_GRAPHICS_SLOT);
    for file_number in OVERWORLD_GRAPHICS_FILES {
        let slot = project
            .load_graphics_file(file_number, profiled.profile.graphics)
            .map_err(|error| format!("could not load overworld GFX{file_number:02X}: {error}"))?
            .tiles;
        append_overworld_graphics_slot(&mut tiles, file_number, slot)?;
    }
    let (gfx32, gfx33) = load_overworld_special_graphics(&project, profiled.profile.graphics)?;
    let animation_options_layout_supported = profiled.profile.game
        == lm_rom::SupportedGame::SuperMarioWorld
        && profiled.profile.region == lm_rom::Region::NorthAmerica
        && profiled.profile.revision == 0
        && profiled.profile.mapper == lm_rom::Mapper::LoRom;
    let animation_options = if animation_options_layout_supported {
        lm_profile::detect_smw_us_v1_overworld_animation_runtime(project.rom.logical_bytes())
            .map_err(|error| {
                format!("could not authenticate overworld animation runtime: {error}")
            })?;
        project
            .load_installed_overworld_animation_options(
                lm_profile::smw_us_v1_overworld_animation_options_layout(),
            )
            .map_err(|error| error.to_string())?
    } else {
        lm_project::LoadedOverworldAnimationOptions {
            feature_bytes: [0; 7],
            lightning_disable_mask: 0xf7,
            runtime_installed: false,
        }
    };
    Ok(crate::overworld_editor_render::OverworldAssets {
        map16: Map16SetFile {
            set: map16.set().clone(),
        },
        graphics: GraphicsInterchangeFile {
            source_slot: u16::try_from(OVERWORLD_GRAPHICS_FILES[0]).unwrap_or_default(),
            graphics: GraphicsFile4bpp { tiles },
        },
        native_sprite_graphics_cache: load_native_sprite_graphics_cache(
            &project,
            profiled.profile.graphics,
        )?,
        external_sprite_assets: lm_graphics::ExternalSpriteAssets::default(),
        gfx32,
        gfx33,
        built_in_animation_addresses: load_builtin_overworld_animation_addresses(&project)?,
        built_in_level_dot_palette: load_builtin_overworld_level_dot_palette(&project)?,
        built_in_lightning: load_builtin_overworld_lightning(&project)?,
        animation_options: crate::overworld_editor_render::decode_overworld_animation_options(
            animation_options.feature_bytes,
            animation_options.lightning_disable_mask,
        ),
        animation_options_runtime_installed: animation_options.runtime_installed,
        animation_options_layout_supported,
        animation_lightning_unused_low_bit: animation_options.lightning_disable_mask & 1 != 0,
        global_animation: load_global_overworld_exanimation(&project, &profiled.profile)?,
    })
}

fn load_global_overworld_exanimation(
    project: &Project,
    profile: &lm_profile::RevisionProfile,
) -> Result<Option<lm_graphics::CompactExAnimation>, String> {
    match project.load_installed_global_exanimation(
        profile.exanimation_installation,
        &profile.exanimation_double_size_modes,
    ) {
        Ok(lm_project::InstalledAsset::Present(animation)) => Ok(Some(animation)),
        Ok(lm_project::InstalledAsset::SubsystemAbsent | lm_project::InstalledAsset::SlotEmpty) => {
            Ok(None)
        }
        Err(lm_project::ExAnimationIoError::GlobalPointerLocatorUnavailable) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn load_builtin_overworld_level_dot_palette(
    project: &Project,
) -> Result<Option<[[Bgr555; 8]; 2]>, String> {
    let logical = project.rom.logical_bytes();
    let mut cycles = [[Bgr555(0); 8]; 2];
    for (cycle, relative) in cycles.iter_mut().zip(VANILLA_LEVEL_DOT_CYCLE_OFFSETS) {
        let start = VANILLA_SHARED_PALETTE_TABLE_OFFSET + relative;
        let bytes = logical
            .get(start..start + 16)
            .ok_or("vanilla overworld level-dot palette cycle is outside the ROM")?;
        for (color, pair) in cycle.iter_mut().zip(bytes.chunks_exact(2)) {
            *color = Bgr555(u16::from_le_bytes([pair[0], pair[1]]));
        }
    }
    // The descriptor's shared-palette block is relocatable in installed ROMs. Until that
    // descriptor is modeled, authenticate the vanilla table shape instead of animating from an
    // unrelated region at the old address.
    if cycles.iter().flatten().any(|color| color.0 & 0x8000 != 0)
        || cycles
            .iter()
            .any(|cycle| cycle.iter().all(|color| *color == cycle[0]))
    {
        Ok(None)
    } else {
        Ok(Some(cycles))
    }
}

fn load_builtin_overworld_lightning(
    project: &Project,
) -> Result<Option<crate::overworld_editor_render::BuiltInOverworldLightning>, String> {
    let logical = project.rom.logical_bytes();
    let delays: [u8; 8] = logical
        .get(VANILLA_LIGHTNING_DELAYS_OFFSET..VANILLA_LIGHTNING_DELAYS_OFFSET + 8)
        .ok_or("vanilla overworld lightning delay table is outside the ROM")?
        .try_into()
        .expect("fixed eight-byte lightning delay slice");
    let initial_colors: [u8; 8] = logical
        .get(VANILLA_LIGHTNING_INITIAL_COLORS_OFFSET..VANILLA_LIGHTNING_INITIAL_COLORS_OFFSET + 8)
        .ok_or("vanilla overworld lightning color table is outside the ROM")?
        .try_into()
        .expect("fixed eight-byte lightning color slice");
    let selectors: [u8; 128] = logical
        .get(VANILLA_LIGHTNING_SELECTORS_OFFSET..VANILLA_LIGHTNING_SELECTORS_OFFSET + 128)
        .ok_or("vanilla overworld lightning selector table is outside the ROM")?
        .try_into()
        .expect("fixed 128-byte lightning selector slice");
    if delays.contains(&0)
        || initial_colors.iter().any(|&color| !(1..=7).contains(&color))
        // The selector deliberately aliases the first 128 bytes of the vanilla routine. These
        // opcodes authenticate that we did not reinterpret a relocated/modified routine.
        || selectors[..8] != [0xa9, 0xf7, 0x20, 0x82, 0xf8, 0xd0, 0x5f, 0xac]
    {
        return Ok(None);
    }
    Ok(Some(
        crate::overworld_editor_render::BuiltInOverworldLightning {
            selectors,
            delays,
            initial_colors,
        },
    ))
}

fn load_builtin_overworld_animation_addresses(project: &Project) -> Result<Vec<u16>, String> {
    let mapper = project
        .identity
        .as_ref()
        .map(|identity| identity.mapper)
        .or_else(|| {
            detect_identity(&project.rom)
                .ok()
                .map(|identity| identity.mapper)
        })
        .ok_or("built-in overworld animation table requires an authenticated ROM identity")?;
    lm_profile::load_smw_us_v1_builtin_overworld_animation_table_for_mapper(&project.rom, mapper)
        .map(|table| table.addresses.to_vec())
        .map_err(|error| error.to_string())
}

fn load_overworld_special_graphics(
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
            .map_err(|error| format!("could not resolve overworld special graphics: {error}"))?;
        (0, 0, special.gfx33, special.gfx32)
    };
    let gfx33 = project
        .load_decompressed_graphics_file(gfx33_file, gfx33_layout)
        .map_err(|error| format!("could not load overworld ExAnimation GFX33: {error}"))?;
    let gfx33 = lm_graphics::decode_planar_tiles(&gfx33, 3)
        .map_err(|error| format!("could not decode overworld ExAnimation GFX33: {error}"))?;
    let gfx32 = project
        .load_decompressed_graphics_file(gfx32_file, gfx32_layout)
        .map_err(|error| format!("could not load overworld ExAnimation GFX32: {error}"))?;
    let gfx32 = lm_graphics::decode_planar_tiles(&gfx32, 4)
        .map_err(|error| format!("could not decode overworld ExAnimation GFX32: {error}"))?;
    Ok((gfx32, gfx33))
}

fn load_native_sprite_graphics_cache(
    project: &Project,
    graphics_layout: lm_project::GraphicsRomLayout,
) -> Result<Vec<IndexedTile>, String> {
    let settings = lm_profile::load_smw_us_v1_overworld_settings(project)
        .map_err(|error| format!("could not load overworld graphics settings: {error}"))?
        .settings;
    let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
    let mut cache = vec![blank.clone(); NATIVE_SPRITE_CACHE_TILES];
    for (submap, record) in settings.records.iter().enumerate() {
        let base = submap * NATIVE_BASE_SUBMAP_STRIDE;
        for (slot, file_number) in native_base_graphics_files(record)?.into_iter().enumerate() {
            load_native_graphics_cache_slot(
                project,
                graphics_layout,
                &mut cache,
                &blank,
                submap,
                "base",
                file_number,
                base + slot * TILES_PER_NATIVE_GRAPHICS_SLOT,
                TILES_PER_NATIVE_GRAPHICS_SLOT,
            )?;
        }
        let base = NATIVE_SPRITE_SUBMAP_BASE + submap * NATIVE_SPRITE_SUBMAP_STRIDE;
        for (slot, file_number) in native_sprite_graphics_files(record)?
            .into_iter()
            .enumerate()
        {
            load_native_graphics_cache_slot(
                project,
                graphics_layout,
                &mut cache,
                &blank,
                submap,
                "sprite",
                file_number,
                base + slot * TILES_PER_NATIVE_GRAPHICS_SLOT,
                TILES_PER_NATIVE_GRAPHICS_SLOT,
            )?;
        }
        load_native_graphics_cache_slot(
            project,
            graphics_layout,
            &mut cache,
            &blank,
            submap,
            "animated",
            usize::from(record.word(0).map_err(|error| error.to_string())? & 0x0fff),
            NATIVE_ANIMATED_SUBMAP_BASE + submap * NATIVE_ANIMATED_SUBMAP_STRIDE,
            NATIVE_ANIMATED_SUBMAP_STRIDE,
        )?;
    }
    Ok(cache)
}

#[allow(clippy::too_many_arguments)]
fn load_native_graphics_cache_slot(
    project: &Project,
    graphics_layout: lm_project::GraphicsRomLayout,
    cache: &mut [IndexedTile],
    blank: &IndexedTile,
    submap: usize,
    domain: &str,
    file_number: usize,
    start: usize,
    capacity: usize,
) -> Result<(), String> {
    if file_number == 0x7f {
        return Ok(());
    }
    let mut tiles = project
        .load_graphics_file(file_number, graphics_layout)
        .map_err(|error| {
            format!(
                "could not load overworld submap {submap} {domain} GFX{file_number:02X}: {error}"
            )
        })?
        .tiles;
    if tiles.len() > capacity {
        return Err(format!(
            "overworld submap {submap} {domain} GFX{file_number:02X} has {} tiles; expected at most {capacity}",
            tiles.len()
        ));
    }
    tiles.resize(capacity, blank.clone());
    let destination = cache
        .get_mut(start..start + capacity)
        .ok_or("internal native overworld graphics cache layout overflow")?;
    destination.clone_from_slice(&tiles);
    Ok(())
}

fn native_base_graphics_files(
    record: &lm_level::ExpandedLevelSettingsRecord,
) -> Result<[usize; 8], String> {
    let mut files = [0; 8];
    for (slot, word) in (0_usize..=7).rev().enumerate() {
        files[slot] = usize::from(record.word(word).map_err(|error| error.to_string())? & 0x0fff);
    }
    Ok(files)
}

fn native_sprite_graphics_files(
    record: &lm_level::ExpandedLevelSettingsRecord,
) -> Result<[usize; 4], String> {
    let mut files = [0; 4];
    for (slot, word) in [11_usize, 10, 9, 8].into_iter().enumerate() {
        files[slot] = usize::from(record.word(word).map_err(|error| error.to_string())? & 0x0fff);
    }
    Ok(files)
}

fn append_overworld_graphics_slot(
    destination: &mut Vec<IndexedTile>,
    file_number: usize,
    mut slot: Vec<IndexedTile>,
) -> Result<(), String> {
    if slot.len() > TILES_PER_NATIVE_GRAPHICS_SLOT {
        return Err(format!(
            "overworld GFX{file_number:02X} has {} tiles; expected at most {TILES_PER_NATIVE_GRAPHICS_SLOT}",
            slot.len()
        ));
    }
    slot.resize_with(TILES_PER_NATIVE_GRAPHICS_SLOT, || {
        IndexedTile::new([0; IndexedTile::PIXEL_COUNT])
    });
    destination.extend(slot);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        TILES_PER_NATIVE_GRAPHICS_SLOT, append_overworld_graphics_slot,
        decode_main_layer2_workspace, decode_native_appearance_siblings,
        load_builtin_overworld_animation_addresses, load_builtin_overworld_level_dot_palette,
        load_builtin_overworld_lightning, native_base_graphics_files, native_sprite_graphics_files,
        parse_slot,
    };
    use lm_graphics::IndexedTile;
    use lm_project::Project;
    use lm_rom::{Mapper, RomImage, detect_identity};
    use std::{fs, path::Path};

    #[test]
    fn configured_overworld_slot_is_bound_as_hexadecimal() {
        assert_eq!(parse_slot("0").unwrap(), 0);
        assert_eq!(parse_slot("01ff").unwrap(), 0x01ff);
        assert!(parse_slot("").is_err());
        assert!(parse_slot("10000").is_err());
        assert!(parse_slot("not-a-slot").is_err());
    }

    #[test]
    fn native_overworld_sprite_slots_follow_recovered_reverse_word_order() {
        let record = lm_profile::smw_us_v1_default_special_expanded_settings_record();
        assert_eq!(
            native_base_graphics_files(&record).unwrap(),
            [0x1c, 0x1d, 0x08, 0x1e, 0x7f, 0x7f, 0x7f, 0x14]
        );
        assert_eq!(
            native_sprite_graphics_files(&record).unwrap(),
            [0x10, 0x0f, 0x1c, 0x1d]
        );
    }

    #[test]
    fn native_overworld_graphics_slots_keep_vram_boundaries() {
        let marked = IndexedTile::new([7; IndexedTile::PIXEL_COUNT]);
        let mut tiles = Vec::new();
        append_overworld_graphics_slot(&mut tiles, 0x1c, vec![marked.clone()]).unwrap();
        append_overworld_graphics_slot(&mut tiles, 0x1d, vec![marked.clone(); 2]).unwrap();
        assert_eq!(tiles.len(), TILES_PER_NATIVE_GRAPHICS_SLOT * 2);
        assert_eq!(tiles[0], marked);
        assert_eq!(
            tiles[TILES_PER_NATIVE_GRAPHICS_SLOT],
            IndexedTile::new([7; IndexedTile::PIXEL_COUNT])
        );
        assert!(tiles[1].pixels().iter().all(|pixel| *pixel == 0));
        assert!(
            append_overworld_graphics_slot(
                &mut tiles,
                0x1e,
                vec![
                    IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
                    TILES_PER_NATIVE_GRAPHICS_SLOT + 1
                ],
            )
            .is_err()
        );
    }

    #[test]
    fn rom_adjacent_native_appearance_siblings_are_optional_independent_and_lossless() {
        let rom = Path::new("World 日本語.smc");
        assert_eq!(
            decode_native_appearance_siblings(Some(rom), Vec::new()).unwrap(),
            None
        );
        let definitions = b"05\t3\t-2,4,8400\n10000\t12\t400-4FF,1234\n".to_vec();
        let pair = decode_native_appearance_siblings(
            Some(rom),
            vec![(rom.with_extension("sscov"), definitions)],
        )
        .unwrap()
        .unwrap();
        assert!(pair.definitions.appearances[&5].shadow);
        assert_eq!(pair.sprite_map16.loaded_len(), 0);

        let pair = decode_native_appearance_siblings(
            Some(rom),
            vec![(rom.with_extension("s16ov"), vec![1, 0, 0, 0, 2])],
        )
        .unwrap()
        .unwrap();
        assert!(pair.definitions.appearances.is_empty());
        assert_eq!(pair.sprite_map16.loaded_len(), 5);
    }

    #[test]
    fn rom_adjacent_native_appearance_group_rejects_unknown_duplicate_and_malformed_files() {
        let rom = Path::new("World.smc");
        assert!(
            decode_native_appearance_siblings(
                Some(rom),
                vec![(rom.with_extension("other"), Vec::new())],
            )
            .is_err()
        );
        assert!(
            decode_native_appearance_siblings(
                Some(rom),
                vec![
                    (rom.with_extension("sscov"), Vec::new()),
                    (rom.with_extension("sscov"), Vec::new()),
                ],
            )
            .is_err()
        );
        assert!(
            decode_native_appearance_siblings(
                Some(rom),
                vec![(rom.with_extension("sscov"), b"05\t3\t0,0,D00\n".to_vec())],
            )
            .is_err()
        );
        assert!(
            decode_native_appearance_siblings(None, vec![("x.sscov".into(), Vec::new())]).is_err()
        );
    }

    #[test]
    fn authentic_lunar_magic_rom_opens_and_renders_profile_free_playable_layer2_workspace() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc");
        let physical = fs::read(fixture).unwrap();
        let physical_image = RomImage::from_bytes(physical.clone()).unwrap();
        let variants = [physical, physical_image.logical_bytes().to_vec()];
        let mut logical_results = Vec::new();
        let mut rendered_pixels = None;

        for original_bytes in variants {
            let original_image = RomImage::from_bytes(original_bytes.clone()).unwrap();
            let original_header = original_image.copier_header_bytes().map(<[u8]>::to_vec);
            let mut app = lm_app::AppState::default();
            app.load_rom(original_bytes.clone()).unwrap();
            app.dispatch(lm_app::Command::ShowOverworld).unwrap();
            let mut workspace =
                decode_main_layer2_workspace(app.controller_snapshot().unwrap()).unwrap();
            assert_eq!(
                (
                    workspace.controller.layer().width,
                    workspace.controller.layer().height,
                ),
                (128, 64)
            );
            assert_eq!(workspace.assets.map16.set.pages.len(), 0x100);
            assert_eq!(workspace.assets.graphics.graphics.tiles.len(), 0x200);
            assert_eq!(workspace.assets.built_in_animation_addresses.len(), 67);
            assert!(workspace.assets.built_in_level_dot_palette.is_some());
            assert!(workspace.assets.built_in_lightning.is_some());
            assert_eq!(workspace.paths, workspace.original_paths);
            assert!(!workspace.paths.links.is_empty());
            let canvas = lm_render::render_smw_overworld_layer2_tilemap(
                workspace.controller.layer(),
                &workspace.assets.graphics,
                &workspace.palette,
            )
            .unwrap();
            assert_eq!((canvas.width(), canvas.height()), (1024, 512));
            if rendered_pixels.is_none()
                && let Some(path) = std::env::var_os("LM_OVERWORLD_LAYER2_SCREENSHOT_TO")
            {
                fs::write(path, lm_render::encode_png(&canvas).unwrap()).unwrap();
            }
            assert!(
                canvas
                    .pixels()
                    .iter()
                    .any(|pixel| pixel.red != 0 || pixel.green != 0 || pixel.blue != 0)
            );
            let pixels = canvas.pixels().to_vec();
            if let Some(expected) = rendered_pixels.as_ref() {
                assert_eq!(&pixels, expected);
            } else {
                rendered_pixels = Some(pixels);
            }

            let original_tile = workspace.controller.layer().tile(12, 9).unwrap();
            let replacement = original_tile ^ 1;
            let cells = [(12, 9), (13, 9), (12, 10), (13, 10)];
            let edits = cells
                .into_iter()
                .map(|(x, y)| lm_app::OverworldControllerEdit::SetLayerTile {
                    layer: lm_app::OverworldLayerId::Layer2,
                    x,
                    y,
                    tile: replacement,
                })
                .collect::<Vec<_>>();
            workspace.controller.apply_edits(&edits).unwrap();
            let command = workspace
                .controller
                .prepare_commit(
                    "visual Layer 2 paint regression",
                    lm_rats::AllocationPolicy {
                        search: 0x0e_0000..0x0f_0000,
                        bank_size: Some(0x8000),
                        fill_bytes: vec![0xff, 0],
                        protected: vec![
                            lm_rats::ProtectedRange(
                                lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD
                                    ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD + 2,
                            ),
                            lm_rats::ProtectedRange(
                                lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK
                                    ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK + 1,
                            ),
                            lm_rats::ProtectedRange(
                                lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD
                                    ..lm_profile::SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD + 2,
                            ),
                            lm_rats::ProtectedRange(0x7fdc..0x7fe0),
                        ],
                    },
                )
                .unwrap()
                .into_command();
            app.dispatch(command).unwrap();

            let original_paths = workspace.paths.clone();
            workspace.paths.links[0].destination.x ^= 1;
            workspace.paths.links[0].target.x_tile ^= 1;
            app.dispatch(lm_app::Command::ReplaceNativeOverworldPathLinks {
                rev: app.project_revision(),
                table: Box::new(workspace.paths.clone()),
            })
            .unwrap();

            let project = app.project().unwrap();
            let reopened = lm_profile::load_smw_us_v1_main_overworld_layer2(project).unwrap();
            for (x, y) in cells {
                assert_eq!(reopened.layer.tile(x, y).unwrap(), replacement);
            }
            assert_eq!(
                project
                    .load_overworld_path_links_detected(
                        lm_profile::smw_us_v1_overworld_path_patch_locator(),
                    )
                    .unwrap()
                    .table,
                workspace.paths
            );
            let result = RomImage::from_bytes(project.save_snapshot()).unwrap();
            assert_eq!(
                result.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(lm_rom::detect_identity(&result).unwrap().checksum_matches());
            logical_results.push(result.logical_bytes().to_vec());

            app.dispatch(lm_app::Command::Undo).unwrap();
            assert_eq!(
                app.project()
                    .unwrap()
                    .load_overworld_path_links_detected(
                        lm_profile::smw_us_v1_overworld_path_patch_locator(),
                    )
                    .unwrap()
                    .table,
                original_paths
            );
            app.dispatch(lm_app::Command::Undo).unwrap();
            assert_eq!(app.project().unwrap().save_snapshot(), original_bytes);
            app.dispatch(lm_app::Command::Redo).unwrap();
            app.dispatch(lm_app::Command::Redo).unwrap();
            assert_eq!(
                app.project().unwrap().rom.logical_bytes(),
                logical_results.last().unwrap()
            );
        }
        assert_eq!(logical_results[0], logical_results[1]);
    }

    #[test]
    fn authentic_pristine_rom_loads_vanilla_overworld_animation_address_table() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/headered.smc");
        let project = Project::new(RomImage::from_bytes(fs::read(fixture).unwrap()).unwrap());
        let addresses = load_builtin_overworld_animation_addresses(&project).unwrap();
        assert_eq!(addresses.len(), 67);
        assert_eq!(&addresses[..4], &[0xb480, 0xb498, 0xb4b0, 0xb300]);
        assert_eq!(addresses[4] - addresses[3], 0x18);
    }

    #[test]
    fn native_lifecycle_uses_selected_mapper_animation_table_without_mirror_fallback() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/headered.smc");
        let source = RomImage::from_bytes(fs::read(fixture).unwrap()).unwrap();
        let mut identity = detect_identity(&source).unwrap();
        let source_offset =
            lm_profile::smw_us_v1_builtin_overworld_animation_table_offset(Mapper::LoRom);
        let table_len = lm_profile::SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_WORDS * 2;
        let source_table = source.read(source_offset, table_len).unwrap().to_vec();

        for mapper in [Mapper::ExLoRom, Mapper::Sa1] {
            let selected_offset =
                lm_profile::smw_us_v1_builtin_overworld_animation_table_offset(mapper);
            let mut logical = source.logical_bytes().to_vec();
            logical.resize(0x80_0000, 0xff);
            // Keep the valid lower LoROM table as an explicit compatibility-mirror decoy.
            logical[selected_offset..selected_offset + table_len].copy_from_slice(&source_table);
            identity.mapper = mapper;
            let mut project = Project::new(RomImage::from_bytes(logical).unwrap());
            project.identity = Some(identity.clone());
            let selected = load_builtin_overworld_animation_addresses(&project).unwrap();
            assert_eq!(&selected[..4], &[0xb480, 0xb498, 0xb4b0, 0xb300]);

            project
                .rom
                .write(selected_offset, &0x1fff_u16.to_le_bytes())
                .unwrap();
            let error = load_builtin_overworld_animation_addresses(&project).unwrap_err();
            assert!(error.contains("source 0 is $1FFF"), "{error}");
        }
    }

    #[test]
    fn authentic_pristine_rom_loads_exact_overworld_palette_animation_tables() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/headered.smc");
        let project = Project::new(RomImage::from_bytes(fs::read(fixture).unwrap()).unwrap());
        let dots = load_builtin_overworld_level_dot_palette(&project)
            .unwrap()
            .unwrap();
        assert_eq!(
            dots[0].map(|color| color.0),
            [
                0x02df, 0x035f, 0x27ff, 0x5fff, 0x73ff, 0x5fff, 0x27ff, 0x035f,
            ]
        );
        assert_eq!(
            dots[1].map(|color| color.0),
            [
                0x01bf, 0x001f, 0x001b, 0x0018, 0x0018, 0x001b, 0x001f, 0x01bf,
            ]
        );
        let lightning = load_builtin_overworld_lightning(&project).unwrap().unwrap();
        assert_eq!(
            lightning.delays,
            [0x20, 0x58, 0x43, 0xcf, 0x18, 0x34, 0xa2, 0x5e]
        );
        assert_eq!(lightning.initial_colors, [7, 5, 6, 7, 4, 6, 7, 5]);
        assert_eq!(
            &lightning.selectors[..8],
            &[0xa9, 0xf7, 0x20, 0x82, 0xf8, 0xd0, 0x5f, 0xac]
        );
    }
}
