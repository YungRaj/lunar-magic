use crate::{
    document_loader::DocumentLoader, level_editor_forms, map16_subtile_form, native_clipboard,
};
use eframe::egui;
use lm_app::{
    AppState, Command, Map16Controller, Map16ControllerEdit, RevisionProfile, SmwMap16Controller,
};
use lm_level::{Map16Address, Map16Page};

mod bitmap_import;
mod commit;
mod complete_file;
mod legacy_page;
mod lifecycle;
mod selected_file;
mod snes_tileset_import;
#[cfg(test)]
mod tests;

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

struct Workspace {
    controller: Controller,
    profile: Option<RevisionProfile>,
    snapshot: lm_app::ControllerSnapshot,
    image: lm_rom::RomImage,
    internal_header: usize,
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
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> (bool, Option<Command>) {
        let mut command = self.poll_bitmap_loader(context);
        self.poll_complete_file_io(context);
        self.poll_selected_file_io(context);
        self.poll_legacy_page_io(context);
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
            egui::Window::new("ROM Complete Map16 Editor")
                .default_size([560.0, 650.0])
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, project_revision) {
                        command = Some(ui_command);
                    }
                });
        }
        if let Some(import_command) = self.bitmap_import_window(context, project_revision) {
            command = Some(import_command);
        }
        if let Some(snes_command) = self.snes_tileset_preview_window(context, project_revision) {
            command = Some(snes_command);
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }
    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
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
                "The ROM changed; reopen before editing or committing.",
            );
        }
        let file_busy = self.complete_loader.is_running()
            || self.complete_persistence.is_running()
            || self.selected_loader.is_running()
            || self.selected_persistence.is_running()
            || self.legacy_page_loader.is_running()
            || self.legacy_page_persistence.is_running()
            || self.bitmap_loader.is_running()
            || self.bitmap_clipboard_loader.is_running()
            || self.bitmap_session.is_some()
            || self.snes_tileset_loader.is_running()
            || self.snes_tileset_preview.is_some();
        let edit_blocked = stale || file_busy;
        if let Some(shortcut) = take_map16_page_shortcut(ui) {
            let next_page = map16_page_after_shortcut(self.page, pages, shortcut);
            if next_page != self.page {
                self.page = next_page;
                self.invalidate();
                self.load();
            }
        }
        self.history_controls(ui, edit_blocked);
        self.selection_and_clipboard(ui, edit_blocked, pages, pasted.as_deref());
        self.visual_page(ui);
        self.tile_fields(ui, edit_blocked, pages);
        self.complete_file_controls(
            ui,
            stale
                || self.selected_loader.is_running()
                || self.selected_persistence.is_running()
                || self.legacy_page_loader.is_running()
                || self.legacy_page_persistence.is_running()
                || self.bitmap_loader.is_running()
                || self.bitmap_clipboard_loader.is_running()
                || self.bitmap_session.is_some()
                || self.snes_tileset_loader.is_running()
                || self.snes_tileset_preview.is_some(),
            project_revision,
        );
        self.selected_file_controls(ui, edit_blocked, project_revision, pasted.as_deref());
        self.legacy_page_controls(
            ui,
            stale
                || self.selected_loader.is_running()
                || self.selected_persistence.is_running()
                || self.bitmap_loader.is_running()
                || self.bitmap_clipboard_loader.is_running()
                || self.bitmap_session.is_some()
                || self.snes_tileset_loader.is_running()
                || self.snes_tileset_preview.is_some(),
            project_revision,
        );
        self.bitmap_import_controls(ui, edit_blocked, project_revision);
        self.snes_tileset_controls(ui, edit_blocked, project_revision);
        self.commit_controls(ui, edit_blocked, project_revision, commit_shortcut)
    }
    fn visual_page(&mut self, ui: &mut egui::Ui) {
        let changed = ui
            .horizontal(|ui| {
                ui.label("Preview level");
                let level = ui.text_edit_singleline(&mut self.preview_level).changed();
                let tileset = ui
                    .add(egui::Slider::new(&mut self.preview_tileset, 0..=15).text("Object set"))
                    .changed();
                let palette = ui
                    .add(egui::Slider::new(&mut self.preview_palette, 0..=7).text("FG palette"))
                    .changed();
                level || tileset || palette
            })
            .inner;
        if changed {
            self.page_texture = None;
            self.page_texture_key = None;
        }
        let Ok(level) = u16::from_str_radix(self.preview_level.trim(), 16) else {
            ui.colored_label(egui::Color32::RED, "Preview level must be hexadecimal.");
            return;
        };
        if level > 0x01ff {
            ui.colored_label(
                egui::Color32::RED,
                "Preview level must be between 000 and 1FF.",
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
        let Some(texture) = &self.page_texture else {
            return;
        };
        let response = ui.add(
            egui::Image::new(texture)
                .fit_to_exact_size(egui::Vec2::splat(256.0))
                .sense(egui::Sense::click()),
        );
        if response.clicked()
            && let Some(position) = response.interact_pointer_pos()
            && let Some(tile) = crate::map16_editor_render::selected_tile(response.rect, position)
        {
            self.tile = tile;
            self.invalidate();
            self.load();
        }
        let column = self.tile % 16;
        let row = self.tile / 16;
        let column = f32::from(u8::try_from(column).unwrap_or(0));
        let row = f32::from(u8::try_from(row).unwrap_or(0));
        let cell = egui::Rect::from_min_size(
            response.rect.min + egui::vec2(column * 16.0, row * 16.0),
            egui::Vec2::splat(16.0),
        );
        ui.painter().rect_stroke(
            cell,
            0.0,
            egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
            egui::StrokeKind::Inside,
        );
        ui.small("Click a rendered 16×16 tile to select it.");
    }

    fn selection_and_clipboard(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        pages: usize,
        pasted: Option<&str>,
    ) {
        let old = (self.page, self.tile, self.quadrant);
        ui.add(egui::Slider::new(&mut self.page, 0..=pages.saturating_sub(1)).text("Page"));
        ui.add(egui::Slider::new(&mut self.tile, 0..=Map16Page::TILE_COUNT - 1).text("Tile"));
        egui::ComboBox::from_label("Quadrant")
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
        ui.heading(format!("Map16 {:02X}:{:02X}", self.page, self.tile));
        ui.horizontal(|ui| {
            if ui.button("Copy tile").clicked()
                && let Some(tile) = self.current_tile()
            {
                match native_clipboard::encode_map16_tile(tile) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(!stale, egui::Button::new("Paste tile"))
                .clicked()
            {
                self.rectangle_clipboard_paste_target = None;
                let revision = self
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.controller.revision());
                self.clipboard_paste_target =
                    revision.map(|revision| (revision, self.staged_revision, self.address()));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
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

    fn history_controls(&mut self, ui: &mut egui::Ui, blocked: bool) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !blocked && !self.undo_history.is_empty(),
                    egui::Button::new("Undo"),
                )
                .clicked()
                && let Err(error) = self.navigate_history(true)
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    !blocked && !self.redo_history.is_empty(),
                    egui::Button::new("Redo"),
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

    fn tile_fields(&mut self, ui: &mut egui::Ui, stale: bool, pages: usize) {
        let supports_acts_like = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.controller.supports_acts_like(self.page));
        ui.horizontal(|ui| {
            ui.label("8×8 tile");
            ui.text_edit_singleline(&mut self.subtile.tile);
        });
        ui.add(egui::Slider::new(&mut self.subtile.palette, 0..=7).text("Palette"));
        ui.checkbox(&mut self.subtile.priority, "Priority");
        ui.checkbox(&mut self.subtile.x_flip, "X flip");
        ui.checkbox(&mut self.subtile.y_flip, "Y flip");
        let mut edit = None;
        if ui
            .add_enabled(!stale, egui::Button::new("Apply subtile"))
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
                ui.label("Acts Like");
                ui.text_edit_singleline(&mut self.acts_like);
            });
        });
        if ui
            .add_enabled(
                !stale && supports_acts_like,
                egui::Button::new("Apply Acts Like"),
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
            ui.small("Background Map16 definitions do not have Acts-Like values.");
        }
    }

    fn commit_controls(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        project_revision: u64,
        commit_shortcut: bool,
    ) -> Option<Command> {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Allocation logical PC hex");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        let commit_clicked = ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit complete Map16 set to ROM").shortcut_text("F9"),
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
                egui::Button::new("Commit and reclaim"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(project_revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged Map16 changes"
        } else {
            "No staged changes"
        });
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

fn take_map16_commit_shortcut(ui: &mut egui::Ui) -> bool {
    ui.input_mut(|input| {
        !input.modifiers.any() && input.consume_key(egui::Modifiers::NONE, egui::Key::F9)
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
