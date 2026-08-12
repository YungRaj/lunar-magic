use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    osc_sidecar_editor_form::{OscSourceForm, diagnostic},
};
use eframe::egui;
use lm_app::{ExtendedUiTextKey, LocalizationCatalog, OscSidecarController};
use lm_level::MAX_OSC_SOURCE_LEN;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct OscSidecarEditor {
    controller: Option<OscSidecarController>,
    form: OscSourceForm,
    loaded_revision: Option<u64>,
    entry_index: usize,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
    resolved: Option<lm_level::OscResolvedTable>,
}

impl OscSidecarEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn resolved(&self) -> Option<&lm_level::OscResolvedTable> {
        self.resolved.as_ref()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_osc_sidecar() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(MAX_OSC_SOURCE_LEN).unwrap_or(u64::MAX),
            "OSC sidecar",
        )]) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() || self.persistence.is_running() {
            self.error = Some("wait for OSC I/O to finish before closing".into());
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

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        self.poll_io(context);
        if self.controller.is_some() {
            self.load_form();
            egui::Window::new(text(catalog, ExtendedUiTextKey::OscEditorTitle))
                .default_size([840.0, 680.0])
                .vscroll(true)
                .show(context, |ui| self.contents(ui, catalog));
        }
        let approved = self.show_close_confirmation(context, catalog);
        self.show_error(context, catalog);
        approved
    }

    fn poll_io(&mut self, context: &egui::Context) {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(|mut loaded| {
                let (path, bytes) = loaded
                    .files
                    .pop()
                    .ok_or_else(|| "OSC loader returned no file".to_string())?;
                OscSidecarController::decode(path, &bytes).map_err(|error| error.to_string())
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
            self.form = OscSourceForm::load(controller.value());
            self.resolved = Some(lm_level::OscResolvedTable::from_sidecar(controller.value()));
            self.loaded_revision = Some(controller.revision());
            self.entry_index = self
                .entry_index
                .min(controller.value().entries().len().saturating_sub(1));
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        self.toolbar(ui, catalog);
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
        ui.label(
            text(catalog, ExtendedUiTextKey::OscSourceSummaryFormat)
                .replace("{bytes}", &source_len.to_string())
                .replace("{records}", &entry_count.to_string()),
        );
        ui.add(
            egui::TextEdit::multiline(&mut self.form.bytes)
                .desired_rows(18)
                .code_editor(),
        );
        if ui
            .button(text(catalog, ExtendedUiTextKey::OscReplaceSource))
            .clicked()
        {
            self.replace_form();
        }
        ui.separator();
        ui.heading(text(catalog, ExtendedUiTextKey::OscDiagnosticsHeading));
        ui.add(
            egui::Slider::new(&mut self.entry_index, 0..=entry_count.saturating_sub(1))
                .text(text(catalog, ExtendedUiTextKey::OscParsedRecord)),
        );
        ui.label(
            entry_diagnostic
                .unwrap_or_else(|| text(catalog, ExtendedUiTextKey::OscNoMetadataRecords)),
        );
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

    fn toolbar(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let mut history = None;
        let mut save = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    controller.can_undo(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::OscUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    controller.can_redo(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::OscRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            save = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::OscSave)),
                )
                .clicked();
            ui.label(text(
                catalog,
                if controller.is_modified() {
                    ExtendedUiTextKey::OscModified
                } else {
                    ExtendedUiTextKey::OscSaved
                },
            ));
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

    fn show_close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(text(catalog, ExtendedUiTextKey::OscDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(text(catalog, ExtendedUiTextKey::OscUnsavedNotice));
                ui.horizontal(|ui| {
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::OscCancel))
                        .clicked()
                    {
                        self.pending_close = None;
                    }
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::OscDiscard))
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
            egui::Window::new(text(catalog, ExtendedUiTextKey::OscErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button(text(catalog, ExtendedUiTextKey::OscOk)).clicked() {
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

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_osc_sidecar_form_uses_every_typed_key_and_live_catalog() {
        let source = include_str!("osc_sidecar_editor.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("Osc"))
        {
            assert!(
                source.contains(&format!("ExtendedUiTextKey::{key:?}")),
                "missing OSC label {key:?}"
            );
        }
        for literal in [
            "Window::new(\"Lossless OSC Custom-Object Metadata\")",
            "Window::new(\"Unsaved OSC sidecar\")",
            "Window::new(\"OSC sidecar error\")",
            "Button::new(\"Undo\")",
            "Button::new(\"Save\")",
            "ui.button(\"Replace complete lossless source\")",
        ] {
            assert!(
                !source.contains(literal),
                "fixed-English control: {literal}"
            );
        }
        assert!(
            include_str!("application/windows.rs")
                .contains(".show(context, self.app.localization())")
        );
    }

    #[test]
    fn lossless_custom_object_metadata_is_revisioned_and_undoable() {
        let original = b"\xef\xbb\xbf10\t2\t0\tfirst\r\nmalformed\xff\n";
        let replacement = b"11\t2\t0\tsecond\n# retained comment\r\n";
        let mut controller = OscSidecarController::decode("objects.osc".into(), original).unwrap();
        controller
            .replace_source(controller.revision(), replacement)
            .unwrap();
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.value().source(), replacement);
        assert!(controller.undo(controller.revision()).unwrap());
        assert_eq!(controller.value().source(), original);
        assert!(controller.redo(controller.revision()).unwrap());
        assert_eq!(controller.value().source(), replacement);
    }
}
