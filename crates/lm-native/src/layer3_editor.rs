use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    layer3_editor_form::Layer3Form,
    native_clipboard,
};
use eframe::egui;
use lm_app::Layer3DocumentController;
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

    pub(crate) fn show(&mut self, context: &egui::Context) -> bool {
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
            egui::Window::new("Portable Layer 3 Editor")
                .default_size([760.0, 650.0])
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
            self.form = Layer3Form::load(&controller.value().0);
            self.loaded_revision = Some(controller.revision());
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        ui.separator();
        for (label, value) in ["Start position", "Tilemap size", "Liquid/type", "Raw flags"]
            .into_iter()
            .zip(self.form.selectors.iter_mut())
        {
            ui.add(egui::Slider::new(value, 0..=u8::MAX).text(label));
        }
        for (slot, value) in self.form.graphics.iter_mut().enumerate() {
            ui.add(egui::Slider::new(value, 0..=0x0fff).text(format!("Graphics {slot}")));
        }
        ui.label("Reserved bytes (exactly 16 hexadecimal bytes):");
        ui.text_edit_singleline(&mut self.form.reserved);
        ui.label("Raw tilemap bytes (maximum 0x2000):");
        ui.add(
            egui::TextEdit::multiline(&mut self.form.tilemap)
                .desired_rows(8)
                .code_editor(),
        );
        ui.label("Literal remap-command bytes (maximum 0x10000):");
        ui.add(
            egui::TextEdit::multiline(&mut self.form.remap)
                .desired_rows(8)
                .code_editor(),
        );
        if let Some(edit) = self.clipboard_controls(ui) {
            self.apply_edit(edit);
        }
        if ui.button("Apply all Layer 3 fields atomically").clicked() {
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

    fn clipboard_controls(&mut self, ui: &mut egui::Ui) -> Option<Result<Layer3Edit, String>> {
        let value = self.controller.as_ref()?.value();
        let (tilemap, remap) = (value.0.tilemap.clone(), value.0.remap_commands.clone());
        let mut copy_result = None;
        ui.horizontal(|ui| {
            if ui.button("Copy tilemap").clicked() {
                copy_result = Some(native_clipboard::encode_layer3_tilemap(&tilemap));
            }
            if ui.button("Paste tilemap").clicked() {
                self.paste_target = Some(PasteTarget::Tilemap);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            if ui.button("Copy remap commands").clicked() {
                copy_result = Some(native_clipboard::encode_layer3_remap(&remap));
            }
            if ui.button("Paste remap commands").clicked() {
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
        egui::Window::new("Unsaved Layer 3 document")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved Layer 3 changes?");
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
            egui::Window::new("Layer 3 editor error")
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
        self.paste_target = None;
    }
}

fn pasted_text(ui: &egui::Ui) -> Option<String> {
    ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        })
    })
}
