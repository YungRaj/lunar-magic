use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    metadata_editor_forms::{LevelNameForm, METADATA_SUBMAP_NAMES, PlayerStartForm, SettingsForm},
};
use eframe::egui;
use lm_app::OverworldMetadataController;
use lm_overworld::{MetadataEdit, OverworldMetadata};

mod panels;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Panel {
    #[default]
    LevelNames,
    PlayerStarts,
    SubmapSettings,
}

#[derive(Default)]
pub(crate) struct MetadataEditor {
    controller: Option<OverworldMetadataController>,
    panel: Panel,
    name_index: usize,
    name: LevelNameForm,
    name_key: Option<(u64, usize)>,
    start_index: usize,
    start: PlayerStartForm,
    start_key: Option<(u64, usize)>,
    settings_index: usize,
    settings: SettingsForm,
    settings_key: Option<(u64, usize)>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl MetadataEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_overworld_metadata_document() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(OverworldMetadata::MAX_FILE_LEN).unwrap_or(u64::MAX),
            "overworld metadata document",
        )]) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for metadata loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for metadata persistence to finish before closing".into());
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
                    .ok_or_else(|| "metadata loader returned no file".to_string())?;
                OverworldMetadataController::decode(path, &bytes).map_err(|error| error.to_string())
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
            egui::Window::new("Portable Overworld Metadata Editor")
                .default_size([720.0, 540.0])
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.panel, Panel::LevelNames, "Level names");
            ui.selectable_value(&mut self.panel, Panel::PlayerStarts, "Player starts");
            ui.selectable_value(&mut self.panel, Panel::SubmapSettings, "Submap settings");
        });
        ui.separator();
        let edit = match self.panel {
            Panel::LevelNames => self.show_level_names(ui),
            Panel::PlayerStarts => self.show_player_starts(ui),
            Panel::SubmapSettings => self.show_submap_settings(ui),
        };
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

    fn apply_edit(&mut self, edit: &MetadataEdit) {
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
        self.name_index = clamp(self.name_index, controller.metadata().level_names.len());
        self.start_index = clamp(self.start_index, controller.metadata().player_starts.len());
        self.settings_index = clamp(
            self.settings_index,
            controller.metadata().submap_settings.len(),
        );
    }

    fn invalidate(&mut self) {
        self.name_key = None;
        self.start_key = None;
        self.settings_key = None;
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved overworld metadata")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved metadata changes?");
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
            egui::Window::new("Metadata editor error")
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

fn selector(ui: &mut egui::Ui, index: &mut usize, len: usize, label: &str) {
    ui.add(egui::Slider::new(index, 0..=len.saturating_sub(1)).text(label));
}

fn clamp(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}

fn submap_combo(ui: &mut egui::Ui, value: &mut usize, id: &str) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(METADATA_SUBMAP_NAMES[(*value).min(6)])
        .show_ui(ui, |ui| {
            for (index, name) in METADATA_SUBMAP_NAMES.into_iter().enumerate() {
                ui.selectable_value(value, index, name);
            }
        });
}

fn edit_buttons(ui: &mut egui::Ui, can_remove: bool, noun: &str) -> (bool, bool) {
    let mut result = (false, false);
    ui.horizontal(|ui| {
        result.0 = ui.button(format!("Upsert {noun}")).clicked();
        result.1 = ui
            .add_enabled(can_remove, egui::Button::new("Remove selected"))
            .clicked();
    });
    result
}
