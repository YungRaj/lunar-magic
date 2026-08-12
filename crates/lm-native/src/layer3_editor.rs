use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    layer3_editor_form::Layer3Form,
    native_clipboard,
};
use eframe::egui;
use lm_app::{ExtendedUiTextKey, Layer3DocumentController, LocalizationCatalog};
use lm_level::{Layer3Edit, Layer3File};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    Tilemap,
    Remap,
}

#[derive(Default)]
pub(crate) struct Layer3Editor {
    controller: Option<Layer3DocumentController>,
    form: Layer3Form,
    loaded_revision: Option<u64>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    paste_target: Option<PasteTarget>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl Layer3Editor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_layer3_document() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(Layer3File::MAX_ENCODED_LEN).unwrap_or(u64::MAX),
            "Layer 3 document",
        )]) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for Layer 3 loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for Layer 3 persistence to finish before closing".into());
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
                    .ok_or_else(|| "Layer 3 loader returned no file".to_string())?;
                Layer3DocumentController::decode(path, &bytes).map_err(|error| error.to_string())
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
            egui::Window::new(text(catalog, ExtendedUiTextKey::Layer3DocumentEditorTitle))
                .default_size([760.0, 650.0])
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
            self.form = Layer3Form::load(&controller.value().0);
            self.loaded_revision = Some(controller.revision());
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        self.toolbar(ui, catalog);
        ui.separator();
        for (key, value) in [
            ExtendedUiTextKey::Layer3DocumentStartPosition,
            ExtendedUiTextKey::Layer3DocumentTilemapSize,
            ExtendedUiTextKey::Layer3DocumentLiquidType,
            ExtendedUiTextKey::Layer3DocumentRawFlags,
        ]
        .into_iter()
        .zip(self.form.selectors.iter_mut())
        {
            ui.add(egui::Slider::new(value, 0..=u8::MAX).text(text(catalog, key)));
        }
        for (slot, value) in self.form.graphics.iter_mut().enumerate() {
            ui.add(
                egui::Slider::new(value, 0..=0x0fff).text(
                    text(catalog, ExtendedUiTextKey::Layer3DocumentGraphicsFormat)
                        .replace("{slot}", &slot.to_string()),
                ),
            );
        }
        ui.label(text(
            catalog,
            ExtendedUiTextKey::Layer3DocumentReservedNotice,
        ));
        ui.text_edit_singleline(&mut self.form.reserved);
        ui.label(text(
            catalog,
            ExtendedUiTextKey::Layer3DocumentTilemapNotice,
        ));
        ui.add(
            egui::TextEdit::multiline(&mut self.form.tilemap)
                .desired_rows(8)
                .code_editor(),
        );
        ui.label(text(catalog, ExtendedUiTextKey::Layer3DocumentRemapNotice));
        ui.add(
            egui::TextEdit::multiline(&mut self.form.remap)
                .desired_rows(8)
                .code_editor(),
        );
        if let Some(edit) = self.clipboard_controls(ui, catalog) {
            self.apply_edit(edit);
        }
        if ui
            .button(text(catalog, ExtendedUiTextKey::Layer3DocumentApplyAll))
            .clicked()
        {
            match self.form.edits() {
                Ok(edits) => {
                    let Some(controller) = self.controller.as_mut() else {
                        return;
                    };
                    if let Err(error) = controller.apply_edits(controller.revision(), &edits) {
                        self.error = Some(error.to_string());
                    } else {
                        self.loaded_revision = None;
                    }
                }
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn clipboard_controls(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Layer3Edit, String>> {
        let value = self.controller.as_ref()?.value();
        let (tilemap, remap) = (value.0.tilemap.clone(), value.0.remap_commands.clone());
        let mut copy_result = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, ExtendedUiTextKey::Layer3DocumentCopyTilemap))
                .clicked()
            {
                copy_result = Some(native_clipboard::encode_layer3_tilemap(&tilemap));
            }
            if ui
                .button(text(catalog, ExtendedUiTextKey::Layer3DocumentPasteTilemap))
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Tilemap);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            if ui
                .button(text(catalog, ExtendedUiTextKey::Layer3DocumentCopyRemap))
                .clicked()
            {
                copy_result = Some(native_clipboard::encode_layer3_remap(&remap));
            }
            if ui
                .button(text(catalog, ExtendedUiTextKey::Layer3DocumentPasteRemap))
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Remap);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(result) = copy_result {
            match result {
                Ok(text) => ui.ctx().copy_text(text),
                Err(error) => return Some(Err(error)),
            }
        }
        let text = pasted_text(ui)?;
        let target = self.paste_target.take()?;
        Some(match target {
            PasteTarget::Tilemap => {
                native_clipboard::decode_layer3_tilemap(&text).map(Layer3Edit::ReplaceTilemap)
            }
            PasteTarget::Remap => {
                native_clipboard::decode_layer3_remap(&text).map(Layer3Edit::ReplaceRemapCommands)
            }
        })
    }

    fn apply_edit(&mut self, edit: Result<Layer3Edit, String>) {
        let result = edit.and_then(|edit| {
            let controller = self
                .controller
                .as_mut()
                .ok_or_else(|| "Layer 3 document is no longer open".to_string())?;
            controller
                .apply_edits(controller.revision(), &[edit])
                .map_err(|error| error.to_string())
        });
        match result {
            Ok(()) => self.loaded_revision = None,
            Err(error) => self.error = Some(error),
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
                    egui::Button::new(text(catalog, ExtendedUiTextKey::Layer3DocumentUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::Layer3DocumentRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::Layer3DocumentSave)),
                )
                .clicked();
            ui.label(text(
                catalog,
                if modified {
                    ExtendedUiTextKey::Layer3DocumentModified
                } else {
                    ExtendedUiTextKey::Layer3DocumentSaved
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
        egui::Window::new(text(catalog, ExtendedUiTextKey::Layer3DocumentDiscardTitle))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(text(
                    catalog,
                    ExtendedUiTextKey::Layer3DocumentUnsavedNotice,
                ));
                ui.horizontal(|ui| {
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::Layer3DocumentCancel))
                        .clicked()
                    {
                        self.pending_close = None;
                    }
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::Layer3DocumentDiscard))
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
            egui::Window::new(text(catalog, ExtendedUiTextKey::Layer3DocumentErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::Layer3DocumentOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                });
        }
    }

    fn clear(&mut self) {
        self.controller = None;
        self.loaded_revision = None;
        self.pending_close = None;
        self.paste_target = None;
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

fn pasted_text(ui: &egui::Ui) -> Option<String> {
    ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{Layer3Data, Layer3Settings};

    fn controller() -> Layer3DocumentController {
        Layer3DocumentController::decode(
            "layer3.lmlayer3".into(),
            &Layer3File(Layer3Data::default()).encode().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn complete_layer3_document_form_uses_every_typed_key_and_live_catalog() {
        let source = include_str!("layer3_editor.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("Layer3Document"))
        {
            assert!(
                source.contains(&format!("ExtendedUiTextKey::{key:?}")),
                "missing Layer 3 document label {key:?}"
            );
        }
        for literal in [
            "Window::new(\"Portable Layer 3 Editor\")",
            "Window::new(\"Unsaved Layer 3 document\")",
            "Window::new(\"Layer 3 editor error\")",
            "Button::new(\"Undo\")",
            "Button::new(\"Save\")",
            "ui.button(\"Copy tilemap\")",
            "ui.button(\"Paste remap commands\")",
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
    fn complete_layer3_form_is_one_canonical_undoable_revision() {
        let expected = Layer3Data {
            settings: Layer3Settings {
                start_position: 0xfe,
                tilemap_size: 3,
                liquid_type: 0x81,
                flags: 0xa5,
                graphics_files: [0, 0x123, 0xabc, 0xfff],
                reserved: std::array::from_fn(|index| index as u8),
            },
            tilemap: vec![0, 1, 2, 0xff],
            remap_commands: vec![0x80, 3, 4, 0xfe],
        };
        let mut controller = controller();
        controller
            .apply_edits(
                controller.revision(),
                &Layer3Form::load(&expected).edits().unwrap(),
            )
            .unwrap();
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.value().0, expected);

        let snapshot = controller.begin_save().unwrap();
        let reopened = Layer3File::decode(&snapshot.bytes).unwrap();
        assert_eq!(reopened, *controller.value());
        controller.cancel_save(snapshot.request_id).unwrap();
        assert!(controller.undo(controller.revision()).unwrap());
        assert_eq!(controller.value().0, Layer3Data::default());
        assert!(controller.redo(controller.revision()).unwrap());
        assert_eq!(controller.value(), &reopened);
    }
}
