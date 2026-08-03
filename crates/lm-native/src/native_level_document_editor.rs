use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader, LoadedDocument},
    document_persistence::DocumentPersistence,
    native_level_document_form::{NativeLevelRecordForm, NativeSpriteHeaderForm},
};
use eframe::egui;
use lm_app::{NativeLevelDocumentController, NativeLevelEdit};
use lm_level::{NativeLevelFile, SpriteLengthTable};

mod canvas;
mod panels;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    Object,
    Sprite,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum NativeLevelCanvasTool {
    #[default]
    Select,
    MoveObject,
    MoveSprite,
}

#[derive(Default)]
pub(crate) struct NativeLevelDocumentEditor {
    controller: Option<NativeLevelDocumentController>,
    form: NativeLevelRecordForm,
    object_index: usize,
    sprite_index: usize,
    sprite_header: NativeSpriteHeaderForm,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    paste_target: Option<PasteTarget>,
    canvas_tool: NativeLevelCanvasTool,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl NativeLevelDocumentEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_native_level_document() else {
            return;
        };
        let Some(length_path) = dialogs::choose_sprite_length_table() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![
            BoundedRead::new(path, NativeLevelFile::MAX_FILE_LEN as u64, "native level"),
            BoundedRead::new(
                length_path,
                SpriteLengthTable::ENCODED_LEN as u64,
                "sprite length table",
            ),
        ]) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for native-level loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for native-level persistence to finish before closing".into());
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
            match result.and_then(decode_loaded) {
                Ok(controller) => {
                    self.sprite_header =
                        NativeSpriteHeaderForm::load(controller.value().sprites.header);
                    self.controller = Some(controller);
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
            egui::Window::new("Native Level Stream Editor")
                .default_size([760.0, 620.0])
                .vscroll(true)
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        self.toolbar(ui);
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let value = controller.value().clone();
        ui.label(format!(
            "Source level: {:04X}  |  {} framing",
            value.source_level,
            if value.sprites.expanded {
                "expanded"
            } else {
                "legacy"
            }
        ));
        ui.label(format!(
            "Legacy header: {}",
            crate::level_editor_forms::format_bytes(&value.layer1.header.encoded())
        ));
        ui.separator();
        self.level_canvas(ui, &value);
        ui.separator();
        self.object_panel(ui, &value);
        ui.separator();
        crate::native_level_document_form::show_sprite_header_form(
            ui,
            "portable-native-sprite-header",
            &mut self.sprite_header,
        );
        self.sprite_panel(ui, &value);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let Some(c) = self.controller.as_ref() else {
            return;
        };
        let (undo, redo, modified) = (c.can_undo(), c.can_redo(), c.is_modified());
        let mut history = None;
        let mut save_requested = false;
        let mut header_requested = false;
        ui.horizontal(|ui| {
            if ui.add_enabled(undo, egui::Button::new("Undo")).clicked() {
                history = Some(true);
            }
            if ui.add_enabled(redo, egui::Button::new("Redo")).clicked() {
                history = Some(false);
            }
            if ui
                .add_enabled(!self.persistence.is_running(), egui::Button::new("Save"))
                .clicked()
            {
                save_requested = true;
            }
            if ui.button("Apply sprite header").clicked() {
                header_requested = true;
            }
            ui.label(if modified { "Modified" } else { "Saved" });
        });
        if let (Some(undo), Some(c)) = (history, self.controller.as_mut()) {
            let result = if undo {
                c.undo(c.revision())
            } else {
                c.redo(c.revision())
            };
            if let Err(e) = result {
                self.error = Some(e.to_string());
            } else {
                self.reload_sprite_header();
            }
        }
        if save_requested {
            self.save();
        }
        if header_requested {
            self.apply_result(self.sprite_header.edit());
        }
    }

    fn apply_result(&mut self, edit: Result<NativeLevelEdit, String>) -> bool {
        match edit {
            Ok(edit) => self.apply(edit),
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }
    fn apply(&mut self, edit: NativeLevelEdit) -> bool {
        if let Some(c) = self.controller.as_mut() {
            let selected = if let NativeLevelEdit::SetSpriteFields { index, fields } = &edit {
                let vertical =
                    lm_profile::smw_us_v1_level_mode(c.value().layer1.header.level_mode()).vertical;
                let mut predicted = c.value().sprites.clone();
                match predicted.set_record_fields(*index, *fields, vertical, c.sprite_lengths()) {
                    Ok(selected) => Some(selected),
                    Err(error) => {
                        self.error = Some(error.to_string());
                        return false;
                    }
                }
            } else {
                None
            };
            if let Err(e) = c.apply_edits(c.revision(), &[edit]) {
                self.error = Some(e.to_string());
                return false;
            }
            if let Some(selected) = selected {
                self.sprite_index = selected;
                self.form
                    .load_sprite(c.value().sprites.tokens.get(selected));
            }
        }
        self.reload_sprite_header();
        true
    }

    fn reload_sprite_header(&mut self) {
        if let Some(controller) = &self.controller {
            self.sprite_header = NativeSpriteHeaderForm::load(controller.value().sprites.header);
        }
    }
    fn save(&mut self) {
        let Some(c) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) = self.persistence.begin(c) {
            self.error = Some(error);
        }
    }
    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved native level")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Discard unsaved native-level stream changes?");
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
            egui::Window::new("Native-level editor error").show(context, |ui| {
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
        self.paste_target = None;
        self.canvas_tool = NativeLevelCanvasTool::Select;
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

fn index_row(ui: &mut egui::Ui, index: &mut usize, len: usize) {
    ui.horizontal(|ui| {
        ui.label("Index");
        ui.add(egui::DragValue::new(index).range(0..=len));
    });
}

fn decode_loaded(loaded: LoadedDocument) -> Result<NativeLevelDocumentController, String> {
    let [(path, bytes), (_, raw)] = loaded.into_exact::<2>("native-level")?;
    let lengths = SpriteLengthTable::decode(&raw)
        .map_err(|n| format!("sprite length table requires 1024 bytes, got {n}"))?;
    NativeLevelDocumentController::decode(path, &bytes, lengths).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn loaded_native_level_requires_exact_interpretation_table() {
        assert!(
            decode_loaded(LoadedDocument {
                files: vec![
                    (PathBuf::from("level.lmlvl"), Vec::new()),
                    (PathBuf::from("sprite-lengths.bin"), vec![0; 1023]),
                ],
            })
            .is_err()
        );
        assert!(
            decode_loaded(LoadedDocument {
                files: vec![(PathBuf::from("level.lmlvl"), Vec::new())],
            })
            .is_err()
        );
    }
}
