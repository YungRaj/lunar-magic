use crate::{
    appearance_editor_form::{AppearanceForm, SOURCE_NAMES},
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
};
use eframe::egui;
use lm_app::{EntityAppearanceDocumentController, EntityAppearanceDocumentEdit};
use lm_level::EntityAppearanceFile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct AppearanceEditor {
    controller: Option<EntityAppearanceDocumentController>,
    index: usize,
    form: AppearanceForm,
    form_key: Option<(u64, usize)>,
    insert_index: usize,
    move_before: usize,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl AppearanceEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_entity_appearance_document() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(EntityAppearanceFile::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "entity appearance document",
        )]) {
            self.error = Some(error);
        }
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
            match result.and_then(|mut loaded| {
                let (path, bytes) = loaded
                    .files
                    .pop()
                    .ok_or_else(|| "appearance loader returned no file".to_string())?;
                EntityAppearanceDocumentController::decode(path, &bytes)
                    .map_err(|error| error.to_string())
            }) {
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
            egui::Window::new("Portable Entity Appearance Editor")
                .default_size([650.0, 500.0])
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let len = controller.value().appearances.len();
        ui.label(format!("Painter-ordered records: {len}"));
        ui.add(egui::Slider::new(&mut self.index, 0..=len.saturating_sub(1)).text("Selected"));
        if self.form_key != Some((controller.revision(), self.index)) {
            self.form = controller
                .value()
                .appearances
                .get(self.index)
                .copied()
                .map_or_else(AppearanceForm::default, AppearanceForm::load);
            self.form_key = Some((controller.revision(), self.index));
        }
        appearance_fields(ui, &mut self.form);
        ui.separator();
        let mut edit = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(len > 0, egui::Button::new("Replace selected"))
                .clicked()
            {
                edit = Some(
                    self.form
                        .parse()
                        .map(|value| EntityAppearanceDocumentEdit::Replace {
                            index: self.index,
                            value,
                        }),
                );
            }
            if ui
                .add_enabled(len > 0, egui::Button::new("Remove selected"))
                .clicked()
            {
                edit = Some(Ok(EntityAppearanceDocumentEdit::Remove {
                    index: self.index,
                }));
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.insert_index).range(0..=len));
            if ui.button("Insert form before index").clicked() {
                edit = Some(
                    self.form
                        .parse()
                        .map(|value| EntityAppearanceDocumentEdit::Insert {
                            index: self.insert_index,
                            value,
                        }),
                );
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.move_before).range(0..=len.saturating_sub(1)));
            if ui
                .add_enabled(len > 1, egui::Button::new("Move selected before index"))
                .clicked()
            {
                edit = Some(Ok(EntityAppearanceDocumentEdit::MoveBefore {
                    from: self.index,
                    before: self.move_before,
                }));
            }
        });
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

    fn apply_edit(&mut self, edit: &EntityAppearanceDocumentEdit) {
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
        let len = controller.value().appearances.len();
        self.index = clamp(self.index, len);
        self.insert_index = self.insert_index.min(len);
        self.move_before = clamp(self.move_before, len);
    }

    fn invalidate(&mut self) {
        self.form_key = None;
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved entity appearances")
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
            egui::Window::new("Appearance editor error")
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

fn appearance_fields(ui: &mut egui::Ui, form: &mut AppearanceForm) {
    egui::ComboBox::from_id_salt("appearance-source-kind")
        .selected_text(SOURCE_NAMES[form.source_kind.min(2)])
        .show_ui(ui, |ui| {
            for (index, name) in SOURCE_NAMES.into_iter().enumerate() {
                ui.selectable_value(&mut form.source_kind, index, name);
            }
        });
    for (label, field) in [
        ("Source ID (hex)", &mut form.source_id),
        ("Tile index (hex)", &mut form.tile_index),
        ("X offset (decimal)", &mut form.x),
        ("Y offset (decimal)", &mut form.y),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(field);
        });
    }
    ui.add(egui::Slider::new(&mut form.palette_index, 0..=7).text("Palette row"));
    ui.horizontal(|ui| {
        ui.checkbox(&mut form.x_flip, "Horizontal flip");
        ui.checkbox(&mut form.y_flip, "Vertical flip");
    });
}

fn clamp(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}
