mod clipboard;
mod form_state;
mod open_workflow;
mod panels;
mod persistence;

use crate::{
    animation_modes, dialogs, document_loader::DocumentLoader,
    document_persistence::DocumentPersistence, exanimation_form, native_clipboard,
};
use clipboard::PasteTarget;
use eframe::egui;
use exanimation_form::{GlobalForm, RecordForm};
use lm_app::{ExAnimationDocumentController, ExtendedUiTextKey as Key, LocalizationCatalog};
use persistence::decode_document;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

struct PendingOpen {
    animation: PathBuf,
    bytes: Vec<u8>,
    modes: [bool; 256],
    maximum_records: String,
}

struct ExAnimationDocument {
    controller: ExAnimationDocumentController,
    modes: [bool; 256],
}

#[derive(Default)]
pub(crate) struct ExAnimationEditor {
    document: Option<ExAnimationDocument>,
    pending_open: Option<PendingOpen>,
    selected_record: usize,
    selected_frame: usize,
    loaded_revision: Option<u64>,
    loaded_record: Option<usize>,
    global: GlobalForm,
    record: RecordForm,
    trigger_index: usize,
    trigger_enabled: bool,
    trigger_value: String,
    record_editable: bool,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    paste_target: Option<PasteTarget>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl ExAnimationEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.document.is_some() || self.pending_open.is_some() || self.loader.is_running()
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for ExAnimation loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for ExAnimation persistence to finish before closing".into());
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
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        self.poll_open_load(context);
        if let Some(document) = self.document.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, &mut document.controller)
        {
            self.error = Some(error);
        }
        self.show_open_configuration(context, catalog);
        if self.document.is_some() {
            self.load_forms();
            egui::Window::new(text(catalog, Key::ExAnimationDocumentTitle))
                .default_size([820.0, 620.0])
                .show(context, |ui| self.contents(ui, catalog));
        }
        let approved = self.show_close_confirmation(context, catalog);
        self.show_error(context, catalog);
        approved
    }

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        self.toolbar(ui, catalog);
        if let Some(text) = pasted {
            match self.paste_target.take() {
                Some(PasteTarget::Record) => self.paste_record(&text),
                Some(PasteTarget::Frame) => self.paste_frame(&text),
                None => {}
            }
        }
        ui.separator();
        ui.columns(2, |columns| {
            self.record_list(&mut columns[0], catalog);
            self.properties(&mut columns[1], catalog);
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let can_undo = document.controller.can_undo();
        let can_redo = document.controller.can_redo();
        let modified = document.controller.is_modified();
        let mut undo = false;
        let mut redo = false;
        let mut save_requested = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_undo,
                    egui::Button::new(text(catalog, Key::ExAnimationDocumentUndo)),
                )
                .clicked()
            {
                undo = true;
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(text(catalog, Key::ExAnimationDocumentRedo)),
                )
                .clicked()
            {
                redo = true;
            }
            if ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, Key::ExAnimationDocumentSave)),
                )
                .clicked()
            {
                save_requested = true;
            }
            if ui
                .button(text(catalog, Key::NativeAssetsAnimationCopyRecord))
                .clicked()
                && let Some(record) = document
                    .controller
                    .value()
                    .animation
                    .records
                    .get(self.selected_record)
            {
                match native_clipboard::encode_exanimation_record(record) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .button(text(catalog, Key::NativeAssetsAnimationPasteRecord))
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Record);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            ui.label(text(
                catalog,
                if modified {
                    Key::ExAnimationDocumentModified
                } else {
                    Key::ExAnimationDocumentSaved
                },
            ));
        });
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let revision = document.controller.revision();
        let changed = (undo && document.controller.undo(revision).is_ok())
            || (redo && document.controller.redo(revision).is_ok());
        if save_requested {
            if let Err(error) = self.persistence.begin(&mut document.controller) {
                self.error = Some(error);
            }
        }
        if changed {
            self.invalidate_forms();
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
        egui::Window::new(text(catalog, Key::ExAnimationDocumentDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(text(catalog, Key::ExAnimationDocumentDiscardNotice));
                ui.horizontal(|ui| {
                    if ui
                        .button(text(catalog, Key::ExAnimationDocumentCancel))
                        .clicked()
                    {
                        self.pending_close = None;
                    }
                    if ui
                        .button(text(catalog, Key::ExAnimationDocumentDiscard))
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
            egui::Window::new(text(catalog, Key::ExAnimationDocumentErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, Key::ExAnimationDocumentOk))
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
        self.pending_close = None;
        self.paste_target = None;
        self.invalidate_forms();
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{CompactExAnimation, CompactExAnimationFile, ExAnimationRecord};

    #[test]
    fn complete_portable_exanimation_surface_uses_every_document_key() {
        let sources = [
            include_str!("exanimation_editor.rs"),
            include_str!("exanimation_editor/open_workflow.rs"),
            include_str!("exanimation_editor/clipboard.rs"),
            include_str!("exanimation_editor/panels.rs"),
        ]
        .join("\n");
        for key in Key::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("ExAnimationDocument"))
        {
            assert!(
                sources.contains(&format!("Key::{key:?}")),
                "missing portable ExAnimation label {key:?}"
            );
        }
    }

    #[test]
    fn complete_portable_exanimation_surface_has_no_literal_widget_text() {
        let sources = [
            include_str!("exanimation_editor.rs"),
            include_str!("exanimation_editor/open_workflow.rs"),
            include_str!("exanimation_editor/clipboard.rs"),
            include_str!("exanimation_editor/panels.rs"),
        ]
        .join("\n");
        for literal_widget in [
            "Window::new(\"",
            "ui.heading(\"",
            "ui.label(\"",
            "ui.button(\"",
            "Button::new(\"",
            ".prefix(\"",
            ".text(\"",
        ] {
            assert!(
                !sources.contains(literal_widget),
                "portable ExAnimation editor regressed to fixed widget text: {literal_widget}"
            );
        }
    }

    fn editor() -> ExAnimationEditor {
        let modes = [false; 256];
        let original =
            ExAnimationRecord::new(1, 1, 0, 0x1111, false, &[1, 2, 3, 4], false).unwrap();
        let file = CompactExAnimationFile {
            source_slot: 3,
            animation: CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: vec![original],
            },
        };
        let bytes = file.encode(&modes).unwrap();
        ExAnimationEditor {
            document: Some(ExAnimationDocument {
                controller: ExAnimationDocumentController::decode(
                    "animation.lmexan".into(),
                    &bytes,
                    8,
                    &modes,
                )
                .unwrap(),
                modes,
            }),
            ..ExAnimationEditor::default()
        }
    }

    #[test]
    fn typed_record_paste_is_one_revision_and_reloads_forms() {
        let replacement = ExAnimationRecord::new(1, 0, 0, 0x2222, true, &[3, 4], false).unwrap();
        let mut editor = editor();
        editor.loaded_revision = Some(0);
        editor.loaded_record = Some(0);
        editor.paste_record(&native_clipboard::encode_exanimation_record(&replacement).unwrap());
        let document = editor.document.as_ref().unwrap();
        assert_eq!(document.controller.revision(), 1);
        assert_eq!(
            document.controller.value().animation.records[0],
            replacement
        );
        assert_eq!(editor.loaded_revision, None);
        assert_eq!(editor.loaded_record, None);
    }

    #[test]
    fn typed_frame_paste_replaces_only_the_selected_frame() {
        let mut editor = editor();
        editor.selected_frame = 1;
        let replacement = lm_graphics::ExAnimationFrame {
            source_words: vec![0x4567],
        };

        editor.paste_frame(&native_clipboard::encode_exanimation_frame(&replacement).unwrap());

        let document = editor.document.as_ref().unwrap();
        let frames = document.controller.record_frames(0).unwrap();
        assert_eq!(document.controller.revision(), 1);
        assert_eq!(frames[0].source_words, vec![0x0201]);
        assert_eq!(frames[1], replacement);
        assert_eq!(editor.loaded_revision, None);
        assert_eq!(editor.loaded_record, None);
    }

    #[test]
    fn wrong_clipboard_domain_does_not_mutate_the_document() {
        let mut editor = editor();
        let text = native_clipboard::encode_palette_color(lm_graphics::Bgr555(0x1234)).unwrap();
        let before = editor.document.as_ref().unwrap().controller.value().clone();

        editor.paste_record(&text);

        let document = editor.document.as_ref().unwrap();
        assert_eq!(document.controller.revision(), 0);
        assert_eq!(document.controller.value(), &before);
        assert!(editor.error.is_some());
    }
}
