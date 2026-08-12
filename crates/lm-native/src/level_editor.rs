mod dimensions;
mod document_io;
mod tilemap;

use crate::{
    document_loader::DocumentLoader, document_persistence::DocumentPersistence,
    level_editor_panels::LevelPanelState, level_editor_render,
};
use dimensions::{parse_dimensions, suggested_dimensions};
use eframe::egui;
use level_editor_render::LevelAssets;
use lm_app::CompleteLevelDocumentController;
use lm_render::PortableLevelRenderDimensions;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

struct PendingOpen {
    controller: CompleteLevelDocumentController,
    assets: LevelAssets,
    dimensions: [String; 4],
}

struct LevelDocument {
    controller: CompleteLevelDocumentController,
    assets: LevelAssets,
    dimensions: PortableLevelRenderDimensions,
}

#[derive(Default)]
pub(crate) struct LevelEditor {
    document: Option<LevelDocument>,
    pending_open: Option<PendingOpen>,
    panels: LevelPanelState,
    edit_layer: usize,
    selected: (usize, usize),
    tile_value: String,
    loaded_tile: Option<(u64, usize, usize, usize)>,
    rendered_revision: Option<u64>,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl LevelEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.document.is_some() || self.pending_open.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(requests) = document_io::choose_requests() else {
            return;
        };
        if let Err(error) = self.loader.start(requests) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for level loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for level persistence to finish before closing".into());
            return false;
        }
        if self.pending_open.is_some() {
            self.pending_open = None;
            return true;
        }
        let Some(document) = &self.document else {
            return true;
        };
        if !document.controller.is_modified() {
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
        catalog: Option<&lm_app::LocalizationCatalog>,
    ) -> bool {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(document_io::decode_loaded) {
                Ok(pending) => self.pending_open = Some(pending),
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(document) = self.document.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, &mut document.controller)
        {
            self.error = Some(error);
        }
        self.show_open_configuration(context);
        if self.document.is_some() {
            self.refresh_texture(context);
            self.load_tile();
            egui::Window::new("Portable Complete Level Editor")
                .default_size([1000.0, 700.0])
                .show(context, |ui| self.contents(ui, catalog));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn show_open_configuration(&mut self, context: &egui::Context) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new("Level dimensions")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Enter exact row-major tilemap dimensions:");
                if let Some(pending) = self.pending_open.as_mut() {
                    for (label, field) in [
                        "Layer 1 width",
                        "Layer 1 height",
                        "Layer 2 width",
                        "Layer 2 height",
                    ]
                    .into_iter()
                    .zip(pending.dimensions.iter_mut())
                    {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            ui.text_edit_singleline(field);
                        });
                    }
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
        match parse_dimensions(&pending.dimensions, pending.controller.value()) {
            Ok(dimensions) => {
                self.document = Some(LevelDocument {
                    controller: pending.controller,
                    assets: pending.assets,
                    dimensions,
                });
                self.invalidate();
            }
            Err(error) => {
                self.error = Some(error);
                self.pending_open = Some(pending);
            }
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&lm_app::LocalizationCatalog>) {
        self.toolbar(ui);
        ui.separator();
        ui.columns(2, |columns| {
            self.level_view(&mut columns[0]);
            self.side_panel(&mut columns[1], catalog);
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let can_undo = document.controller.can_undo();
        let can_redo = document.controller.can_redo();
        let modified = document.controller.is_modified();
        let mut action = None;
        let mut save_requested = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_undo, egui::Button::new("Undo"))
                .clicked()
            {
                action = Some(true);
            }
            if ui
                .add_enabled(can_redo, egui::Button::new("Redo"))
                .clicked()
            {
                action = Some(false);
            }
            if ui
                .add_enabled(!self.persistence.is_running(), egui::Button::new("Save"))
                .clicked()
            {
                save_requested = true;
            }
            ui.label(if modified { "Modified" } else { "Saved" });
        });
        let mut changed = false;
        if let Some(document) = self.document.as_mut() {
            if let Some(undo) = action {
                let revision = document.controller.revision();
                let result = if undo {
                    document.controller.undo(revision)
                } else {
                    document.controller.redo(revision)
                };
                if let Err(error) = result {
                    self.error = Some(error.to_string());
                } else {
                    changed = true;
                }
            }
            if save_requested {
                if let Err(error) = self.persistence.begin(&mut document.controller) {
                    self.error = Some(error);
                }
            }
        }
        if changed {
            self.invalidate();
        }
    }

    fn refresh_texture(&mut self, context: &egui::Context) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let revision = document.controller.revision();
        if self.rendered_revision == Some(revision) {
            return;
        }
        match level_editor_render::render_texture(
            context,
            document.controller.value(),
            &document.assets,
            document.dimensions,
        ) {
            Ok(texture) => self.texture = Some(texture),
            Err(error) => {
                self.texture = None;
                self.error = Some(error);
            }
        }
        self.rendered_revision = Some(revision);
    }

    fn invalidate(&mut self) {
        self.loaded_tile = None;
        self.rendered_revision = None;
        self.panels.invalidate();
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved complete level")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved level changes?");
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

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("Level editor error")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
    }

    fn clear(&mut self) {
        self.document = None;
        self.pending_open = None;
        self.texture = None;
        self.pending_close = None;
        self.invalidate();
    }
}
