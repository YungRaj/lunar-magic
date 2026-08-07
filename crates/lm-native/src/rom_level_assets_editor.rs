use crate::{document_loader::DocumentLoader, native_level_assets_panels::AggregatePanels};
use eframe::egui;
use lm_app::{
    AppState, Command, NativeLevelAssetsController, NativeLevelAssetsControllerEdit,
    NativeLevelAssetsControllerError, ProfiledControllerSnapshot, RevisionProfile,
    RevisionProfileControllers,
};
use lm_graphics::PaletteOwnership;
use lm_level::{Map16Set, NativeLayer2Data, ObjectStream};
use lm_project::NativeLevelAssetsFile;
use lm_render::{
    MaterializedSuperGraphicsVram, NativeLevelMap16Layout, NativeLevelRasterRequest,
    NativeMap16Composition, NativeMap16DefinitionBank, NativeMap16PaletteRouting,
    NativeMap16Placement, Rgba, StandardLevelOrientation, StandardObjectDefinitionSet,
    StandardObjectPaintedCell, StandardSpritePreviewMode, StandardSpritePreviewSource,
    draw_native_sprite_preview_definition_pages, install_lunar_magic_shared_extended_objects,
    install_lunar_magic_shared_standard_objects, install_lunar_magic_tileset_extended_objects,
    lunar_magic_standard_sprite_preview_source, render_lunar_magic_standard_sprite_with_mode,
    render_mapped_standard_object_stream,
    render_native_level_framebuffer_with_layer_palette_routing,
};

mod commit;
mod image_batch;
mod lifecycle;
mod mwl;
mod mwl_batch;
mod palette_transfer;

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

#[derive(Clone, Debug)]
struct PendingLayer2ModeReset {
    edit: NativeLevelAssetsControllerEdit,
    revision: u64,
    from: u8,
    to: u8,
}

#[derive(Clone)]
struct BatchImageSource {
    snapshot: lm_app::ControllerSnapshot,
    profile: RevisionProfile,
    image: lm_rom::RomImage,
    ownership: PaletteOwnership,
    animation_phase: Option<usize>,
    special_world_passed: bool,
    visibility: crate::application::LevelViewVisibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeSpritePreviewPlacement {
    token_index: usize,
    part_index: usize,
    sprite_number: u8,
    source: StandardSpritePreviewSource,
    definition_index: u16,
    subtiles: [u16; 4],
    x: i32,
    y: i32,
}

struct VisibleLevelParts<'a> {
    layers: [&'a [NativeMap16Placement]; 2],
    sprites: &'a [NativeSpritePreviewPlacement],
}

fn visible_level_parts<'a>(
    visibility: crate::application::LevelViewVisibility,
    layer2: &'a [NativeMap16Placement],
    layer1: &'a [NativeMap16Placement],
    sprites: &'a [NativeSpritePreviewPlacement],
) -> VisibleLevelParts<'a> {
    VisibleLevelParts {
        layers: [
            visibility.layer2.then_some(layer2).unwrap_or(&[]),
            visibility.layer1.then_some(layer1).unwrap_or(&[]),
        ],
        sprites: visibility.sprites.then_some(sprites).unwrap_or(&[]),
    }
}

struct ResolvedLevelGraphics {
    vram: MaterializedSuperGraphicsVram,
    foreground_background_files: usize,
    sprite_files: usize,
    source: &'static str,
}

struct InstalledLayer3 {
    tilemap: Vec<u16>,
    tiles: Vec<lm_graphics::IndexedTile>,
    initial_x: i16,
    initial_y: i16,
    clamp_editor_row_at_30: bool,
    vertical: bool,
    painter_slots: lm_level::LevelLayerSlotAssignments,
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

#[derive(Clone, Copy, Debug)]
struct PreviewDragState {
    pointer_x: f32,
    pointer_y: f32,
    origin_x: i64,
    origin_y: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreviewMap16Selection {
    cell_x: i64,
    cell_y: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InstalledPreviewPhases {
    refresh: Option<usize>,
    assets: Option<usize>,
    selection: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreviewMap16Layer {
    Layer2,
    Layer1,
}

impl PreviewMap16Layer {
    const fn label(self) -> &'static str {
        match self {
            Self::Layer2 => "Layer 2",
            Self::Layer1 => "Layer 1",
        }
    }
}

const fn preview_sprite_source_label(source: StandardSpritePreviewSource) -> &'static str {
    match source {
        StandardSpritePreviewSource::BuiltIn => "built-in",
        StandardSpritePreviewSource::NativeEmpty => "native-empty",
        StandardSpritePreviewSource::CustomDisplay => "custom-display",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreviewSpriteGraphicsPage {
    Ordinary,
    Animated,
}

impl PreviewSpriteGraphicsPage {
    const fn label(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary SP",
            Self::Animated => "animated GFX33",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreviewSpriteSubtile {
    tile: u16,
    page: PreviewSpriteGraphicsPage,
    cgram_row: u8,
    high_priority: bool,
    x_flip: bool,
    y_flip: bool,
}

const fn decode_preview_sprite_subtile(word: u16) -> PreviewSpriteSubtile {
    PreviewSpriteSubtile {
        tile: word & 0x01ff,
        page: if word & 0x0200 == 0 {
            PreviewSpriteGraphicsPage::Ordinary
        } else {
            PreviewSpriteGraphicsPage::Animated
        },
        cgram_row: 8 + ((word >> 10) & 7) as u8,
        high_priority: word & 0x2000 != 0,
        x_flip: word & 0x4000 != 0,
        y_flip: word & 0x8000 != 0,
    }
}

const fn preview_sprite_quadrant_label(index: usize) -> &'static str {
    match index {
        0 => "top-left",
        1 => "bottom-left",
        2 => "top-right",
        3 => "bottom-right",
        _ => "unknown",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreviewMap16Subtile {
    visual_quadrant: usize,
    source_quadrant: usize,
    word: u16,
    tile: u16,
    encoded_palette_row: u8,
    cgram_row: u8,
    high_priority: bool,
    x_flip: bool,
    y_flip: bool,
}

fn decode_preview_map16_subtiles(
    definition: lm_level::Map16Tile,
    outer_x_flip: bool,
    outer_y_flip: bool,
    palette_routing: NativeMap16PaletteRouting,
) -> [PreviewMap16Subtile; 4] {
    let source = [
        definition.top_left,
        definition.top_right,
        definition.bottom_left,
        definition.bottom_right,
    ];
    std::array::from_fn(|visual_quadrant| {
        let output_x = visual_quadrant % 2;
        let output_y = visual_quadrant / 2;
        let source_x = if outer_x_flip { 1 - output_x } else { output_x };
        let source_y = if outer_y_flip { 1 - output_y } else { output_y };
        let source_quadrant = source_y * 2 + source_x;
        let subtile = source[source_quadrant];
        PreviewMap16Subtile {
            visual_quadrant,
            source_quadrant,
            word: subtile.0,
            tile: subtile.tile_number(),
            encoded_palette_row: subtile.palette(),
            cgram_row: palette_routing.palette_row(subtile.palette()),
            high_priority: subtile.priority(),
            x_flip: subtile.x_flip() ^ outer_x_flip,
            y_flip: subtile.y_flip() ^ outer_y_flip,
        }
    })
}

const fn preview_map16_quadrant_label(index: usize) -> &'static str {
    match index {
        0 => "top-left",
        1 => "top-right",
        2 => "bottom-left",
        3 => "bottom-right",
        _ => "unknown",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreviewMap16Hit {
    layer: PreviewMap16Layer,
    definition_bank: NativeMap16DefinitionBank,
    palette_routing: NativeMap16PaletteRouting,
    composition: NativeMap16Composition,
    word: u16,
    definition_index: u16,
    outer_x_flip: bool,
    outer_y_flip: bool,
    definition: Option<lm_level::Map16Tile>,
    acts_like: Option<Result<lm_level::ActsLikeResolution, lm_level::Map16SetError>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreviewMap16Inspection {
    selection: PreviewMap16Selection,
    hits: Vec<PreviewMap16Hit>,
    sprites: Vec<NativeSpritePreviewPlacement>,
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

    fn zoom_at(
        &mut self,
        zoom_index: u8,
        screen_anchor: lm_render::Point,
        width: usize,
        height: usize,
    ) -> Result<(), lm_render::ViewportError> {
        let (numerator, denominator) = Self::ZOOMS
            .get(usize::from(zoom_index))
            .copied()
            .ok_or(lm_render::ViewportError::ZeroZoom)?;
        let mut viewport = self.viewport()?;
        viewport.zoom_at(screen_anchor, numerator, denominator)?;
        self.origin_x = viewport.origin.x;
        self.origin_y = viewport.origin.y;
        self.zoom_index = zoom_index;
        self.clamp_to_world(width, height);
        Ok(())
    }

    fn pan_from_drag(
        &mut self,
        drag: PreviewDragState,
        pointer_x: f32,
        pointer_y: f32,
        width: usize,
        height: usize,
    ) {
        let (numerator, denominator) = Self::ZOOMS
            .get(usize::from(self.zoom_index))
            .copied()
            .unwrap_or((1, 1));
        let screen_x = (pointer_x - drag.pointer_x).round() as i64;
        let screen_y = (pointer_y - drag.pointer_y).round() as i64;
        let world_x = i128::from(screen_x) * i128::from(denominator) / i128::from(numerator);
        let world_y = i128::from(screen_y) * i128::from(denominator) / i128::from(numerator);
        self.origin_x = drag
            .origin_x
            .saturating_sub(i64::try_from(world_x).unwrap_or_else(|_| {
                if world_x.is_negative() {
                    i64::MIN
                } else {
                    i64::MAX
                }
            }));
        self.origin_y = drag
            .origin_y
            .saturating_sub(i64::try_from(world_y).unwrap_or_else(|_| {
                if world_y.is_negative() {
                    i64::MIN
                } else {
                    i64::MAX
                }
            }));
        self.clamp_to_world(width, height);
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

fn preview_wheel_zoom_index(current: u8, delta_y: f32) -> Option<u8> {
    if !delta_y.is_finite() || delta_y == 0.0 {
        return None;
    }
    let maximum =
        u8::try_from(PreviewViewportState::ZOOMS.len() - 1).expect("preview zoom count fits in u8");
    let current = current.min(maximum);
    let changed = if delta_y.is_sign_positive() {
        current.saturating_add(1).min(maximum)
    } else {
        current.saturating_sub(1)
    };
    (changed != current).then_some(changed)
}

fn preview_modified_wheel_delta(modifiers: egui::Modifiers, delta_y: f32) -> Option<f32> {
    (modifiers.ctrl || modifiers.command || modifiers.mac_cmd).then_some(delta_y)
}

fn preview_pointer_anchor(rect: egui::Rect, pointer: egui::Pos2) -> lm_render::Point {
    let maximum_x = i64::from(PreviewViewportState::WIDTH.saturating_sub(1));
    let maximum_y = i64::from(PreviewViewportState::HEIGHT.saturating_sub(1));
    lm_render::Point {
        x: ((pointer.x - rect.left()).round() as i64).clamp(0, maximum_x),
        y: ((pointer.y - rect.top()).round() as i64).clamp(0, maximum_y),
    }
}

fn preview_map16_selection(
    viewport: PreviewViewportState,
    screen: lm_render::Point,
) -> Result<PreviewMap16Selection, lm_render::ViewportError> {
    let world = viewport.viewport()?.screen_to_world(screen)?;
    Ok(PreviewMap16Selection {
        cell_x: world.x.div_euclid(16),
        cell_y: world.y.div_euclid(16),
    })
}

fn inspect_preview_map16_selection(
    selection: PreviewMap16Selection,
    layer2: &[NativeMap16Placement],
    layer1: &[NativeMap16Placement],
    sprites: &[NativeSpritePreviewPlacement],
    map16: &Map16Set,
    background_definitions: &[lm_level::Map16Tile],
    layer2_palette_routing: NativeMap16PaletteRouting,
) -> PreviewMap16Inspection {
    let mut hits = Vec::new();
    let resolution_limit = map16
        .pages
        .len()
        .checked_mul(lm_level::Map16Page::TILE_COUNT)
        .unwrap_or(usize::MAX);
    for (layer, placements) in [
        (PreviewMap16Layer::Layer2, layer2),
        (PreviewMap16Layer::Layer1, layer1),
    ] {
        hits.extend(
            placements
                .iter()
                .filter(|placement| {
                    i64::from(placement.x) == selection.cell_x
                        && i64::from(placement.y) == selection.cell_y
                })
                .map(|placement| {
                    let tile = placement.definition_index;
                    let definition = match placement.definition_bank {
                        NativeMap16DefinitionBank::Foreground => map16.tile(tile).copied(),
                        NativeMap16DefinitionBank::Background => {
                            background_definitions.get(usize::from(tile)).copied()
                        }
                    };
                    PreviewMap16Hit {
                        layer,
                        definition_bank: placement.definition_bank,
                        palette_routing: if layer == PreviewMap16Layer::Layer2 {
                            layer2_palette_routing
                        } else {
                            NativeMap16PaletteRouting::Direct
                        },
                        composition: placement.composition,
                        word: placement.word,
                        definition_index: placement.definition_index,
                        outer_x_flip: placement.outer_x_flip,
                        outer_y_flip: placement.outer_y_flip,
                        definition,
                        acts_like: (placement.definition_bank
                            == NativeMap16DefinitionBank::Foreground)
                            .then(|| {
                                definition.map(|_| map16.resolve_acts_like(tile, resolution_limit))
                            })
                            .flatten(),
                    }
                }),
        );
    }
    let left = selection.cell_x.saturating_mul(16);
    let top = selection.cell_y.saturating_mul(16);
    let right = left.saturating_add(16);
    let bottom = top.saturating_add(16);
    let sprites = sprites
        .iter()
        .filter(|sprite| {
            let sprite_left = i64::from(sprite.x);
            let sprite_top = i64::from(sprite.y);
            let sprite_right = sprite_left.saturating_add(16);
            let sprite_bottom = sprite_top.saturating_add(16);
            sprite_left < right && left < sprite_right && sprite_top < bottom && top < sprite_bottom
        })
        .copied()
        .collect();
    PreviewMap16Inspection {
        selection,
        hits,
        sprites,
    }
}

impl InstalledAnimationOptions {
    const fn active(self) -> bool {
        self.vanilla_tiles || self.palette
    }
}

fn installed_preview_phases(
    options: InstalledAnimationOptions,
    has_selection: bool,
    seconds: f64,
) -> InstalledPreviewPhases {
    let refresh =
        (options.active() || has_selection).then(|| installed_preview_animation_phase(seconds));
    InstalledPreviewPhases {
        refresh,
        assets: options.active().then_some(refresh.unwrap_or(0)),
        selection: has_selection.then_some(u32::try_from(refresh.unwrap_or(0)).unwrap_or(0)),
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
    palette_loader: DocumentLoader,
    palette_persistence: crate::persistence_worker::PersistenceWorker,
    pending_palette_transfer: Option<crate::rom_palette_editor::transfer::PendingTransfer>,
    palette_rgb_expansion: Option<lm_graphics::RgbChannelExpansion>,
    pending_legacy_mwl_load: Option<mwl::PendingLegacyMwlLoad>,
    legacy_mwl_status: Option<String>,
    mwl_batch_worker: mwl_batch::MwlBatchExportWorker,
    mwl_batch_status: Option<String>,
    level_image_status: Option<String>,
    image_batch_worker: image_batch::LevelImageBatchWorker,
    image_batch_options: image_batch::LevelImageBatchOptions,
    pending_load: Option<PendingLoad>,
    pending_layer2_mode_reset: Option<PendingLayer2ModeReset>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
    bypass_validation: Option<String>,
    bypass_layer2_texture: Option<egui::TextureHandle>,
    bypass_preview: LivePreviewState,
    bypass_viewport: PreviewViewportState,
    bypass_drag: Option<PreviewDragState>,
    bypass_map16_grid: bool,
    bypass_selection: Option<PreviewMap16Selection>,
    bypass_inspection: Option<PreviewMap16Inspection>,
}

impl RomLevelAssetsEditor {
    pub(crate) fn invalidate_graphics_preview(&mut self) {
        self.bypass_preview.invalidate();
        self.bypass_layer2_texture = None;
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
        special_world_passed: bool,
        visibility: crate::application::LevelViewVisibility,
    ) -> (bool, Option<Command>) {
        self.poll_palette_file_io(context, project_revision);
        if let Some(result) = self.mwl_batch_worker.show(context) {
            match result {
                Ok(Some(count)) => {
                    self.mwl_batch_status =
                        Some(format!("{count} levels were exported successfully."));
                }
                Ok(None) => self.mwl_batch_status = Some("Batch MWL export cancelled.".into()),
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(result) = self.image_batch_worker.show(context) {
            match result {
                Ok(Some(report)) => {
                    self.level_image_status = Some(format!(
                        "{} level images exported; {} unrenderable levels skipped.",
                        report.exported, report.skipped_unrenderable
                    ));
                }
                Ok(None) => self.level_image_status = Some("Level image export cancelled.".into()),
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
                    if let Some(ui_command) = self.contents(
                        ui,
                        project_revision,
                        special_world_passed,
                        visibility,
                        command.is_some(),
                    ) {
                        command = Some(ui_command);
                    }
                });
        }
        if command.is_none() {
            self.show_layer2_mode_reset_confirmation(context, project_revision);
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        project_revision: u64,
        special_world_passed: bool,
        visibility: crate::application::LevelViewVisibility,
        rom_command_pending: bool,
    ) -> Option<Command> {
        let stale = self.workspace.as_ref()?.controller.revision() != project_revision;
        let palette_busy =
            self.palette_loader.is_running() || self.palette_persistence.is_running();
        let editing_blocked =
            level_asset_editing_blocked(stale, self.mutation_file_busy(), rom_command_pending);
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed. Close and reopen this workspace before committing.",
            );
        }
        if !stale && editing_blocked {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Level import or commit preparation is active; staged editing is temporarily disabled.",
            );
        }
        if let Some(mode) = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.controller.normalized_reserved_level_mode())
        {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!(
                    "Mode ${mode:02X} is reserved. Lunar Magic compatibility uses mode $00 instead."
                ),
            );
        }
        let history = self.workspace.as_ref().map(|workspace| {
            (
                workspace.controller.can_undo(),
                workspace.controller.can_redo(),
                workspace.controller.is_modified(),
            )
        });
        let mut history_action = None;
        if let Some((can_undo, can_redo, modified)) = history {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!editing_blocked && can_undo, egui::Button::new("Undo"))
                    .clicked()
                {
                    history_action = Some(true);
                }
                if ui
                    .add_enabled(!editing_blocked && can_redo, egui::Button::new("Redo"))
                    .clicked()
                {
                    history_action = Some(false);
                }
                ui.label(if modified { "Modified" } else { "Unmodified" });
            });
        }
        if let Some(undo) = history_action {
            let changed = self.workspace.as_mut().is_some_and(|workspace| {
                if undo {
                    workspace.controller.undo()
                } else {
                    workspace.controller.redo()
                }
            });
            if changed {
                self.pending_layer2_mode_reset = None;
                self.invalidate_after_asset_edit();
            }
        }
        ui.horizontal(|ui| {
            ui.label("Allocation search (logical PC hex, end-exclusive)");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        self.palette_file_controls(ui, stale, project_revision);
        let workspace = self.workspace.as_ref()?;
        let file = NativeLevelAssetsFile {
            source_slot: workspace.source_slot,
            assets: workspace.controller.assets().clone(),
        };
        let edit = ui
            .add_enabled_ui(!editing_blocked, |ui| {
                self.panels.show(
                    ui,
                    workspace.controller.revision(),
                    &file,
                    (
                        workspace.controller.layer2(),
                        workspace.controller.layer2_descriptor(),
                    ),
                    workspace.controller.exanimation_features(),
                    workspace.controller.lfix3_level_fields(),
                    &workspace.profile.exanimation_double_size_modes,
                    &workspace.ownership,
                    workspace.controller.sprite_lengths(),
                )
            })
            .inner;
        if let Some(edit) = edit {
            match edit {
                Ok(edit) if !editing_blocked => {
                    if let Some(workspace) = self.workspace.as_mut() {
                        match workspace
                            .controller
                            .apply_edits(std::slice::from_ref(&edit))
                        {
                            Err(
                                NativeLevelAssetsControllerError::Layer2ModeChangeRequiresReset {
                                    from,
                                    to,
                                    ..
                                },
                            ) => {
                                self.pending_layer2_mode_reset = Some(PendingLayer2ModeReset {
                                    edit,
                                    revision: workspace.controller.revision(),
                                    from,
                                    to,
                                });
                            }
                            Err(error) => {
                                self.panels.reject_pending_edit();
                                self.error = Some(error.to_string());
                            }
                            Ok(()) => self.invalidate_after_asset_edit(),
                        }
                    } else {
                        self.panels.reject_pending_edit();
                        self.error = Some("level-assets workspace is closed".into());
                    }
                }
                Ok(_) if !stale => {
                    self.panels.reject_pending_edit();
                    self.error = Some(
                        "level-assets editing is disabled during import or commit preparation"
                            .into(),
                    );
                }
                Ok(_) => {
                    self.panels.reject_pending_edit();
                    self.error = Some("stale ROM workspace cannot accept more edits".into());
                }
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
            if world_width < 16 || world_height < 16 {
                if self.bypass_selection.take().is_some() {
                    self.bypass_inspection = None;
                    self.bypass_preview.invalidate();
                }
            } else if let Some(selection) = self.bypass_selection.as_mut() {
                let previous = *selection;
                selection.cell_x = selection.cell_x.clamp(
                    0,
                    i64::try_from(world_width / 16)
                        .unwrap_or(i64::MAX)
                        .saturating_sub(1),
                );
                selection.cell_y = selection.cell_y.clamp(
                    0,
                    i64::try_from(world_height / 16)
                        .unwrap_or(i64::MAX)
                        .saturating_sub(1),
                );
                if *selection != previous {
                    self.bypass_inspection = None;
                    self.bypass_preview.invalidate();
                }
            }
            let (maximum_x, maximum_y) = self
                .bypass_viewport
                .camera_maximum(world_width, world_height);
            let mut viewport_changed = false;
            let mut selected_zoom = self.bypass_viewport.zoom_index;
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
                            ui.selectable_value(
                                &mut selected_zoom,
                                u8::try_from(index).expect("five zoom entries"),
                                *label,
                            );
                        }
                    });
                if ui.button("Reset view").clicked() {
                    self.bypass_viewport = PreviewViewportState::default();
                    selected_zoom = self.bypass_viewport.zoom_index;
                    self.bypass_drag = None;
                    viewport_changed = true;
                }
                viewport_changed |= ui
                    .checkbox(&mut self.bypass_map16_grid, "Map16 grid")
                    .changed();
            });
            if selected_zoom != self.bypass_viewport.zoom_index {
                let anchor = lm_render::Point {
                    x: i64::from(PreviewViewportState::WIDTH / 2),
                    y: i64::from(PreviewViewportState::HEIGHT / 2),
                };
                if self
                    .bypass_viewport
                    .zoom_at(selected_zoom, anchor, world_width, world_height)
                    .is_ok()
                {
                    self.bypass_drag = None;
                    viewport_changed = true;
                }
            }
            if viewport_changed {
                self.bypass_preview.invalidate();
            }
            if let Some(selection) = self.bypass_selection {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Selected Map16 cell X ${:03X}, Y ${:03X}",
                        selection.cell_x, selection.cell_y
                    ));
                    if ui.button("Clear selection").clicked() {
                        self.bypass_selection = None;
                        self.bypass_inspection = None;
                        self.bypass_preview.invalidate();
                    }
                });
            }
        }
        let animation_options = self.workspace.as_ref().map_or(
            InstalledAnimationOptions {
                vanilla_tiles: false,
                palette: false,
            },
            installed_animation_options,
        );
        let phases = installed_preview_phases(
            animation_options,
            self.bypass_selection.is_some(),
            ui.input(|input| input.time),
        );
        if self.bypass_preview.take_refresh(phases.refresh) {
            let result = self
                .workspace
                .as_ref()
                .ok_or_else(|| "level-assets workspace is closed".to_owned())
                .and_then(|workspace| {
                    render_super_graphics_level_preview(
                        workspace,
                        phases.assets,
                        self.bypass_viewport,
                        self.bypass_map16_grid,
                        self.bypass_selection,
                        phases.selection,
                        special_world_passed,
                        visibility,
                    )
                });
            self.bypass_preview.finish_refresh(result.is_ok());
            match result {
                Ok((image, diagnostics, inspection)) => {
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
                    self.bypass_inspection = inspection;
                }
                Err(error) => {
                    self.bypass_layer2_texture = None;
                    self.bypass_inspection = None;
                    self.bypass_validation = Some(error);
                }
            }
        }
        if self.bypass_preview.enabled
            && (animation_options.active() || self.bypass_selection.is_some())
            && !self.bypass_preview.failed
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(60));
        }
        if let Some(validation) = &self.bypass_validation {
            ui.label(validation);
        }
        if let Some(inspection) = &self.bypass_inspection {
            ui.group(|ui| {
                ui.label(format!(
                    "Resolved staged Map16 cell X ${:03X}, Y ${:03X} in painter order",
                    inspection.selection.cell_x, inspection.selection.cell_y
                ));
                if inspection.hits.is_empty() {
                    ui.monospace("No Layer 2 or Layer 1 placement resolves at this cell.");
                }
                for (paint_index, hit) in inspection.hits.iter().enumerate() {
                    let outer_flips = match (hit.outer_x_flip, hit.outer_y_flip) {
                        (false, false) => "--",
                        (true, false) => "X-",
                        (false, true) => "-Y",
                        (true, true) => "XY",
                    };
                    let definition_number = match hit.definition_bank {
                        NativeMap16DefinitionBank::Foreground => u32::from(hit.definition_index),
                        NativeMap16DefinitionBank::Background => {
                            0x8000 + u32::from(hit.definition_index)
                        }
                    };
                    ui.monospace(format!(
                        "Paint {} {}: word ${:04X}, {} definition ${definition_number:04X}, outer flips {outer_flips}, composition {}",
                        paint_index + 1,
                        hit.layer.label(),
                        hit.word,
                        match hit.definition_bank {
                            NativeMap16DefinitionBank::Foreground => "foreground",
                            NativeMap16DefinitionBank::Background => "background",
                        },
                        match hit.composition {
                            NativeMap16Composition::Opaque => "opaque",
                            NativeMap16Composition::Average => "average",
                            NativeMap16Composition::HalfColor => "half-color",
                        },
                    ));
                    if let Some(definition) = hit.definition {
                        ui.monospace(format!(
                            "  {}subtiles ${:04X} ${:04X} ${:04X} ${:04X}",
                            if hit.definition_bank == NativeMap16DefinitionBank::Foreground {
                                format!("acts like ${:04X}; ", definition.acts_like)
                            } else {
                                "no Acts-Like; ".to_owned()
                            },
                            definition.top_left.0,
                            definition.top_right.0,
                            definition.bottom_left.0,
                            definition.bottom_right.0,
                        ));
                        for subtile in
                            decode_preview_map16_subtiles(
                                definition,
                                hit.outer_x_flip,
                                hit.outer_y_flip,
                                hit.palette_routing,
                            )
                        {
                            ui.monospace(format!(
                                "    {} <= {} word ${:04X}: tile ${:03X}, palette row {} => CGRAM row {}, priority {}, flips {}{}",
                                preview_map16_quadrant_label(subtile.visual_quadrant),
                                preview_map16_quadrant_label(subtile.source_quadrant),
                                subtile.word,
                                subtile.tile,
                                subtile.encoded_palette_row,
                                subtile.cgram_row,
                                if subtile.high_priority { "high" } else { "low" },
                                if subtile.x_flip { "X" } else { "-" },
                                if subtile.y_flip { "Y" } else { "-" },
                            ));
                        }
                        match &hit.acts_like {
                            Some(Ok(resolution)) => {
                                let chain = resolution
                                    .chain
                                    .iter()
                                    .map(|tile| format!("${tile:04X}"))
                                    .collect::<Vec<_>>()
                                    .join(" -> ");
                                ui.monospace(format!(
                                    "  resolved acts-like chain {chain}; terminal ${:04X}",
                                    resolution.terminal
                                ));
                            }
                            Some(Err(error)) => {
                                ui.monospace(format!("  acts-like resolution failed: {error}"));
                            }
                            None => {}
                        }
                    } else {
                        ui.monospace("  Map16 definition is unavailable.");
                    }
                }
                ui.label("Overlapping staged sprite-preview parts in painter order");
                if inspection.sprites.is_empty() {
                    ui.monospace("No materialized sprite-preview part overlaps this cell.");
                }
                for (paint_index, sprite) in inspection.sprites.iter().enumerate() {
                    ui.monospace(format!(
                        "Sprite paint {}: token {}, ID ${:02X} {}, part {}, definition ${:04X}, origin ({}, {})",
                        paint_index + 1,
                        sprite.token_index,
                        sprite.sprite_number,
                        preview_sprite_source_label(sprite.source),
                        sprite.part_index,
                        sprite.definition_index,
                        sprite.x,
                        sprite.y,
                    ));
                    ui.monospace(format!(
                        "  subtiles ${:04X} ${:04X} ${:04X} ${:04X}",
                        sprite.subtiles[0],
                        sprite.subtiles[1],
                        sprite.subtiles[2],
                        sprite.subtiles[3],
                    ));
                    for (quadrant, word) in sprite.subtiles.into_iter().enumerate() {
                        let subtile = decode_preview_sprite_subtile(word);
                        ui.monospace(format!(
                            "    {}: tile ${:03X}, {}, CGRAM row {}, priority {}, flips {}{}",
                            preview_sprite_quadrant_label(quadrant),
                            subtile.tile,
                            subtile.page.label(),
                            subtile.cgram_row,
                            if subtile.high_priority { "high" } else { "low" },
                            if subtile.x_flip { "X" } else { "-" },
                            if subtile.y_flip { "Y" } else { "-" },
                        ));
                    }
                }
            });
        }
        if let Some(texture) = &self.bypass_layer2_texture {
            let response = ui
                .add(
                    egui::Image::new((texture.id(), texture.size_vec2()))
                        .sense(egui::Sense::click_and_drag()),
                )
                .on_hover_cursor(egui::CursorIcon::Grab)
                .on_hover_text(
                    "Click to select a Map16 cell; drag to pan; Ctrl/Command-wheel zooms",
                );
            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let anchor = preview_pointer_anchor(response.rect, pointer);
                if let Ok(selection) = preview_map16_selection(self.bypass_viewport, anchor)
                    && self.bypass_selection != Some(selection)
                {
                    self.bypass_selection = Some(selection);
                    self.bypass_inspection = None;
                    self.bypass_preview.invalidate();
                }
            }
            if response.drag_started() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    self.bypass_drag = Some(PreviewDragState {
                        pointer_x: pointer.x,
                        pointer_y: pointer.y,
                        origin_x: self.bypass_viewport.origin_x,
                        origin_y: self.bypass_viewport.origin_y,
                    });
                }
            }
            if response.dragged() {
                if let (Some(drag), Some(pointer), Some(header)) =
                    (self.bypass_drag, response.interact_pointer_pos(), header)
                {
                    let previous = self.bypass_viewport;
                    let (world_width, world_height) = preview_world_extent(header);
                    self.bypass_viewport.pan_from_drag(
                        drag,
                        pointer.x,
                        pointer.y,
                        world_width,
                        world_height,
                    );
                    if self.bypass_viewport != previous {
                        self.bypass_preview.invalidate();
                    }
                }
            }
            if response.drag_stopped() {
                self.bypass_drag = None;
            }
            let wheel_delta = ui.input(|input| {
                preview_modified_wheel_delta(input.modifiers, input.raw_scroll_delta.y)
            });
            if response.hovered()
                && let (Some(delta_y), Some(pointer), Some(header)) =
                    (wheel_delta, response.hover_pos(), header)
                && let Some(zoom_index) =
                    preview_wheel_zoom_index(self.bypass_viewport.zoom_index, delta_y)
            {
                let previous = self.bypass_viewport;
                let anchor = preview_pointer_anchor(response.rect, pointer);
                let (world_width, world_height) = preview_world_extent(header);
                if self
                    .bypass_viewport
                    .zoom_at(zoom_index, anchor, world_width, world_height)
                    .is_ok()
                    && self.bypass_viewport != previous
                {
                    self.bypass_drag = None;
                    self.bypass_preview.invalidate();
                }
            }
        }
        ui.separator();
        let image_animation_phase = Some(installed_preview_animation_phase(
            ui.input(|input| input.time),
        ));
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.controller.is_modified());
        if ui
            .add_enabled(
                !stale && !self.image_batch_worker.is_running() && !palette_busy,
                egui::Button::new("Export full level image…"),
            )
            .clicked()
        {
            match self.export_level_image(image_animation_phase, special_world_passed, visibility) {
                Ok(Some(path)) => {
                    self.level_image_status =
                        Some(format!("Exported full level image to {}.", path.display()));
                }
                Ok(None) => {}
                Err(error) => self.error = Some(error),
            }
        }
        ui.horizontal(|ui| {
            let enabled = !stale
                && !modified
                && !self.image_batch_worker.is_running()
                && !self.mwl_batch_worker.is_running()
                && !palette_busy;
            for (label, format) in [
                (
                    "Export multiple level PNGs…",
                    image_batch::LevelImageFormat::Png,
                ),
                (
                    "Export multiple level BMPs…",
                    image_batch::LevelImageFormat::Bmp,
                ),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked()
                    && let Err(error) = self.start_level_image_batch(
                        format,
                        image_animation_phase,
                        special_world_passed,
                        visibility,
                    )
                {
                    self.error = Some(error);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled(
                !self.image_batch_worker.is_running(),
                egui::Checkbox::new(
                    &mut self.image_batch_options.modified_only,
                    "Only levels stored in expanded ROM space",
                ),
            );
            ui.add_enabled(
                !self.image_batch_worker.is_running(),
                egui::Checkbox::new(
                    &mut self.image_batch_options.auto_set_screens,
                    "Auto-set image screen count",
                ),
            );
        });
        if let Some(status) = &self.level_image_status {
            ui.label(status);
        }
        self.show_mwl_actions(ui, stale, modified);
        if let Some(status) = &self.legacy_mwl_status {
            ui.label(status);
        }
        if let Some(status) = &self.mwl_batch_status {
            ui.label(status);
        }
        if ui
            .add_enabled(
                modified
                    && !editing_blocked
                    && !self.manifest_loader.is_running()
                    && !self.image_batch_worker.is_running()
                    && !palette_busy,
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
                modified
                    && !editing_blocked
                    && !self.manifest_loader.is_running()
                    && !self.image_batch_worker.is_running()
                    && !palette_busy,
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

    fn mutation_file_busy(&self) -> bool {
        self.palette_loader.is_running()
            || self.mwl_loader.is_running()
            || self.legacy_mwl_loader.is_running()
            || self.manifest_loader.is_running()
    }

    fn show_layer2_mode_reset_confirmation(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) {
        if self.mutation_file_busy() {
            return;
        }
        let Some(pending) = self.pending_layer2_mode_reset.clone() else {
            return;
        };
        let current_revision = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.controller.revision());
        if !layer2_reset_confirmation_is_current(
            pending.revision,
            current_revision,
            project_revision,
        ) {
            self.pending_layer2_mode_reset = None;
            self.error = Some(
                "the ROM workspace changed before the Layer 2 reset was confirmed; stage the header change again"
                    .into(),
            );
            return;
        }
        egui::Window::new("Reset Layer 2 for level mode change?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format!(
                    "Changing level mode ${:02X} to ${:02X} switches Layer 2 storage formats.",
                    pending.from, pending.to
                ));
                ui.label(
                    "Lunar Magic clears the tilemap workspace when entering a tilemap-backed mode. Object-backed data remains available if you switch back before saving.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_layer2_mode_reset = None;
                    }
                    if ui.button("Reset Layer 2 and stage changes").clicked() {
                        let result = self
                            .workspace
                            .as_mut()
                            .ok_or_else(|| "level-assets workspace is closed".to_owned())
                            .and_then(|workspace| {
                                workspace
                                    .controller
                                    .apply_edits_with_layer2_reset(
                                        std::slice::from_ref(&pending.edit),
                                        true,
                                    )
                                    .map_err(|error| error.to_string())
                            });
                        self.pending_layer2_mode_reset = None;
                        match result {
                            Ok(()) => self.invalidate_after_asset_edit(),
                            Err(error) => self.error = Some(error),
                        }
                    }
                });
            });
    }

    fn invalidate_after_asset_edit(&mut self) {
        self.bypass_validation = None;
        self.bypass_layer2_texture = None;
        self.bypass_inspection = None;
        self.bypass_preview.invalidate();
        self.panels.invalidate();
    }
}

const fn level_asset_editing_blocked(
    stale: bool,
    mutation_file_busy: bool,
    rom_command_pending: bool,
) -> bool {
    stale || mutation_file_busy || rom_command_pending
}

fn layer2_reset_confirmation_is_current(
    pending_revision: u64,
    controller_revision: Option<u64>,
    project_revision: u64,
) -> bool {
    controller_revision == Some(pending_revision) && project_revision == pending_revision
}

impl RomLevelAssetsEditor {
    fn start_level_image_batch(
        &mut self,
        format: image_batch::LevelImageFormat,
        animation_phase: Option<usize>,
        special_world_passed: bool,
        visibility: crate::application::LevelViewVisibility,
    ) -> Result<(), String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let Some(template) = crate::dialogs::choose_level_image_batch_template(format.extension())
        else {
            return Ok(());
        };
        let source = BatchImageSource {
            snapshot: workspace.snapshot.clone(),
            profile: workspace.profile.clone(),
            image: workspace.image.clone(),
            ownership: workspace.ownership.clone(),
            animation_phase,
            special_world_passed,
            visibility,
        };
        self.level_image_status = None;
        self.image_batch_worker
            .start(source, template, format, self.image_batch_options)
    }

    fn export_level_image(
        &self,
        animation_phase: Option<usize>,
        special_world_passed: bool,
        visibility: crate::application::LevelViewVisibility,
    ) -> Result<Option<std::path::PathBuf>, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let Some(destination) = crate::dialogs::choose_level_image_save_path(workspace.source_slot)
        else {
            return Ok(None);
        };
        let (canvas, _, _) = render_super_graphics_level_canvas(
            workspace,
            animation_phase,
            None,
            special_world_passed,
            visibility,
        )?;
        let canvas = crop_level_image_canvas(
            &canvas,
            &workspace.controller.assets().level,
            lm_level::LevelScreenExtentMode::Auto,
        )?;
        publish_level_image(&destination, &canvas)?;
        Ok(Some(destination))
    }
}

fn render_batch_level_canvas(
    source: &BatchImageSource,
    level: u16,
    auto_set_screens: bool,
) -> Result<lm_render::Canvas, String> {
    let mut snapshot = source.snapshot.clone();
    snapshot.mode = lm_app::EditorMode::Level(level);
    let controller = source
        .profile
        .decode_native_level_assets(&snapshot, source.ownership.clone())
        .map_err(|error| error.to_string())?;
    let workspace = Workspace {
        controller,
        internal_header: snapshot.identity.internal_header_offset,
        snapshot,
        profile: source.profile.clone(),
        source_slot: level,
        image: source.image.clone(),
        ownership: source.ownership.clone(),
    };
    let (canvas, _, _) = render_super_graphics_level_canvas(
        &workspace,
        source.animation_phase,
        None,
        source.special_world_passed,
        source.visibility,
    )?;
    crop_level_image_canvas(
        &canvas,
        &workspace.controller.assets().level,
        if auto_set_screens {
            lm_level::LevelScreenExtentMode::Auto
        } else {
            lm_level::LevelScreenExtentMode::Stored
        },
    )
}

fn crop_level_image_canvas(
    canvas: &lm_render::Canvas,
    level: &lm_project::LoadedLevelSlot,
    extent_mode: lm_level::LevelScreenExtentMode,
) -> Result<lm_render::Canvas, String> {
    let mode = lm_profile::smw_us_v1_level_mode(level.layer1.header.level_mode());
    if mode.editor_major_screens == 0 {
        return Err(format!(
            "level mode {:02X} has no editor canvas",
            mode.index
        ));
    }
    let screens = lm_level::native_level_screen_count_with_header(
        level.layer1.header,
        &level.layer1.objects,
        &level.sprites,
        extent_mode,
    )
    .min(mode.editor_major_screens);
    let (width, height) = if mode.vertical {
        (canvas.width(), usize::from(screens) * 16 * 16)
    } else {
        (usize::from(screens) * 16 * 16, canvas.height())
    };
    canvas
        .crop_origin(width, height)
        .map_err(|error| error.to_string())
}

fn publish_level_image(
    destination: &std::path::Path,
    canvas: &lm_render::Canvas,
) -> Result<(), String> {
    let extension = destination
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or("full level image output requires a .png or .bmp extension")?;
    let bytes = if extension.eq_ignore_ascii_case("png") {
        lm_render::encode_png(canvas).map_err(|error| error.to_string())?
    } else if extension.eq_ignore_ascii_case("bmp") {
        lm_render::encode_bmp(canvas).map_err(|error| error.to_string())?
    } else {
        return Err("full level image output requires a .png or .bmp extension".into());
    };
    lm_app::file_persistence::write_new(destination, &bytes).map_err(|error| error.to_string())
}

fn validate_super_graphics(workspace: &Workspace) -> String {
    let project = lm_project::Project::new(workspace.image.clone());
    let header = workspace.controller.assets().level.layer1.header;
    match resolve_level_graphics(workspace, &project, header, false) {
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
    show_map16_grid: bool,
    selection: Option<PreviewMap16Selection>,
    selection_phase: Option<u32>,
    special_world_passed: bool,
    visibility: crate::application::LevelViewVisibility,
) -> Result<
    (
        egui::ColorImage,
        Vec<String>,
        Option<PreviewMap16Inspection>,
    ),
    String,
> {
    let (canvas, diagnostics, inspection) = render_super_graphics_level_canvas(
        workspace,
        animation_phase,
        selection,
        special_world_passed,
        visibility,
    )?;
    render_level_viewport_canvas(
        &canvas,
        viewport,
        show_map16_grid,
        selection,
        selection_phase,
    )
    .map(canvas_to_color_image)
    .map(|image| (image, diagnostics, inspection))
}

fn render_super_graphics_level_canvas(
    workspace: &Workspace,
    animation_phase: Option<usize>,
    selection: Option<PreviewMap16Selection>,
    special_world_passed: bool,
    visibility: crate::application::LevelViewVisibility,
) -> Result<
    (
        lm_render::Canvas,
        Vec<String>,
        Option<PreviewMap16Inspection>,
    ),
    String,
> {
    let project = lm_project::Project::new(workspace.image.clone());
    let header = workspace.controller.assets().level.layer1.header;
    let resolved = resolve_level_graphics(workspace, &project, header, special_world_passed)?;
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
    let map16 = if is_smw_us_v1_profile(&workspace.profile) {
        let mut snapshot = workspace.snapshot.clone();
        snapshot.mode = lm_app::EditorMode::Map16;
        lm_app::SmwMap16Controller::decode(&snapshot)
            .map_err(|error| error.to_string())?
            .set()
            .clone()
    } else {
        project
            .load_map16_set(workspace.profile.map16)
            .map_err(|error| error.to_string())?
    };
    let (layer1, layout, mut diagnostics) = render_object_placements(
        &workspace.image,
        &workspace.controller.assets().level.layer1.objects,
        header.level_mode(),
        header.object_tileset(),
    )?;
    if !visibility.layer1 {
        diagnostics.clear();
    }
    let layer2_data = workspace.controller.layer2();
    let background_definitions = if matches!(layer2_data, Some(NativeLayer2Data::Tilemap(_))) {
        if !is_smw_us_v1_profile(&workspace.profile) {
            return Err(format!(
                "background Map16 definitions are not recovered for profile {}",
                workspace.profile.name
            ));
        }
        background_map16_definitions(&project)?
    } else {
        Vec::new()
    };
    let layer2_palette_routing = installed_layer2_palette_routing(
        matches!(layer2_data, Some(NativeLayer2Data::Objects(_))),
        header.object_tileset(),
    );
    let layer2 = match layer2_data {
        Some(NativeLayer2Data::Tilemap(tilemap)) => layer2_placements(
            tilemap,
            workspace
                .controller
                .layer2_descriptor()
                .map_or(0, lm_level::MwlLayer2Descriptor::active_bank),
            header.object_tileset(),
            installed_layer2_background_half_color(header.level_mode()),
        )?,
        Some(NativeLayer2Data::Objects(objects)) => {
            let (placements, _, layer2_diagnostics) = render_object_placements(
                &workspace.image,
                &objects.objects,
                header.level_mode(),
                header.object_tileset(),
            )?;
            if visibility.layer2 {
                diagnostics.extend(
                    layer2_diagnostics
                        .into_iter()
                        .map(|diagnostic| format!("Layer 2 {diagnostic}")),
                );
            }
            placements
        }
        None => Vec::new(),
    };
    let (sprites, sprite_diagnostics) = render_sprite_placements(
        &workspace.controller.assets().level.sprites,
        header.level_mode(),
        header.sprite_tileset(),
    );
    if visibility.sprites {
        diagnostics.extend(sprite_diagnostics);
    }
    let visible = visible_level_parts(visibility, &layer2, &layer1, &sprites);
    let layer3 = load_installed_layer3(workspace, &project, header, visibility.layer3)?;
    let inspection = selection.map(|selection| {
        inspect_preview_map16_selection(
            selection,
            visible.layers[0],
            visible.layers[1],
            visible.sprites,
            &map16,
            &background_definitions,
            layer2_palette_routing,
        )
    });
    let animated_sprite_tiles =
        crate::vanilla_map16_preview::materialize_sprite_display_tiles(special_graphics.gfx33);
    render_level_canvas_with_layer_palette_routing(
        &visible.layers,
        visible.sprites,
        layout,
        &map16,
        &background_definitions,
        &vram.foreground_background,
        &vram.sprites,
        &animated_sprite_tiles,
        &palette,
        &[layer2_palette_routing, NativeMap16PaletteRouting::Direct],
        layer3.as_ref(),
    )
    .map(|canvas| (canvas, diagnostics, inspection))
}

const fn installed_layer2_palette_routing(
    object_backed: bool,
    object_tileset: u8,
) -> NativeMap16PaletteRouting {
    if object_backed && object_tileset == 3 {
        NativeMap16PaletteRouting::ShiftLowRowsByFour
    } else {
        NativeMap16PaletteRouting::Direct
    }
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
    let (gfx33_file, gfx32_file, gfx33_layout, gfx32_layout) = if entries > 0x33 {
        (0x33, 0x32, layout, layout)
    } else {
        let special = lm_profile::smw_us_v1_special_graphics_layouts(&project.rom)
            .map_err(|error| format!("cannot resolve profiled special graphics: {error}"))?;
        (0, 0, special.gfx33, special.gfx32)
    };
    let decoded_gfx33 = project
        .load_decompressed_graphics_file(gfx33_file, gfx33_layout)
        .map_err(|error| format!("cannot load profiled GFX33: {error}"))?;
    let mut gfx33 = lm_graphics::decode_planar_tiles(&decoded_gfx33, 3)
        .map_err(|error| format!("cannot decode profiled GFX33 as 3bpp: {error}"))?;
    gfx33.resize_with(gfx33.len() + 0x30, || {
        lm_graphics::IndexedTile::new([0; lm_graphics::IndexedTile::PIXEL_COUNT])
    });
    let gfx32 = include_gfx32
        .then(|| {
            let decoded = project
                .load_decompressed_graphics_file(gfx32_file, gfx32_layout)
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

fn load_installed_layer3(
    workspace: &Workspace,
    project: &lm_project::Project,
    header: lm_level::LegacyLevelHeader,
    visible: bool,
) -> Result<Option<InstalledLayer3>, String> {
    if !visible {
        return Ok(None);
    }
    let expanded_settings = workspace.controller.assets().expanded_settings.as_ref();
    let custom_tilemap = expanded_settings
        .filter(|settings| settings.layer3_tilemap_enabled())
        .map(lm_level::ExpandedLevelSettingsRecord::layer3_tilemap_graphics_descriptor)
        .transpose()
        .map_err(|error| error.to_string())?;
    if !is_smw_us_v1_profile(&workspace.profile) {
        return Err(format!(
            "ordinary Layer 3 tables are not recovered for profile {}",
            workspace.profile.name
        ));
    }
    let entrance = project
        .load_vanilla_main_entrance(
            usize::from(workspace.source_slot),
            lm_profile::smw_us_v1_vanilla_entrance_layout(),
        )
        .map_err(|error| error.to_string())?;
    let Some(mut layer3) =
        lm_profile::load_smw_us_v1_level_layer3(project, entrance, header.object_tileset())
            .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    if let Some(descriptor) = custom_tilemap {
        let decoded = if descriptor.file() == 0x7f {
            Vec::new()
        } else {
            project
                .load_decompressed_graphics_file(
                    usize::from(descriptor.file()),
                    workspace.profile.graphics,
                )
                .map_err(|error| {
                    format!(
                        "cannot load custom Layer 3 tilemap GFX{:03X}: {error}",
                        descriptor.file()
                    )
                })?
        };
        layer3.tilemap = materialize_custom_layer3_tilemap(descriptor, &decoded)?;
    }
    let tiles = crate::vanilla_map16_preview::load_layer3_tiles(
        project,
        usize::from(workspace.source_slot),
        workspace.profile.graphics,
    )?;
    let (initial_y, clamp_editor_row_at_30) = installed_layer3_editor_position(
        expanded_settings.map(lm_level::ExpandedLevelSettingsRecord::layer3_expanded_mode_flags),
        layer3.setting,
        header.object_tileset(),
        layer3.behavior,
        layer3.initial_y,
    );
    let expanded_mode =
        expanded_settings.map(lm_level::ExpandedLevelSettingsRecord::layer3_expanded_mode_flags);
    let painter_slots = lm_level::lunar_magic_level_layer_slots(
        header.level_mode(),
        header.split_layer3_priority(),
        expanded_mode,
    )
    .ok_or_else(|| {
        format!(
            "level mode {:02X} has no Lunar Magic painter-slot assignment",
            header.level_mode()
        )
    })?;
    Ok(Some(InstalledLayer3 {
        tilemap: layer3.tilemap,
        tiles,
        initial_x: layer3.initial_x,
        initial_y,
        clamp_editor_row_at_30,
        vertical: lm_profile::smw_us_v1_level_mode(header.level_mode()).vertical,
        painter_slots,
    }))
}

fn installed_layer3_editor_position(
    expanded_mode: Option<lm_level::Layer3ExpandedModeFlags>,
    layer3_setting: u8,
    object_tileset: u8,
    vanilla_behavior: lm_profile::SmwUsV1Layer3Behavior,
    vanilla_initial_y: i16,
) -> (i16, bool) {
    if let Some(row) =
        expanded_mode.and_then(|flags| flags.editor_row(layer3_setting, object_tileset))
    {
        return (row.offset * 16, row.clamp_at_30);
    }
    (
        crate::vanilla_map16_preview::vanilla_layer3_editor_row_offset(
            vanilla_behavior,
            object_tileset,
        )
        .map_or(vanilla_initial_y, |row| row * 16),
        false,
    )
}

fn materialize_custom_layer3_tilemap(
    descriptor: lm_level::Layer3TilemapGraphicsDescriptor,
    decoded_file: &[u8],
) -> Result<Vec<u16>, String> {
    let mut workspace = lm_level::Layer3TilemapWorkspace::decode(
        &0x38fc_u16
            .to_le_bytes()
            .repeat(lm_profile::SMW_US_V1_LAYER3_TILEMAP_SIDE.pow(2)),
    )
    .map_err(|error| error.to_string())?;
    // LoadLayer3TilemapGraphics @ $00465080 initializes every word to $38FC and treats file $07F
    // as the no-file sentinel after decoding the descriptor, regardless of its length selector.
    if descriptor.file() != 0x7f {
        workspace
            .apply_decoded_file(descriptor, decoded_file)
            .map_err(|error| error.to_string())?;
    }
    Ok(workspace
        .encoded()
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect())
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
    special_world_passed: bool,
) -> Result<ResolvedLevelGraphics, String> {
    if let Some(settings) = workspace.controller.assets().expanded_settings.as_ref()
        && let Some(loaded) = project
            .load_super_graphics_bypass(settings, workspace.profile.graphics)
            .map_err(|error| error.to_string())?
    {
        let mut resolved = ResolvedLevelGraphics {
            vram: lm_render::materialize_super_graphics_vram(&loaded),
            foreground_background_files: loaded.foreground_background.len(),
            sprite_files: loaded.sprites.len(),
            source: "bypassed",
        };
        apply_special_world_graphics(
            &mut resolved,
            project,
            workspace.profile.graphics,
            special_world_passed,
        )?;
        return Ok(resolved);
    }
    resolve_legacy_level_graphics(workspace, project, header, special_world_passed)
}

fn resolve_legacy_level_graphics(
    workspace: &Workspace,
    project: &lm_project::Project,
    header: lm_level::LegacyLevelHeader,
    special_world_passed: bool,
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
    let mut sprite_files = lm_profile::smw_us_v1_sprite_tileset_graphics_files(
        &workspace.image,
        usize::from(header.sprite_tileset()),
    )
    .map_err(|error| error.to_string())?;
    if special_world_passed {
        sprite_files[1] = 0x31;
    }
    materialize_legacy_level_graphics(
        project,
        workspace.profile.graphics,
        &foreground_files,
        &sprite_files,
    )
}

fn apply_special_world_graphics(
    resolved: &mut ResolvedLevelGraphics,
    project: &lm_project::Project,
    layout: lm_project::GraphicsRomLayout,
    enabled: bool,
) -> Result<(), String> {
    if !enabled {
        return Ok(());
    }
    let decoded = project
        .load_decompressed_graphics_file(0x31, layout)
        .map_err(|error| format!("cannot load Special World file GFX31: {error}"))?;
    let special = decode_special_world_graphics(&decoded)?;
    if resolved.vram.sprites.len() < 256 {
        return Err("resolved sprite VRAM does not contain a complete SP2 slot".into());
    }
    resolved.vram.sprites[128..256].clone_from_slice(&special);
    Ok(())
}

fn decode_special_world_graphics(decoded: &[u8]) -> Result<Vec<lm_graphics::IndexedTile>, String> {
    let bitplanes = match decoded.len() {
        0x600 | 0xc00 => 3,
        0x1000 => 4,
        length => {
            return Err(format!(
                "Special World file GFX31 expands to unsupported length {length}"
            ));
        }
    };
    let mut special = lm_graphics::decode_planar_tiles(decoded, bitplanes).map_err(|error| {
        format!("cannot decode Special World file GFX31 as {bitplanes}bpp: {error}")
    })?;
    if special.len() > 128 {
        return Err(format!(
            "Special World file GFX31 has {} tiles, exceeding the 128-tile SP2 slot",
            special.len()
        ));
    }
    special.resize_with(128, || {
        lm_graphics::IndexedTile::new([0; lm_graphics::IndexedTile::PIXEL_COUNT])
    });
    Ok(special)
}

fn is_smw_us_v1_profile(profile: &RevisionProfile) -> bool {
    profile.game == lm_rom::SupportedGame::SuperMarioWorld
        && profile.region == lm_rom::Region::NorthAmerica
        && profile.revision == 0
}

fn background_map16_definitions(
    project: &lm_project::Project,
) -> Result<Vec<lm_level::Map16Tile>, String> {
    let loaded =
        lm_profile::load_smw_us_v1_secondary_map16(project).map_err(|error| error.to_string())?;
    if loaded.definitions.len() % 4 != 0 {
        return Err(format!(
            "background Map16 data has {} words instead of complete four-word definitions",
            loaded.definitions.len()
        ));
    }
    Ok(loaded
        .definitions
        .chunks_exact(4)
        .map(|words| lm_level::Map16Tile {
            top_left: lm_level::Subtile(words[0]),
            top_right: lm_level::Subtile(words[1]),
            bottom_left: lm_level::Subtile(words[2]),
            bottom_right: lm_level::Subtile(words[3]),
            acts_like: 0,
        })
        .collect())
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
    background_definitions: &[lm_level::Map16Tile],
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
        background_definitions,
        tiles,
        sprite_tiles,
        animated_sprite_tiles,
        palette,
    )
    .map(canvas_to_color_image)
}

#[cfg(test)]
fn render_level_viewport_image(
    layers: &[&[NativeMap16Placement]],
    sprites: &[NativeSpritePreviewPlacement],
    layout: NativeLevelMap16Layout,
    map16: &Map16Set,
    background_definitions: &[lm_level::Map16Tile],
    tiles: &[lm_graphics::IndexedTile],
    sprite_tiles: &[lm_graphics::IndexedTile],
    animated_sprite_tiles: &[lm_graphics::IndexedTile],
    palette: &lm_graphics::Palette,
    layer_palette_routing: &[NativeMap16PaletteRouting],
    viewport: PreviewViewportState,
    show_map16_grid: bool,
    selection: Option<PreviewMap16Selection>,
    selection_phase: Option<u32>,
) -> Result<egui::ColorImage, String> {
    let source = render_level_canvas_with_layer_palette_routing(
        layers,
        sprites,
        layout,
        map16,
        background_definitions,
        tiles,
        sprite_tiles,
        animated_sprite_tiles,
        palette,
        layer_palette_routing,
        None,
    )?;
    render_level_viewport_canvas(
        &source,
        viewport,
        show_map16_grid,
        selection,
        selection_phase,
    )
    .map(canvas_to_color_image)
}

fn render_level_viewport_canvas(
    source: &lm_render::Canvas,
    viewport: PreviewViewportState,
    show_map16_grid: bool,
    selection: Option<PreviewMap16Selection>,
    selection_phase: Option<u32>,
) -> Result<lm_render::Canvas, String> {
    let viewport = viewport.viewport().map_err(|error| error.to_string())?;
    let mut output = lm_render::rasterize_canvas_viewport(source, viewport)
        .map_err(|error| error.to_string())?;
    let mut overlays = Vec::with_capacity(2);
    if show_map16_grid {
        let world_origin = viewport
            .world_to_screen(lm_render::Point::default())
            .map_err(|error| error.to_string())?;
        let next_cell = viewport
            .world_to_screen(lm_render::Point { x: 16, y: 16 })
            .map_err(|error| error.to_string())?;
        let cell_width = u32::try_from(next_cell.x - world_origin.x)
            .map_err(|_| "Map16 grid width is not representable".to_owned())?;
        let cell_height = u32::try_from(next_cell.y - world_origin.y)
            .map_err(|_| "Map16 grid height is not representable".to_owned())?;
        overlays.push(lm_render::EditorOverlay::Grid(lm_render::GridOverlay {
            origin_x: world_origin.x,
            origin_y: world_origin.y,
            cell_width,
            cell_height,
            color: Rgba {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 96,
            },
        }));
    }
    if let Some(selection) = selection {
        let left = selection
            .cell_x
            .checked_mul(16)
            .ok_or_else(|| "Map16 selection X coordinate overflowed".to_owned())?;
        let top = selection
            .cell_y
            .checked_mul(16)
            .ok_or_else(|| "Map16 selection Y coordinate overflowed".to_owned())?;
        let top_left = viewport
            .world_to_screen(lm_render::Point { x: left, y: top })
            .map_err(|error| error.to_string())?;
        let bottom_right = viewport
            .world_to_screen(lm_render::Point {
                x: left
                    .checked_add(16)
                    .ok_or_else(|| "Map16 selection right edge overflowed".to_owned())?,
                y: top
                    .checked_add(16)
                    .ok_or_else(|| "Map16 selection bottom edge overflowed".to_owned())?,
            })
            .map_err(|error| error.to_string())?;
        overlays.push(lm_render::EditorOverlay::Selection(
            lm_render::SelectionOverlay {
                bounds: lm_render::WorldRect {
                    left: top_left.x,
                    top: top_left.y,
                    right: bottom_right.x,
                    bottom: bottom_right.y,
                },
                light: Rgba {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                },
                dark: Rgba {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                },
                dash_length: 4,
                phase: selection_phase.unwrap_or(0),
            },
        ));
    }
    if !overlays.is_empty() {
        lm_render::draw_editor_overlays(&mut output, &overlays)
            .map_err(|error| error.to_string())?;
    }
    Ok(output)
}

#[cfg(test)]
fn render_level_canvas(
    layers: &[&[NativeMap16Placement]],
    sprites: &[NativeSpritePreviewPlacement],
    layout: NativeLevelMap16Layout,
    map16: &Map16Set,
    background_definitions: &[lm_level::Map16Tile],
    tiles: &[lm_graphics::IndexedTile],
    sprite_tiles: &[lm_graphics::IndexedTile],
    animated_sprite_tiles: &[lm_graphics::IndexedTile],
    palette: &lm_graphics::Palette,
) -> Result<lm_render::Canvas, String> {
    let routing = vec![NativeMap16PaletteRouting::Direct; layers.len()];
    render_level_canvas_with_layer_palette_routing(
        layers,
        sprites,
        layout,
        map16,
        background_definitions,
        tiles,
        sprite_tiles,
        animated_sprite_tiles,
        palette,
        &routing,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_level_canvas_with_layer_palette_routing(
    layers: &[&[NativeMap16Placement]],
    sprites: &[NativeSpritePreviewPlacement],
    layout: NativeLevelMap16Layout,
    map16: &Map16Set,
    background_definitions: &[lm_level::Map16Tile],
    tiles: &[lm_graphics::IndexedTile],
    sprite_tiles: &[lm_graphics::IndexedTile],
    animated_sprite_tiles: &[lm_graphics::IndexedTile],
    palette: &lm_graphics::Palette,
    layer_palette_routing: &[NativeMap16PaletteRouting],
    installed_layer3: Option<&InstalledLayer3>,
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
    let request = |layers| NativeLevelRasterRequest {
        width: layout.width * 16,
        height: layout.height * 16,
        camera_x: 0,
        camera_y: 0,
        backdrop,
        layers,
        definitions: &definitions,
        background_definitions,
        tiles,
        palette,
    };
    let mut canvas = if let Some(layer3) = installed_layer3 {
        if layers.len() != 2 || layer_palette_routing.len() != 2 {
            return Err(format!(
                "Layer 3 composition requires exactly two Map16 layers and routing entries; got {} layers and {} routing entries",
                layers.len(),
                layer_palette_routing.len()
            ));
        }
        let pixel_count = (layout.width * 16)
            .checked_mul(layout.height * 16)
            .ok_or_else(|| "level image dimensions overflowed".to_owned())?;
        let mut canvas = lm_render::Canvas::from_pixels(
            layout.width * 16,
            layout.height * 16,
            vec![backdrop; pixel_count],
        )
        .map_err(|error| error.to_string())?;
        let layer2 = [layers[0]];
        let layer1 = [layers[1]];
        let layer2_routing = [layer_palette_routing[0]];
        let layer1_routing = [layer_palette_routing[1]];
        for slot in layer3.painter_slots.slots {
            if !slot.enabled {
                continue;
            }
            match slot.source {
                Some(lm_level::LevelLayerSlotSource::Layer2) => {
                    lm_render::draw_native_level_layers_with_layer_palette_routing_and_addition(
                        &mut canvas,
                        request(&layer2),
                        &layer2_routing,
                        &[slot.additive],
                    )
                    .map_err(|error| error.to_string())?;
                }
                Some(lm_level::LevelLayerSlotSource::Layer1) => {
                    lm_render::draw_native_level_layers_with_layer_palette_routing_and_addition(
                        &mut canvas,
                        request(&layer1),
                        &layer1_routing,
                        &[slot.additive],
                    )
                    .map_err(|error| error.to_string())?;
                }
                Some(lm_level::LevelLayerSlotSource::Layer3) => {
                    let draw = |canvas: &mut lm_render::Canvas, high_priority| {
                        draw_installed_layer3_plane(
                            canvas,
                            layer3,
                            palette,
                            high_priority,
                            slot.additive,
                            slot.half_color,
                        );
                    };
                    match slot.layer3_priority {
                        lm_level::Layer3PrioritySelection::Both => {
                            draw(&mut canvas, false);
                            draw(&mut canvas, true);
                        }
                        lm_level::Layer3PrioritySelection::Low => draw(&mut canvas, false),
                        lm_level::Layer3PrioritySelection::High => draw(&mut canvas, true),
                    }
                }
                None => {}
            }
        }
        canvas
    } else {
        render_native_level_framebuffer_with_layer_palette_routing(
            request(layers),
            layer_palette_routing,
        )
        .map_err(|error| error.to_string())?
    };
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

fn draw_installed_layer3_plane(
    canvas: &mut lm_render::Canvas,
    layer3: &InstalledLayer3,
    palette: &lm_graphics::Palette,
    high_priority: bool,
    additive: bool,
    half_color: bool,
) {
    const TILES_PER_SIDE: usize = lm_profile::SMW_US_V1_LAYER3_TILEMAP_SIDE;
    const TILE_PIXELS: usize = lm_graphics::IndexedTile::WIDTH;
    let x_origins = repeating_layer3_plane_origins(layer3.initial_x, canvas.width());
    if layer3.clamp_editor_row_at_30 {
        draw_clamped_installed_layer3_plane(
            canvas,
            layer3,
            palette,
            high_priority,
            additive,
            half_color,
            &x_origins,
        );
        return;
    }
    let y_origins = if layer3.vertical {
        repeating_layer3_plane_origins(layer3.initial_y, canvas.height())
    } else {
        vec![-i32::from(layer3.initial_y)]
    };
    for (position, &word) in layer3
        .tilemap
        .iter()
        .take(TILES_PER_SIDE * TILES_PER_SIDE)
        .enumerate()
    {
        if word == 0x38fc || (word & 0x2000 != 0) != high_priority {
            continue;
        }
        let Some(tile) = layer3.tiles.get(usize::from(word & 0x03ff)) else {
            continue;
        };
        let palette_number = usize::from((word >> 10) & 7);
        let x_flip = word & 0x4000 != 0;
        let y_flip = word & 0x8000 != 0;
        let plane_x = (position % TILES_PER_SIDE) * TILE_PIXELS;
        let plane_y = (position / TILES_PER_SIDE) * TILE_PIXELS;
        for &origin_y in &y_origins {
            for &origin_x in &x_origins {
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
                        let Some(color) = palette
                            .colors
                            .get(palette_number * 4 + usize::from(index))
                            .copied()
                            .map(lm_graphics::Bgr555::to_rgb8)
                        else {
                            continue;
                        };
                        let target_x = origin_x + i32::try_from(plane_x + x).unwrap_or(i32::MAX);
                        let target_y = origin_y + i32::try_from(plane_y + y).unwrap_or(i32::MAX);
                        let (Ok(target_x), Ok(target_y)) =
                            (usize::try_from(target_x), usize::try_from(target_y))
                        else {
                            continue;
                        };
                        if target_x >= canvas.width() || target_y >= canvas.height() {
                            continue;
                        }
                        let source = Rgba {
                            red: if half_color {
                                color.red >> 1
                            } else {
                                color.red
                            },
                            green: if half_color {
                                color.green >> 1
                            } else {
                                color.green
                            },
                            blue: if half_color {
                                color.blue >> 1
                            } else {
                                color.blue
                            },
                            alpha: 255,
                        };
                        let output = if additive {
                            let destination = canvas.get(target_x, target_y).unwrap_or_default();
                            Rgba {
                                red: destination.red.saturating_add(source.red),
                                green: destination.green.saturating_add(source.green),
                                blue: destination.blue.saturating_add(source.blue),
                                alpha: 255,
                            }
                        } else {
                            source
                        };
                        canvas.set(target_x, target_y, output);
                    }
                }
            }
        }
    }
}

fn draw_clamped_installed_layer3_plane(
    canvas: &mut lm_render::Canvas,
    layer3: &InstalledLayer3,
    palette: &lm_graphics::Palette,
    high_priority: bool,
    additive: bool,
    half_color: bool,
    x_origins: &[i32],
) {
    const TILES_PER_SIDE: usize = lm_profile::SMW_US_V1_LAYER3_TILEMAP_SIDE;
    const TILE_PIXELS: usize = lm_graphics::IndexedTile::WIDTH;
    const CELL_PIXELS: usize = TILE_PIXELS * 2;
    // The authenticated 64x64 8x8 plane contains 32 rows of 16-pixel editor cells.
    const SOURCE_CELL_ROWS: i32 = 32;
    const CLAMP_ROW: i32 = 30;

    let row_offset = i32::from(layer3.initial_y) / i32::try_from(CELL_PIXELS).unwrap_or(16);
    let target_cell_rows = canvas.height().div_ceil(CELL_PIXELS);
    for target_cell_row in 0..target_cell_rows {
        let Ok(target_cell_row_i32) = i32::try_from(target_cell_row) else {
            break;
        };
        let source_cell_row = (target_cell_row_i32 + row_offset).min(CLAMP_ROW);
        let source_cell_row = source_cell_row.rem_euclid(SOURCE_CELL_ROWS) as usize;
        for tile_row_in_cell in 0..2 {
            let source_tile_row = source_cell_row * 2 + tile_row_in_cell;
            let target_y = target_cell_row * CELL_PIXELS + tile_row_in_cell * TILE_PIXELS;
            for source_tile_column in 0..TILES_PER_SIDE {
                let position = source_tile_row * TILES_PER_SIDE + source_tile_column;
                let Some(&word) = layer3.tilemap.get(position) else {
                    continue;
                };
                draw_installed_layer3_tile_at_origins(
                    canvas,
                    layer3,
                    palette,
                    high_priority,
                    additive,
                    half_color,
                    word,
                    source_tile_column * TILE_PIXELS,
                    target_y,
                    x_origins,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_installed_layer3_tile_at_origins(
    canvas: &mut lm_render::Canvas,
    layer3: &InstalledLayer3,
    palette: &lm_graphics::Palette,
    high_priority: bool,
    additive: bool,
    half_color: bool,
    word: u16,
    plane_x: usize,
    target_y: usize,
    x_origins: &[i32],
) {
    const TILE_PIXELS: usize = lm_graphics::IndexedTile::WIDTH;
    if word == 0x38fc || (word & 0x2000 != 0) != high_priority {
        return;
    }
    let Some(tile) = layer3.tiles.get(usize::from(word & 0x03ff)) else {
        return;
    };
    let palette_number = usize::from((word >> 10) & 7);
    let x_flip = word & 0x4000 != 0;
    let y_flip = word & 0x8000 != 0;
    for &origin_x in x_origins {
        for y in 0..TILE_PIXELS {
            let output_y = target_y + y;
            if output_y >= canvas.height() {
                continue;
            }
            for x in 0..TILE_PIXELS {
                let source_x = if x_flip { TILE_PIXELS - 1 - x } else { x };
                let source_y = if y_flip { TILE_PIXELS - 1 - y } else { y };
                let Some(index) = tile.pixel(source_x, source_y) else {
                    continue;
                };
                if index == 0 {
                    continue;
                }
                let Some(color) = palette
                    .colors
                    .get(palette_number * 4 + usize::from(index))
                    .copied()
                    .map(lm_graphics::Bgr555::to_rgb8)
                else {
                    continue;
                };
                let target_x = origin_x + i32::try_from(plane_x + x).unwrap_or(i32::MAX);
                let Ok(target_x) = usize::try_from(target_x) else {
                    continue;
                };
                if target_x >= canvas.width() {
                    continue;
                }
                let source = Rgba {
                    red: if half_color {
                        color.red >> 1
                    } else {
                        color.red
                    },
                    green: if half_color {
                        color.green >> 1
                    } else {
                        color.green
                    },
                    blue: if half_color {
                        color.blue >> 1
                    } else {
                        color.blue
                    },
                    alpha: 255,
                };
                let output = if additive {
                    let destination = canvas.get(target_x, output_y).unwrap_or_default();
                    Rgba {
                        red: destination.red.saturating_add(source.red),
                        green: destination.green.saturating_add(source.green),
                        blue: destination.blue.saturating_add(source.blue),
                        alpha: 255,
                    }
                } else {
                    source
                };
                canvas.set(target_x, output_y, output);
            }
        }
    }
}

fn repeating_layer3_plane_origins(position: i16, world_extent: usize) -> Vec<i32> {
    const PLANE_PIXELS: i32 = 512;
    let world_extent = i32::try_from(world_extent).unwrap_or(i32::MAX);
    let mut origin = -i32::from(position);
    while origin > 0 {
        origin -= PLANE_PIXELS;
    }
    while origin + PLANE_PIXELS <= 0 {
        origin += PLANE_PIXELS;
    }
    let mut origins = Vec::new();
    while origin < world_extent {
        origins.push(origin);
        origin += PLANE_PIXELS;
    }
    origins
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
    let placements = object_paints_to_placements(&rendered.painted_cells, layout, object_tileset)?;
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

fn object_paints_to_placements(
    painted_cells: &[StandardObjectPaintedCell],
    layout: NativeLevelMap16Layout,
    object_tileset: u8,
) -> Result<Vec<NativeMap16Placement>, String> {
    let mut coordinates = vec![None; lm_render::LEVEL_MAP16_CACHE_CELLS];
    for y in 0..layout.height {
        for x in 0..layout.width {
            let index = lm_render::NativeLevelMap16Cache::cell_index(layout, x, y);
            if let Some(coordinate) = coordinates.get_mut(index) {
                *coordinate = Some((x, y));
            }
        }
    }
    let mut placements = Vec::with_capacity(painted_cells.len());
    for paint in painted_cells {
        let Some((x, y)) = coordinates.get(paint.index).copied().flatten() else {
            continue;
        };
        placements.push(NativeMap16Placement {
            x: i32::try_from(x).map_err(|_| "object-layer X overflow".to_owned())?,
            y: i32::try_from(y).map_err(|_| "object-layer Y overflow".to_owned())?,
            word: paint.tile,
            definition_index: paint.tile & 0x7fff,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
            composition: object_map16_composition(object_tileset, paint.tile),
        });
    }
    Ok(placements)
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
        let token_index = placement.token_index;
        let source = lunar_magic_standard_sprite_preview_source(placement.sprite_number);
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
            if source == StandardSpritePreviewSource::BuiltIn {
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
        for (part_index, part) in parts.into_iter().enumerate() {
            rendered.push(NativeSpritePreviewPlacement {
                token_index,
                part_index,
                sprite_number: placement.sprite_number,
                source,
                definition_index: part.definition_index,
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

const fn object_map16_composition(
    object_tileset: u8,
    definition_index: u16,
) -> NativeMap16Composition {
    if object_tileset == 4 && matches!(definition_index & 0x7fff, 0x027..=0x02a) {
        NativeMap16Composition::Average
    } else {
        NativeMap16Composition::Opaque
    }
}

const fn layer2_map16_composition(object_tileset: u8, word: u16) -> NativeMap16Composition {
    if object_tileset == 4 && matches!(word & 0x3fff, 0x027..=0x02a) {
        NativeMap16Composition::Average
    } else {
        NativeMap16Composition::Opaque
    }
}

const fn installed_layer2_background_half_color(level_mode: u8) -> bool {
    lm_profile::smw_us_v1_level_mode(level_mode).background_half_color
}

fn layer2_placements(
    tilemap: &[u8],
    active_bank: u8,
    object_tileset: u8,
    background_half_color: bool,
) -> Result<Vec<NativeMap16Placement>, String> {
    if tilemap.len() != lm_level::NATIVE_LAYER2_TILEMAP_LEN {
        return Err(format!(
            "native Layer 2 tilemap has {} bytes instead of {}",
            tilemap.len(),
            lm_level::NATIVE_LAYER2_TILEMAP_LEN
        ));
    }
    if active_bank >= 8 {
        return Err(format!(
            "native Layer 2 background bank {active_bank} is outside $0-$7"
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
            let word = u16::from_le_bytes([tilemap[offset], tilemap[offset + 1]]);
            placements.push(NativeMap16Placement {
                x: i32::try_from(x).map_err(|_| "Layer 2 X coordinate overflow".to_owned())?,
                y: i32::try_from(y).map_err(|_| "Layer 2 Y coordinate overflow".to_owned())?,
                word,
                definition_index: u16::from(active_bank) * 0x1000 + (word & 0x0fff),
                outer_x_flip: word & 0x4000 != 0,
                outer_y_flip: word & 0x8000 != 0,
                definition_bank: NativeMap16DefinitionBank::Background,
                composition: if background_half_color {
                    NativeMap16Composition::HalfColor
                } else {
                    layer2_map16_composition(object_tileset, word)
                },
            });
        }
    }
    Ok(placements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, IndexedTile, Palette};
    use lm_level::{
        LegacyLevelHeader, LevelObjectData, Map16Page, Map16Tile, NativeSpriteStream, ObjectRecord,
        ObjectStream, SpriteLengthTable, Subtile,
    };
    use lm_project::LoadedLevelSlot;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_IMAGE_PATH: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn level_import_workers_and_completion_frame_block_staged_edits() {
        assert!(!level_asset_editing_blocked(false, false, false));
        assert!(level_asset_editing_blocked(true, false, false));
        assert!(level_asset_editing_blocked(false, true, false));
        assert!(level_asset_editing_blocked(false, false, true));

        let mut editor = RomLevelAssetsEditor::default();
        editor
            .mwl_loader
            .start(vec![crate::document_loader::BoundedRead::new(
                std::env::temp_dir()
                    .join(format!("lm-missing-level-import-{}", std::process::id())),
                1,
                "missing MWL fixture",
            )])
            .unwrap();
        assert!(editor.mutation_file_busy());
        assert!(level_asset_editing_blocked(
            false,
            editor.mutation_file_busy(),
            false
        ));
    }

    #[test]
    fn legacy_manifest_compatibility_diagnostics_are_visible_before_sidecar_commit() {
        let mut editor = RomLevelAssetsEditor {
            pending_legacy_mwl_load: Some(mwl::PendingLegacyMwlLoad::Manifest { revision: 0 }),
            ..RomLevelAssetsEditor::default()
        };
        let bytes = b"Lunar Magic Version 3.63\n\
                      attribution\n\
                      FFF 00 00 00 00 00\n\
                      00 000001 level.mw0\n\
                      00 000002 level.mw1\n\
                      00 000003 level.mw2\n"
            .to_vec();
        let result = editor
            .finish_legacy_mwl_load(
                Ok(crate::document_loader::LoadedDocument {
                    files: vec![(std::path::PathBuf::from("manifest.mwl"), bytes)],
                }),
                0,
            )
            .unwrap();
        assert!(result.is_none());
        assert!(
            editor
                .legacy_mwl_status
                .as_deref()
                .unwrap()
                .contains("$FFF was clamped to $1FF")
        );
        assert!(editor.legacy_mwl_loader.is_running());
    }

    #[test]
    fn missing_legacy_palette_surfaces_lunar_magic_fallback_warning() {
        let manifest = lm_level::LegacyMwlManifest::decode(
            b"Lunar Magic Version 3.63\n\
              attribution\n\
              000 00 00 00 00 00\n\
              01 000001 level.mw0\n\
              00 000002 level.mw1\n\
              00 000003 level.mw2\n",
        )
        .unwrap();
        let mut editor = RomLevelAssetsEditor {
            pending_legacy_mwl_load: Some(mwl::PendingLegacyMwlLoad::Sidecars {
                revision: 0,
                manifest,
            }),
            ..RomLevelAssetsEditor::default()
        };
        let error = editor
            .finish_legacy_mwl_load(
                Ok(crate::document_loader::LoadedDocument {
                    files: vec![
                        (std::path::PathBuf::from("level.mw0"), vec![0]),
                        (std::path::PathBuf::from("level.mw1"), vec![0]),
                        (std::path::PathBuf::from("level.mw2"), vec![0]),
                    ],
                }),
                0,
            )
            .unwrap_err();
        assert_eq!(error, "workspace is closed");
        assert_eq!(
            editor.legacy_mwl_status.as_deref(),
            Some(
                "Legacy import compatibility: Couldn't locate the palette file! Switching to non-custom shared palette."
            )
        );
    }

    #[test]
    fn layer2_reset_confirmation_rejects_closed_or_stale_workspaces() {
        assert!(layer2_reset_confirmation_is_current(7, Some(7), 7));
        assert!(!layer2_reset_confirmation_is_current(7, None, 7));
        assert!(!layer2_reset_confirmation_is_current(7, Some(8), 7));
        assert!(!layer2_reset_confirmation_is_current(7, Some(7), 8));
    }

    #[test]
    fn level_visibility_filters_each_domain_without_reordering_painter_layers() {
        let placement = |word| NativeMap16Placement {
            x: 0,
            y: 0,
            word,
            definition_index: word,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
            composition: NativeMap16Composition::Opaque,
        };
        let layer2 = [placement(2)];
        let layer1 = [placement(1)];
        let sprites = [NativeSpritePreviewPlacement {
            token_index: 0,
            part_index: 0,
            sprite_number: 0,
            source: StandardSpritePreviewSource::BuiltIn,
            definition_index: 0,
            subtiles: [0; 4],
            x: 0,
            y: 0,
        }];
        let all = visible_level_parts(
            crate::application::LevelViewVisibility::default(),
            &layer2,
            &layer1,
            &sprites,
        );
        assert_eq!(all.layers[0][0].word, 2);
        assert_eq!(all.layers[1][0].word, 1);
        assert_eq!(all.sprites.len(), 1);

        let hidden = visible_level_parts(
            crate::application::LevelViewVisibility {
                layer1: false,
                layer2: true,
                layer3: false,
                sprites: false,
                tile_grid: false,
            },
            &layer2,
            &layer1,
            &sprites,
        );
        assert_eq!(hidden.layers[0][0].word, 2);
        assert!(hidden.layers[1].is_empty());
        assert!(hidden.sprites.is_empty());
    }

    fn test_installed_layer3(word: u16, additive: bool) -> InstalledLayer3 {
        let mut tilemap = vec![0x38fc; lm_profile::SMW_US_V1_LAYER3_TILEMAP_SIDE.pow(2)];
        tilemap[0] = word;
        let mut painter_slots = lm_level::lunar_magic_level_layer_slots(0, false, None).unwrap();
        for slot in &mut painter_slots.slots {
            if slot.source == Some(lm_level::LevelLayerSlotSource::Layer3) {
                slot.additive = additive;
            }
        }
        InstalledLayer3 {
            tilemap,
            tiles: vec![IndexedTile::new([1; IndexedTile::PIXEL_COUNT])],
            initial_x: 0,
            initial_y: 0,
            clamp_editor_row_at_30: false,
            vertical: true,
            painter_slots,
        }
    }

    #[test]
    fn installed_layer3_raster_honors_priority_addition_and_plane_wrapping() {
        let mut colors = vec![Bgr555(0); 128];
        colors[1] = Bgr555(0x001f);
        let palette = Palette { colors };
        let backdrop = Rgba {
            red: 10,
            green: 20,
            blue: 30,
            alpha: 255,
        };
        let mut canvas =
            lm_render::Canvas::from_pixels(520, 520, vec![backdrop; 520 * 520]).unwrap();
        let layer3 = test_installed_layer3(0x2000, true);

        draw_installed_layer3_plane(&mut canvas, &layer3, &palette, false, true, false);
        assert_eq!(canvas.get(0, 0), Some(backdrop));
        draw_installed_layer3_plane(&mut canvas, &layer3, &palette, true, true, false);
        let expected = Rgba {
            red: 255,
            green: 20,
            blue: 30,
            alpha: 255,
        };
        assert_eq!(canvas.get(0, 0), Some(expected));
        assert_eq!(canvas.get(512, 0), Some(expected));
        assert_eq!(canvas.get(0, 512), Some(expected));
    }

    #[test]
    fn installed_layer3_raster_halves_source_before_composition() {
        let mut colors = vec![Bgr555(0); 128];
        colors[1] = Bgr555(0x001f);
        let palette = Palette { colors };
        let backdrop = Rgba {
            red: 10,
            green: 20,
            blue: 30,
            alpha: 255,
        };
        let mut canvas = lm_render::Canvas::from_pixels(8, 8, vec![backdrop; 64]).unwrap();
        let layer3 = test_installed_layer3(0, false);

        draw_installed_layer3_plane(&mut canvas, &layer3, &palette, false, true, true);
        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 137,
                green: 20,
                blue: 30,
                alpha: 255,
            })
        );
    }

    #[test]
    fn installed_layer3_clamp_repeats_editor_source_row_thirty() {
        let mut colors = vec![Bgr555(0); 128];
        colors[1] = Bgr555(0x001f);
        let palette = Palette { colors };
        let backdrop = Rgba::default();
        let mut canvas = lm_render::Canvas::from_pixels(8, 520, vec![backdrop; 8 * 520]).unwrap();
        let mut layer3 = test_installed_layer3(0x38fc, false);
        layer3.tilemap[60 * 64] = 0;
        layer3.tilemap[61 * 64] = 0;
        layer3.clamp_editor_row_at_30 = true;

        draw_installed_layer3_plane(&mut canvas, &layer3, &palette, false, false, false);

        assert_eq!(canvas.get(0, 479), Some(backdrop));
        assert_eq!(canvas.get(0, 480).unwrap().red, 255);
        assert_eq!(canvas.get(0, 495).unwrap().red, 255);
        assert_eq!(canvas.get(0, 496).unwrap().red, 255);
        assert_eq!(canvas.get(0, 512).unwrap().red, 255);
        assert_eq!(canvas.get(0, 519).unwrap().red, 255);
    }

    #[test]
    fn custom_layer3_tilemap_starts_blank_and_applies_the_exact_descriptor_range() {
        let descriptor = lm_level::Layer3TilemapGraphicsDescriptor::new(0x123, 2, 3).unwrap();
        let mut decoded = vec![0; usize::from(descriptor.effective_byte_length())];
        decoded[..4].copy_from_slice(&[0x34, 0x12, 0x78, 0x56]);
        let tilemap = materialize_custom_layer3_tilemap(descriptor, &decoded).unwrap();

        assert_eq!(tilemap.len(), 64 * 64);
        assert!(tilemap[..0x800].iter().all(|word| *word == 0x38fc));
        assert_eq!(tilemap[0x800..0x802], [0x1234, 0x5678]);
        assert!(tilemap[0x802..0xc00].iter().all(|word| *word == 0));
        assert!(tilemap[0xc00..].iter().all(|word| *word == 0x38fc));
    }

    #[test]
    fn custom_layer3_no_file_sentinel_retains_the_blank_workspace() {
        let descriptor = lm_level::Layer3TilemapGraphicsDescriptor::new(0x7f, 0, 0).unwrap();
        let tilemap = materialize_custom_layer3_tilemap(descriptor, &[]).unwrap();
        assert!(tilemap.iter().all(|word| *word == 0x38fc));
    }

    #[test]
    fn custom_layer3_short_graphics_file_rejects_before_materialization() {
        let descriptor = lm_level::Layer3TilemapGraphicsDescriptor::new(0x123, 2, 0).unwrap();
        assert!(materialize_custom_layer3_tilemap(descriptor, &[0; 0x7ff]).is_err());
    }

    #[test]
    fn retained_lunar_magic_custom_layer3_descriptor_loads_its_rom_graphics_file() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image = lm_rom::RomImage::from_bytes(
            std::fs::read(
                root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
            )
            .unwrap(),
        )
        .unwrap();
        let project = lm_project::Project::new(image);
        let settings = lm_profile::load_smw_us_v1_expanded_level_settings(&project, 0)
            .unwrap()
            .settings;
        assert!(settings.layer3_tilemap_enabled());
        let descriptor = settings.layer3_tilemap_graphics_descriptor().unwrap();
        assert_eq!(descriptor.packed(), 0x2028);
        assert_eq!(settings.layer3_expanded_mode_flags().packed(), 0x000f_0000);
        assert!(!settings.layer3_expanded_mode_flags().enabled());
        let decoded = project
            .load_decompressed_graphics_file(
                usize::from(descriptor.file()),
                lm_profile::smw_us_v1_vanilla_graphics_layout(),
            )
            .unwrap();
        assert_eq!(decoded.len(), 0x800);
        let tilemap = materialize_custom_layer3_tilemap(descriptor, &decoded).unwrap();
        let bytes = tilemap
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(
            lm_oracle::sha256_hex(&bytes),
            "c27d65244b56078b4627798a525c979a024711f85359addb1cf745d0772c2ee0"
        );
        assert!(tilemap.iter().any(|word| *word != 0x38fc));
    }

    #[test]
    fn installed_layer3_expanded_row_supersedes_vanilla_position_and_selects_clamp() {
        let clamped = lm_level::Layer3ExpandedModeFlags::from_packed(0x0000_1021);
        assert_eq!(
            installed_layer3_editor_position(
                Some(clamped),
                1,
                0,
                lm_profile::SmwUsV1Layer3Behavior::HighTide,
                64,
            ),
            (64, true)
        );

        // Encoded row 20 with ordinary type 2 receives Lunar Magic's twelve-row bias.
        let ordinary = lm_level::Layer3ExpandedModeFlags::from_packed(0x0000_20a1);
        assert_eq!(
            installed_layer3_editor_position(
                Some(ordinary),
                1,
                0,
                lm_profile::SmwUsV1Layer3Behavior::HighTide,
                64,
            ),
            (128, false)
        );

        assert_eq!(
            installed_layer3_editor_position(
                Some(clamped),
                2,
                1,
                lm_profile::SmwUsV1Layer3Behavior::HighTide,
                64,
            ),
            (-128, false)
        );
    }

    #[test]
    fn installed_layer3_priority_split_uses_the_recovered_painter_slots() {
        let definition = |tile| Map16Tile {
            top_left: Subtile(tile),
            top_right: Subtile(tile),
            bottom_left: Subtile(tile),
            bottom_right: Subtile(tile),
            acts_like: 0,
        };
        let mut definitions = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        definitions[0] = definition(0);
        definitions[1] = definition(1);
        let map16 = Map16Set {
            pages: vec![Map16Page::new(definitions.clone()).unwrap()],
        };
        let tiles = [
            IndexedTile::new([2; IndexedTile::PIXEL_COUNT]),
            IndexedTile::new([3; IndexedTile::PIXEL_COUNT]),
        ];
        let mut colors = vec![Bgr555(0); 128];
        colors[1] = Bgr555(0x001f);
        colors[2] = Bgr555(0x03e0);
        colors[3] = Bgr555(0x7c00);
        let palette = Palette { colors };
        let placement = |definition_index| NativeMap16Placement {
            x: 0,
            y: 0,
            word: definition_index,
            definition_index,
            outer_x_flip: false,
            outer_y_flip: false,
            definition_bank: NativeMap16DefinitionBank::Foreground,
            composition: NativeMap16Composition::Opaque,
        };
        let layer2 = [placement(0)];
        let layer1 = [placement(1)];
        let layer_stack: [&[NativeMap16Placement]; 2] = [&layer2, &layer1];
        let layout = NativeLevelMap16Layout {
            width: 1,
            height: 1,
            page_stride: 1,
            base_cell: 0,
            vertical: false,
        };
        let routing = [NativeMap16PaletteRouting::Direct; 2];
        let mut layer3 = test_installed_layer3(0x2000, false);
        layer3.painter_slots = lm_level::lunar_magic_level_layer_slots(0, false, None).unwrap();
        let unsplit = render_level_canvas_with_layer_palette_routing(
            &layer_stack,
            &[],
            layout,
            &map16,
            &definitions,
            &tiles,
            &[],
            &[],
            &palette,
            &routing,
            Some(&layer3),
        )
        .unwrap();
        assert_eq!(unsplit.get(0, 0).unwrap().blue, 255);

        layer3.painter_slots = lm_level::lunar_magic_level_layer_slots(0, true, None).unwrap();
        let split = render_level_canvas_with_layer_palette_routing(
            &layer_stack,
            &[],
            layout,
            &map16,
            &definitions,
            &tiles,
            &[],
            &[],
            &palette,
            &routing,
            Some(&layer3),
        )
        .unwrap();
        assert_eq!(split.get(0, 0).unwrap().red, 255);

        layer3.painter_slots = lm_level::lunar_magic_level_layer_slots(0x1e, false, None).unwrap();
        let layer1_additive = render_level_canvas_with_layer_palette_routing(
            &layer_stack,
            &[],
            layout,
            &map16,
            &definitions,
            &tiles,
            &[],
            &[],
            &palette,
            &routing,
            Some(&layer3),
        )
        .unwrap();
        let pixel = layer1_additive.get(0, 0).unwrap();
        assert_eq!((pixel.red, pixel.green, pixel.blue), (0, 255, 255));
    }

    fn level_for_image_crop(mode: u8, stored_screen: u8, sprite_screen: u8) -> LoadedLevelSlot {
        LoadedLevelSlot {
            number: 0,
            layer1: LevelObjectData {
                header: LegacyLevelHeader::decode(&[stored_screen, mode, 0, 0, 0]).unwrap(),
                objects: ObjectStream {
                    records: vec![
                        ObjectRecord::new(vec![0x01, 0x12, 0x10]).unwrap(),
                        ObjectRecord::new(vec![stored_screen, 0, 1]).unwrap(),
                    ],
                },
            },
            sprites: NativeSpriteStream::parse(
                &[0, 0, sprite_screen, 1, 0xff],
                false,
                &SpriteLengthTable::standard(),
            )
            .unwrap(),
        }
    }

    #[test]
    fn level_image_crop_uses_stored_or_automatic_horizontal_extent() {
        let canvas = lm_render::Canvas::try_new(32 * 256, 27 * 16).unwrap();
        let level = level_for_image_crop(0, 8, 5);
        let stored =
            crop_level_image_canvas(&canvas, &level, lm_level::LevelScreenExtentMode::Stored)
                .unwrap();
        let automatic =
            crop_level_image_canvas(&canvas, &level, lm_level::LevelScreenExtentMode::Auto)
                .unwrap();
        assert_eq!((stored.width(), stored.height()), (9 * 256, 27 * 16));
        assert_eq!((automatic.width(), automatic.height()), (6 * 256, 27 * 16));
    }

    #[test]
    fn level_image_crop_applies_automatic_extent_to_the_vertical_axis() {
        let canvas = lm_render::Canvas::try_new(32 * 16, 28 * 256).unwrap();
        let level = level_for_image_crop(0x0a, 7, 2);
        let automatic =
            crop_level_image_canvas(&canvas, &level, lm_level::LevelScreenExtentMode::Auto)
                .unwrap();
        assert_eq!((automatic.width(), automatic.height()), (32 * 16, 3 * 256));
    }

    #[test]
    fn special_world_view_replaces_only_the_materialized_sp2_slot() {
        let image =
            lm_rom::RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        let project = lm_project::Project::new(image);
        let blank = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        let mut resolved = ResolvedLevelGraphics {
            vram: MaterializedSuperGraphicsVram {
                foreground_background: vec![blank.clone(); 6 * 128],
                sprites: vec![blank.clone(); 4 * 128],
            },
            foreground_background_files: 6,
            sprite_files: 4,
            source: "bypassed",
        };
        apply_special_world_graphics(
            &mut resolved,
            &project,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
            false,
        )
        .unwrap();
        assert!(resolved.vram.sprites.iter().all(|tile| tile == &blank));
        apply_special_world_graphics(
            &mut resolved,
            &project,
            lm_profile::smw_us_v1_vanilla_graphics_layout(),
            true,
        )
        .unwrap();
        let special = project
            .load_decompressed_graphics_file(0x31, lm_profile::smw_us_v1_vanilla_graphics_layout())
            .unwrap();
        let mut special = lm_graphics::decode_planar_tiles(&special, 3).unwrap();
        special.resize_with(128, || blank.clone());

        assert!(
            resolved.vram.sprites[..128]
                .iter()
                .all(|tile| tile == &blank)
        );
        assert_eq!(&resolved.vram.sprites[128..256], special.as_slice());
        assert!(
            resolved.vram.sprites[256..]
                .iter()
                .all(|tile| tile == &blank)
        );
    }

    #[test]
    fn special_world_decoder_accepts_legacy_and_expanded_native_forms() {
        assert_eq!(
            decode_special_world_graphics(&[0; 0x600]).unwrap().len(),
            128
        );
        assert_eq!(
            decode_special_world_graphics(&[0; 0xc00]).unwrap().len(),
            128
        );
        assert_eq!(
            decode_special_world_graphics(&[0; 0x1000]).unwrap().len(),
            128
        );
        assert!(decode_special_world_graphics(&[0; 0x800]).is_err());
    }

    #[test]
    fn full_level_image_publication_routes_formats_and_is_create_new() {
        let base = std::env::temp_dir().join(format!(
            "lm-native-level-image-{}-{}",
            std::process::id(),
            NEXT_IMAGE_PATH.fetch_add(1, Ordering::Relaxed)
        ));
        let png_path = base.with_extension("png");
        let bmp_path = base.with_extension("BMP");
        let canvas = lm_render::Canvas::from_pixels(
            2,
            3,
            vec![
                lm_render::Rgba {
                    red: 0x12,
                    green: 0x34,
                    blue: 0x56,
                    alpha: 0xff,
                };
                6
            ],
        )
        .unwrap();
        publish_level_image(&png_path, &canvas).unwrap();
        let png = std::fs::read(&png_path).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 2);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 3);
        assert!(publish_level_image(&png_path, &canvas).is_err());

        publish_level_image(&bmp_path, &canvas).unwrap();
        let bmp = std::fs::read(&bmp_path).unwrap();
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(i32::from_le_bytes(bmp[18..22].try_into().unwrap()), 2);
        assert_eq!(i32::from_le_bytes(bmp[22..26].try_into().unwrap()), 3);
        assert!(publish_level_image(&base.with_extension("gif"), &canvas).is_err());
        std::fs::remove_file(png_path).unwrap();
        std::fs::remove_file(bmp_path).unwrap();
    }

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
    fn selection_clock_does_not_enable_disabled_asset_animation() {
        let disabled = InstalledAnimationOptions {
            vanilla_tiles: false,
            palette: false,
        };
        assert_eq!(
            installed_preview_phases(disabled, false, 0.12),
            InstalledPreviewPhases {
                refresh: None,
                assets: None,
                selection: None,
            }
        );
        assert_eq!(
            installed_preview_phases(disabled, true, 0.12),
            InstalledPreviewPhases {
                refresh: Some(2),
                assets: None,
                selection: Some(2),
            }
        );

        let tiles_only = InstalledAnimationOptions {
            vanilla_tiles: true,
            palette: false,
        };
        assert_eq!(
            installed_preview_phases(tiles_only, false, 0.12),
            InstalledPreviewPhases {
                refresh: Some(2),
                assets: Some(2),
                selection: None,
            }
        );
        assert_eq!(
            installed_preview_phases(tiles_only, true, 0.12),
            InstalledPreviewPhases {
                refresh: Some(2),
                assets: Some(2),
                selection: Some(2),
            }
        );
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
        let placements = layer2_placements(&bytes, 0, 0, false).unwrap();
        assert_eq!(
            placements[0],
            NativeMap16Placement {
                x: 0,
                y: 0,
                word: 0x0123,
                definition_index: 0x0123,
                outer_x_flip: false,
                outer_y_flip: false,
                definition_bank: NativeMap16DefinitionBank::Background,
                composition: NativeMap16Composition::Opaque,
            }
        );
        assert_eq!(
            placements[31],
            NativeMap16Placement {
                x: 31,
                y: 0,
                word: 0x4567,
                definition_index: 0x0567,
                outer_x_flip: true,
                outer_y_flip: false,
                definition_bank: NativeMap16DefinitionBank::Background,
                composition: NativeMap16Composition::Opaque,
            }
        );
        assert_eq!(
            placements[31 * 32],
            NativeMap16Placement {
                x: 0,
                y: 31,
                word: 0x89ab,
                definition_index: 0x09ab,
                outer_x_flip: false,
                outer_y_flip: true,
                definition_bank: NativeMap16DefinitionBank::Background,
                composition: NativeMap16Composition::Opaque,
            }
        );
    }

    #[test]
    fn layer2_preview_rejects_noncanonical_tilemap_lengths() {
        assert!(layer2_placements(&[0; 0x7ff], 0, 0, false).is_err());
    }

    #[test]
    fn layer2_preview_combines_descriptor_bank_with_low_twelve_tile_bits() {
        let mut bytes = vec![0; lm_level::NATIVE_LAYER2_TILEMAP_LEN];
        bytes[0..2].copy_from_slice(&0xd234_u16.to_le_bytes());
        let placements = layer2_placements(&bytes, 5, 0, false).unwrap();
        assert_eq!(placements[0].word, 0xd234);
        assert_eq!(placements[0].definition_index, 0x5234);
        assert!(layer2_placements(&bytes, 8, 0, false).is_err());
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
            pages: vec![Map16Page::new(definitions.clone()).unwrap()],
        };
        let tiles = [IndexedTile::new([1; IndexedTile::PIXEL_COUNT])];
        let mut colors = vec![Bgr555(0); 128];
        colors[1] = Bgr555(0x001f);
        let palette = Palette { colors };
        let placements = layer2_placements(&tilemap, 0, 0, false).unwrap();
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
            &definitions,
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
            &definitions,
            &tiles,
            &[],
            &[],
            &palette,
            &[NativeMap16PaletteRouting::Direct],
            PreviewViewportState {
                origin_x: 16,
                origin_y: 32,
                zoom_index: 2,
            },
            false,
            None,
            None,
        )
        .unwrap();
        assert_eq!(viewport_image.size, [512, 448]);
        assert_eq!(viewport_image[(0, 0)], egui::Color32::from_rgb(255, 0, 0));
        assert_eq!(viewport_image[(31, 31)], egui::Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn installed_layer2_palette_shift_is_scoped_to_tileset_three_objects() {
        assert_eq!(
            installed_layer2_palette_routing(true, 3),
            NativeMap16PaletteRouting::ShiftLowRowsByFour
        );
        assert_eq!(
            installed_layer2_palette_routing(false, 3),
            NativeMap16PaletteRouting::Direct
        );
        assert_eq!(
            installed_layer2_palette_routing(true, 2),
            NativeMap16PaletteRouting::Direct
        );
        assert_eq!(
            installed_layer2_palette_routing(true, 4),
            NativeMap16PaletteRouting::Direct
        );
    }

    #[test]
    fn installed_preview_map16_grid_tracks_exact_pan_and_zoom() {
        let red = Rgba {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        let source = lm_render::Canvas::from_pixels(512, 512, vec![red; 512 * 512]).unwrap();
        let output = render_level_viewport_canvas(
            &source,
            PreviewViewportState {
                origin_x: 17,
                origin_y: 9,
                zoom_index: 2,
            },
            true,
            None,
            None,
        )
        .unwrap();
        assert_eq!(output.get(29, 0), Some(red));
        assert_ne!(output.get(30, 0), Some(red));
        assert_eq!(output.get(0, 13), Some(red));
        assert_ne!(output.get(0, 14), Some(red));
        assert_ne!(output.get(30, 14), Some(red));

        let fractional = render_level_viewport_canvas(
            &source,
            PreviewViewportState {
                origin_x: 3,
                origin_y: 5,
                zoom_index: 0,
            },
            true,
            None,
            None,
        )
        .unwrap();
        assert_eq!(fractional.get(5, 0), Some(red));
        assert_ne!(fractional.get(6, 0), Some(red));
        assert_eq!(fractional.get(0, 4), Some(red));
        assert_ne!(fractional.get(0, 5), Some(red));

        let without_grid = render_level_viewport_canvas(
            &source,
            PreviewViewportState {
                origin_x: 17,
                origin_y: 9,
                zoom_index: 2,
            },
            false,
            None,
            None,
        )
        .unwrap();
        assert_eq!(without_grid.get(30, 14), Some(red));
    }

    #[test]
    fn installed_preview_selection_maps_world_cell_and_animates_after_sampling() {
        let viewport = PreviewViewportState {
            origin_x: 17,
            origin_y: 9,
            zoom_index: 2,
        };
        let selection =
            preview_map16_selection(viewport, lm_render::Point { x: 30, y: 14 }).unwrap();
        assert_eq!(
            selection,
            PreviewMap16Selection {
                cell_x: 2,
                cell_y: 1
            }
        );

        let red = Rgba {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        let source = lm_render::Canvas::from_pixels(512, 512, vec![red; 512 * 512]).unwrap();
        let phase_zero =
            render_level_viewport_canvas(&source, viewport, false, Some(selection), Some(0))
                .unwrap();
        let phase_four =
            render_level_viewport_canvas(&source, viewport, false, Some(selection), Some(4))
                .unwrap();

        assert_ne!(phase_zero.get(30, 14), Some(red));
        assert_eq!(phase_zero.get(31, 15), Some(red));
        assert_ne!(phase_zero.get(30, 14), phase_four.get(30, 14));
        assert_ne!(phase_zero.get(61, 45), Some(red));
        assert_eq!(phase_zero.get(62, 46), Some(red));
    }

    #[test]
    fn installed_preview_inspection_preserves_layer_and_placement_painter_order() {
        let mut definitions = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        definitions[1] = Map16Tile {
            top_left: Subtile(0x1001),
            top_right: Subtile(0x1002),
            bottom_left: Subtile(0x1003),
            bottom_right: Subtile(0x1004),
            acts_like: 0x0101,
        };
        definitions[2] = Map16Tile {
            top_left: Subtile(0x2001),
            top_right: Subtile(0x2002),
            bottom_left: Subtile(0x2003),
            bottom_right: Subtile(0x2004),
            acts_like: 0x0202,
        };
        definitions[3] = Map16Tile {
            top_left: Subtile(0x3001),
            top_right: Subtile(0x3002),
            bottom_left: Subtile(0x3003),
            bottom_right: Subtile(0x3004),
            acts_like: 0x0303,
        };
        let expected = [definitions[1], definitions[2], definitions[3]];
        let mut background_definitions = vec![Map16Tile::default(); 0x1003];
        background_definitions[0x1001] = expected[0];
        background_definitions[0x1002] = expected[1];
        let map16 = Map16Set {
            pages: vec![Map16Page::new(definitions).unwrap()],
        };
        let layer2 = [
            NativeMap16Placement {
                x: 2,
                y: 1,
                word: 0x4001,
                definition_index: 0x1001,
                outer_x_flip: true,
                outer_y_flip: false,
                definition_bank: NativeMap16DefinitionBank::Background,
                composition: NativeMap16Composition::Average,
            },
            NativeMap16Placement {
                x: 8,
                y: 8,
                word: 0x0007,
                definition_index: 7,
                outer_x_flip: false,
                outer_y_flip: false,
                definition_bank: NativeMap16DefinitionBank::Background,
                composition: NativeMap16Composition::Opaque,
            },
            NativeMap16Placement {
                x: 2,
                y: 1,
                word: 0x0002,
                definition_index: 0x1002,
                outer_x_flip: false,
                outer_y_flip: false,
                definition_bank: NativeMap16DefinitionBank::Background,
                composition: NativeMap16Composition::HalfColor,
            },
        ];
        let layer1 = [
            NativeMap16Placement {
                x: 2,
                y: 1,
                word: 0x8003,
                definition_index: 3,
                outer_x_flip: false,
                outer_y_flip: false,
                definition_bank: NativeMap16DefinitionBank::Foreground,
                composition: NativeMap16Composition::Opaque,
            },
            NativeMap16Placement {
                x: 2,
                y: 1,
                word: 0x3fff,
                definition_index: 0x3fff,
                outer_x_flip: false,
                outer_y_flip: false,
                definition_bank: NativeMap16DefinitionBank::Foreground,
                composition: NativeMap16Composition::Opaque,
            },
        ];
        let selection = PreviewMap16Selection {
            cell_x: 2,
            cell_y: 1,
        };
        assert_eq!(
            inspect_preview_map16_selection(
                selection,
                &layer2,
                &layer1,
                &[],
                &map16,
                &background_definitions,
                NativeMap16PaletteRouting::Direct,
            ),
            PreviewMap16Inspection {
                selection,
                hits: vec![
                    PreviewMap16Hit {
                        layer: PreviewMap16Layer::Layer2,
                        definition_bank: NativeMap16DefinitionBank::Background,
                        palette_routing: NativeMap16PaletteRouting::Direct,
                        composition: NativeMap16Composition::Average,
                        word: 0x4001,
                        definition_index: 0x1001,
                        outer_x_flip: true,
                        outer_y_flip: false,
                        definition: Some(expected[0]),
                        acts_like: None,
                    },
                    PreviewMap16Hit {
                        layer: PreviewMap16Layer::Layer2,
                        definition_bank: NativeMap16DefinitionBank::Background,
                        palette_routing: NativeMap16PaletteRouting::Direct,
                        composition: NativeMap16Composition::HalfColor,
                        word: 0x0002,
                        definition_index: 0x1002,
                        outer_x_flip: false,
                        outer_y_flip: false,
                        definition: Some(expected[1]),
                        acts_like: None,
                    },
                    PreviewMap16Hit {
                        layer: PreviewMap16Layer::Layer1,
                        definition_bank: NativeMap16DefinitionBank::Foreground,
                        palette_routing: NativeMap16PaletteRouting::Direct,
                        composition: NativeMap16Composition::Opaque,
                        word: 0x8003,
                        definition_index: 3,
                        outer_x_flip: false,
                        outer_y_flip: false,
                        definition: Some(expected[2]),
                        acts_like: Some(Err(lm_level::Map16SetError::ActsLikeOutOfRange {
                            tile: 3,
                            target: 0x0303,
                        })),
                    },
                    PreviewMap16Hit {
                        layer: PreviewMap16Layer::Layer1,
                        definition_bank: NativeMap16DefinitionBank::Foreground,
                        palette_routing: NativeMap16PaletteRouting::Direct,
                        composition: NativeMap16Composition::Opaque,
                        word: 0x3fff,
                        definition_index: 0x3fff,
                        outer_x_flip: false,
                        outer_y_flip: false,
                        definition: None,
                        acts_like: None,
                    },
                ],
                sprites: Vec::new(),
            }
        );

        let empty = PreviewMap16Selection {
            cell_x: 9,
            cell_y: 9,
        };
        assert_eq!(
            inspect_preview_map16_selection(
                empty,
                &layer2,
                &layer1,
                &[],
                &map16,
                &background_definitions,
                NativeMap16PaletteRouting::Direct,
            ),
            PreviewMap16Inspection {
                selection: empty,
                hits: Vec::new(),
                sprites: Vec::new(),
            }
        );
    }

    #[test]
    fn installed_preview_inspection_resolves_terminal_and_cyclic_acts_like_chains() {
        let mut definitions = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        definitions[1].acts_like = 2;
        definitions[2].acts_like = 3;
        definitions[3].acts_like = 3;
        definitions[4].acts_like = 5;
        definitions[5].acts_like = 4;
        let map16 = Map16Set {
            pages: vec![Map16Page::new(definitions).unwrap()],
        };
        let layer1 = [
            NativeMap16Placement {
                x: 1,
                y: 1,
                word: 1,
                definition_index: 1,
                outer_x_flip: false,
                outer_y_flip: false,
                definition_bank: NativeMap16DefinitionBank::Foreground,
                composition: NativeMap16Composition::Opaque,
            },
            NativeMap16Placement {
                x: 1,
                y: 1,
                word: 3,
                definition_index: 3,
                outer_x_flip: false,
                outer_y_flip: false,
                definition_bank: NativeMap16DefinitionBank::Foreground,
                composition: NativeMap16Composition::Opaque,
            },
            NativeMap16Placement {
                x: 1,
                y: 1,
                word: 4,
                definition_index: 4,
                outer_x_flip: false,
                outer_y_flip: false,
                definition_bank: NativeMap16DefinitionBank::Foreground,
                composition: NativeMap16Composition::Opaque,
            },
        ];
        let inspection = inspect_preview_map16_selection(
            PreviewMap16Selection {
                cell_x: 1,
                cell_y: 1,
            },
            &[],
            &layer1,
            &[],
            &map16,
            &[],
            NativeMap16PaletteRouting::Direct,
        );
        assert_eq!(
            inspection.hits[0].acts_like,
            Some(Ok(lm_level::ActsLikeResolution {
                chain: vec![1, 2, 3],
                terminal: 3,
            }))
        );
        assert_eq!(
            inspection.hits[1].acts_like,
            Some(Ok(lm_level::ActsLikeResolution {
                chain: vec![3],
                terminal: 3,
            }))
        );
        assert_eq!(
            inspection.hits[2].acts_like,
            Some(Err(lm_level::Map16SetError::ActsLikeCycle {
                cycle: vec![4, 5, 4],
            }))
        );
    }

    #[test]
    fn map16_subtile_inspection_matches_visual_quadrants_and_composed_flips() {
        let definition = Map16Tile {
            top_left: Subtile(0x0155),
            top_right: Subtile(0x56aa),
            bottom_left: Subtile(0xabff),
            bottom_right: Subtile(0xfc00),
            acts_like: 0,
        };
        let source_words = [0x0155, 0x56aa, 0xabff, 0xfc00];
        for (placement_word, expected_sources) in [
            (0x0000, [0, 1, 2, 3]),
            (0x4000, [1, 0, 3, 2]),
            (0x8000, [2, 3, 0, 1]),
            (0xc000, [3, 2, 1, 0]),
        ] {
            let decoded = decode_preview_map16_subtiles(
                definition,
                placement_word & 0x4000 != 0,
                placement_word & 0x8000 != 0,
                NativeMap16PaletteRouting::Direct,
            );
            assert_eq!(
                decoded.map(|subtile| subtile.source_quadrant),
                expected_sources
            );
            for subtile in decoded {
                let source_word = source_words[subtile.source_quadrant];
                assert_eq!(subtile.word, source_word);
                assert_eq!(subtile.tile, source_word & 0x03ff);
                assert_eq!(subtile.encoded_palette_row, ((source_word >> 10) & 7) as u8);
                assert_eq!(subtile.cgram_row, subtile.encoded_palette_row);
                assert_eq!(subtile.high_priority, source_word & 0x2000 != 0);
                assert_eq!(
                    subtile.x_flip,
                    (source_word & 0x4000 != 0) ^ (placement_word & 0x4000 != 0)
                );
                assert_eq!(
                    subtile.y_flip,
                    (source_word & 0x8000 != 0) ^ (placement_word & 0x8000 != 0)
                );
            }
        }
        let shifted = decode_preview_map16_subtiles(
            definition,
            false,
            false,
            NativeMap16PaletteRouting::ShiftLowRowsByFour,
        );
        assert_eq!(
            shifted.map(|subtile| (subtile.encoded_palette_row, subtile.cgram_row)),
            [(0, 4), (5, 5), (2, 6), (7, 7)]
        );
        assert_eq!(
            (0..4).map(preview_map16_quadrant_label).collect::<Vec<_>>(),
            ["top-left", "top-right", "bottom-left", "bottom-right"]
        );
    }

    #[test]
    fn installed_preview_inspection_uses_half_open_sprite_overlap_bounds() {
        let sprite = |token_index, x, y| NativeSpritePreviewPlacement {
            token_index,
            part_index: 0,
            sprite_number: 0x42,
            source: StandardSpritePreviewSource::BuiltIn,
            definition_index: u16::try_from(token_index).unwrap(),
            subtiles: [u16::try_from(token_index).unwrap(); 4],
            x,
            y,
        };
        let sprites = [
            sprite(0, 0, 0),
            sprite(1, 1, 1),
            sprite(2, 31, 31),
            sprite(3, 32, 16),
            sprite(4, 16, 32),
            sprite(5, 16, 16),
        ];
        let inspection = inspect_preview_map16_selection(
            PreviewMap16Selection {
                cell_x: 1,
                cell_y: 1,
            },
            &[],
            &[],
            &sprites,
            &Map16Set::default(),
            &[],
            NativeMap16PaletteRouting::Direct,
        );
        assert_eq!(
            inspection
                .sprites
                .iter()
                .map(|sprite| sprite.token_index)
                .collect::<Vec<_>>(),
            vec![1, 2, 5]
        );
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
    fn installed_preview_zoom_preserves_center_and_drag_uses_exact_scale() {
        let anchor = lm_render::Point { x: 256, y: 224 };
        let mut viewport = PreviewViewportState {
            origin_x: 1_000,
            origin_y: 500,
            zoom_index: 1,
        };
        let anchored_world = viewport
            .viewport()
            .unwrap()
            .screen_to_world(anchor)
            .unwrap();
        viewport.zoom_at(2, anchor, 4096, 2048).unwrap();
        assert_eq!(
            viewport
                .viewport()
                .unwrap()
                .screen_to_world(anchor)
                .unwrap(),
            anchored_world
        );
        assert_eq!((viewport.origin_x, viewport.origin_y), (1_128, 612));

        let drag = PreviewDragState {
            pointer_x: 10.0,
            pointer_y: 10.0,
            origin_x: viewport.origin_x,
            origin_y: viewport.origin_y,
        };
        viewport.pan_from_drag(drag, 74.0, -22.0, 4096, 2048);
        assert_eq!((viewport.origin_x, viewport.origin_y), (1_096, 628));

        viewport.zoom_index = 0;
        viewport.origin_x = 1_000;
        viewport.origin_y = 500;
        let drag = PreviewDragState {
            pointer_x: 4.0,
            pointer_y: 8.0,
            origin_x: viewport.origin_x,
            origin_y: viewport.origin_y,
        };
        viewport.pan_from_drag(drag, 9.0, 5.0, 4096, 2048);
        assert_eq!((viewport.origin_x, viewport.origin_y), (990, 506));

        viewport.pan_from_drag(drag, f32::MAX, f32::MAX, 4096, 2048);
        assert_eq!((viewport.origin_x, viewport.origin_y), (0, 0));
    }

    #[test]
    fn installed_preview_wheel_zoom_is_discrete_and_pointer_anchored() {
        assert_eq!(
            preview_modified_wheel_delta(egui::Modifiers::NONE, 14.0),
            None
        );
        assert_eq!(
            preview_modified_wheel_delta(egui::Modifiers::ALT, 14.0),
            None
        );
        assert_eq!(
            preview_modified_wheel_delta(egui::Modifiers::CTRL, 14.0),
            Some(14.0)
        );
        assert_eq!(
            preview_modified_wheel_delta(egui::Modifiers::COMMAND, -14.0),
            Some(-14.0)
        );
        assert_eq!(
            preview_modified_wheel_delta(egui::Modifiers::MAC_CMD, 14.0),
            Some(14.0)
        );
        assert_eq!(preview_wheel_zoom_index(1, 14.0), Some(2));
        assert_eq!(preview_wheel_zoom_index(1, -14.0), Some(0));
        assert_eq!(preview_wheel_zoom_index(0, -1.0), None);
        assert_eq!(preview_wheel_zoom_index(4, 1.0), None);
        assert_eq!(preview_wheel_zoom_index(1, 0.0), None);
        assert_eq!(preview_wheel_zoom_index(1, f32::NAN), None);
        assert_eq!(preview_wheel_zoom_index(1, f32::INFINITY), None);

        let rect = egui::Rect::from_min_size(
            egui::pos2(100.0, 200.0),
            egui::vec2(
                PreviewViewportState::WIDTH as f32,
                PreviewViewportState::HEIGHT as f32,
            ),
        );
        assert_eq!(
            preview_pointer_anchor(rect, egui::pos2(500.0, 300.0)),
            lm_render::Point { x: 400, y: 100 }
        );
        assert_eq!(
            preview_pointer_anchor(rect, egui::pos2(-100.0, 1_000.0)),
            lm_render::Point { x: 0, y: 447 }
        );

        let anchor = lm_render::Point { x: 400, y: 100 };
        let mut viewport = PreviewViewportState {
            origin_x: 1_000,
            origin_y: 500,
            zoom_index: 1,
        };
        let anchored_world = viewport
            .viewport()
            .unwrap()
            .screen_to_world(anchor)
            .unwrap();
        viewport.zoom_at(2, anchor, 4096, 2048).unwrap();
        assert_eq!(
            viewport
                .viewport()
                .unwrap()
                .screen_to_world(anchor)
                .unwrap(),
            anchored_world
        );
        assert_eq!((viewport.origin_x, viewport.origin_y), (1_200, 550));
    }

    #[test]
    fn object_stream_preview_uses_recovered_vertical_mode_dimensions() {
        let image = lm_rom::RomImage::from_bytes(vec![0; 0x80000]).unwrap();
        let (placements, layout, diagnostics) =
            render_object_placements(&image, &ObjectStream::default(), 3, 0).unwrap();
        assert_eq!(layout.width, 32);
        assert_eq!(layout.height, 13 * 16);
        assert!(layout.vertical);
        assert!(placements.is_empty());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn object_paints_preserve_sparse_duplicate_painter_order() {
        let layout = NativeLevelMap16Layout {
            width: 32,
            height: 27,
            page_stride: 0x1b0,
            base_cell: 0,
            vertical: false,
        };
        let index = lm_render::NativeLevelMap16Cache::cell_index(layout, 3, 4);
        let placements = object_paints_to_placements(
            &[
                StandardObjectPaintedCell {
                    record_index: 0,
                    index,
                    tile: 0x123,
                },
                StandardObjectPaintedCell {
                    record_index: 1,
                    index,
                    tile: 0x4456,
                },
            ],
            layout,
            0,
        )
        .unwrap();
        assert_eq!(
            placements,
            [
                NativeMap16Placement {
                    x: 3,
                    y: 4,
                    word: 0x123,
                    definition_index: 0x123,
                    outer_x_flip: false,
                    outer_y_flip: false,
                    definition_bank: NativeMap16DefinitionBank::Foreground,
                    composition: NativeMap16Composition::Opaque,
                },
                NativeMap16Placement {
                    x: 3,
                    y: 4,
                    word: 0x4456,
                    definition_index: 0x4456,
                    outer_x_flip: false,
                    outer_y_flip: false,
                    definition_bank: NativeMap16DefinitionBank::Foreground,
                    composition: NativeMap16Composition::Opaque,
                },
            ]
        );
    }

    #[test]
    fn underground_map16_paints_select_native_averaging() {
        assert_eq!(
            object_map16_composition(4, 0x0027),
            NativeMap16Composition::Average
        );
        assert_eq!(
            object_map16_composition(4, 0x4027),
            NativeMap16Composition::Opaque
        );
        assert_eq!(
            layer2_map16_composition(4, 0xc02a),
            NativeMap16Composition::Average
        );
        assert_eq!(
            object_map16_composition(0, 0x0028),
            NativeMap16Composition::Opaque
        );
        assert_eq!(
            object_map16_composition(4, 0x002b),
            NativeMap16Composition::Opaque
        );

        let layout = NativeLevelMap16Layout {
            width: 32,
            height: 27,
            page_stride: 0x1b0,
            base_cell: 0,
            vertical: false,
        };
        let index = lm_render::NativeLevelMap16Cache::cell_index(layout, 1, 2);
        let object = object_paints_to_placements(
            &[
                StandardObjectPaintedCell {
                    record_index: 0,
                    index,
                    tile: 0x0027,
                },
                StandardObjectPaintedCell {
                    record_index: 1,
                    index,
                    tile: 0x4027,
                },
            ],
            layout,
            4,
        )
        .unwrap();
        assert_eq!(object[0].composition, NativeMap16Composition::Average);
        assert_eq!(object[1].definition_index, 0x4027);
        assert_eq!(object[1].composition, NativeMap16Composition::Opaque);

        let mut tilemap_bytes = vec![0; lm_level::NATIVE_LAYER2_TILEMAP_LEN];
        let tilemap_index = lm_level::native_layer2_tilemap_index(1, 2).unwrap();
        tilemap_bytes[tilemap_index * 2..tilemap_index * 2 + 2]
            .copy_from_slice(&0x0027_u16.to_le_bytes());
        let tilemap = layer2_placements(&tilemap_bytes, 0, 4, false).unwrap();
        assert_eq!(
            tilemap
                .iter()
                .find(|placement| placement.x == 1 && placement.y == 2)
                .unwrap()
                .composition,
            NativeMap16Composition::Average
        );

        let half_color = layer2_placements(&tilemap_bytes, 0, 4, true).unwrap();
        assert_eq!(
            half_color
                .iter()
                .find(|placement| placement.x == 1 && placement.y == 2)
                .unwrap()
                .composition,
            NativeMap16Composition::HalfColor
        );
    }

    #[test]
    fn recovered_level_modes_select_layer2_background_half_color() {
        assert!(installed_layer2_background_half_color(0x0c));
        assert!(installed_layer2_background_half_color(0x0d));
        assert!(!installed_layer2_background_half_color(0x0b));
        assert!(!installed_layer2_background_half_color(0x0e));
    }

    #[test]
    fn standard_sprite_stream_uses_native_placement_and_preview_dispatch() {
        let sprites = lm_level::NativeSpriteStream {
            header: 0,
            expanded: false,
            tokens: vec![
                lm_level::SpriteToken::Control(0x80),
                lm_level::SpriteToken::Record(lm_level::SpriteRecord {
                    encoded: vec![0x20, 0x10, 0x00],
                }),
            ],
        };
        let (rendered, diagnostics) = render_sprite_placements(&sprites, 0, 0);
        assert!(diagnostics.is_empty());
        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].token_index, 1);
        assert_eq!(rendered[0].part_index, 0);
        assert_eq!(rendered[0].sprite_number, 0);
        assert_eq!(rendered[0].source, StandardSpritePreviewSource::BuiltIn);
        assert_eq!(rendered[0].x, 16);
        assert_eq!(rendered[0].y, 33);
        let expected = lm_render::render_lunar_magic_standard_sprite(0, false).unwrap();
        assert_eq!(rendered[0].definition_index, expected[0].definition_index);
        assert_eq!(rendered[0].subtiles, expected[0].subtiles);
    }

    #[test]
    fn sprite_subtile_inspection_decodes_the_native_raster_fields() {
        assert_eq!(
            decode_preview_sprite_subtile(0xf755),
            PreviewSpriteSubtile {
                tile: 0x155,
                page: PreviewSpriteGraphicsPage::Animated,
                cgram_row: 13,
                high_priority: true,
                x_flip: true,
                y_flip: true,
            }
        );
        assert_eq!(
            decode_preview_sprite_subtile(0x01ab),
            PreviewSpriteSubtile {
                tile: 0x1ab,
                page: PreviewSpriteGraphicsPage::Ordinary,
                cgram_row: 8,
                high_priority: false,
                x_flip: false,
                y_flip: false,
            }
        );
        assert_eq!(
            (0..4)
                .map(preview_sprite_quadrant_label)
                .collect::<Vec<_>>(),
            ["top-left", "bottom-left", "top-right", "bottom-right"]
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
