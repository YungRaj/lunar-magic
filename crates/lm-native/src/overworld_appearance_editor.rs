use crate::{
    document_loader::DocumentLoader,
    document_persistence::DocumentPersistence,
    native_clipboard,
    overworld_appearance_editor_forms::{DefinitionForm, PartForm},
};
use eframe::egui;
use lm_app::{OverworldAppearanceDocumentController, OverworldAppearanceDocumentEdit};

mod document_io;
mod form_fields;
mod panels;
mod preview;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PartPasteMode {
    Replace,
    InsertAfter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PartPasteTarget {
    revision: u64,
    sprite_id: u16,
    index: usize,
    mode: PartPasteMode,
}

#[derive(Default)]
pub(crate) struct OverworldAppearanceEditor {
    controller: Option<OverworldAppearanceDocumentController>,
    definition_index: usize,
    definition: DefinitionForm,
    definition_key: Option<(u64, usize)>,
    part_index: usize,
    part: PartForm,
    part_key: Option<(u64, u16, usize)>,
    preview_drag: Option<preview::PreviewDrag>,
    clipboard_paste_target: Option<PartPasteTarget>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl OverworldAppearanceEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for appearance loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for appearance persistence to finish before closing".into());
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
            match result.and_then(document_io::decode) {
                Ok(controller) => {
                    self.controller = Some(controller);
                    self.clipboard_paste_target = None;
                    self.invalidate();
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
            self.clamp_indices();
            egui::Window::new("Portable Overworld Appearance Editor")
                .default_size([720.0, 600.0])
                .vscroll(true)
                .show(context, |ui| self.contents(ui));
        }
        let approved = self.show_close_confirmation(context);
        self.show_error(context);
        approved
    }

    fn contents(&mut self, ui: &mut egui::Ui) {
        let pasted = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        self.toolbar(ui);
        if let Some(text) = pasted
            && let Some(target) = self.clipboard_paste_target.take()
        {
            self.paste_part_at(&text, target);
        }
        ui.separator();
        let (revision, definitions) = {
            let Some(controller) = self.controller.as_ref() else {
                return;
            };
            (
                controller.revision(),
                controller.value().definitions.clone(),
            )
        };
        ui.label(format!("Sprite definitions: {}", definitions.len()));
        ui.add(
            egui::Slider::new(
                &mut self.definition_index,
                0..=definitions.len().saturating_sub(1),
            )
            .text("Definition"),
        );
        let selected = definitions.get(self.definition_index);
        if self.definition_key != Some((revision, self.definition_index)) {
            self.definition = selected.map_or_else(DefinitionForm::default, |definition| {
                DefinitionForm::load(definition.sprite_id, self.definition_index)
            });
            self.definition_key = Some((revision, self.definition_index));
        }
        let mut edit = self.definition_fields(ui, &definitions);
        ui.separator();
        if let Some(definition) = selected {
            let preview_edit = self.appearance_preview(ui, revision, definition);
            let part_edit = self.part_fields(ui, revision, definition);
            edit = edit.or(preview_edit.map(Ok)).or(part_edit);
        } else {
            ui.label("Insert a sprite definition before adding tile parts.");
        }
        if let Some(edit) = edit {
            match edit {
                Ok(edit) => self.apply_edit(&edit),
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

    fn apply_edit(&mut self, edit: &OverworldAppearanceDocumentEdit) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) =
            controller.apply_edits(controller.revision(), std::slice::from_ref(edit))
        {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    fn paste_part_at(&mut self, text: &str, target: PartPasteTarget) {
        let part = match native_clipboard::decode_overworld_appearance_part(text) {
            Ok(part) => part,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let (edit, selected_index) = match target.mode {
            PartPasteMode::Replace => (
                OverworldAppearanceDocumentEdit::ReplacePart {
                    sprite_id: target.sprite_id,
                    index: target.index,
                    value: part,
                },
                target.index,
            ),
            PartPasteMode::InsertAfter => {
                let Some(index) = target.index.checked_add(1) else {
                    self.error = Some("overworld appearance paste index overflow".into());
                    return;
                };
                (
                    OverworldAppearanceDocumentEdit::InsertPart {
                        sprite_id: target.sprite_id,
                        index,
                        value: part,
                    },
                    index,
                )
            }
        };
        let Some(controller) = self.controller.as_mut() else {
            self.error = Some("overworld appearance document closed before paste delivery".into());
            return;
        };
        match controller.apply_edits(target.revision, &[edit]) {
            Ok(()) => {
                self.part_index = selected_index;
                self.invalidate();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn clamp_indices(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let definitions = &controller.value().definitions;
        self.definition_index = clamp(self.definition_index, definitions.len());
        let part_len = definitions
            .get(self.definition_index)
            .map_or(0, |definition| definition.parts.len());
        self.part_index = clamp(self.part_index, part_len);
        self.definition.insert_index = self.definition.insert_index.min(definitions.len());
        self.definition.move_before = clamp(self.definition.move_before, definitions.len());
        self.part.insert_index = self.part.insert_index.min(part_len);
        self.part.move_before = clamp(self.part.move_before, part_len);
    }

    fn invalidate(&mut self) {
        self.definition_key = None;
        self.part_key = None;
        self.preview_drag = None;
    }

    fn show_close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Unsaved overworld appearances")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label("Discard unsaved appearance changes?");
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
            egui::Window::new("Overworld appearance editor error")
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
        self.clipboard_paste_target = None;
        self.pending_close = None;
        self.invalidate();
    }
}

fn clamp(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::{SpriteAppearanceDefinition, SpriteAppearanceFile, SpriteAppearancePart};

    fn part(tile_index: u16) -> SpriteAppearancePart {
        SpriteAppearancePart {
            tile_index,
            palette_index: 3,
            x_offset: -4,
            y_offset: 5,
            x_flip: true,
            y_flip: false,
        }
    }

    fn editor() -> OverworldAppearanceEditor {
        let file = SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 0x1234,
                parts: vec![part(1), part(2)],
            }],
        };
        OverworldAppearanceEditor {
            controller: Some(
                OverworldAppearanceDocumentController::decode(
                    "appearance.lmowapp".into(),
                    &file.encode().unwrap(),
                )
                .unwrap(),
            ),
            ..OverworldAppearanceEditor::default()
        }
    }

    #[test]
    fn typed_part_paste_replaces_or_inserts_as_one_revision() {
        let replacement = part(0xabcd);
        let text = native_clipboard::encode_overworld_appearance_part(replacement).unwrap();
        let mut editor = editor();
        editor.paste_part_at(
            &text,
            PartPasteTarget {
                revision: 0,
                sprite_id: 0x1234,
                index: 0,
                mode: PartPasteMode::Replace,
            },
        );
        let controller = editor.controller.as_ref().unwrap();
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.value().definitions[0].parts[0], replacement);
        assert_eq!(editor.part_index, 0);

        editor.paste_part_at(
            &text,
            PartPasteTarget {
                revision: 1,
                sprite_id: 0x1234,
                index: 0,
                mode: PartPasteMode::InsertAfter,
            },
        );
        let controller = editor.controller.as_ref().unwrap();
        assert_eq!(controller.revision(), 2);
        assert_eq!(controller.value().definitions[0].parts.len(), 3);
        assert_eq!(controller.value().definitions[0].parts[1], replacement);
        assert_eq!(editor.part_index, 1);
    }

    #[test]
    fn part_paste_rejects_stale_targets_and_other_domains_without_mutation() {
        let mut editor = editor();
        let target = PartPasteTarget {
            revision: 0,
            sprite_id: 0x1234,
            index: 0,
            mode: PartPasteMode::Replace,
        };
        let text = native_clipboard::encode_overworld_appearance_part(part(3)).unwrap();
        editor.paste_part_at(&text, target);
        editor.error = None;
        editor.paste_part_at(&text, target);
        assert!(editor.error.is_some());
        assert_eq!(editor.controller.as_ref().unwrap().revision(), 1);

        editor.error = None;
        let wrong_domain = native_clipboard::encode_palette_color(lm_graphics::Bgr555(1)).unwrap();
        editor.paste_part_at(
            &wrong_domain,
            PartPasteTarget {
                revision: 1,
                ..target
            },
        );
        assert!(editor.error.is_some());
        assert_eq!(editor.controller.as_ref().unwrap().revision(), 1);
    }
}
