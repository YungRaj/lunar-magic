use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    dsc_sidecar_editor_form::{DscSourceForm, diagnostic},
};
use eframe::egui;
use lm_app::{DscSidecarController, ExtendedUiTextKey, LocalizationCatalog};
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

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
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
            egui::Window::new(text(catalog, ExtendedUiTextKey::DscEditorTitle))
                .default_size([800.0, 650.0])
                .vscroll(true)
                .show(context, |ui| self.contents(ui, catalog));
        }
        let approved = self.show_close_confirmation(context, catalog);
        self.show_error(context, catalog);
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

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        self.toolbar(ui, catalog);
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
        ui.label(
            text(catalog, ExtendedUiTextKey::DscSourceSummaryFormat)
                .replace("{bytes}", &source_len.to_string())
                .replace("{records}", &entry_count.to_string()),
        );
        ui.label(text(catalog, ExtendedUiTextKey::DscSourceNotice));
        ui.add(
            egui::TextEdit::multiline(&mut self.form.bytes)
                .desired_rows(16)
                .code_editor(),
        );
        if ui
            .button(text(catalog, ExtendedUiTextKey::DscReplaceSource))
            .clicked()
        {
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
        ui.heading(text(catalog, ExtendedUiTextKey::DscDiagnosticsHeading));
        ui.add(
            egui::Slider::new(&mut self.entry_index, 0..=entry_count.saturating_sub(1))
                .text(text(catalog, ExtendedUiTextKey::DscParsedRecord)),
        );
        if let Some(entry_diagnostic) = entry_diagnostic {
            ui.label(entry_diagnostic);
        } else {
            ui.label(text(catalog, ExtendedUiTextKey::DscNoRecoveredRecords));
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
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
                .add_enabled(
                    can_undo,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::DscUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::DscRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::DscSave)),
                )
                .clicked();
            ui.label(text(
                catalog,
                if modified {
                    ExtendedUiTextKey::DscModified
                } else {
                    ExtendedUiTextKey::DscSaved
                },
            ));
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

    fn show_close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(text(catalog, ExtendedUiTextKey::DscDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(text(catalog, ExtendedUiTextKey::DscUnsavedNotice));
                ui.horizontal(|ui| {
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::DscCancel))
                        .clicked()
                    {
                        self.pending_close = None;
                    }
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::DscDiscard))
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
            egui::Window::new(text(catalog, ExtendedUiTextKey::DscErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button(text(catalog, ExtendedUiTextKey::DscOk)).clicked() {
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

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_dsc_sidecar_form_uses_every_typed_key_and_live_catalog() {
        let source = include_str!("dsc_sidecar_editor.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("Dsc"))
        {
            assert!(
                source.contains(&format!("ExtendedUiTextKey::{key:?}")),
                "missing DSC label {key:?}"
            );
        }
        for literal in [
            "Window::new(\"Lossless DSC Sidecar Editor\")",
            "Window::new(\"Unsaved DSC sidecar\")",
            "Window::new(\"DSC sidecar error\")",
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
    fn lossless_source_replacement_is_revisioned_and_undoable() {
        let original = b"\xef\xbb\xbf0001\t20\tfirst\r\nmalformed\xff\n";
        let replacement = b"0002\t21\tsecond\n# retained comment\r\n";
        let mut controller = DscSidecarController::decode("sprites.dsc".into(), original).unwrap();
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
