use crate::{
    document_loader::DocumentLoader,
    document_persistence::DocumentPersistence,
    overworld_appearance_editor_forms::{DefinitionForm, PartForm},
};
use eframe::egui;
use lm_app::{OverworldAppearanceDocumentController, OverworldAppearanceDocumentEdit};

mod document_io;
mod form_fields;
mod panels;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct OverworldAppearanceEditor {
    controller: Option<OverworldAppearanceDocumentController>,
    definition_index: usize,
    definition: DefinitionForm,
    definition_key: Option<(u64, usize)>,
    part_index: usize,
    part: PartForm,
    part_key: Option<(u64, u16, usize)>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl OverworldAppearanceEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for appearance loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for appearance persistence to finish before closing".into());
            return false;
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

    pub(crate) fn show(&mut self, context: &egui::Context) -> bool {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(document_io::decode) {
                Ok(controller) => {
                    self.controller = Some(controller);
                    self.invalidate();
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(controller) = self.controller.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, controller)
        {
            self.error = Some(error);
        }
        if self.controller.is_some() {
            self.clamp_indices();
            egui::Window::new("Portable Overworld Appearance Editor")
                .default_size([720.0, 600.0])
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        let (revision, definitions) = {
            let Some(controller) = self.controller.as_ref() else {
                return;
            };
            (
                controller.revision(),
                controller.value().definitions.clone(),
            )
        };
        ui.label(format!("Sprite definitions: {}", definitions.len()));
        ui.add(
            egui::Slider::new(
                &mut self.definition_index,
                0..=definitions.len().saturating_sub(1),
            )
            .text("Definition"),
        );
        let selected = definitions.get(self.definition_index);
        if self.definition_key != Some((revision, self.definition_index)) {
            self.definition = selected.map_or_else(DefinitionForm::default, |definition| {
                DefinitionForm::load(definition.sprite_id, self.definition_index)
            });
            self.definition_key = Some((revision, self.definition_index));
        }
        let mut edit = self.definition_fields(ui, &definitions);
        ui.separator();
        if let Some(definition) = selected {
            edit = edit.or_else(|| self.part_fields(ui, revision, definition));
        } else {
            ui.label("Insert a sprite definition before adding tile parts.");
        }
        if let Some(edit) = edit {
            match edit {
                Ok(edit) => self.apply_edit(&edit),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
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
                .add_enabled(can_undo, egui::Button::new("Undo"))
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(can_redo, egui::Button::new("Redo"))
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(!self.persistence.is_running(), egui::Button::new("Save"))
                .clicked();
            ui.label(if modified { "Modified" } else { "Saved" });
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
            self.invalidate();
        }
    }

    fn apply_edit(&mut self, edit: &OverworldAppearanceDocumentEdit) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) =
            controller.apply_edits(controller.revision(), std::slice::from_ref(edit))
        {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    fn clamp_indices(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let definitions = &controller.value().definitions;
        self.definition_index = clamp(self.definition_index, definitions.len());
        let part_len = definitions
            .get(self.definition_index)
            .map_or(0, |definition| definition.parts.len());
        self.part_index = clamp(self.part_index, part_len);
        self.definition.insert_index = self.definition.insert_index.min(definitions.len());
        self.definition.move_before = clamp(self.definition.move_before, definitions.len());
        self.part.insert_index = self.part.insert_index.min(part_len);
    }

    fn invalidate(&mut self) {
        self.definition_key = None;
        self.part_key = None;
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved overworld appearances")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved appearance changes?");
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
            egui::Window::new("Overworld appearance editor error")
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
        self.controller = None;
        self.pending_close = None;
        self.invalidate();
    }
}

fn clamp(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}
