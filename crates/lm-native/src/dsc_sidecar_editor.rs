use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    dsc_sidecar_editor_form::{DscSourceForm, diagnostic},
};
use eframe::egui;
use lm_app::DscSidecarController;
use lm_level::{DscDescriptionStyle, DscResolvedTable, MAX_DSC_SOURCE_LEN};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct DscSidecarEditor {
    controller: Option<DscSidecarController>,
    resolved: Option<DscResolvedTable>,
    form: DscSourceForm,
    loaded_revision: Option<u64>,
    entry_index: usize,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl DscSidecarEditor {
    pub(crate) fn resolved(&self) -> Option<&DscResolvedTable> {
        self.resolved.as_ref()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_dsc_sidecar() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(MAX_DSC_SOURCE_LEN).unwrap_or(u64::MAX),
            "DSC sidecar",
        )]) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for DSC loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for DSC persistence to finish before closing".into());
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
                    .ok_or_else(|| "DSC loader returned no file".to_string())?;
                DscSidecarController::decode(path, &bytes).map_err(|error| error.to_string())
            }) {
                Ok(controller) => {
                    self.controller = Some(controller);
                    self.loaded_revision = None;
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
            self.load_form();
            egui::Window::new("Lossless DSC Sidecar Editor")
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
        if self.loaded_revision != Some(controller.revision()) {
            self.form = DscSourceForm::load(controller.value());
            self.resolved = Some(DscResolvedTable::from_sidecar(
                controller.value(),
                DscDescriptionStyle {
                    background: 0,
                    detail: 0,
                    foreground: 0,
                    mode: 0,
                },
            ));
            self.loaded_revision = Some(controller.revision());
            self.entry_index = self
                .entry_index
                .min(controller.value().entries().len().saturating_sub(1));
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        let (source_len, entry_count, entry_diagnostic) = {
            let Some(controller) = self.controller.as_ref() else {
                return;
            };
            let value = controller.value();
            (
                value.source().len(),
                value.entries().len(),
                value.entries().get(self.entry_index).map(diagnostic),
            )
        };
        ui.label(format!(
            "Lossless source: {source_len} bytes; valid parsed records: {entry_count}"
        ));
        ui.label(
            "Complete source bytes (malformed lines, BOM, line endings, and non-UTF-8 retained):",
        );
        ui.add(
            egui::TextEdit::multiline(&mut self.form.bytes)
                .desired_rows(16)
                .code_editor(),
        );
        if ui.button("Replace complete lossless source").clicked() {
            match self.form.parse() {
                Ok(bytes) => {
                    let Some(controller) = self.controller.as_mut() else {
                        return;
                    };
                    if let Err(error) = controller.replace_source(controller.revision(), &bytes) {
                        self.error = Some(error.to_string());
                    } else {
                        self.loaded_revision = None;
                    }
                }
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        ui.heading("Read-only recovered-record diagnostics");
        ui.add(
            egui::Slider::new(&mut self.entry_index, 0..=entry_count.saturating_sub(1))
                .text("Parsed record"),
        );
        if let Some(entry_diagnostic) = entry_diagnostic {
            ui.label(entry_diagnostic);
        } else {
            ui.label("No valid recovered records; all source bytes remain preserved.");
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
            self.loaded_revision = None;
        }
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved DSC sidecar")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved lossless source changes?");
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
            egui::Window::new("DSC sidecar error")
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
        self.resolved = None;
        self.loaded_revision = None;
        self.pending_close = None;
    }
}
