use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    mwl_editor_form::{MwlForm, SECTION_NAMES},
};
use eframe::egui;
use lm_app::{MwlDocumentController, MwlDocumentEdit};
use lm_level::MwlFile;

mod optional_import;
mod optional_panel;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingLoad {
    Open,
    OptionalInterpretation { maximum_records: usize },
    OptionalAssets { maximum_records: usize },
}

#[derive(Clone, Debug)]
struct OptionalAssetsInterpretation {
    maximum_records: usize,
    modes: [bool; 256],
}

pub(crate) struct MwlEditor {
    controller: Option<MwlDocumentController>,
    form: MwlForm,
    loaded_header_revision: Option<u64>,
    loaded_section_key: Option<(u64, usize)>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    pending_load: Option<PendingLoad>,
    optional_interpretation: Option<OptionalAssetsInterpretation>,
    optional_maximum_records: String,
    optional_panel: optional_panel::MwlOptionalAssetsPanel,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl Default for MwlEditor {
    fn default() -> Self {
        Self {
            controller: None,
            form: MwlForm::default(),
            loaded_header_revision: None,
            loaded_section_key: None,
            error: None,
            pending_close: None,
            pending_load: None,
            optional_interpretation: None,
            optional_maximum_records: "32".into(),
            optional_panel: optional_panel::MwlOptionalAssetsPanel::default(),
            persistence: DocumentPersistence::default(),
            loader: DocumentLoader::default(),
        }
    }
}

impl MwlEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_mwl_document() else {
            return;
        };
        match self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(MwlFile::MAX_FILE_BYTES).unwrap_or(u64::MAX),
            "MWL document",
        )]) {
            Ok(()) => self.pending_load = Some(PendingLoad::Open),
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for MWL loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for MWL persistence to finish before closing".into());
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
        self.poll_load(context);
        if let Some(controller) = self.controller.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, controller)
        {
            self.error = Some(error);
        }
        if self.controller.is_some() {
            self.load_form();
            egui::Window::new("Portable MWL Editor")
                .default_size([800.0, 650.0])
                .vscroll(true)
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn load_form(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let revision = controller.revision();
        if self.loaded_header_revision != Some(revision) {
            let section_index = self.form.section_index;
            self.form = MwlForm::load_header(controller.value());
            self.form.section_index = section_index.min(MwlFile::SECTION_COUNT - 1);
            self.loaded_header_revision = Some(revision);
            self.loaded_section_key = None;
        }
        if self.loaded_section_key != Some((revision, self.form.section_index)) {
            self.form
                .load_section(controller.value(), self.form.section_index);
            self.loaded_section_key = Some((revision, self.form.section_index));
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        let version = self
            .controller
            .as_ref()
            .map_or(0, |controller| controller.value().version);
        ui.label(format!("Preserved MWL version: {version:04X}"));
        text_field(ui, "Flags (hex)", &mut self.form.flags);
        ui.label("Attribution (exactly 48 hexadecimal bytes):");
        ui.add(
            egui::TextEdit::multiline(&mut self.form.attribution)
                .desired_rows(3)
                .code_editor(),
        );
        text_field(
            ui,
            "Level number (hex; blank if header is not exact 64 bytes)",
            &mut self.form.level_number,
        );
        if ui.button("Apply recovered MWL header fields").clicked() {
            match self.form.header_edits() {
                Ok(edits) => self.apply_edits(&edits),
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        self.optional_assets_import_controls(ui);
        self.show_optional_assets_panel(ui);
        ui.separator();
        let previous_section = self.form.section_index;
        egui::ComboBox::from_id_salt("mwl-section")
            .selected_text(SECTION_NAMES[self.form.section_index])
            .show_ui(ui, |ui| {
                for (index, name) in SECTION_NAMES.into_iter().enumerate() {
                    ui.selectable_value(&mut self.form.section_index, index, name);
                }
            });
        if previous_section != self.form.section_index {
            self.loaded_section_key = None;
            self.load_form();
        }
        let section_len = self.controller.as_ref().map_or(0, |controller| {
            controller.value().sections[self.form.section_index]
                .bytes
                .len()
        });
        ui.label(format!("Current section length: {section_len} bytes"));
        ui.label("Opaque section bytes:");
        ui.add(
            egui::TextEdit::multiline(&mut self.form.section_bytes)
                .desired_rows(14)
                .code_editor(),
        );
        if ui.button("Replace selected section atomically").clicked() {
            match self.form.section_edit() {
                Ok(edit) => self.apply_edits(std::slice::from_ref(&edit)),
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

    fn apply_edits(&mut self, edits: &[MwlDocumentEdit]) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) = controller.apply_edits(controller.revision(), edits) {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    fn invalidate(&mut self) {
        self.loaded_header_revision = None;
        self.loaded_section_key = None;
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved MWL document")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved MWL changes?");
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
            egui::Window::new("MWL editor error")
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
        self.pending_load = None;
        self.optional_interpretation = None;
        self.optional_panel.invalidate();
        self.invalidate();
    }
}

fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}
