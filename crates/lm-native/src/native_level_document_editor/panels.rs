use super::{NativeLevelDocumentEditor, PasteTarget, index_row, pasted_text};
use crate::native_clipboard;
use eframe::egui;
use lm_app::NativeLevelEdit;
use lm_level::{NativeLevelFile, SpriteToken};

impl NativeLevelDocumentEditor {
    pub(super) fn object_panel(&mut self, ui: &mut egui::Ui, value: &NativeLevelFile) {
        ui.heading(format!("Objects ({})", value.layer1.objects.records.len()));
        index_row(
            ui,
            &mut self.object_index,
            value.layer1.objects.records.len(),
        );
        ui.text_edit_singleline(&mut self.form.object);
        let mut object_action = None;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Load selected").clicked() {
                self.form.object = value
                    .layer1
                    .objects
                    .records
                    .get(self.object_index)
                    .map_or_else(String::new, |r| {
                        crate::level_editor_forms::format_bytes(r.encoded())
                    });
            }
            if ui.button("Insert").clicked() {
                object_action = Some(true);
            }
            if ui.button("Replace").clicked() {
                object_action = Some(false);
            }
            if ui.button("Remove").clicked() {
                object_action = Some(false);
                self.form.object.clear();
            }
            if ui
                .add_enabled(
                    self.object_index < value.layer1.objects.records.len(),
                    egui::Button::new("Copy"),
                )
                .clicked()
                && let Some(record) = value.layer1.objects.records.get(self.object_index)
            {
                match native_clipboard::encode_level_object(record) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui.button("Paste").clicked() {
                self.paste_target = Some(PasteTarget::Object);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(error) = copy_error {
            self.error = Some(error);
        }
        let pasted = (self.paste_target == Some(PasteTarget::Object))
            .then(|| pasted_text(ui))
            .flatten();
        if let Some(text) = pasted {
            self.paste_target = None;
            let edit = native_clipboard::decode_level_object(&text).map(|record| {
                NativeLevelEdit::Objects(vec![lm_level::ObjectEdit::Replace {
                    index: self.object_index,
                    record,
                }])
            });
            self.apply_result(edit);
        }
        if let Some(insert) = object_action {
            let edit = if !insert && self.form.object.trim().is_empty() {
                Ok(NativeLevelEdit::Objects(vec![
                    lm_level::ObjectEdit::Remove {
                        index: self.object_index,
                    },
                ]))
            } else {
                self.form.object_edit(self.object_index, insert)
            };
            self.apply_result(edit);
        }
    }

    pub(super) fn sprite_panel(&mut self, ui: &mut egui::Ui, value: &NativeLevelFile) {
        ui.heading(format!("Sprite tokens ({})", value.sprites.tokens.len()));
        index_row(ui, &mut self.sprite_index, value.sprites.tokens.len());
        ui.horizontal(|ui| {
            ui.label("Header");
            ui.text_edit_singleline(&mut self.sprite_header);
        });
        ui.text_edit_singleline(&mut self.form.sprite);
        let mut sprite_action = None;
        let mut remove_sprite = false;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Load record").clicked() {
                self.form.sprite = match value.sprites.tokens.get(self.sprite_index) {
                    Some(SpriteToken::Record(r)) => {
                        crate::level_editor_forms::format_bytes(&r.encoded)
                    }
                    Some(SpriteToken::Screen(v)) => format!("screen {v:02X}"),
                    Some(SpriteToken::Control(v)) => format!("control {v:02X}"),
                    None => String::new(),
                };
            }
            if ui.button("Insert record").clicked() {
                sprite_action = Some(true);
            }
            if ui.button("Replace record").clicked() {
                sprite_action = Some(false);
            }
            if ui.button("Remove token").clicked() {
                remove_sprite = true;
            }
            let record = value
                .sprites
                .tokens
                .get(self.sprite_index)
                .and_then(|token| {
                    if let SpriteToken::Record(record) = token {
                        Some(record)
                    } else {
                        None
                    }
                });
            if ui
                .add_enabled(record.is_some(), egui::Button::new("Copy record"))
                .clicked()
                && let Some(record) = record
            {
                match native_clipboard::encode_level_sprite(record) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui.button("Paste record").clicked() {
                self.paste_target = Some(PasteTarget::Sprite);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(error) = copy_error {
            self.error = Some(error);
        }
        let pasted = (self.paste_target == Some(PasteTarget::Sprite))
            .then(|| pasted_text(ui))
            .flatten();
        if let Some(text) = pasted {
            self.paste_target = None;
            let edit = native_clipboard::decode_level_sprite(&text).map(|record| {
                NativeLevelEdit::ReplaceSprite {
                    index: self.sprite_index,
                    token: SpriteToken::Record(record),
                }
            });
            self.apply_result(edit);
        }
        if let Some(insert) = sprite_action {
            self.apply_result(self.form.sprite_edit(self.sprite_index, insert));
        }
        if remove_sprite {
            self.apply(NativeLevelEdit::RemoveSprite {
                index: self.sprite_index,
            });
        }
    }
}
