use super::{NativeLevelDocumentEditor, PasteTarget, index_row, pasted_text};
use crate::native_clipboard;
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog, NativeLevelEdit};
use lm_level::{NativeLevelFile, SpriteLengthTable, SpriteToken};

impl NativeLevelDocumentEditor {
    pub(super) fn object_panel(
        &mut self,
        ui: &mut egui::Ui,
        value: &NativeLevelFile,
        catalog: Option<&LocalizationCatalog>,
    ) {
        ui.heading(
            text(catalog, Key::NativeLevelDocumentObjectsFormat)
                .replace("{count}", &value.layer1.objects.records.len().to_string()),
        );
        index_row(
            ui,
            &mut self.object_index,
            value.layer1.objects.records.len(),
            catalog,
        );
        ui.text_edit_singleline(&mut self.form.object);
        object_semantic_fields(ui, &mut self.form, catalog);
        let mut object_action = None;
        let mut semantic_action = false;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, Key::NativeLevelDocumentLoadSelected))
                .clicked()
            {
                let screen = object_screen(value, self.object_index);
                self.form
                    .load_object(value.layer1.objects.records.get(self.object_index), screen);
            }
            if ui
                .button(text(catalog, Key::NativeLevelDocumentInsert))
                .clicked()
            {
                object_action = Some(true);
            }
            if ui
                .button(text(catalog, Key::NativeLevelDocumentReplace))
                .clicked()
            {
                object_action = Some(false);
            }
            if ui
                .button(text(catalog, Key::NativeLevelDocumentRemove))
                .clicked()
            {
                object_action = Some(false);
                self.form.object.clear();
            }
            if ui
                .add_enabled(
                    self.form.object_fields_loaded
                        && self.object_index < value.layer1.objects.records.len(),
                    egui::Button::new(text(catalog, Key::NativeLevelDocumentApplyObjectFields)),
                )
                .clicked()
            {
                semantic_action = true;
            }
            if ui
                .add_enabled(
                    self.object_index < value.layer1.objects.records.len(),
                    egui::Button::new(text(catalog, Key::NativeLevelDocumentCopy)),
                )
                .clicked()
                && let Some(record) = value.layer1.objects.records.get(self.object_index)
            {
                match native_clipboard::encode_level_object(record) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui
                .button(text(catalog, Key::NativeLevelDocumentPaste))
                .clicked()
            {
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
        if semantic_action {
            self.apply_result(self.form.object_field_edit(self.object_index));
        }
    }

    pub(super) fn sprite_panel(
        &mut self,
        ui: &mut egui::Ui,
        value: &NativeLevelFile,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let lengths = self.current_sprite_lengths();
        ui.heading(
            text(catalog, Key::NativeLevelDocumentSpriteTokensFormat)
                .replace("{count}", &value.sprites.tokens.len().to_string()),
        );
        index_row(
            ui,
            &mut self.sprite_index,
            value.sprites.tokens.len(),
            catalog,
        );
        ui.text_edit_singleline(&mut self.form.sprite);
        sprite_semantic_fields(ui, &mut self.form, catalog);
        let mut sprite_action = None;
        let mut semantic_action = false;
        let mut remove_sprite = false;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, Key::NativeLevelDocumentLoadRecord))
                .clicked()
            {
                self.form
                    .load_sprite(value.sprites.tokens.get(self.sprite_index));
            }
            if ui
                .button(text(catalog, Key::NativeLevelDocumentInsertRecord))
                .clicked()
            {
                sprite_action = Some(true);
            }
            if ui
                .button(text(catalog, Key::NativeLevelDocumentReplaceRecord))
                .clicked()
            {
                sprite_action = Some(false);
            }
            if ui
                .button(text(catalog, Key::NativeLevelDocumentRemoveToken))
                .clicked()
            {
                remove_sprite = true;
            }
            if ui
                .add_enabled(
                    self.form.sprite_fields_loaded
                        && self.sprite_index < value.sprites.tokens.len(),
                    egui::Button::new(text(catalog, Key::NativeLevelDocumentApplySpriteFields)),
                )
                .clicked()
            {
                semantic_action = true;
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
                .add_enabled(
                    record.is_some(),
                    egui::Button::new(text(catalog, Key::NativeLevelDocumentCopyRecord)),
                )
                .clicked()
                && let Some(record) = record
            {
                match native_clipboard::encode_level_sprite(record) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui
                .button(text(catalog, Key::NativeLevelDocumentPasteRecord))
                .clicked()
            {
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
        if semantic_action {
            let edit = self.form.sprite_field_edit(
                self.sprite_index,
                value.sprites.tokens.get(self.sprite_index),
                &lengths,
            );
            self.apply_result(edit);
        }
        if remove_sprite {
            self.apply(NativeLevelEdit::RemoveSprite {
                index: self.sprite_index,
            });
        }
    }

    pub(super) fn current_sprite_lengths(&self) -> SpriteLengthTable {
        self.controller
            .as_ref()
            .expect("native level panel requires controller")
            .sprite_lengths()
            .clone()
    }
}

fn sprite_field_row(ui: &mut egui::Ui, label: &str, value: &mut u8, maximum: u8) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).range(0..=maximum));
    ui.end_row();
}

fn object_semantic_fields(
    ui: &mut egui::Ui,
    form: &mut crate::native_level_document_form::NativeLevelRecordForm,
    catalog: Option<&LocalizationCatalog>,
) {
    egui::Grid::new("native-level-object-semantic-fields").show(ui, |ui| {
        sprite_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentObjectCommand),
            &mut form.object_command,
            0x3f,
        );
        sprite_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentObjectParameter),
            &mut form.object_parameter,
            0xff,
        );
        sprite_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentObjectFirstCoordinate),
            &mut form.object_first,
            0x0f,
        );
        sprite_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentObjectSecondCoordinate),
            &mut form.object_second,
            0x0f,
        );
        ui.label(text(catalog, Key::NativeLevelDocumentScreen));
        ui.add(egui::DragValue::new(&mut form.object_screen).range(0..=0x1f));
        ui.end_row();
        ui.label(text(
            catalog,
            Key::NativeLevelDocumentObjectPerpendicularHigh,
        ));
        ui.checkbox(&mut form.object_perpendicular_high, "");
        ui.end_row();
    });
}

pub(super) fn object_screen(value: &NativeLevelFile, index: usize) -> Option<u16> {
    value
        .layer1
        .objects
        .native_placements()
        .into_iter()
        .find(|placement| placement.record_index == index)
        .map(|placement| placement.screen)
}

fn sprite_semantic_fields(
    ui: &mut egui::Ui,
    form: &mut crate::native_level_document_form::NativeLevelRecordForm,
    catalog: Option<&LocalizationCatalog>,
) {
    egui::Grid::new("native-level-sprite-semantic-fields").show(ui, |ui| {
        sprite_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentSpriteNumber),
            &mut form.sprite_number,
            0xff,
        );
        sprite_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentScreen),
            &mut form.sprite_screen,
            0x1f,
        );
        sprite_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentSpriteX),
            &mut form.sprite_x,
            0x0f,
        );
        sprite_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentSpriteYLow),
            &mut form.sprite_y_low,
            0x1f,
        );
        sprite_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentSpriteExtraBits),
            &mut form.sprite_extra_bits,
            3,
        );
    });
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}
