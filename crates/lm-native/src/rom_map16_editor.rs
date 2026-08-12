use crate::{
    document_loader::DocumentLoader, level_editor_forms, map16_subtile_form, native_clipboard,
};
use eframe::egui;
use lm_app::{
    AppState, Command, ExtendedUiTextKey, LocalizationCatalog, Map16Controller,
    Map16ControllerEdit, RevisionProfile, SmwMap16Controller,
};
use lm_level::{Map16Address, Map16Page};

mod bitmap_import;
mod commit;
mod complete_file;
mod legacy_page;
mod lifecycle;
mod selected_file;
mod sidecar_export;
mod snes_tileset_import;
#[cfg(test)]
mod tests;

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Map16PageShortcut {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Map16GridShortcut {
    Toggle,
    ToggleColor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Map16ZoomShortcut {
    Reset,
    Increase,
    Decrease,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Map16F1Shortcut {
    TogglePageNumbers,
    ToggleProtectedPages,
}

struct Workspace {
    controller: Controller,
    profile: Option<RevisionProfile>,
    snapshot: lm_app::ControllerSnapshot,
    image: lm_rom::RomImage,
    internal_header: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Map16SidecarKind {
    M16,
    S16,
}

struct PendingSidecarExport {
    kind: Map16SidecarKind,
    path: std::path::PathBuf,
    bytes: Vec<u8>,
    revision: u64,
}

#[derive(Clone)]
enum Controller {
    Profile(Map16Controller),
    Smw(SmwMap16Controller),
}

impl Controller {
    fn revision(&self) -> u64 {
        match self {
            Self::Profile(controller) => controller.revision(),
            Self::Smw(controller) => controller.revision(),
        }
    }

    fn set(&self) -> &lm_level::Map16Set {
        match self {
            Self::Profile(controller) => controller.set(),
            Self::Smw(controller) => controller.set(),
        }
    }

    fn is_modified(&self) -> bool {
        match self {
            Self::Profile(controller) => controller.is_modified(),
            Self::Smw(controller) => controller.is_modified(),
        }
    }

    fn apply_edits(&mut self, edits: &[Map16ControllerEdit]) -> Result<(), String> {
        match self {
            Self::Profile(controller) => controller.apply_edits(edits).map_err(|e| e.to_string()),
            Self::Smw(controller) => controller.apply_edits(edits).map_err(|e| e.to_string()),
        }
    }

    fn replace_set(&mut self, set: &lm_level::Map16Set) -> Result<(), String> {
        if self.set().pages.len() != set.pages.len() {
            return Err("Map16 history page count changed unexpectedly".into());
        }
        let mut replacements = Vec::with_capacity(set.pages.len() * Map16Page::TILE_COUNT);
        for (page, value) in set.pages.iter().enumerate() {
            if value.tiles.len() != Map16Page::TILE_COUNT {
                return Err(format!(
                    "Map16 history page {page:02X} has {} tiles",
                    value.tiles.len()
                ));
            }
            replacements.extend(
                value
                    .tiles
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(tile, value)| (Map16Address { page, tile }, value)),
            );
        }
        self.apply_edits(&[Map16ControllerEdit::ReplaceTiles {
            replacements,
            resolution_limit: set.pages.len() * Map16Page::TILE_COUNT,
        }])
    }

    const fn supports_reclamation(&self) -> bool {
        matches!(self, Self::Profile(_))
    }

    const fn supports_complete_lm_file(&self) -> bool {
        matches!(self, Self::Smw(_))
    }

    fn supports_acts_like(&self, page: usize) -> bool {
        match self {
            Self::Profile(_) => true,
            Self::Smw(_) => page < lm_app::SMW_COMPLETE_MAP16_FOREGROUND_PAGES,
        }
    }
}

#[derive(Default)]
pub(crate) struct RomMap16Editor {
    workspace: Option<Workspace>,
    page: usize,
    tile: usize,
    selection_generation: u64,
    rectangle_drag_anchor: Option<usize>,
    quadrant: usize,
    subtile: map16_subtile_form::SubtileForm,
    acts_like: String,
    loaded: Option<(u64, usize, usize, usize)>,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    clipboard_paste_target: Option<(u64, u64, Map16Address)>,
    rectangle_clipboard_paste_target: Option<(u64, u64, usize)>,
    staged_revision: u64,
    undo_history: Vec<lm_level::Map16Set>,
    redo_history: Vec<lm_level::Map16Set>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
    page_texture: Option<egui::TextureHandle>,
    page_texture_key: Option<(usize, u64, u16, u8, u8)>,
    show_grid: bool,
    dark_grid: bool,
    page_zoom_percent: u16,
    show_page_number: bool,
    protected_pages_unlocked: bool,
    pending_protected_page_toggle: bool,
    preview_level: String,
    preview_tileset: u8,
    preview_palette: u8,
    bitmap_loader: DocumentLoader,
    bitmap_clipboard_loader: bitmap_import::BitmapClipboardLoader,
    pending_bitmap_import: Option<bitmap_import::PendingBitmapImport>,
    /// Last accepted process-local bitmap conversion choices, matching Lunar Magic's globals.
    bitmap_import_options: Option<lm_app::Map16BitmapImportOptions>,
    complete_loader: DocumentLoader,
    complete_persistence: crate::persistence_worker::PersistenceWorker,
    complete_template: Option<lm_level::Lm16Map16File>,
    pending_complete_revision: Option<u64>,
    selected_loader: DocumentLoader,
    selected_persistence: crate::persistence_worker::PersistenceWorker,
    pending_selected_import: Option<selected_file::PendingSelectedImport>,
    selected_width: String,
    selected_height: String,
    selected_use_file_origin: bool,
    legacy_page_loader: DocumentLoader,
    legacy_page_persistence: crate::persistence_worker::PersistenceWorker,
    pending_legacy_import: Option<legacy_page::PendingLegacyImport>,
    associated_sidecar_loader: DocumentLoader,
    associated_sidecar_persistence: crate::persistence_worker::PersistenceWorker,
    associated_sidecar_paths: Option<(std::path::PathBuf, std::path::PathBuf)>,
    associated_m16: Option<lm_level::M16Sidecar>,
    associated_s16: Option<lm_level::S16Sidecar>,
    pending_sidecar_export: Option<PendingSidecarExport>,
    sidecar_export_in_flight: Option<(Map16SidecarKind, Vec<u8>)>,
    bitmap_session: Option<lm_app::NativeMap16BitmapImportSession>,
    bitmap_extra_slot_4: String,
    bitmap_extra_slot_5: String,
    bitmap_original_texture: Option<egui::TextureHandle>,
    bitmap_converted_texture: Option<egui::TextureHandle>,
    bitmap_preview_zoom: u8,
    bitmap_preview_scroll: egui::Vec2,
    snes_tileset_loader: DocumentLoader,
    pending_snes_tileset: Option<snes_tileset_import::PendingSnesTileset>,
    snes_tileset_preview: Option<snes_tileset_import::SnesTilesetPreview>,
    /// Lunar Magic keeps these dialog globals for the lifetime of the process.
    snes_tileset_options_initialized: bool,
    snes_tileset_include_palette: bool,
    snes_tileset_palette_row: u8,
    snes_tileset_deduplicate: bool,
    snes_tileset_graphics_offset: u16,
    snes_tileset_map_offset: u16,
    snes_tileset_color_filter: bool,
    snes_tileset_color_filter_index: u8,
    snes_tileset_color_maps: [[u8; 16]; 16],
}

impl RomMap16Editor {
    pub(crate) fn stage_recovery_on_project(
        &self,
        app: &AppState,
        staged: &mut lm_project::Project,
    ) -> Result<(), String> {
        let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
        if !workspace.controller.is_modified() {
            return Err("Map16 workspace has no staged recovery edit".into());
        }
        if workspace.controller.revision() != app.project_revision() {
            return Err("Map16 recovery controller was prepared from a stale revision".into());
        }
        match &workspace.controller {
            Controller::Profile(controller) => {
                let mut options = self.profile_save_options(workspace)?;
                let search = crate::rom_allocation::parse_search_range(
                    &self.search_start,
                    &self.search_end,
                )?;
                let profile = workspace
                    .profile
                    .as_ref()
                    .ok_or("revision profile is unavailable")?;
                let allocation = profile
                    .allocation_policy_for_rom(search, &staged.rom, workspace.internal_header)
                    .map_err(|error| error.to_string())?;
                options.graphics_allocation = allocation.clone();
                options.acts_like_allocation = allocation;
                controller
                    .save_to_project(staged, &options)
                    .map_err(|error| error.to_string())
            }
            Controller::Smw(controller) => {
                let mut options = self.smw_save_options(workspace)?;
                options.allocation.search = crate::rom_allocation::parse_search_range(
                    &self.search_start,
                    &self.search_end,
                )?;
                controller
                    .save_to_project(staged, &options)
                    .map_err(|error| error.to_string())
            }
        }
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        workspace.controller.is_modified().then(|| {
            app.project_revision().wrapping_mul(0xa076_1d64_78bd_642f)
                ^ workspace.controller.revision().rotate_left(23)
                ^ 0x4d41_5031_3600_0000
        })
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
        if !workspace.controller.is_modified() {
            return Ok(app.recovery_snapshot());
        }
        let command = self.prepare_commit()?;
        let Command::CommitRomMutation {
            expected_revision,
            mutation,
            ..
        } = command
        else {
            return Err("Map16 recovery expected one prepared ROM mutation".into());
        };
        if expected_revision != app.project_revision() {
            return Err("Map16 recovery mutation was prepared from a stale revision".into());
        }
        app.recovery_snapshot_with_mutation(&mutation, None)
            .map_err(|error| error.to_string())
    }

    pub(crate) const fn selection_generation(&self) -> u64 {
        self.selection_generation
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
        active_sidecar: Option<&lm_app::NativeMap16SidecarDocument>,
        catalog: Option<&LocalizationCatalog>,
    ) -> (bool, Option<Command>) {
        let mut command = self.poll_bitmap_loader(context, catalog);
        self.poll_complete_file_io(context);
        self.poll_selected_file_io(context);
        self.poll_legacy_page_io(context);
        self.poll_associated_sidecar_io(context);
        self.poll_snes_tileset_io(context);
        let manifest_command = match self.manifest_loader.show(context, project_revision) {
            Some(Ok(manifest)) => match self.prepare_commit_owned(&manifest) {
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
        if manifest_command.is_some() {
            command = manifest_command;
        }
        if self.workspace.is_some() {
            self.clamp();
            self.load();
            egui::Window::new(text(catalog, ExtendedUiTextKey::RomMap16EditorTitle))
                .default_size([560.0, 650.0])
                .show(context, |ui| {
                    if let Some(ui_command) =
                        self.contents(ui, project_revision, active_sidecar, catalog)
                    {
                        command = Some(ui_command);
                    }
                });
        }
        if let Some(import_command) = self.bitmap_import_window(context, project_revision, catalog)
        {
            command = Some(import_command);
        }
        if let Some(snes_command) =
            self.snes_tileset_preview_window(context, project_revision, catalog)
        {
            command = Some(snes_command);
        }
        self.protected_page_confirmation(context, catalog);
        self.sidecar_export_confirmation(context, catalog);
        let approved = self.close_confirmation(context, catalog);
        self.show_error(context, catalog);
        (approved, command)
    }
    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        project_revision: u64,
        active_sidecar: Option<&lm_app::NativeMap16SidecarDocument>,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let commit_shortcut = take_map16_commit_shortcut(ui);
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let (stale, pages) = {
            let workspace = self.workspace.as_ref()?;
            (
                workspace.controller.revision() != project_revision,
                workspace.controller.set().pages.len(),
            )
        };
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                text(catalog, ExtendedUiTextKey::RomMap16StaleNotice),
            );
        }
        let file_busy = self.complete_loader.is_running()
            || self.complete_persistence.is_running()
            || self.selected_loader.is_running()
            || self.selected_persistence.is_running()
            || self.legacy_page_loader.is_running()
            || self.legacy_page_persistence.is_running()
            || self.associated_sidecar_loader.is_running()
            || self.associated_sidecar_persistence.is_running()
            || self.pending_sidecar_export.is_some()
            || self.bitmap_loader.is_running()
            || self.bitmap_clipboard_loader.is_running()
            || self.bitmap_session.is_some()
            || self.snes_tileset_loader.is_running()
            || self.snes_tileset_preview.is_some();
        let edit_blocked = stale || file_busy;
        if let Some(shortcut) = take_map16_grid_shortcut(ui) {
            apply_map16_grid_shortcut(&mut self.show_grid, &mut self.dark_grid, shortcut);
        }
        if let Some(shortcut) = take_map16_zoom_shortcut(ui) {
            self.page_zoom_percent = map16_zoom_after_shortcut(self.page_zoom_percent, shortcut);
        }
        if let Some(shortcut) = take_map16_f1_shortcut(ui) {
            match shortcut {
                Map16F1Shortcut::TogglePageNumbers => {
                    self.show_page_number = !self.show_page_number;
                }
                Map16F1Shortcut::ToggleProtectedPages => {
                    self.pending_protected_page_toggle = true;
                }
            }
        }
        if let Some(shortcut) = take_map16_page_shortcut(ui) {
            let next_page = map16_page_after_shortcut(self.page, pages, shortcut);
            if next_page != self.page {
                self.page = next_page;
                self.invalidate();
                self.load();
            }
        }
        self.history_controls(ui, edit_blocked, catalog);
        self.selection_and_clipboard(ui, edit_blocked, pages, pasted.as_deref(), catalog);
        self.visual_page(ui, catalog);
        self.tile_fields(ui, edit_blocked, pages, catalog);
        self.complete_file_controls(
            ui,
            stale
                || self.selected_loader.is_running()
                || self.selected_persistence.is_running()
                || self.legacy_page_loader.is_running()
                || self.legacy_page_persistence.is_running()
                || self.associated_sidecar_loader.is_running()
                || self.associated_sidecar_persistence.is_running()
                || self.pending_sidecar_export.is_some()
                || self.bitmap_loader.is_running()
                || self.bitmap_clipboard_loader.is_running()
                || self.bitmap_session.is_some()
                || self.snes_tileset_loader.is_running()
                || self.snes_tileset_preview.is_some(),
            project_revision,
            catalog,
        );
        self.selected_file_controls(
            ui,
            edit_blocked,
            project_revision,
            pasted.as_deref(),
            catalog,
        );
        self.legacy_page_controls(
            ui,
            stale
                || self.selected_loader.is_running()
                || self.selected_persistence.is_running()
                || self.associated_sidecar_loader.is_running()
                || self.associated_sidecar_persistence.is_running()
                || self.pending_sidecar_export.is_some()
                || self.bitmap_loader.is_running()
                || self.bitmap_clipboard_loader.is_running()
                || self.bitmap_session.is_some()
                || self.snes_tileset_loader.is_running()
                || self.snes_tileset_preview.is_some(),
            project_revision,
            catalog,
        );
        self.bitmap_import_controls(ui, edit_blocked, project_revision, catalog);
        self.snes_tileset_controls(ui, edit_blocked, project_revision, catalog);
        self.sidecar_export_controls(ui, edit_blocked, project_revision, active_sidecar, catalog);
        self.commit_controls(ui, edit_blocked, project_revision, commit_shortcut, catalog)
    }
    fn visual_page(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let changed = ui
            .horizontal_wrapped(|ui| {
                ui.label(text(catalog, ExtendedUiTextKey::RomMap16PreviewLevel));
                let level = ui.text_edit_singleline(&mut self.preview_level).changed();
                let tileset = ui
                    .add(
                        egui::Slider::new(&mut self.preview_tileset, 0..=15)
                            .text(text(catalog, ExtendedUiTextKey::RomMap16ObjectSet)),
                    )
                    .changed();
                let palette = ui
                    .add(
                        egui::Slider::new(&mut self.preview_palette, 0..=7)
                            .text(text(catalog, ExtendedUiTextKey::RomMap16FgPalette)),
                    )
                    .changed();
                ui.checkbox(
                    &mut self.show_grid,
                    text(catalog, ExtendedUiTextKey::RomMap16Grid),
                )
                .on_hover_text(text(catalog, ExtendedUiTextKey::RomMap16GridNotice));
                if ui
                    .button(text(catalog, ExtendedUiTextKey::RomMap16GridColor))
                    .clicked()
                {
                    self.dark_grid = !self.dark_grid;
                }
                if ui
                    .button(text(catalog, ExtendedUiTextKey::RomMap16ZoomOut))
                    .on_hover_text("Ctrl+Numpad −")
                    .clicked()
                {
                    self.page_zoom_percent = map16_zoom_after_shortcut(
                        self.page_zoom_percent,
                        Map16ZoomShortcut::Decrease,
                    );
                }
                if ui
                    .button(text(catalog, ExtendedUiTextKey::RomMap16ZoomReset))
                    .on_hover_text("Ctrl+Numpad 0")
                    .clicked()
                {
                    self.page_zoom_percent =
                        map16_zoom_after_shortcut(self.page_zoom_percent, Map16ZoomShortcut::Reset);
                }
                if ui
                    .button(text(catalog, ExtendedUiTextKey::RomMap16ZoomIn))
                    .on_hover_text("Ctrl+Numpad +")
                    .clicked()
                {
                    self.page_zoom_percent = map16_zoom_after_shortcut(
                        self.page_zoom_percent,
                        Map16ZoomShortcut::Increase,
                    );
                }
                ui.add(
                    egui::Slider::new(&mut self.page_zoom_percent, 100..=5000)
                        .step_by(100.0)
                        .suffix("%"),
                );
                ui.checkbox(
                    &mut self.show_page_number,
                    text(catalog, ExtendedUiTextKey::RomMap16PageNumber),
                )
                .on_hover_text(text(catalog, ExtendedUiTextKey::RomMap16PageNumberNotice));
                if ui
                    .button(if self.protected_pages_unlocked {
                        text(catalog, ExtendedUiTextKey::RomMap16LockPages)
                    } else {
                        text(catalog, ExtendedUiTextKey::RomMap16UnlockPages)
                    })
                    .on_hover_text("Ctrl+F1")
                    .clicked()
                {
                    self.pending_protected_page_toggle = true;
                }
                level || tileset || palette
            })
            .inner;
        if changed {
            self.page_texture = None;
            self.page_texture_key = None;
        }
        let Ok(level) = u16::from_str_radix(self.preview_level.trim(), 16) else {
            ui.colored_label(
                egui::Color32::RED,
                text(catalog, ExtendedUiTextKey::RomMap16PreviewHexError),
            );
            return;
        };
        if level > 0x01ff {
            ui.colored_label(
                egui::Color32::RED,
                text(catalog, ExtendedUiTextKey::RomMap16PreviewRangeError),
            );
            return;
        }
        let mut header = lm_level::LegacyLevelHeader::default();
        if header.set_object_tileset(self.preview_tileset).is_err()
            || header.set_foreground_palette(self.preview_palette).is_err()
        {
            return;
        }
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        let key = (
            self.page,
            workspace.controller.revision(),
            level,
            self.preview_tileset,
            self.preview_palette,
        );
        if self.page_texture_key != Some(key) {
            self.page_texture = None;
            if let Some(page) = workspace.controller.set().pages.get(self.page) {
                match crate::vanilla_map16_preview::render_rom_map16_page(
                    workspace.image.as_file_bytes().to_vec(),
                    level,
                    header,
                    page,
                ) {
                    Ok(image) => {
                        self.page_texture = Some(ui.ctx().load_texture(
                            format!(
                                "rom-map16-page-{}-{}-{}-{}-{}",
                                key.0, key.1, key.2, key.3, key.4
                            ),
                            image,
                            egui::TextureOptions::NEAREST,
                        ));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            self.page_texture_key = Some(key);
        }
        let Some(texture) = self.page_texture.clone() else {
            return;
        };
        let zoom = f32::from(self.page_zoom_percent.clamp(100, 5000)) / 100.0;
        let image_size = egui::Vec2::splat(256.0 * zoom);
        let show_grid = self.show_grid;
        let dark_grid = self.dark_grid;
        let selected_tile = self.tile;
        let selected_rectangle =
            selected_file::parse_dimensions(&self.selected_width, &self.selected_height)
                .ok()
                .and_then(|(width, height)| page_rectangle(selected_tile, width, height));
        let show_page_number = self.show_page_number;
        let page = self.page;
        let response = egui::ScrollArea::both()
            .max_height(420.0)
            .show(ui, |ui| {
                let response = ui.add(
                    egui::Image::new(&texture)
                        .fit_to_exact_size(image_size)
                        .sense(egui::Sense::click_and_drag()),
                );
                let cell_size = response.rect.width() / 16.0;
                if show_grid {
                    let color = if dark_grid {
                        egui::Color32::BLACK
                    } else {
                        egui::Color32::WHITE
                    };
                    for cell in 1..16 {
                        let offset = cell as f32 * cell_size;
                        ui.painter().line_segment(
                            [
                                response.rect.left_top() + egui::vec2(offset, 0.0),
                                response.rect.left_bottom() + egui::vec2(offset, 0.0),
                            ],
                            egui::Stroke::new(1.0_f32, color),
                        );
                        ui.painter().line_segment(
                            [
                                response.rect.left_top() + egui::vec2(0.0, offset),
                                response.rect.right_top() + egui::vec2(0.0, offset),
                            ],
                            egui::Stroke::new(1.0_f32, color),
                        );
                    }
                }
                if show_page_number {
                    ui.painter().text(
                        response.rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("Page 0x{page:X}"),
                        egui::FontId::proportional((24.0 * zoom).clamp(24.0, 96.0)),
                        egui::Color32::WHITE,
                    );
                }
                let (origin, width, height) = selected_rectangle.unwrap_or((selected_tile, 1, 1));
                let column = f32::from(u8::try_from(origin % 16).unwrap_or(0));
                let row = f32::from(u8::try_from(origin / 16).unwrap_or(0));
                let cell = egui::Rect::from_min_size(
                    response.rect.min + egui::vec2(column * cell_size, row * cell_size),
                    egui::vec2(width as f32 * cell_size, height as f32 * cell_size),
                );
                for (start, end, color) in map16_selection_marquee(cell, zoom) {
                    ui.painter()
                        .line_segment([start, end], egui::Stroke::new(zoom.max(1.0), color));
                }
                response
            })
            .inner;
        let pointer_tile = response.interact_pointer_pos().and_then(|position| {
            crate::map16_editor_render::selected_tile(response.rect, position)
        });
        if response.drag_started() {
            self.rectangle_drag_anchor = pointer_tile;
        }
        if response.dragged()
            && let (Some(anchor), Some(current)) = (self.rectangle_drag_anchor, pointer_tile)
        {
            let (origin, width, height) = map16_drag_rectangle(anchor, current);
            if self.tile != origin {
                self.tile = origin;
                self.invalidate();
                self.load();
            }
            self.selected_width = format!("{width:X}");
            self.selected_height = format!("{height:X}");
            self.selection_generation = self.selection_generation.wrapping_add(1);
        } else if response.clicked()
            && let Some(tile) = pointer_tile
        {
            self.tile = tile;
            self.selected_width = "1".into();
            self.selected_height = "1".into();
            self.selection_generation = self.selection_generation.wrapping_add(1);
            self.invalidate();
            self.load();
        }
        if response.drag_stopped() {
            self.rectangle_drag_anchor = None;
        }
        ui.small(text(catalog, ExtendedUiTextKey::RomMap16SelectionNotice));
    }

    fn selection_and_clipboard(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        pages: usize,
        pasted: Option<&str>,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let old = (self.page, self.tile, self.quadrant);
        let paste_shortcut = take_map16_paste_shortcut(ui);
        let editable = map16_page_is_editable(self.page, self.protected_pages_unlocked);
        ui.add(
            egui::Slider::new(&mut self.page, 0..=pages.saturating_sub(1))
                .text(text(catalog, ExtendedUiTextKey::RomMap16Page)),
        );
        ui.add(
            egui::Slider::new(&mut self.tile, 0..=Map16Page::TILE_COUNT - 1)
                .text(text(catalog, ExtendedUiTextKey::RomMap16Tile)),
        );
        egui::ComboBox::from_label(text(catalog, ExtendedUiTextKey::RomMap16Quadrant))
            .selected_text(map16_subtile_form::quadrant_name(self.quadrant))
            .show_ui(ui, |ui| {
                for index in 0..4 {
                    ui.selectable_value(
                        &mut self.quadrant,
                        index,
                        map16_subtile_form::quadrant_name(index),
                    );
                }
            });
        if old != (self.page, self.tile, self.quadrant) {
            self.loaded = None;
            self.load();
        }
        ui.heading(
            text(catalog, ExtendedUiTextKey::RomMap16AddressFormat)
                .replace("{page}", &format!("{:02X}", self.page))
                .replace("{tile}", &format!("{:02X}", self.tile)),
        );
        let mut native_paste = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, ExtendedUiTextKey::RomMap16CopyTile))
                .clicked()
                && let Some(tile) = self.current_tile()
            {
                if let Err(error) = native_clipboard::copy_map16_tile_to_system(ui.ctx(), tile) {
                    self.error = Some(error);
                }
            }
            let paste_clicked = ui
                .add_enabled(
                    !stale && editable,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::RomMap16PasteTile)),
                )
                .clicked();
            if !stale && editable && (paste_clicked || paste_shortcut) {
                self.rectangle_clipboard_paste_target = None;
                let revision = self
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.controller.revision());
                self.clipboard_paste_target =
                    revision.map(|revision| (revision, self.staged_revision, self.address()));
                match native_clipboard::request_map16_tile_paste(ui.ctx()) {
                    Ok(Some(tile)) => {
                        let target = self
                            .clipboard_paste_target
                            .take()
                            .expect("native paste target was just recorded");
                        native_paste = Some((target, tile));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.clipboard_paste_target = None;
                        self.error = Some(error);
                    }
                }
            }
        });
        if let Some(((revision, staged_revision, address), tile)) = native_paste {
            match native_clipboard::encode_map16_tile(tile) {
                Ok(text) => self.paste_tile_at(&text, revision, staged_revision, address, pages),
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(text) = pasted
            && let Some((revision, staged_revision, address)) = self.clipboard_paste_target.take()
        {
            if stale {
                self.error =
                    Some("the ROM or Map16 editor changed while waiting for clipboard data".into());
            } else {
                self.paste_tile_at(text, revision, staged_revision, address, pages);
            }
        }
    }

    fn paste_tile_at(
        &mut self,
        text: &str,
        revision: u64,
        staged_revision: u64,
        address: Map16Address,
        pages: usize,
    ) {
        let result = native_clipboard::decode_map16_tile(text).and_then(|tile| {
            let workspace = self.workspace.as_ref().ok_or("Map16 workspace is closed")?;
            if !map16_page_is_editable(address.page, self.protected_pages_unlocked) {
                return Err("built-in Map16 pages 00–01 are protected".into());
            }
            if workspace.controller.revision() != revision
                || self.staged_revision != staged_revision
            {
                return Err("the ROM Map16 state changed while waiting for clipboard data".into());
            }
            self.apply_staged_edits(&[Map16ControllerEdit::ReplaceTiles {
                replacements: vec![(address, tile)],
                resolution_limit: pages * Map16Page::TILE_COUNT,
            }])
        });
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    fn history_controls(
        &mut self,
        ui: &mut egui::Ui,
        blocked: bool,
        catalog: Option<&LocalizationCatalog>,
    ) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !blocked && !self.undo_history.is_empty(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::RomMap16Undo)),
                )
                .clicked()
                && let Err(error) = self.navigate_history(true)
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    !blocked && !self.redo_history.is_empty(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::RomMap16Redo)),
                )
                .clicked()
                && let Err(error) = self.navigate_history(false)
            {
                self.error = Some(error);
            }
        });
    }

    fn navigate_history(&mut self, undo: bool) -> Result<(), String> {
        let target = if undo {
            self.undo_history.pop()
        } else {
            self.redo_history.pop()
        };
        let Some(target) = target else {
            return Ok(());
        };
        let next_revision = self
            .staged_revision
            .checked_add(1)
            .ok_or("Map16 staged revision exhausted")?;
        let workspace = self.workspace.as_mut().ok_or("Map16 workspace is closed")?;
        let current = workspace.controller.set().clone();
        if let Err(error) = workspace.controller.replace_set(&target) {
            if undo {
                self.undo_history.push(target);
            } else {
                self.redo_history.push(target);
            }
            return Err(error);
        }
        if undo {
            push_history(&mut self.redo_history, current);
        } else {
            push_history(&mut self.undo_history, current);
        }
        self.staged_revision = next_revision;
        self.clipboard_paste_target = None;
        self.rectangle_clipboard_paste_target = None;
        self.page_texture = None;
        self.page_texture_key = None;
        self.invalidate();
        Ok(())
    }

    fn tile_fields(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        pages: usize,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let protected = !map16_page_is_editable(self.page, self.protected_pages_unlocked);
        let supports_acts_like = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.controller.supports_acts_like(self.page));
        ui.horizontal(|ui| {
            ui.label(text(catalog, ExtendedUiTextKey::RomMap16Subtile));
            ui.text_edit_singleline(&mut self.subtile.tile);
        });
        ui.add(
            egui::Slider::new(&mut self.subtile.palette, 0..=7)
                .text(text(catalog, ExtendedUiTextKey::RomMap16Palette)),
        );
        ui.checkbox(
            &mut self.subtile.priority,
            text(catalog, ExtendedUiTextKey::RomMap16Priority),
        );
        ui.checkbox(
            &mut self.subtile.x_flip,
            text(catalog, ExtendedUiTextKey::RomMap16XFlip),
        );
        ui.checkbox(
            &mut self.subtile.y_flip,
            text(catalog, ExtendedUiTextKey::RomMap16YFlip),
        );
        let mut edit = None;
        if ui
            .add_enabled(
                !stale && !protected,
                egui::Button::new(text(catalog, ExtendedUiTextKey::RomMap16ApplySubtile)),
            )
            .clicked()
        {
            edit = Some(
                self.subtile
                    .parse()
                    .map(|subtile| Map16ControllerEdit::SetSubtile {
                        address: self.address(),
                        quadrant: map16_subtile_form::quadrant(self.quadrant),
                        subtile,
                        resolution_limit: pages * Map16Page::TILE_COUNT,
                    }),
            );
        }
        ui.add_enabled_ui(supports_acts_like, |ui| {
            ui.horizontal(|ui| {
                ui.label(text(catalog, ExtendedUiTextKey::RomMap16ActsLike));
                ui.text_edit_singleline(&mut self.acts_like);
            });
        });
        if ui
            .add_enabled(
                !stale && !protected && supports_acts_like,
                egui::Button::new(text(catalog, ExtendedUiTextKey::RomMap16ApplyActsLike)),
            )
            .clicked()
        {
            edit = Some(
                level_editor_forms::parse_hex_u16(&self.acts_like, "Acts Like").map(|acts_like| {
                    Map16ControllerEdit::SetActsLike {
                        address: self.address(),
                        acts_like,
                        resolution_limit: pages * Map16Page::TILE_COUNT,
                    }
                }),
            );
        }
        if let Some(edit) = edit {
            match edit {
                Ok(edit) => self.apply(edit),
                Err(error) => self.error = Some(error),
            }
        }
        if !supports_acts_like {
            ui.small(text(catalog, ExtendedUiTextKey::RomMap16NoActsLikeNotice));
        }
        if protected {
            ui.colored_label(
                egui::Color32::YELLOW,
                text(catalog, ExtendedUiTextKey::RomMap16ProtectedNotice),
            );
        }
    }

    fn protected_page_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) {
        if !self.pending_protected_page_toggle {
            return;
        }
        let unlocking = !self.protected_pages_unlocked;
        egui::Window::new(if unlocking {
            text(catalog, ExtendedUiTextKey::RomMap16UnlockTitle)
        } else {
            text(catalog, ExtendedUiTextKey::RomMap16LockTitle)
        })
        .collapsible(false)
        .resizable(false)
        .show(context, |ui| {
            if unlocking {
                ui.label(text(catalog, ExtendedUiTextKey::RomMap16UnlockWarning));
            } else {
                ui.label(text(catalog, ExtendedUiTextKey::RomMap16LockQuestion));
            }
            ui.horizontal(|ui| {
                if ui
                    .button(crate::frontend_ui::localized_text(
                        catalog,
                        lm_app::UiTextKey::CommonCancel,
                    ))
                    .clicked()
                {
                    self.pending_protected_page_toggle = false;
                }
                if ui
                    .button(text(
                        catalog,
                        if unlocking {
                            ExtendedUiTextKey::RomMap16Unlock
                        } else {
                            ExtendedUiTextKey::RomMap16Lock
                        },
                    ))
                    .clicked()
                {
                    self.protected_pages_unlocked = unlocking;
                    self.pending_protected_page_toggle = false;
                }
            });
        });
    }

    fn commit_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        project_revision: u64,
        commit_shortcut: bool,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(text(catalog, ExtendedUiTextKey::RomMap16AllocationPc));
            ui.text_edit_singleline(&mut self.search_start);
            ui.label(text(
                catalog,
                ExtendedUiTextKey::RomMap16AllocationSeparator,
            ));
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        let commit_clicked = ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new(text(catalog, ExtendedUiTextKey::RomMap16Commit))
                    .shortcut_text("F9"),
            )
            .clicked();
        if modified
            && !stale
            && !self.manifest_loader.is_running()
            && (commit_clicked || commit_shortcut)
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
                    && !stale
                    && !self.manifest_loader.is_running()
                    && self
                        .workspace
                        .as_ref()
                        .is_some_and(|workspace| workspace.controller.supports_reclamation()),
                egui::Button::new(text(catalog, ExtendedUiTextKey::RomMap16CommitReclaim)),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(project_revision) {
                self.error = Some(error);
            }
        }
        ui.label(text(
            catalog,
            if modified {
                ExtendedUiTextKey::RomMap16Staged
            } else {
                ExtendedUiTextKey::RomMap16Unchanged
            },
        ));
        None
    }
    fn apply(&mut self, edit: Map16ControllerEdit) {
        if let Err(error) = self.apply_staged_edits(&[edit]) {
            self.error = Some(error);
        }
    }

    fn apply_staged_edits(&mut self, edits: &[Map16ControllerEdit]) -> Result<(), String> {
        let workspace = self.workspace.as_mut().ok_or("Map16 workspace is closed")?;
        let before = workspace.controller.set().clone();
        let mut staged = workspace.controller.clone();
        staged.apply_edits(edits)?;
        if staged.set() == &before {
            return Ok(());
        }
        let next_revision = self
            .staged_revision
            .checked_add(1)
            .ok_or("Map16 staged revision exhausted")?;
        workspace.controller = staged;
        push_history(&mut self.undo_history, before);
        self.redo_history.clear();
        self.staged_revision = next_revision;
        self.clipboard_paste_target = None;
        self.rectangle_clipboard_paste_target = None;
        self.page_texture = None;
        self.page_texture_key = None;
        self.invalidate();
        Ok(())
    }
    fn load(&mut self) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let key = (
            workspace.controller.revision(),
            self.page,
            self.tile,
            self.quadrant,
        );
        if self.loaded == Some(key) {
            return;
        }
        if let Some(tile) = workspace
            .controller
            .set()
            .pages
            .get(self.page)
            .and_then(|p| p.tiles.get(self.tile))
        {
            self.subtile = map16_subtile_form::SubtileForm::from_subtile(
                map16_subtile_form::quadrant_value(*tile, self.quadrant),
            );
            self.acts_like = format!("{:04X}", tile.acts_like);
            self.loaded = Some(key);
        }
    }
    fn address(&self) -> Map16Address {
        Map16Address {
            page: self.page,
            tile: self.tile,
        }
    }
    fn current_tile(&self) -> Option<lm_level::Map16Tile> {
        self.workspace
            .as_ref()?
            .controller
            .set()
            .pages
            .get(self.page)?
            .tiles
            .get(self.tile)
            .copied()
    }
    fn clamp(&mut self) {
        let pages = self
            .workspace
            .as_ref()
            .map_or(0, |w| w.controller.set().pages.len());
        self.page = self.page.min(pages.saturating_sub(1));
        self.tile = self.tile.min(Map16Page::TILE_COUNT - 1);
    }
    fn invalidate(&mut self) {
        self.loaded = None;
    }
}

fn map16_drag_rectangle(anchor: usize, current: usize) -> (usize, usize, usize) {
    let anchor = anchor.min(Map16Page::TILE_COUNT - 1);
    let current = current.min(Map16Page::TILE_COUNT - 1);
    let left = (anchor % 16).min(current % 16);
    let right = (anchor % 16).max(current % 16);
    let top = (anchor / 16).min(current / 16);
    let bottom = (anchor / 16).max(current / 16);
    (top * 16 + left, right - left + 1, bottom - top + 1)
}

fn page_rectangle(origin: usize, width: usize, height: usize) -> Option<(usize, usize, usize)> {
    (origin < Map16Page::TILE_COUNT
        && width > 0
        && height > 0
        && (origin % 16)
            .checked_add(width)
            .is_some_and(|end| end <= 16)
        && (origin / 16)
            .checked_add(height)
            .is_some_and(|end| end <= 16))
    .then_some((origin, width, height))
}

fn map16_selection_marquee(
    rectangle: egui::Rect,
    source_pixel_scale: f32,
) -> Vec<(egui::Pos2, egui::Pos2, egui::Color32)> {
    let scale = source_pixel_scale.max(1.0);
    let mut segments = Vec::new();
    for (start, end) in [
        (rectangle.left_top(), rectangle.right_top()),
        (rectangle.left_bottom(), rectangle.right_bottom()),
        (rectangle.left_top(), rectangle.left_bottom()),
        (rectangle.right_top(), rectangle.right_bottom()),
    ] {
        let vector = end - start;
        let length = vector.length();
        if length <= 0.0 {
            continue;
        }
        let direction = vector / length;
        let mut offset = 0.0;
        let mut source_pixel = 0_usize;
        while offset < length {
            let next = (offset + scale).min(length);
            let color = if matches!(source_pixel % 4, 0 | 3) {
                egui::Color32::WHITE
            } else {
                egui::Color32::BLACK
            };
            segments.push((start + direction * offset, start + direction * next, color));
            offset = next;
            source_pixel += 1;
        }
    }
    segments
}

fn take_map16_commit_shortcut(ui: &mut egui::Ui) -> bool {
    ui.input_mut(|input| {
        !input.modifiers.any() && input.consume_key(egui::Modifiers::NONE, egui::Key::F9)
    })
}

fn take_map16_paste_shortcut(ui: &mut egui::Ui) -> bool {
    ui.input_mut(|input| {
        let modifiers = input.modifiers;
        modifiers.ctrl && input.consume_key(modifiers, egui::Key::V)
    })
}

fn take_map16_page_shortcut(ui: &mut egui::Ui) -> Option<Map16PageShortcut> {
    ui.input_mut(|input| {
        let modifiers = input.modifiers;
        if input.consume_key(modifiers, egui::Key::ArrowUp) {
            Some(Map16PageShortcut::Previous)
        } else if input.consume_key(modifiers, egui::Key::ArrowDown) {
            Some(Map16PageShortcut::Next)
        } else {
            None
        }
    })
}

fn take_map16_grid_shortcut(ui: &mut egui::Ui) -> Option<Map16GridShortcut> {
    ui.input_mut(|input| {
        let modifiers = input.modifiers;
        if !input.consume_key(modifiers, egui::Key::F8) {
            None
        } else if modifiers.ctrl && modifiers.alt {
            Some(Map16GridShortcut::ToggleColor)
        } else {
            Some(Map16GridShortcut::Toggle)
        }
    })
}

fn take_map16_zoom_shortcut(ui: &mut egui::Ui) -> Option<Map16ZoomShortcut> {
    ui.input_mut(|input| {
        let modifiers = input.modifiers;
        let shortcut = if input.consume_key(modifiers, egui::Key::Num0) {
            Some(Map16ZoomShortcut::Reset)
        } else if input.consume_key(modifiers, egui::Key::Plus) {
            Some(Map16ZoomShortcut::Increase)
        } else if input.consume_key(modifiers, egui::Key::Minus) {
            Some(Map16ZoomShortcut::Decrease)
        } else {
            None
        };
        modifiers.ctrl.then_some(shortcut).flatten()
    })
}

fn take_map16_f1_shortcut(ui: &mut egui::Ui) -> Option<Map16F1Shortcut> {
    ui.input_mut(|input| {
        let modifiers = input.modifiers;
        if !input.consume_key(modifiers, egui::Key::F1) || modifiers.shift {
            None
        } else if modifiers.ctrl {
            Some(Map16F1Shortcut::ToggleProtectedPages)
        } else {
            Some(Map16F1Shortcut::TogglePageNumbers)
        }
    })
}

fn map16_page_is_editable(page: usize, protected_pages_unlocked: bool) -> bool {
    page >= 2 || protected_pages_unlocked
}

fn map16_zoom_after_shortcut(current: u16, shortcut: Map16ZoomShortcut) -> u16 {
    match shortcut {
        Map16ZoomShortcut::Reset => 100,
        Map16ZoomShortcut::Increase => current.saturating_add(100).min(5000),
        Map16ZoomShortcut::Decrease => current.saturating_sub(100).max(100),
    }
}

fn apply_map16_grid_shortcut(visible: &mut bool, dark: &mut bool, shortcut: Map16GridShortcut) {
    match shortcut {
        Map16GridShortcut::Toggle => *visible = !*visible,
        Map16GridShortcut::ToggleColor => *dark = !*dark,
    }
}

fn map16_page_after_shortcut(current: usize, pages: usize, shortcut: Map16PageShortcut) -> usize {
    match shortcut {
        Map16PageShortcut::Previous => current.saturating_sub(1),
        Map16PageShortcut::Next => current.saturating_add(1).min(pages.saturating_sub(1)),
    }
}

const MAP16_HISTORY_LIMIT: usize = 100;

fn push_history(history: &mut Vec<lm_level::Map16Set>, value: lm_level::Map16Set) {
    if history.len() == MAP16_HISTORY_LIMIT {
        history.remove(0);
    }
    history.push(value);
}
