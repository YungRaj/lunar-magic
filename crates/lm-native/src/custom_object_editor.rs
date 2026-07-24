use crate::{
    custom_object_editor_form::{CustomObjectForm, DescriptionFormatForm},
    dialogs,
    document_loader::DocumentLoader,
    document_persistence::DocumentPersistence,
    native_clipboard,
};
use eframe::egui;
use lm_app::{CustomObjectLibraryController, CustomObjectLibraryEdit};

mod document_io;
mod editing;
mod lifecycle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct CustomObjectEditor {
    controller: Option<CustomObjectLibraryController>,
    index: usize,
    form: CustomObjectForm,
    form_key: Option<(u64, usize)>,
    format: Option<DescriptionFormatForm>,
    search: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl CustomObjectEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(data_path) = dialogs::choose_custom_object_data() else {
            return;
        };
        let Some(descriptions_path) = dialogs::choose_custom_object_descriptions() else {
            return;
        };
        if let Err(error) = self
            .loader
            .start(document_io::requests(data_path, descriptions_path))
        {
            self.error = Some(error);
        }
    }

    fn load_form(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let key = (controller.revision(), self.index);
        if self.form_key != Some(key) {
            self.form = controller
                .library()
                .entries()
                .get(self.index)
                .map_or_else(CustomObjectForm::default, |entry| {
                    CustomObjectForm::load(entry, self.index)
                });
            self.format = Some(DescriptionFormatForm::load(controller.library()));
            self.form_key = Some(key);
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        self.toolbar(ui);
        if let Some(text) = pasted {
            self.paste_object(&text);
        }
        ui.separator();
        let entries = self
            .controller
            .as_ref()
            .map_or(0, |controller| controller.library().entries().len());
        self.entry_navigation(ui, entries);
        ui.label("Variable-width object bytes:");
        ui.text_edit_singleline(&mut self.form.object_bytes);
        ui.label("Description (one line, UTF-8):");
        ui.text_edit_singleline(&mut self.form.description);
        let mut edit = None;
        self.clipboard_actions(ui, entries);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(entries > 0, egui::Button::new("Replace selected"))
                .clicked()
            {
                edit = Some(
                    self.form
                        .entry()
                        .map(|entry| CustomObjectLibraryEdit::Replace {
                            index: self.index,
                            entry,
                        }),
                );
            }
            if ui
                .add_enabled(entries > 0, egui::Button::new("Remove selected"))
                .clicked()
            {
                edit = Some(Ok(CustomObjectLibraryEdit::Remove { index: self.index }));
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.form.insert_index).range(0..=entries));
            if ui.button("Insert form at index").clicked() {
                edit = Some(
                    self.form
                        .entry()
                        .map(|entry| CustomObjectLibraryEdit::Insert {
                            index: self.form.insert_index,
                            entry,
                        }),
                );
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.form.move_to).range(0..=entries.saturating_sub(1)),
            );
            if ui
                .add_enabled(entries > 1, egui::Button::new("Move selected to index"))
                .clicked()
            {
                edit = Some(Ok(CustomObjectLibraryEdit::Move {
                    from: self.index,
                    to: self.form.move_to,
                }));
            }
        });
        ui.separator();
        if let Some(format) = self.format.as_mut() {
            ui.horizontal(|ui| {
                ui.checkbox(&mut format.utf8_bom, "UTF-8 BOM");
                ui.checkbox(&mut format.crlf, "CRLF (off = LF)");
                ui.checkbox(&mut format.trailing_line_ending, "Trailing line ending");
            });
            if ui.button("Apply description framing").clicked() {
                edit = Some(Ok(CustomObjectLibraryEdit::SetDescriptionFormat(
                    format.value(),
                )));
            }
        }
        if let Some(edit) = edit {
            match edit {
                Ok(edit) => self.apply_edit(&edit),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn entry_navigation(&mut self, ui: &mut egui::Ui, entries: usize) {
        ui.label(format!("Synchronized entries: {entries}"));
        ui.horizontal(|ui| {
            ui.label("Unicode description search");
            if ui.text_edit_singleline(&mut self.search).changed() && !self.search.is_empty() {
                if let Some(found) = self.controller.as_ref().and_then(|controller| {
                    controller.library().search(&self.search).first().copied()
                }) {
                    self.index = found;
                    self.form_key = None;
                }
            }
        });
        ui.add(egui::Slider::new(&mut self.index, 0..=entries.saturating_sub(1)).text("Entry"));
    }

    fn clipboard_actions(&mut self, ui: &mut egui::Ui, entries: usize) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(entries > 0, egui::Button::new("Copy object"))
                .clicked()
                && let Some(object) = self.current_object()
            {
                match native_clipboard::encode_level_object(object) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(entries > 0, egui::Button::new("Paste object"))
                .clicked()
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
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
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new("Save paired files"),
                )
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
                if let Err(error) = self.persistence.begin_pair(controller) {
                    self.error = Some(error);
                }
            }
        }
        if changed {
            self.invalidate();
        }
    }

    fn clamp_index(&mut self) {
        let len = self
            .controller
            .as_ref()
            .map_or(0, |controller| controller.library().entries().len());
        self.index = if len == 0 { 0 } else { self.index.min(len - 1) };
        self.form.insert_index = self.form.insert_index.min(len);
        self.form.move_to = if len == 0 {
            0
        } else {
            self.form.move_to.min(len - 1)
        };
    }

    fn invalidate(&mut self) {
        self.form_key = None;
        self.format = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::ObjectRecord;

    fn editor() -> CustomObjectEditor {
        let controller = CustomObjectLibraryController::decode(
            "objects.mw0".into(),
            "objects.mw0t".into(),
            &[1, 0, 3, 0xff],
            b"Original description\n",
        )
        .unwrap();
        let mut editor = CustomObjectEditor {
            controller: Some(controller),
            ..CustomObjectEditor::default()
        };
        editor.load_form();
        editor
    }

    #[test]
    fn typed_object_paste_replaces_bytes_and_preserves_description() {
        let mut editor = editor();
        let replacement = ObjectRecord::new(vec![2, 8, 4]).unwrap();

        editor.paste_object(&native_clipboard::encode_level_object(&replacement).unwrap());

        let controller = editor.controller.as_ref().unwrap();
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.library().entries()[0].object, replacement);
        assert_eq!(
            controller.library().entries()[0].description,
            "Original description"
        );
        assert!(editor.form_key.is_none());
    }

    #[test]
    fn wrong_clipboard_domain_preserves_object_library_and_revision() {
        let mut editor = editor();
        let before = editor.controller.as_ref().unwrap().library().clone();
        let text = native_clipboard::encode_palette_color(lm_graphics::Bgr555(0x1234)).unwrap();

        editor.paste_object(&text);

        let controller = editor.controller.as_ref().unwrap();
        assert_eq!(controller.revision(), 0);
        assert_eq!(controller.library(), &before);
        assert!(editor.error.is_some());
    }
}
