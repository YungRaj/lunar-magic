use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    map16_subtile_form::{self, SubtileForm},
    native_map16_sidecar_form::NativeMap16SidecarForm,
};
use eframe::egui;
use lm_app::{
    ExtendedUiTextKey as Key, LocalizationCatalog, NativeMap16SidecarController,
    NativeMap16SidecarDocumentKind, NativeMap16SidecarEdit,
};
use lm_level::S16Sidecar;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

struct PendingOpen {
    path: PathBuf,
    kind: NativeMap16SidecarDocumentKind,
}

#[derive(Default)]
pub(crate) struct NativeMap16SidecarEditor {
    controller: Option<NativeMap16SidecarController>,
    pending_open: Option<PendingOpen>,
    form: NativeMap16SidecarForm,
    quadrant: usize,
    subtile: SubtileForm,
    loaded_key: Option<(u64, usize, usize)>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
    loading_kind: Option<NativeMap16SidecarDocumentKind>,
}

impl NativeMap16SidecarEditor {
    pub(crate) fn value(&self) -> Option<&lm_app::NativeMap16SidecarDocument> {
        self.controller
            .as_ref()
            .map(NativeMap16SidecarController::value)
    }

    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.pending_open.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_native_map16_sidecar() else {
            return;
        };
        self.pending_open = Some(PendingOpen {
            path,
            kind: NativeMap16SidecarDocumentKind::M16,
        });
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for Map16 sidecar loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for Map16 sidecar persistence to finish before closing".into());
            return false;
        }
        if self.pending_open.is_some() {
            self.pending_open = None;
            return true;
        }
        let Some(controller) = &self.controller else {
            return true;
        };
        if !controller.is_modified() {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Document
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        foreground_texture: Option<&egui::TextureHandle>,
        sprite_texture: Option<&egui::TextureHandle>,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        if let Some(result) = self.loader.show(context) {
            let kind = self.loading_kind.take();
            match result {
                Err(error) => self.error = Some(error),
                Ok(mut loaded) => match (kind, loaded.files.pop()) {
                    (Some(kind), Some((path, bytes))) => {
                        match NativeMap16SidecarController::decode(path.clone(), kind, &bytes) {
                            Ok(controller) => {
                                self.controller = Some(controller);
                                self.loaded_key = None;
                            }
                            Err(error) => {
                                self.error = Some(error.to_string());
                                self.pending_open = Some(PendingOpen { path, kind });
                            }
                        }
                    }
                    (None, _) => {
                        self.error = Some("Map16 sidecar load lost its kind".into());
                    }
                    (_, None) => {
                        self.error = Some("Map16 sidecar loader returned no file".into());
                    }
                },
            }
        }
        if let Some(controller) = self.controller.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, controller)
        {
            self.error = Some(error);
        }
        self.show_open_configuration(context, catalog);
        if self.controller.is_some() {
            self.clamp_and_load();
            egui::Window::new(text(catalog, Key::Map16SidecarEditorTitle))
                .default_size([540.0, 360.0])
                .show(context, |ui| {
                    self.contents(ui, foreground_texture, sprite_texture, catalog);
                });
        }
        let approved = self.show_close_confirmation(context, catalog);
        self.show_error(context, catalog);
        approved
    }

    fn show_open_configuration(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new(text(catalog, Key::Map16SidecarInterpretTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                if let Some(pending) = self.pending_open.as_mut() {
                    ui.radio_value(
                        &mut pending.kind,
                        NativeMap16SidecarDocumentKind::M16,
                        text(catalog, Key::Map16SidecarM16Kind),
                    );
                    ui.radio_value(
                        &mut pending.kind,
                        NativeMap16SidecarDocumentKind::S16,
                        text(catalog, Key::Map16SidecarS16Kind),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button(text(catalog, Key::Map16SidecarCancel)).clicked() {
                        self.pending_open = None;
                    }
                    if ui.button(text(catalog, Key::Map16SidecarOpen)).clicked() {
                        self.finish_open();
                    }
                });
            });
    }

    fn finish_open(&mut self) {
        let Some(pending) = self.pending_open.take() else {
            return;
        };
        let request = BoundedRead::new(
            pending.path.clone(),
            u64::try_from(S16Sidecar::CAPACITY).unwrap_or(u64::MAX),
            "native Map16 sidecar",
        );
        match self.loader.start(vec![request]) {
            Ok(()) => self.loading_kind = Some(pending.kind),
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending);
            }
        }
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        foreground_texture: Option<&egui::TextureHandle>,
        sprite_texture: Option<&egui::TextureHandle>,
        catalog: Option<&LocalizationCatalog>,
    ) {
        self.toolbar(ui, catalog);
        ui.separator();
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let document_kind = controller.value().kind();
        let kind = match document_kind {
            NativeMap16SidecarDocumentKind::M16 => text(catalog, Key::Map16SidecarM16Exact),
            NativeMap16SidecarDocumentKind::S16 => text(catalog, Key::Map16SidecarS16Canonical),
        };
        let count = controller.value().entry_count();
        let tile_count = controller.value().tile_count();
        let encoded_len = controller.value().encode().len();
        let current_tile = controller.value().tile(self.form.entry / 2);
        ui.label(
            text(catalog, Key::Map16SidecarSummaryFormat)
                .replace("{kind}", &kind)
                .replace("{count}", &count.to_string())
                .replace("{tile_count}", &tile_count.to_string())
                .replace("{encoded_len}", &encoded_len.to_string()),
        );
        let previous = self.form.entry;
        ui.add(
            egui::Slider::new(&mut self.form.entry, 0..=count.saturating_sub(1))
                .text(text(catalog, Key::Map16SidecarRawEntry)),
        );
        if previous != self.form.entry {
            self.loaded_key = None;
            self.clamp_and_load();
        }
        ui.horizontal(|ui| {
            ui.label(text(catalog, Key::Map16SidecarRawDword));
            ui.text_edit_singleline(&mut self.form.value);
        });
        if ui
            .button(text(catalog, Key::Map16SidecarApplyRaw))
            .clicked()
        {
            match self.form.edit() {
                Ok(edit) => self.apply_edit(edit),
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(tile) = current_tile {
            show_definition_preview(
                ui,
                self.form.entry / 2,
                tile,
                match document_kind {
                    NativeMap16SidecarDocumentKind::M16 => {
                        foreground_texture.map(DefinitionTexture::Foreground)
                    }
                    NativeMap16SidecarDocumentKind::S16 => {
                        sprite_texture.map(DefinitionTexture::Sprite)
                    }
                },
                catalog,
            );
            ui.horizontal(|ui| {
                ui.label(text(catalog, Key::Map16SidecarQuadrant));
                for index in 0..4 {
                    if ui
                        .selectable_value(
                            &mut self.quadrant,
                            index,
                            map16_subtile_form::quadrant_name(index),
                        )
                        .changed()
                    {
                        self.loaded_key = None;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(text(catalog, Key::Map16SidecarTile));
                ui.text_edit_singleline(&mut self.subtile.tile);
            });
            ui.add(
                egui::Slider::new(&mut self.subtile.palette, 0..=7)
                    .text(text(catalog, Key::Map16SidecarPalette)),
            );
            ui.checkbox(
                &mut self.subtile.priority,
                text(catalog, Key::Map16SidecarPriority),
            );
            ui.checkbox(
                &mut self.subtile.x_flip,
                text(catalog, Key::Map16SidecarHorizontalFlip),
            );
            ui.checkbox(
                &mut self.subtile.y_flip,
                text(catalog, Key::Map16SidecarVerticalFlip),
            );
            if ui
                .button(text(catalog, Key::Map16SidecarApplySubtile))
                .clicked()
            {
                self.apply_subtile();
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let (can_undo, can_redo, modified) = (
            controller.can_undo(),
            controller.can_redo(),
            controller.is_modified(),
        );
        let mut history = None;
        let mut save_requested = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_undo,
                    egui::Button::new(text(catalog, Key::Map16SidecarUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(text(catalog, Key::Map16SidecarRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, Key::Map16SidecarSave)),
                )
                .clicked();
            ui.label(if modified {
                text(catalog, Key::Map16SidecarModified)
            } else {
                text(catalog, Key::Map16SidecarSaved)
            });
        });
        let mut changed = false;
        if let Some(controller) = self.controller.as_mut() {
            if let Some(undo) = history {
                let result = if undo {
                    controller.undo(controller.revision())
                } else {
                    controller.redo(controller.revision())
                };
                match result {
                    Ok(value) => changed = value,
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            if save_requested {
                if let Err(error) = self.persistence.begin(controller) {
                    self.error = Some(error);
                }
            }
        }
        if changed {
            self.loaded_key = None;
        }
    }

    fn apply_edit(&mut self, edit: NativeMap16SidecarEdit) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) =
            controller.apply_edits(controller.revision(), std::slice::from_ref(&edit))
        {
            self.error = Some(error.to_string());
        } else {
            self.loaded_key = None;
        }
    }

    fn apply_subtile(&mut self) {
        let subtile = match self.subtile.parse() {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let tile_index = self.form.entry / 2;
        let entry = tile_index * 2 + self.quadrant / 2;
        let Some(current) = self
            .controller
            .as_ref()
            .and_then(|controller| controller.value().entry(entry))
        else {
            self.error = Some("selected Map16 subtile is outside the sidecar".into());
            return;
        };
        self.apply_edit(NativeMap16SidecarEdit {
            entry,
            value: replace_subtile_word(current, self.quadrant % 2, subtile.0),
        });
    }

    fn clamp_and_load(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let count = controller.value().entry_count();
        self.form.entry = self.form.entry.min(count.saturating_sub(1));
        self.quadrant = self.quadrant.min(3);
        let key = (controller.revision(), self.form.entry, self.quadrant);
        if self.loaded_key != Some(key) {
            let value = controller.value().entry(self.form.entry).unwrap_or(0);
            self.form = NativeMap16SidecarForm::load(self.form.entry, value);
            if let Some(tile) = controller.value().tile(self.form.entry / 2) {
                self.subtile = SubtileForm::from_subtile(map16_subtile_form::quadrant_value(
                    tile,
                    self.quadrant,
                ));
            }
            self.loaded_key = Some(key);
        }
    }

    fn show_close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(text(catalog, Key::Map16SidecarDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(text(catalog, Key::Map16SidecarDiscardNotice));
                ui.horizontal(|ui| {
                    if ui.button(text(catalog, Key::Map16SidecarCancel)).clicked() {
                        self.pending_close = None;
                    }
                    if ui.button(text(catalog, Key::Map16SidecarDiscard)).clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, Key::Map16SidecarErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button(text(catalog, Key::Map16SidecarOk)).clicked() {
                        self.error = None;
                    }
                });
        }
    }

    fn clear(&mut self) {
        self.controller = None;
        self.pending_open = None;
        self.pending_close = None;
        self.loaded_key = None;
    }
}

fn show_definition_preview(
    ui: &mut egui::Ui,
    index: usize,
    tile: lm_level::Map16Tile,
    texture: Option<DefinitionTexture<'_>>,
    catalog: Option<&LocalizationCatalog>,
) {
    ui.separator();
    ui.label(
        text(catalog, Key::Map16SidecarDefinitionFormat)
            .replace("{index}", &format!("{index:04X}")),
    );
    if let Some(texture) = texture {
        let (response, painter) = ui.allocate_painter(egui::vec2(96.0, 96.0), egui::Sense::hover());
        match texture {
            DefinitionTexture::Foreground(texture) => {
                crate::vanilla_level_editor::draw_custom_map16_tile(
                    &painter,
                    texture,
                    response.rect,
                    tile,
                );
            }
            DefinitionTexture::Sprite(texture) => {
                crate::vanilla_level_editor::draw_sprite_preview_definition(
                    &painter,
                    texture,
                    response.rect,
                    [
                        tile.top_left.0,
                        tile.top_right.0,
                        tile.bottom_left.0,
                        tile.bottom_right.0,
                    ],
                );
            }
        }
    }
    egui::Grid::new("native-map16-definition-preview")
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            subtile_label(ui, "TL", tile.top_left);
            subtile_label(ui, "TR", tile.top_right);
            ui.end_row();
            subtile_label(ui, "BL", tile.bottom_left);
            subtile_label(ui, "BR", tile.bottom_right);
            ui.end_row();
        });
}

#[derive(Clone, Copy)]
enum DefinitionTexture<'a> {
    Foreground(&'a egui::TextureHandle),
    Sprite(&'a egui::TextureHandle),
}

fn subtile_label(ui: &mut egui::Ui, name: &str, subtile: lm_level::Subtile) {
    ui.group(|ui| {
        ui.monospace(format!(
            "{name} {:04X}\ntile {:03X} pal {}{}{}{}",
            subtile.0,
            subtile.tile_number(),
            subtile.palette(),
            if subtile.priority() { " P" } else { "" },
            if subtile.x_flip() { " X" } else { "" },
            if subtile.y_flip() { " Y" } else { "" },
        ));
    });
}

fn replace_subtile_word(entry: u32, half: usize, word: u16) -> u32 {
    let shift = if half == 0 { 0 } else { 16 };
    (entry & !(0xffff_u32 << shift)) | (u32::from(word) << shift)
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::replace_subtile_word;

    #[test]
    fn complete_sidecar_surface_has_no_literal_widget_text() {
        let source = include_str!("native_map16_sidecar_editor.rs");
        for literal in [
            "egui::Window::new(\"",
            "ui.button(\"",
            "egui::Button::new(\"",
            "ui.label(\"",
            ".text(\"",
        ] {
            assert!(
                !source.contains(literal),
                "literal sidecar widget text: {literal}"
            );
        }
    }

    #[test]
    fn semantic_subtile_edit_preserves_the_other_packed_word() {
        assert_eq!(replace_subtile_word(0x4433_2211, 0, 0xaabb), 0x4433_aabb);
        assert_eq!(replace_subtile_word(0x4433_2211, 1, 0xccdd), 0xccdd_2211);
    }
}
