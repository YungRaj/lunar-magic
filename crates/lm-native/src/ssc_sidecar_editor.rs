use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    ssc_sidecar_editor_form::{SscSourceForm, diagnostic},
};
use eframe::egui;
use lm_app::SscSidecarController;
use lm_level::MAX_SSC_SOURCE_LEN;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct SscSidecarEditor {
    controller: Option<SscSidecarController>,
    form: SscSourceForm,
    loaded_revision: Option<u64>,
    entry_index: usize,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
    resolved: Option<lm_level::SscResolvedTable>,
}

impl SscSidecarEditor {
    pub(crate) fn resolved(&self) -> Option<&lm_level::SscResolvedTable> {
        self.resolved.as_ref()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_ssc_sidecar() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(MAX_SSC_SOURCE_LEN).unwrap_or(u64::MAX),
            "SSC sidecar",
        )]) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() || self.persistence.is_running() {
            self.error = Some("wait for SSC I/O to finish before closing".into());
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
        self.poll_io(context);
        if self.controller.is_some() {
            self.load_form();
            egui::Window::new("Lossless SSC Custom-Sprite Metadata")
                .default_size([840.0, 680.0])
                .vscroll(true)
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn poll_io(&mut self, context: &egui::Context) {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(|mut loaded| {
                let (path, bytes) = loaded
                    .files
                    .pop()
                    .ok_or_else(|| "SSC loader returned no file".to_string())?;
                SscSidecarController::decode(path, &bytes).map_err(|error| error.to_string())
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
    }

    fn load_form(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        if self.loaded_revision != Some(controller.revision()) {
            self.form = SscSourceForm::load(controller.value());
            self.resolved = Some(lm_level::SscResolvedTable::from_sidecar(controller.value()));
            self.loaded_revision = Some(controller.revision());
            self.entry_index = self
                .entry_index
                .min(controller.value().entries().len().saturating_sub(1));
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let source_len = controller.value().source().len();
        let entry_count = controller.value().entries().len();
        let entry_diagnostic = controller
            .value()
            .entries()
            .get(self.entry_index)
            .map(diagnostic);
        ui.label(format!(
            "Lossless source: {source_len} bytes; valid metadata records: {entry_count}"
        ));
        ui.add(
            egui::TextEdit::multiline(&mut self.form.bytes)
                .desired_rows(18)
                .code_editor(),
        );
        if ui.button("Replace complete lossless source").clicked() {
            self.replace_form();
        }
        ui.separator();
        ui.heading("Recovered-record diagnostics");
        ui.add(
            egui::Slider::new(&mut self.entry_index, 0..=entry_count.saturating_sub(1))
                .text("Parsed record"),
        );
        ui.label(entry_diagnostic.unwrap_or_else(|| "No valid metadata records.".into()));
    }

    fn replace_form(&mut self) {
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

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let mut history = None;
        let mut save = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(controller.can_undo(), egui::Button::new("Undo"))
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(controller.can_redo(), egui::Button::new("Redo"))
                .clicked()
            {
                history = Some(false);
            }
            save = ui
                .add_enabled(!self.persistence.is_running(), egui::Button::new("Save"))
                .clicked();
            ui.label(if controller.is_modified() {
                "Modified"
            } else {
                "Saved"
            });
        });
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Some(undo) = history {
            let result = if undo {
                controller.undo(controller.revision())
            } else {
                controller.redo(controller.revision())
            };
            match result {
                Ok(true) => self.loaded_revision = None,
                Ok(false) => {}
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        if save && let Err(error) = self.persistence.begin(controller) {
            self.error = Some(error);
        }
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved SSC sidecar")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved custom-sprite metadata changes?");
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
            egui::Window::new("SSC sidecar error")
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
        self.loaded_revision = None;
        self.pending_close = None;
        self.resolved = None;
    }
}
