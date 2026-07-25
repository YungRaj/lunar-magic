use super::{AggregatePanels, PasteTarget, index, pasted_text};
use crate::{level_editor_forms, native_clipboard, native_level_document_form};
use eframe::egui;
use lm_app::{NativeLevelAssetsControllerEdit, NativeLevelEdit};
use lm_level::{ObjectEdit, SpriteToken};
use lm_project::{LoadedLevelSlot, NativeLevelAssetsFile};

impl AggregatePanels {
    pub(super) fn level_panel(
        &mut self,
        ui: &mut egui::Ui,
        file: &NativeLevelAssetsFile,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let level = &file.assets.level;
        ui.label(format!(
            "Source slot {:04X}; header {}",
            file.source_slot,
            level_editor_forms::format_bytes(&level.layer1.header.encoded())
        ));
        ui.heading(format!("Objects ({})", level.layer1.objects.records.len()));
        index(
            ui,
            &mut self.object_index,
            level.layer1.objects.records.len(),
        );
        ui.text_edit_singleline(&mut self.object);
        let mut action = None;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Load").clicked() {
                self.object = level
                    .layer1
                    .objects
                    .records
                    .get(self.object_index)
                    .map_or_else(String::new, |record| {
                        level_editor_forms::format_bytes(record.encoded())
                    });
            }
            for (label, value) in [("Insert", 0), ("Replace", 1), ("Remove", 2)] {
                if ui.button(label).clicked() {
                    action = Some(value);
                }
            }
            if ui
                .add_enabled(
                    self.object_index < level.layer1.objects.records.len(),
                    egui::Button::new("Copy"),
                )
                .clicked()
            {
                let record = level
                    .layer1
                    .objects
                    .records
                    .get(self.object_index)
                    .expect("copy is enabled only for an existing object");
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
            return Some(Err(error));
        }
        if self.paste_target == Some(PasteTarget::Object)
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            return Some(native_clipboard::decode_level_object(&text).map(|record| {
                NativeLevelAssetsControllerEdit::Level(vec![NativeLevelEdit::Objects(vec![
                    ObjectEdit::Replace {
                        index: self.object_index,
                        record,
                    },
                ])])
            }));
        }
        if let Some(action) = action {
            let edit = match action {
                2 => Ok(ObjectEdit::Remove {
                    index: self.object_index,
                }),
                _ => level_editor_forms::parse_object(&self.object).map(|record| {
                    if action == 0 {
                        ObjectEdit::Insert {
                            index: self.object_index,
                            record,
                        }
                    } else {
                        ObjectEdit::Replace {
                            index: self.object_index,
                            record,
                        }
                    }
                }),
            };
            return Some(edit.map(|edit| {
                NativeLevelAssetsControllerEdit::Level(vec![NativeLevelEdit::Objects(vec![edit])])
            }));
        }
        ui.separator();
        self.sprite_panel(ui, level)
    }

    fn sprite_panel(
        &mut self,
        ui: &mut egui::Ui,
        level: &LoadedLevelSlot,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        ui.heading(format!("Sprite tokens ({})", level.sprites.tokens.len()));
        index(ui, &mut self.sprite_index, level.sprites.tokens.len());
        ui.horizontal(|ui| {
            ui.label("Header");
            ui.text_edit_singleline(&mut self.sprite_header);
        });
        ui.text_edit_singleline(&mut self.sprite);
        let mut action = None;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Load record").clicked() {
                self.sprite = match level.sprites.tokens.get(self.sprite_index) {
                    Some(SpriteToken::Record(record)) => {
                        level_editor_forms::format_bytes(&record.encoded)
                    }
                    Some(SpriteToken::Screen(value)) => format!("yhigh {value:02X}"),
                    Some(SpriteToken::Control(value)) => format!("control {value:02X}"),
                    None => String::new(),
                };
            }
            for (label, value) in [
                ("Apply header", 0),
                ("Insert record", 1),
                ("Replace record", 2),
                ("Remove token", 3),
            ] {
                if ui.button(label).clicked() {
                    action = Some(value);
                }
            }
            let selected = level.sprites.tokens.get(self.sprite_index);
            if ui
                .add_enabled(
                    matches!(selected, Some(SpriteToken::Record(_))),
                    egui::Button::new("Copy record"),
                )
                .clicked()
                && let Some(SpriteToken::Record(record)) = selected
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
            return Some(Err(error));
        }
        if self.paste_target == Some(PasteTarget::Sprite)
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            return Some(native_clipboard::decode_level_sprite(&text).map(|record| {
                NativeLevelAssetsControllerEdit::Level(vec![NativeLevelEdit::ReplaceSprite {
                    index: self.sprite_index,
                    token: SpriteToken::Record(record),
                }])
            }));
        }
        action.map(|action| {
            let edit = match action {
                0 => level_editor_forms::parse_hex_u8(&self.sprite_header, "sprite header")
                    .map(NativeLevelEdit::SetSpriteHeader),
                3 => Ok(NativeLevelEdit::RemoveSprite {
                    index: self.sprite_index,
                }),
                _ => native_level_document_form::parse_sprite_token(&self.sprite).map(|token| {
                    if action == 1 {
                        NativeLevelEdit::InsertSprite {
                            index: self.sprite_index,
                            token,
                        }
                    } else {
                        NativeLevelEdit::ReplaceSprite {
                            index: self.sprite_index,
                            token,
                        }
                    }
                }),
            };
            edit.map(|edit| NativeLevelAssetsControllerEdit::Level(vec![edit]))
        })
    }
}
