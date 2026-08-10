mod document_io;
mod tilemap;

use document_io::decode_document;

use crate::{
    document_loader::DocumentLoader,
    document_persistence::DocumentPersistence,
    overworld_editor_animation::OverworldAnimationPanel,
    overworld_editor_palette::OverworldPalettePanel,
    overworld_editor_records::OverworldRecordPanels,
    overworld_editor_render::{self, OverworldAssets},
    user_toolbar_images::{MainToolbarImageSet, OriginalToolbarAction, OriginalToolbarImages},
};
use eframe::egui;
use lm_app::OverworldDocumentController;
use lm_graphics::PaletteOwnership;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

struct PendingOpen {
    path: PathBuf,
    bytes: Vec<u8>,
    modes: [bool; 256],
    assets: OverworldAssets,
    maximum_records: String,
}

struct OverworldDocument {
    controller: OverworldDocumentController,
    modes: [bool; 256],
    ownership: PaletteOwnership,
    assets: OverworldAssets,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Panel {
    #[default]
    Records,
    Palette,
    Animation,
}

#[derive(Default)]
pub(crate) struct OverworldEditor {
    document: Option<OverworldDocument>,
    pending_open: Option<PendingOpen>,
    panel: Panel,
    records: OverworldRecordPanels,
    palette: OverworldPalettePanel,
    animation: OverworldAnimationPanel,
    edit_layer: usize,
    selected: (usize, usize),
    tile_value: String,
    loaded_tile: Option<(u64, usize, usize, usize)>,
    completed_reveals: usize,
    rendered_key: Option<(u64, usize)>,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl OverworldEditor {
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
            self.error = Some("wait for overworld loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for overworld persistence to finish before closing".into());
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
        toolbar_images: &MainToolbarImageSet,
    ) -> bool {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(document_io::pending_from_loaded) {
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
            egui::Window::new("Portable Complete Overworld Editor")
                .default_size([1020.0, 720.0])
                .show(context, |ui| self.contents(ui, toolbar_images));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn show_open_configuration(&mut self, context: &egui::Context) {
        if self.pending_open.is_none() {
            return;
        }
        egui::Window::new("Open complete overworld")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Maximum ExAnimation records from this revision/profile:");
                if let Some(pending) = self.pending_open.as_mut() {
                    ui.text_edit_singleline(&mut pending.maximum_records);
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
        let maximum = match pending.maximum_records.trim().parse::<usize>() {
            Ok(maximum) => maximum,
            Err(error) => {
                self.error = Some(format!("invalid maximum animation record count: {error}"));
                self.pending_open = Some(pending);
                return;
            }
        };
        match decode_document(pending, maximum) {
            Ok(document) => {
                self.document = Some(document);
                self.completed_reveals = 0;
                self.invalidate();
            }
            Err(error) => {
                let (error, pending) = *error;
                self.error = Some(error);
                self.pending_open = Some(pending);
            }
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, toolbar_images: &MainToolbarImageSet) {
        self.toolbar(ui, toolbar_images);
        ui.separator();
        ui.columns(2, |columns| {
            self.world_view(&mut columns[0]);
            self.side_panel(&mut columns[1]);
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, toolbar_images: &MainToolbarImageSet) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let can_undo = document.controller.can_undo();
        let can_redo = document.controller.can_redo();
        let modified = document.controller.is_modified();
        let mut history = None;
        let mut save_requested = false;
        ui.horizontal(|ui| {
            if toolbar_images
                .original_action_button(
                    ui,
                    OriginalToolbarImages::Overworld,
                    OriginalToolbarAction::Undo,
                    "Undo",
                    can_undo,
                )
                .clicked()
            {
                history = Some(true);
            }
            if toolbar_images
                .original_action_button(
                    ui,
                    OriginalToolbarImages::Overworld,
                    OriginalToolbarAction::Redo,
                    "Redo",
                    can_redo,
                )
                .clicked()
            {
                history = Some(false);
            }
            if toolbar_images
                .original_action_button(
                    ui,
                    OriginalToolbarImages::Overworld,
                    OriginalToolbarAction::Save,
                    "Save",
                    !self.persistence.is_running(),
                )
                .clicked()
            {
                save_requested = true;
            }
            ui.label(if modified { "Modified" } else { "Saved" });
        });
        let mut changed = false;
        if let Some(document) = self.document.as_mut() {
            if let Some(undo) = history {
                let revision = document.controller.revision();
                let result = if undo {
                    document.controller.undo(revision)
                } else {
                    document.controller.redo(revision)
                };
                match result {
                    Ok(_) => changed = true,
                    Err(error) => self.error = Some(error.to_string()),
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
        let key = (document.controller.revision(), self.completed_reveals);
        if self.rendered_key == Some(key) {
            return;
        }
        match overworld_editor_render::render_texture(
            context,
            document.controller.value(),
            &document.assets,
            None,
            None,
            self.completed_reveals,
        ) {
            Ok(texture) => self.texture = Some(texture),
            Err(error) => {
                self.texture = None;
                self.error = Some(error);
            }
        }
        self.rendered_key = Some(key);
    }

    fn invalidate(&mut self) {
        self.loaded_tile = None;
        self.rendered_key = None;
        self.records.invalidate();
        self.animation.invalidate();
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved complete overworld")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved overworld changes?");
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
            egui::Window::new("Overworld editor error")
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
