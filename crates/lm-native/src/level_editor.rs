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
use lm_app::{CompleteLevelDocumentController, ExtendedUiTextKey as Key, LocalizationCatalog};
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
        self.show_open_configuration(context, catalog);
        if self.document.is_some() {
            self.refresh_texture(context);
            self.load_tile();
            egui::Window::new(level_document_text(catalog, Key::LevelDocumentTitle))
                .default_size([1000.0, 700.0])
                .show(context, |ui| self.contents(ui, catalog));
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
        egui::Window::new(level_document_text(
            catalog,
            Key::LevelDocumentDimensionsTitle,
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(context, |ui| {
            ui.label(level_document_text(
                catalog,
                Key::LevelDocumentDimensionsNotice,
            ));
            if let Some(pending) = self.pending_open.as_mut() {
                for (label, field) in [
                    level_document_text(catalog, Key::LevelDocumentLayer1Width),
                    level_document_text(catalog, Key::LevelDocumentLayer1Height),
                    level_document_text(catalog, Key::LevelDocumentLayer2Width),
                    level_document_text(catalog, Key::LevelDocumentLayer2Height),
                ]
                .into_iter()
                .zip(pending.dimensions.iter_mut())
                {
                    ui.horizontal(|ui| {
                        ui.label(&label);
                        ui.text_edit_singleline(field);
                    });
                }
            }
            ui.horizontal(|ui| {
                if ui
                    .button(level_document_text(catalog, Key::LevelDocumentCancel))
                    .clicked()
                {
                    self.pending_open = None;
                }
                if ui
                    .button(level_document_text(catalog, Key::LevelDocumentOpen))
                    .clicked()
                {
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
        self.toolbar(ui, catalog);
        ui.separator();
        ui.columns(2, |columns| {
            self.level_view(&mut columns[0], catalog);
            self.side_panel(&mut columns[1], catalog);
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
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
                .add_enabled(
                    can_undo,
                    egui::Button::new(level_document_text(catalog, Key::LevelDocumentUndo)),
                )
                .clicked()
            {
                action = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(level_document_text(catalog, Key::LevelDocumentRedo)),
                )
                .clicked()
            {
                action = Some(false);
            }
            if ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(level_document_text(catalog, Key::LevelDocumentSave)),
                )
                .clicked()
            {
                save_requested = true;
            }
            ui.label(level_document_text(
                catalog,
                if modified {
                    Key::LevelDocumentModified
                } else {
                    Key::LevelDocumentSaved
                },
            ));
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

    fn show_close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(level_document_text(catalog, Key::LevelDocumentDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(level_document_text(
                    catalog,
                    Key::LevelDocumentDiscardNotice,
                ));
                ui.horizontal(|ui| {
                    if ui
                        .button(level_document_text(catalog, Key::LevelDocumentCancel))
                        .clicked()
                    {
                        self.pending_close = None;
                    }
                    if ui
                        .button(level_document_text(catalog, Key::LevelDocumentDiscard))
                        .clicked()
                    {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(level_document_text(catalog, Key::LevelDocumentErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui
                        .button(level_document_text(catalog, Key::LevelDocumentOk))
                        .clicked()
                    {
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

fn level_document_text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

#[cfg(test)]
mod localization_tests {
    use super::Key;

    #[test]
    fn complete_level_document_shell_has_no_literal_widget_text() {
        let sources = [
            include_str!("level_editor.rs"),
            include_str!("level_editor/tilemap.rs"),
        ];
        for literal_widget in [
            "ui.button(\"",
            "ui.label(\"",
            "ui.heading(\"",
            "Button::new(\"",
            "Window::new(\"",
        ] {
            assert!(
                sources
                    .iter()
                    .all(|source| !source.contains(literal_widget)),
                "complete-level document shell bypasses localization with {literal_widget}"
            );
        }
        let joined = sources.join("\n");
        for key in Key::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("LevelDocument"))
        {
            assert!(
                joined.contains(&format!("Key::{key:?}")),
                "complete-level document shell does not consume {key:?}"
            );
        }
    }
}
