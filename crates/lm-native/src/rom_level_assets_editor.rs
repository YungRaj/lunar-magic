use crate::{document_loader::DocumentLoader, native_level_assets_panels::AggregatePanels};
use eframe::egui;
use lm_app::{
    AppState, Command, NativeLevelAssetsController, ProfiledControllerSnapshot, RevisionProfile,
};
use lm_graphics::PaletteOwnership;
use lm_level::{Map16Set, NativeLayer2Data, ObjectStream};
use lm_project::NativeLevelAssetsFile;
use lm_render::{
    MaterializedSuperGraphicsVram, NativeLevelMap16Layout, NativeLevelRasterRequest,
    NativeMap16Placement, Rgba, StandardLevelOrientation, StandardObjectDefinitionSet,
    StandardSpritePreviewMode, StandardSpritePreviewSource,
    draw_native_sprite_preview_definition_pages, install_lunar_magic_shared_extended_objects,
    install_lunar_magic_shared_standard_objects, install_lunar_magic_tileset_extended_objects,
    lunar_magic_standard_sprite_preview_source, render_lunar_magic_standard_sprite_with_mode,
    render_mapped_standard_object_stream,
};

mod commit;
mod lifecycle;
mod mwl;
mod mwl_batch;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    controller: NativeLevelAssetsController,
    snapshot: lm_app::ControllerSnapshot,
    profile: RevisionProfile,
    source_slot: u16,
    image: lm_rom::RomImage,
    internal_header: usize,
    ownership: PaletteOwnership,
}

struct PendingLoad {
    profiled: ProfiledControllerSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeSpritePreviewPlacement {
    subtiles: [u16; 4],
    x: i32,
    y: i32,
}

struct ResolvedLevelGraphics {
    vram: MaterializedSuperGraphicsVram,
    foreground_background_files: usize,
    sprite_files: usize,
    source: &'static str,
}

struct SpecialGraphicsTiles {
    gfx33: Vec<lm_graphics::IndexedTile>,
    gfx32: Option<Vec<lm_graphics::IndexedTile>>,
}

#[derive(Clone, Copy)]
struct InstalledAnimationOptions {
    vanilla_tiles: bool,
    palette: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreviewViewportState {
    origin_x: i64,
    origin_y: i64,
    zoom_index: u8,
}

impl Default for PreviewViewportState {
    fn default() -> Self {
        Self {
            origin_x: 0,
            origin_y: 0,
            zoom_index: 1,
        }
    }
}

impl PreviewViewportState {
    const WIDTH: u32 = 512;
    const HEIGHT: u32 = 448;
    const ZOOMS: [(u32, u32); 5] = [(1, 2), (1, 1), (2, 1), (3, 1), (4, 1)];
    const LABELS: [&'static str; 5] = ["50%", "100%", "200%", "300%", "400%"];

    fn viewport(self) -> Result<lm_render::Viewport, lm_render::ViewportError> {
        let (numerator, denominator) = Self::ZOOMS
            .get(usize::from(self.zoom_index))
            .copied()
            .unwrap_or((1, 1));
        lm_render::Viewport::new(
            lm_render::Point {
                x: self.origin_x,
                y: self.origin_y,
            },
            Self::WIDTH,
            Self::HEIGHT,
            numerator,
            denominator,
        )
    }

    fn clamp_to_world(&mut self, width: usize, height: usize) {
        if usize::from(self.zoom_index) >= Self::ZOOMS.len() {
            self.zoom_index = 1;
        }
        let (maximum_x, maximum_y) = self.camera_maximum(width, height);
        self.origin_x = self.origin_x.clamp(0, maximum_x);
        self.origin_y = self.origin_y.clamp(0, maximum_y);
    }

    fn camera_maximum(self, width: usize, height: usize) -> (i64, i64) {
        let world_width = i64::try_from(width).unwrap_or(i64::MAX);
        let world_height = i64::try_from(height).unwrap_or(i64::MAX);
        let visible = Self {
            origin_x: 0,
            origin_y: 0,
            ..self
        }
        .viewport()
        .and_then(lm_render::Viewport::visible_world)
        .ok();
        let visible_width = visible.map_or(0, |bounds| bounds.right - bounds.left);
        let visible_height = visible.map_or(0, |bounds| bounds.bottom - bounds.top);
        (
            world_width.saturating_sub(visible_width).max(0),
            world_height.saturating_sub(visible_height).max(0),
        )
    }
}

impl InstalledAnimationOptions {
    const fn active(self) -> bool {
        self.vanilla_tiles || self.palette
    }
}

#[derive(Default)]
struct LivePreviewState {
    enabled: bool,
    dirty: bool,
    failed: bool,
    phase: Option<usize>,
}

impl LivePreviewState {
    fn toggle(&mut self) {
        self.enabled = !self.enabled;
        self.dirty = self.enabled;
        self.failed = false;
        self.phase = None;
    }

    fn invalidate(&mut self) {
        if self.enabled {
            self.dirty = true;
            self.failed = false;
        }
    }

    fn take_refresh(&mut self, phase: Option<usize>) -> bool {
        let refresh = self.enabled && !self.failed && (self.dirty || self.phase != phase);
        self.dirty = false;
        self.phase = phase;
        refresh
    }

    fn finish_refresh(&mut self, succeeded: bool) {
        self.failed = !succeeded;
    }
}

#[derive(Default)]
pub(crate) struct RomLevelAssetsEditor {
    workspace: Option<Workspace>,
    panels: AggregatePanels,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    loader: DocumentLoader,
    mwl_loader: DocumentLoader,
    legacy_mwl_loader: DocumentLoader,
    pending_legacy_mwl_load: Option<mwl::PendingLegacyMwlLoad>,
    mwl_batch_worker: mwl_batch::MwlBatchExportWorker,
    mwl_batch_status: Option<String>,
    pending_load: Option<PendingLoad>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
    bypass_validation: Option<String>,
    bypass_layer2_texture: Option<egui::TextureHandle>,
    bypass_preview: LivePreviewState,
    bypass_viewport: PreviewViewportState,
}

impl RomLevelAssetsEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> (bool, Option<Command>) {
        if let Some(result) = self.mwl_batch_worker.show(context) {
            match result {
                Ok(count) => {
                    self.mwl_batch_status =
                        Some(format!("{count} levels were exported successfully."));
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(result) = self.loader.show(context) {
            self.finish_ownership_load(result, project_revision);
        }
        let mut command = self.mwl_loader.show(context).and_then(|result| {
            match self.finish_mwl_import(result, project_revision) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            }
        });
        if let Some(result) = self.legacy_mwl_loader.show(context) {
            match self.finish_legacy_mwl_load(result, project_revision) {
                Ok(Some(legacy_command)) => command = Some(legacy_command),
                Ok(None) => {}
                Err(error) => self.error = Some(error),
            }
        }
        let reclamation_command = match self.manifest_loader.show(context, project_revision) {
            Some(Ok(manifest)) => match self.prepare_commit_with_reclamation(&manifest) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            },
            Some(Err(error)) => {
                self.error = Some(error);
                None
            }
            None => None,
        };
        if reclamation_command.is_some() {
            command = reclamation_command;
        }
        if self.workspace.is_some() {
            egui::Window::new("ROM Native Level Assets")
                .default_size([900.0, 720.0])
                .vscroll(true)
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, project_revision) {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.controller.revision() != project_revision;
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed. Close and reopen this workspace before committing.",
            );
        }
        ui.horizontal(|ui| {
            ui.label("Allocation search (logical PC hex, end-exclusive)");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let file = NativeLevelAssetsFile {
            source_slot: workspace.source_slot,
            assets: workspace.controller.assets().clone(),
        };
        let edit = self.panels.show(
            ui,
            workspace.controller.revision(),
            &file,
            (
                workspace.controller.layer2(),
                workspace.controller.layer2_descriptor(),
            ),
            workspace.controller.exanimation_features(),
            &workspace.profile.exanimation_double_size_modes,
            &workspace.ownership,
        );
        if let Some(edit) = edit {
            match edit {
                Ok(edit) if !stale => {
                    if let Some(workspace) = self.workspace.as_mut() {
                        if let Err(error) = workspace.controller.apply_edits(&[edit]) {
                            self.error = Some(error.to_string());
                        } else {
                            self.bypass_validation = None;
                            self.bypass_layer2_texture = None;
                            self.bypass_preview.invalidate();
                            self.panels.invalidate();
                        }
                    } else {
                        self.error = Some("level-assets workspace is closed".into());
                    }
                }
                Ok(_) => self.error = Some("stale ROM workspace cannot accept more edits".into()),
                Err(error) => self.error = Some(error),
            }
        }
        if ui.button("Validate selected Super GFX files").clicked() {
            self.bypass_validation = self.workspace.as_ref().map(validate_super_graphics);
        }
        let preview_button = if self.bypass_preview.enabled {
            "Stop live bypass-aware preview"
        } else {
            "Start live bypass-aware preview"
        };
        if ui.button(preview_button).clicked() {
            self.bypass_preview.toggle();
            if self.bypass_preview.enabled {
                self.bypass_layer2_texture = None;
            }
        }
        let header = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.controller.assets().level.layer1.header);
        if let Some(header) = header {
            let (world_width, world_height) = preview_world_extent(header);
            self.bypass_viewport
                .clamp_to_world(world_width, world_height);
            let (maximum_x, maximum_y) = self
                .bypass_viewport
                .camera_maximum(world_width, world_height);
            let mut viewport_changed = false;
            ui.horizontal(|ui| {
                ui.label("Preview camera");
                viewport_changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.bypass_viewport.origin_x)
                            .range(0..=maximum_x)
                            .prefix("X "),
                    )
                    .changed();
                viewport_changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.bypass_viewport.origin_y)
                            .range(0..=maximum_y)
                            .prefix("Y "),
                    )
                    .changed();
                let selected = PreviewViewportState::LABELS
                    .get(usize::from(self.bypass_viewport.zoom_index))
                    .copied()
                    .unwrap_or("100%");
                egui::ComboBox::from_id_salt("installed-super-gfx-preview-zoom")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (index, label) in PreviewViewportState::LABELS.iter().enumerate() {
                            viewport_changed |= ui
                                .selectable_value(
                                    &mut self.bypass_viewport.zoom_index,
                                    u8::try_from(index).expect("five zoom entries"),
                                    *label,
                                )
                                .changed();
                        }
                    });
                if ui.button("Reset view").clicked() {
                    self.bypass_viewport = PreviewViewportState::default();
                    viewport_changed = true;
                }
            });
            if viewport_changed {
                self.bypass_preview.invalidate();
            }
        }
        let animation_options = self.workspace.as_ref().map_or(
            InstalledAnimationOptions {
                vanilla_tiles: false,
                palette: false,
            },
            installed_animation_options,
        );
        let animation_phase = animation_options
            .active()
            .then(|| installed_preview_animation_phase(ui.input(|input| input.time)));
        if self.bypass_preview.take_refresh(animation_phase) {
            let result = self
                .workspace
                .as_ref()
                .ok_or_else(|| "level-assets workspace is closed".to_owned())
                .and_then(|workspace| {
                    render_super_graphics_level_preview(
                        workspace,
                        animation_phase,
                        self.bypass_viewport,
                    )
                });
            self.bypass_preview.finish_refresh(result.is_ok());
            match result {
                Ok((image, diagnostics)) => {
                    self.bypass_layer2_texture = Some(ui.ctx().load_texture(
                        "installed-super-gfx-level-preview",
                        image,
                        egui::TextureOptions::NEAREST,
                    ));
                    self.bypass_validation = Some(if diagnostics.is_empty() {
                        "Rendered installed Layer 2 and Layer 1 object streams with the selected Super GFX files, installed Map16 definitions, and staged level palette.".into()
                    } else {
                        format!(
                            "Rendered the installed object layers with unresolved definitions: {}",
                            diagnostics.join("; ")
                        )
                    });
                }
                Err(error) => {
                    self.bypass_layer2_texture = None;
                    self.bypass_validation = Some(error);
                }
            }
        }
        if self.bypass_preview.enabled && animation_options.active() && !self.bypass_preview.failed
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(60));
        }
        if let Some(validation) = &self.bypass_validation {
            ui.label(validation);
        }
        if let Some(texture) = &self.bypass_layer2_texture {
            egui::ScrollArea::horizontal()
                .id_salt("installed-super-gfx-layer2-preview")
                .show(ui, |ui| {
                    ui.image(texture);
                });
        }
        ui.separator();
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        self.show_mwl_actions(ui, stale, modified);
        if let Some(status) = &self.mwl_batch_status {
            ui.label(status);
        }
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit all domains to ROM"),
            )
            .clicked()
        {
            match self.prepare_commit() {
                Ok(command) => {
                    return Some(command);
                }
                Err(error) => self.error = Some(error),
            }
        }
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit and reclaim with LMRATS01 evidence"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(project_revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged aggregate changes"
        } else {
            "No staged changes"
        });
        None
    }
}

fn validate_super_graphics(workspace: &Workspace) -> String {
    let project = lm_project::Project::new(workspace.image.clone());
    let header = workspace.controller.assets().level.layer1.header;
    match resolve_level_graphics(workspace, &project, header) {
        Ok(resolved) => {
            let foreground_tiles = resolved.vram.foreground_background.len();
            let sprite_tiles = resolved.vram.sprites.len();
            format!(
                "Validated and materialized {} {} FG/BG files ({foreground_tiles} VRAM tiles) and {} sprite files ({sprite_tiles} VRAM tiles).",
                resolved.source, resolved.foreground_background_files, resolved.sprite_files,
            )
        }
        Err(error) => error.to_string(),
    }
}

fn render_super_graphics_level_preview(
    workspace: &Workspace,
    animation_phase: Option<usize>,
    viewport: PreviewViewportState,
) -> Result<(egui::ColorImage, Vec<String>), String> {
    let project = lm_project::Project::new(workspace.image.clone());
    let header = workspace.controller.assets().level.layer1.header;
    let resolved = resolve_level_graphics(workspace, &project, header)?;
    let mut vram = resolved.vram;
    let mut palette = workspace.controller.assets().palette.clone();
    let animation_options = installed_animation_options(workspace);
    let special_graphics = load_profiled_special_graphics(
        &project,
        workspace.profile.graphics,
        animation_options.vanilla_tiles,
    )?;
    if let Some(phase) = animation_phase {
        if !is_smw_us_v1_profile(&workspace.profile) {
            return Err(format!(
                "built-in animation tables are not recovered for profile {}",
                workspace.profile.name
            ));
        }
        if animation_options.vanilla_tiles {
            crate::vanilla_map16_preview::apply_vanilla_common_animation_frame_with_tiles(
                &project,
                &mut vram.foreground_background,
                phase,
                header.object_tileset(),
                &special_graphics.gfx33,
                special_graphics
                    .gfx32
                    .as_deref()
                    .ok_or_else(|| "enabled vanilla animation did not load GFX32".to_owned())?,
            )?;
        }
        if animation_options.palette {
            crate::vanilla_map16_preview::apply_vanilla_editor_palette_animation(
                &mut palette,
                phase,
            );
        }
    }
    let map16 = project
        .load_map16_set(workspace.profile.map16)
        .map_err(|error| error.to_string())?;
    let (layer1, layout, mut diagnostics) = render_object_placements(
        &workspace.image,
        &workspace.controller.assets().level.layer1.objects,
        header.level_mode(),
        header.object_tileset(),
    )?;
    let layer2 = match workspace.controller.layer2() {
        Some(NativeLayer2Data::Tilemap(tilemap)) => layer2_placements(tilemap)?,
        Some(NativeLayer2Data::Objects(objects)) => {
            let (placements, _, layer2_diagnostics) = render_object_placements(
                &workspace.image,
                &objects.objects,
                header.level_mode(),
                header.object_tileset(),
            )?;
            diagnostics.extend(
                layer2_diagnostics
                    .into_iter()
                    .map(|diagnostic| format!("Layer 2 {diagnostic}")),
            );
            placements
        }
        None => Vec::new(),
    };
    let (sprites, sprite_diagnostics) = render_sprite_placements(
        &workspace.controller.assets().level.sprites,
        header.level_mode(),
        header.sprite_tileset(),
    );
    diagnostics.extend(sprite_diagnostics);
    let animated_sprite_tiles =
        crate::vanilla_map16_preview::materialize_sprite_display_tiles(special_graphics.gfx33);
    render_level_viewport_image(
        &[&layer2, &layer1],
        &sprites,
        layout,
        &map16,
        &vram.foreground_background,
        &vram.sprites,
        &animated_sprite_tiles,
        &palette,
        viewport,
    )
    .map(|image| (image, diagnostics))
}

fn preview_world_extent(header: lm_level::LegacyLevelHeader) -> (usize, usize) {
    let mode = lm_profile::smw_us_v1_level_mode(header.level_mode());
    if mode.vertical {
        (32 * 16, usize::from(mode.editor_major_screens) * 16 * 16)
    } else {
        (usize::from(mode.editor_major_screens) * 16 * 16, 27 * 16)
    }
}

fn load_profiled_special_graphics(
    project: &lm_project::Project,
    layout: lm_project::GraphicsRomLayout,
    include_gfx32: bool,
) -> Result<SpecialGraphicsTiles, String> {
    let entries = layout
        .split_pointer_planes
        .map_or(layout.pointers.entries, |planes| planes.entries);
    let (gfx33_file, gfx32_file, source_layout) = if entries > 0x33 {
        (0x33, 0x32, layout)
    } else {
        (
            0,
            1,
            lm_profile::smw_us_v1_vanilla_special_graphics_layout(),
        )
    };
    let decoded_gfx33 = project
        .load_decompressed_graphics_file(gfx33_file, source_layout)
        .map_err(|error| format!("cannot load profiled GFX33: {error}"))?;
    let mut gfx33 = lm_graphics::decode_planar_tiles(&decoded_gfx33, 3)
        .map_err(|error| format!("cannot decode profiled GFX33 as 3bpp: {error}"))?;
    gfx33.resize_with(gfx33.len() + 0x30, || {
        lm_graphics::IndexedTile::new([0; lm_graphics::IndexedTile::PIXEL_COUNT])
    });
    let gfx32 = include_gfx32
        .then(|| {
            let decoded = project
                .load_decompressed_graphics_file(gfx32_file, source_layout)
                .map_err(|error| format!("cannot load profiled GFX32: {error}"))?;
            lm_graphics::decode_planar_tiles(&decoded, 4)
                .map_err(|error| format!("cannot decode profiled GFX32 as 4bpp: {error}"))
        })
        .transpose()?;
    Ok(SpecialGraphicsTiles { gfx33, gfx32 })
}

fn installed_animation_options(workspace: &Workspace) -> InstalledAnimationOptions {
    animation_options_from_features(
        workspace
            .controller
            .exanimation_features()
            .map(|features| features.options),
    )
}

fn animation_options_from_features(
    features: Option<lm_graphics::ExAnimationFeatureOptions>,
) -> InstalledAnimationOptions {
    InstalledAnimationOptions {
        vanilla_tiles: features.is_none_or(|options| {
            options.enabled(lm_graphics::ExAnimationFeature::VanillaAnimation)
        }),
        palette: features.is_none_or(|options| {
            options.enabled(lm_graphics::ExAnimationFeature::PaletteAnimation)
        }),
    }
}

fn installed_preview_animation_phase(seconds: f64) -> usize {
    if let Ok(phase) = std::env::var("LM_NATIVE_ANIMATION_PHASE")
        && let Ok(phase) = phase.parse::<usize>()
        && phase < 8
    {
        return phase;
    }
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks = (seconds / 0.06).floor() as u64;
    usize::try_from(ticks & 7).expect("three-bit animation phase")
}

fn resolve_level_graphics(
    workspace: &Workspace,
    project: &lm_project::Project,
    header: lm_level::LegacyLevelHeader,
) -> Result<ResolvedLevelGraphics, String> {
    if let Some(settings) = workspace.controller.assets().expanded_settings.as_ref()
        && let Some(loaded) = project
            .load_super_graphics_bypass(settings, workspace.profile.graphics)
            .map_err(|error| error.to_string())?
    {
        return Ok(ResolvedLevelGraphics {
            vram: lm_render::materialize_super_graphics_vram(&loaded),
            foreground_background_files: loaded.foreground_background.len(),
            sprite_files: loaded.sprites.len(),
            source: "bypassed",
        });
    }
    resolve_legacy_level_graphics(workspace, project, header)
}

fn resolve_legacy_level_graphics(
    workspace: &Workspace,
    project: &lm_project::Project,
    header: lm_level::LegacyLevelHeader,
) -> Result<ResolvedLevelGraphics, String> {
    if !is_smw_us_v1_profile(&workspace.profile) {
        return Err(format!(
            "legacy graphics assignment tables are not recovered for profile {}",
            workspace.profile.name
        ));
    }
    let foreground_files = lm_profile::smw_us_v1_object_tileset_graphics_files(
        &workspace.image,
        usize::from(header.object_tileset()),
    )
    .map_err(|error| error.to_string())?;
    let sprite_files = lm_profile::smw_us_v1_sprite_tileset_graphics_files(
        &workspace.image,
        usize::from(header.sprite_tileset()),
    )
    .map_err(|error| error.to_string())?;
    materialize_legacy_level_graphics(
        project,
        workspace.profile.graphics,
        &foreground_files,
        &sprite_files,
    )
}

fn is_smw_us_v1_profile(profile: &RevisionProfile) -> bool {
    profile.game == lm_rom::SupportedGame::SuperMarioWorld
        && profile.region == lm_rom::Region::NorthAmerica
        && profile.revision == 0
}

fn materialize_legacy_level_graphics(
    project: &lm_project::Project,
    layout: lm_project::GraphicsRomLayout,
    foreground_files: &[usize],
    sprite_files: &[usize],
) -> Result<ResolvedLevelGraphics, String> {
    if foreground_files.len() != 4 || sprite_files.len() != 4 {
        return Err(format!(
            "legacy level graphics require 4 FG/BG and 4 sprite files, received {} and {}",
            foreground_files.len(),
            sprite_files.len()
        ));
    }
    let load_files = |domain: &str,
                      files: &[usize]|
     -> Result<Vec<lm_graphics::IndexedTile>, String> {
        let mut tiles = Vec::with_capacity(files.len() * 128);
        for (slot, file) in files.iter().copied().enumerate() {
            let file = u16::try_from(file)
                .map_err(|_| format!("{domain} slot {slot} graphics file {file} exceeds $FFFF"))?;
            let loaded = project
                .load_super_graphics_file(file, layout)
                .map_err(|error| {
                    format!("cannot load legacy {domain} slot {slot} file GFX{file:02X}: {error}")
                })?;
            tiles.extend(loaded.tiles);
        }
        Ok(tiles)
    };
    let mut foreground_background = load_files("FG/BG", foreground_files)?;
    foreground_background.resize_with(6 * 128, || {
        lm_graphics::IndexedTile::new([0; lm_graphics::IndexedTile::PIXEL_COUNT])
    });
    let sprites = load_files("sprite", sprite_files)?;
    Ok(ResolvedLevelGraphics {
        vram: MaterializedSuperGraphicsVram {
            foreground_background,
            sprites,
        },
        foreground_background_files: foreground_files.len(),
        sprite_files: sprite_files.len(),
        source: "legacy",
    })
}

#[cfg(test)]
fn render_level_image(
    layers: &[&[NativeMap16Placement]],
    sprites: &[NativeSpritePreviewPlacement],
    layout: NativeLevelMap16Layout,
    map16: &Map16Set,
    tiles: &[lm_graphics::IndexedTile],
    sprite_tiles: &[lm_graphics::IndexedTile],
    animated_sprite_tiles: &[lm_graphics::IndexedTile],
    palette: &lm_graphics::Palette,
) -> Result<egui::ColorImage, String> {
    render_level_canvas(
        layers,
        sprites,
        layout,
        map16,
        tiles,
        sprite_tiles,
        animated_sprite_tiles,
        palette,
    )
    .map(canvas_to_color_image)
}

fn render_level_viewport_image(
    layers: &[&[NativeMap16Placement]],
    sprites: &[NativeSpritePreviewPlacement],
    layout: NativeLevelMap16Layout,
    map16: &Map16Set,
    tiles: &[lm_graphics::IndexedTile],
    sprite_tiles: &[lm_graphics::IndexedTile],
    animated_sprite_tiles: &[lm_graphics::IndexedTile],
    palette: &lm_graphics::Palette,
    viewport: PreviewViewportState,
) -> Result<egui::ColorImage, String> {
    let source = render_level_canvas(
        layers,
        sprites,
        layout,
        map16,
        tiles,
        sprite_tiles,
        animated_sprite_tiles,
        palette,
    )?;
    lm_render::rasterize_canvas_viewport(
        &source,
        viewport.viewport().map_err(|error| error.to_string())?,
    )
    .map(canvas_to_color_image)
    .map_err(|error| error.to_string())
}

fn render_level_canvas(
    layers: &[&[NativeMap16Placement]],
    sprites: &[NativeSpritePreviewPlacement],
    layout: NativeLevelMap16Layout,
    map16: &Map16Set,
    tiles: &[lm_graphics::IndexedTile],
    sprite_tiles: &[lm_graphics::IndexedTile],
    animated_sprite_tiles: &[lm_graphics::IndexedTile],
    palette: &lm_graphics::Palette,
) -> Result<lm_render::Canvas, String> {
    let definitions = map16
        .pages
        .iter()
        .flat_map(|page| page.tiles.iter().copied())
        .collect::<Vec<_>>();
    let backdrop = palette
        .colors
        .first()
        .copied()
        .map(lm_graphics::Bgr555::to_rgb8)
        .map_or_else(Rgba::default, |color| Rgba {
            red: color.red,
            green: color.green,
            blue: color.blue,
            alpha: 255,
        });
    let mut canvas = lm_render::render_native_level_framebuffer(NativeLevelRasterRequest {
        width: layout.width * 16,
        height: layout.height * 16,
        camera_x: 0,
        camera_y: 0,
        backdrop,
        layers,
        definitions: &definitions,
        tiles,
        palette,
    })
    .map_err(|error| error.to_string())?;
    for sprite in sprites {
        draw_native_sprite_preview_definition_pages(
            &mut canvas,
            sprite.subtiles,
            sprite_tiles,
            animated_sprite_tiles,
            palette,
            sprite.x,
            sprite.y,
        );
    }
    Ok(canvas)
}

fn canvas_to_color_image(canvas: lm_render::Canvas) -> egui::ColorImage {
    let mut rgba = Vec::with_capacity(canvas.pixels().len() * 4);
    for pixel in canvas.pixels() {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    egui::ColorImage::from_rgba_unmultiplied([canvas.width(), canvas.height()], &rgba)
}

fn render_object_placements(
    image: &lm_rom::RomImage,
    objects: &ObjectStream,
    level_mode: u8,
    object_tileset: u8,
) -> Result<
    (
        Vec<NativeMap16Placement>,
        NativeLevelMap16Layout,
        Vec<String>,
    ),
    String,
> {
    let mode = lm_profile::smw_us_v1_level_mode(level_mode);
    if mode.editor_major_screens == 0 {
        return Err(format!("level mode {level_mode:02X} has no editor canvas"));
    }
    let layout = NativeLevelMap16Layout {
        width: if mode.vertical {
            32
        } else {
            usize::from(mode.editor_major_screens) * 16
        },
        height: if mode.vertical {
            usize::from(mode.editor_major_screens) * 16
        } else {
            27
        },
        page_stride: 0x1b0,
        base_cell: 0,
        vertical: mode.vertical,
    };
    let object_map = lm_profile::load_smw_us_v1_standard_object_definition_map(image)
        .map_err(|error| error.to_string())?;
    let family = match lm_profile::smw_us_v1_object_family(object_tileset) {
        lm_profile::VanillaObjectFamily::Normal => 0,
        lm_profile::VanillaObjectFamily::Castle => 1,
        lm_profile::VanillaObjectFamily::Rope => 2,
        lm_profile::VanillaObjectFamily::Underground => 3,
        lm_profile::VanillaObjectFamily::GhostHouse => 4,
    };
    let handler_map = object_map
        .family(family)
        .ok_or_else(|| format!("object-definition family {family} is unavailable"))?;
    let mut definitions = StandardObjectDefinitionSet::empty();
    install_lunar_magic_shared_extended_objects(&mut definitions)
        .and_then(|()| install_lunar_magic_shared_standard_objects(&mut definitions))
        .and_then(|()| {
            install_lunar_magic_tileset_extended_objects(&mut definitions, object_tileset)
        })
        .map_err(|error| error.to_string())?;
    let rendered =
        render_mapped_standard_object_stream(objects, &definitions, handler_map, layout, 0x25)
            .map_err(|error| error.to_string())?;
    let mut placements = Vec::with_capacity(layout.width * layout.height);
    for y in 0..layout.height {
        for x in 0..layout.width {
            let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
            placements.push(NativeMap16Placement {
                x: i32::try_from(x).map_err(|_| "object-layer X overflow".to_owned())?,
                y: i32::try_from(y).map_err(|_| "object-layer Y overflow".to_owned())?,
                word: rendered.cache.cells()[index],
            });
        }
    }
    let mut diagnostics = Vec::new();
    if !rendered.missing_commands.is_empty() {
        diagnostics.push(format!("commands {:?}", rendered.missing_commands));
    }
    if !rendered.missing_extended_objects.is_empty() {
        diagnostics.push(format!(
            "extended objects {:?}",
            rendered.missing_extended_objects
        ));
    }
    Ok((placements, layout, diagnostics))
}

fn render_sprite_placements(
    sprites: &lm_level::NativeSpriteStream,
    level_mode: u8,
    sprite_tileset: u8,
) -> (Vec<NativeSpritePreviewPlacement>, Vec<String>) {
    let mode = lm_profile::smw_us_v1_level_mode(level_mode);
    let orientation = if mode.vertical {
        StandardLevelOrientation::Vertical
    } else {
        StandardLevelOrientation::Horizontal
    };
    let mut rendered = Vec::new();
    let mut diagnostics = Vec::new();
    let mut sprite_8a_sequence_index = 0_u8;
    for placement in sprites.native_placements() {
        let preview = render_lunar_magic_standard_sprite_with_mode(
            placement.sprite_number,
            StandardSpritePreviewMode {
                placement_first: placement.packed_display_position(),
                placement_major: placement.major,
                placement_minor: placement.minor,
                level_mode,
                level_orientation: orientation,
                sprite_graphics_mode: sprite_tileset,
                sprite_8a_sequence_index,
                ..StandardSpritePreviewMode::default()
            },
        );
        if placement.sprite_number == 0x8a {
            sprite_8a_sequence_index = sprite_8a_sequence_index.saturating_add(1);
        }
        let Some(parts) = preview else {
            if lunar_magic_standard_sprite_preview_source(placement.sprite_number)
                == StandardSpritePreviewSource::BuiltIn
            {
                diagnostics.push(format!(
                    "sprite ${:02X} has no materialized preview",
                    placement.sprite_number
                ));
            }
            continue;
        };
        let (tile_x, tile_y) = placement.tile_coordinates(mode.vertical);
        let origin_x = i32::from(tile_x).saturating_mul(16);
        let origin_y = i32::from(tile_y).saturating_mul(16);
        for part in parts {
            rendered.push(NativeSpritePreviewPlacement {
                subtiles: part.subtiles,
                x: origin_x.saturating_add(i32::from(part.x)),
                y: origin_y.saturating_add(i32::from(part.y)),
            });
        }
    }
    diagnostics.sort();
    diagnostics.dedup();
    (rendered, diagnostics)
}

fn layer2_placements(tilemap: &[u8]) -> Result<Vec<NativeMap16Placement>, String> {
    if tilemap.len() != lm_level::NATIVE_LAYER2_TILEMAP_LEN {
        return Err(format!(
            "native Layer 2 tilemap has {} bytes instead of {}",
            tilemap.len(),
            lm_level::NATIVE_LAYER2_TILEMAP_LEN
        ));
    }
    let mut placements = Vec::with_capacity(
        lm_level::NATIVE_LAYER2_TILEMAP_WIDTH * lm_level::NATIVE_LAYER2_TILEMAP_HEIGHT,
    );
    for y in 0..lm_level::NATIVE_LAYER2_TILEMAP_HEIGHT {
        for x in 0..lm_level::NATIVE_LAYER2_TILEMAP_WIDTH {
            let index = lm_level::native_layer2_tilemap_index(x, y)
                .ok_or_else(|| "bounded Layer 2 coordinate did not map to storage".to_owned())?;
            let offset = index * 2;
            placements.push(NativeMap16Placement {
                x: i32::try_from(x).map_err(|_| "Layer 2 X coordinate overflow".to_owned())?,
                y: i32::try_from(y).map_err(|_| "Layer 2 Y coordinate overflow".to_owned())?,
                word: u16::from_le_bytes([tilemap[offset], tilemap[offset + 1]]),
            });
        }
    }
    Ok(placements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, IndexedTile, Palette};
    use lm_level::{Map16Page, Map16Tile, Subtile};

    #[test]
    fn live_preview_refreshes_once_per_enable_or_accepted_edit() {
        let mut preview = LivePreviewState::default();
        assert!(!preview.take_refresh(None));
        preview.invalidate();
        assert!(!preview.take_refresh(None));

        preview.toggle();
        assert!(preview.take_refresh(None));
        preview.finish_refresh(true);
        assert!(!preview.take_refresh(None));

        preview.invalidate();
        assert!(preview.take_refresh(None));
        preview.finish_refresh(true);
        assert!(!preview.take_refresh(None));

        preview.toggle();
        preview.invalidate();
        assert!(!preview.take_refresh(None));
    }

    #[test]
    fn live_preview_tracks_animation_phases_and_suspends_failures() {
        let mut preview = LivePreviewState::default();
        preview.toggle();
        assert!(preview.take_refresh(Some(0)));
        preview.finish_refresh(true);
        assert!(!preview.take_refresh(Some(0)));
        assert!(preview.take_refresh(Some(1)));
        preview.finish_refresh(false);
        assert!(!preview.take_refresh(Some(2)));
        preview.invalidate();
        assert!(preview.take_refresh(Some(2)));
    }

    #[test]
    fn installed_animation_clock_is_bounded_and_deterministic() {
        assert_eq!(installed_preview_animation_phase(f64::NAN), 0);
        assert_eq!(installed_preview_animation_phase(-1.0), 0);
        assert_eq!(installed_preview_animation_phase(0.0), 0);
        assert_eq!(installed_preview_animation_phase(0.059), 0);
        assert_eq!(installed_preview_animation_phase(0.06), 1);
        assert_eq!(installed_preview_animation_phase(0.42), 7);
        assert_eq!(installed_preview_animation_phase(0.48), 0);
    }

    #[test]
    fn staged_feature_options_gate_tile_and_palette_clocks_independently() {
        let defaults = animation_options_from_features(None);
        assert!(defaults.vanilla_tiles);
        assert!(defaults.palette);
        assert!(defaults.active());

        let mut options = lm_graphics::ExAnimationFeatureOptions::decode(0);
        options.set_enabled(lm_graphics::ExAnimationFeature::VanillaAnimation, false);
        let palette_only = animation_options_from_features(Some(options));
        assert!(!palette_only.vanilla_tiles);
        assert!(palette_only.palette);
        assert!(palette_only.active());

        options.set_enabled(lm_graphics::ExAnimationFeature::PaletteAnimation, false);
        let disabled = animation_options_from_features(Some(options));
        assert!(!disabled.vanilla_tiles);
        assert!(!disabled.palette);
        assert!(!disabled.active());
    }

    #[test]
    fn installed_palette_animation_changes_only_the_recovered_color() {
        let mut colors = vec![Bgr555(0x1234); 128];
        let before = colors.clone();
        let mut palette = Palette {
            colors: colors.clone(),
        };
        crate::vanilla_map16_preview::apply_vanilla_editor_palette_animation(&mut palette, 0);
        colors[0x64] = Bgr555(0x02df);
        assert_eq!(palette.colors, colors);
        assert_ne!(palette.colors, before);
    }

    #[test]
    fn legacy_graphics_materializer_rejects_incomplete_assignment_rows() {
        let project =
            lm_project::Project::new(lm_rom::RomImage::from_bytes(vec![0; 0x8000]).unwrap());
        let error = materialize_legacy_level_graphics(
            &project,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
            &[0, 1, 2],
            &[3, 4, 5, 6],
        )
        .err()
        .unwrap();
        assert!(error.contains("require 4 FG/BG and 4 sprite files"));
    }

    #[test]
    #[ignore = "requires the retained pristine SMW-US ROM fixture"]
    fn retained_legacy_assignments_materialize_fixed_preview_slots() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = std::fs::read(root.join("sysLMRestore/smwOrig.smc")).unwrap();
        let image = lm_rom::RomImage::from_bytes(bytes).unwrap();
        let foreground = lm_profile::smw_us_v1_object_tileset_graphics_files(&image, 0).unwrap();
        let sprites = lm_profile::smw_us_v1_sprite_tileset_graphics_files(&image, 0).unwrap();
        let project = lm_project::Project::new(image);
        let resolved = materialize_legacy_level_graphics(
            &project,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
            &foreground,
            &sprites,
        )
        .unwrap();
        assert_eq!(resolved.source, "legacy");
        assert_eq!(resolved.foreground_background_files, 4);
        assert_eq!(resolved.sprite_files, 4);
        assert_eq!(resolved.vram.foreground_background.len(), 6 * 128);
        assert_eq!(resolved.vram.sprites.len(), 4 * 128);
        assert!(
            resolved.vram.foreground_background[..4 * 128]
                .iter()
                .any(|tile| tile.pixels().iter().any(|pixel| *pixel != 0))
        );
        assert!(
            resolved.vram.foreground_background[4 * 128..]
                .iter()
                .all(|tile| tile.pixels().iter().all(|pixel| *pixel == 0))
        );
        let special = load_profiled_special_graphics(
            &project,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
            true,
        )
        .unwrap();
        assert!(!special.gfx33.is_empty());
        assert!(
            special
                .gfx32
                .as_ref()
                .is_some_and(|tiles| !tiles.is_empty())
        );
        let mut profiled_bytes = project.rom.logical_bytes().to_vec();
        let pointer_table = 0x70000;
        let gfx33_pointer: [u8; 3] = profiled_bytes[0x3882..0x3885].try_into().unwrap();
        let gfx32_pointer: [u8; 3] = profiled_bytes[0x3885..0x3888].try_into().unwrap();
        profiled_bytes[pointer_table + 0x33 * 3..pointer_table + 0x34 * 3]
            .copy_from_slice(&gfx33_pointer);
        profiled_bytes[pointer_table + 0x32 * 3..pointer_table + 0x33 * 3]
            .copy_from_slice(&gfx32_pointer);
        let profiled_project =
            lm_project::Project::new(lm_rom::RomImage::from_bytes(profiled_bytes).unwrap());
        let profiled_special = load_profiled_special_graphics(
            &profiled_project,
            lm_project::GraphicsRomLayout {
                mapper: lm_rom::Mapper::LoRom,
                pointers: lm_project::LevelPointerTable {
                    offset: pointer_table,
                    entries: 0x34,
                    stride: 3,
                },
                split_pointer_planes: None,
                compression: lm_project::GraphicsCompression::Lz2,
                maximum_compressed_len: 0x8000,
                maximum_decompressed_len: 0x10000,
            },
            true,
        )
        .unwrap();
        assert_eq!(profiled_special.gfx33, special.gfx33);
        assert_eq!(profiled_special.gfx32, special.gfx32);
        let mut phase_zero = resolved.vram.foreground_background.clone();
        let mut phase_one = phase_zero.clone();
        crate::vanilla_map16_preview::apply_vanilla_common_animation_frame_with_tiles(
            &project,
            &mut phase_zero,
            0,
            0,
            &special.gfx33,
            special.gfx32.as_deref().unwrap(),
        )
        .unwrap();
        crate::vanilla_map16_preview::apply_vanilla_common_animation_frame_with_tiles(
            &project,
            &mut phase_one,
            1,
            0,
            &special.gfx33,
            special.gfx32.as_deref().unwrap(),
        )
        .unwrap();
        assert_ne!(
            phase_zero, phase_one,
            "recovered animation phases must alter the materialized legacy cache"
        );
    }

    #[test]
    fn layer2_preview_uses_visual_coordinates_and_native_storage_planes() {
        let mut bytes = vec![0; lm_level::NATIVE_LAYER2_TILEMAP_LEN];
        for (x, y, word) in [(0, 0, 0x0123_u16), (31, 0, 0x4567_u16), (0, 31, 0x89ab_u16)] {
            let index = lm_level::native_layer2_tilemap_index(x, y).unwrap();
            bytes[index * 2..index * 2 + 2].copy_from_slice(&word.to_le_bytes());
        }
        let placements = layer2_placements(&bytes).unwrap();
        assert_eq!(
            placements[0],
            NativeMap16Placement {
                x: 0,
                y: 0,
                word: 0x0123
            }
        );
        assert_eq!(
            placements[31],
            NativeMap16Placement {
                x: 31,
                y: 0,
                word: 0x4567
            }
        );
        assert_eq!(
            placements[31 * 32],
            NativeMap16Placement {
                x: 0,
                y: 31,
                word: 0x89ab
            }
        );
    }

    #[test]
    fn layer2_preview_rejects_noncanonical_tilemap_lengths() {
        assert!(layer2_placements(&[0; 0x7ff]).is_err());
    }

    #[test]
    fn layer2_preview_rasterizes_installed_map16_vram_and_palette() {
        let mut tilemap = vec![0; lm_level::NATIVE_LAYER2_TILEMAP_LEN];
        let index = lm_level::native_layer2_tilemap_index(1, 2).unwrap();
        tilemap[index * 2..index * 2 + 2].copy_from_slice(&1_u16.to_le_bytes());
        let mut definitions = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        definitions[1] = Map16Tile {
            top_left: Subtile(0),
            top_right: Subtile(0),
            bottom_left: Subtile(0),
            bottom_right: Subtile(0),
            acts_like: 1,
        };
        let map16 = Map16Set {
            pages: vec![Map16Page::new(definitions).unwrap()],
        };
        let tiles = [IndexedTile::new([1; IndexedTile::PIXEL_COUNT])];
        let mut colors = vec![Bgr555(0); 128];
        colors[1] = Bgr555(0x001f);
        let palette = Palette { colors };
        let placements = layer2_placements(&tilemap).unwrap();
        let layers: [&[NativeMap16Placement]; 1] = [&placements];
        let image = render_level_image(
            &layers,
            &[],
            NativeLevelMap16Layout {
                width: 32,
                height: 32,
                page_stride: 0x1b0,
                base_cell: 0,
                vertical: false,
            },
            &map16,
            &tiles,
            &[],
            &[],
            &palette,
        )
        .unwrap();
        assert_eq!(image.size, [512, 512]);
        assert_eq!(image[(16, 32)], egui::Color32::from_rgb(255, 0, 0));
        let viewport_image = render_level_viewport_image(
            &layers,
            &[],
            NativeLevelMap16Layout {
                width: 32,
                height: 32,
                page_stride: 0x1b0,
                base_cell: 0,
                vertical: false,
            },
            &map16,
            &tiles,
            &[],
            &[],
            &palette,
            PreviewViewportState {
                origin_x: 16,
                origin_y: 32,
                zoom_index: 2,
            },
        )
        .unwrap();
        assert_eq!(viewport_image.size, [512, 448]);
        assert_eq!(viewport_image[(0, 0)], egui::Color32::from_rgb(255, 0, 0));
        assert_eq!(viewport_image[(31, 31)], egui::Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn installed_preview_viewport_clamps_camera_and_exact_zoom() {
        let mut viewport = PreviewViewportState {
            origin_x: -10,
            origin_y: i64::MAX,
            zoom_index: u8::MAX,
        };
        viewport.clamp_to_world(4096, 432);
        assert_eq!(viewport.origin_x, 0);
        assert_eq!(viewport.origin_y, 0);
        assert_eq!(viewport.zoom_index, 1);
        assert_eq!(viewport.viewport().unwrap().zoom(), (1, 1));
        assert_eq!(viewport.camera_maximum(4096, 432), (3584, 0));

        viewport.zoom_index = 0;
        assert_eq!(viewport.viewport().unwrap().zoom(), (1, 2));
        assert_eq!(viewport.camera_maximum(4096, 432), (3072, 0));
        viewport.zoom_index = 4;
        assert_eq!(viewport.viewport().unwrap().zoom(), (4, 1));
        assert_eq!(viewport.camera_maximum(4096, 432), (3968, 320));
    }

    #[test]
    fn object_stream_preview_uses_recovered_vertical_mode_dimensions() {
        let image = lm_rom::RomImage::from_bytes(vec![0; 0x80000]).unwrap();
        let (placements, layout, diagnostics) =
            render_object_placements(&image, &ObjectStream::default(), 3, 0).unwrap();
        assert_eq!(layout.width, 32);
        assert_eq!(layout.height, 13 * 16);
        assert!(layout.vertical);
        assert_eq!(placements.len(), 32 * 13 * 16);
        assert!(placements.iter().all(|placement| placement.word == 0x25));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn standard_sprite_stream_uses_native_placement_and_preview_dispatch() {
        let sprites = lm_level::NativeSpriteStream {
            header: 0,
            expanded: false,
            tokens: vec![lm_level::SpriteToken::Record(lm_level::SpriteRecord {
                encoded: vec![0x20, 0x10, 0x00],
            })],
        };
        let (rendered, diagnostics) = render_sprite_placements(&sprites, 0, 0);
        assert!(diagnostics.is_empty());
        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].x, 16);
        assert_eq!(rendered[0].y, 33);
        assert_eq!(
            rendered[0].subtiles,
            lm_render::render_lunar_magic_standard_sprite(0, false).unwrap()[0].subtiles
        );
    }

    #[test]
    #[ignore = "requires the retained Lunar Magic-created SMW-US ROM fixture"]
    fn retained_level_zero_object_stream_materializes_nonblank_cells() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = std::fs::read(
            root.join("oracle-work/lm363/pristine-us/exanimation-install-positive/after.smc"),
        )
        .unwrap();
        let image = lm_rom::RomImage::from_bytes(bytes).unwrap();
        let mut level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
        level_layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
        let level = lm_project::Project::new(image.clone())
            .load_level_slot(0, level_layout, &lm_level::SpriteLengthTable::standard())
            .unwrap();
        let (placements, _, diagnostics) = render_object_placements(
            &image,
            &level.layer1.objects,
            level.layer1.header.level_mode(),
            level.layer1.header.object_tileset(),
        )
        .unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(placements.iter().any(|placement| placement.word != 0x25));
        let (sprites, sprite_diagnostics) = render_sprite_placements(
            &level.sprites,
            level.layer1.header.level_mode(),
            level.layer1.header.sprite_tileset(),
        );
        assert!(!sprites.is_empty(), "{sprite_diagnostics:?}");
    }
}
