use super::{
    AggregatePanels, PasteTarget, PendingSelectionMove, index, move_before_indexes, pasted_text,
    text,
};
use crate::{level_editor_forms, native_clipboard, native_level_document_form};
use eframe::egui;
use lm_app::{
    ExtendedUiTextKey as Key, LocalizationCatalog, NativeLevelAssetsControllerEdit, NativeLevelEdit,
};
use lm_level::{ObjectEdit, SpriteLengthTable, SpriteToken};
use lm_project::{LoadedLevelSlot, NativeLevelAssetsFile};

impl AggregatePanels {
    pub(super) fn level_panel(
        &mut self,
        ui: &mut egui::Ui,
        file: &NativeLevelAssetsFile,
        sprite_lengths: &SpriteLengthTable,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let level = &file.assets.level;
        ui.label(
            text(catalog, Key::NativeAssetsLevelSourceFormat)
                .replace("{slot}", &format!("{:04X}", file.source_slot))
                .replace(
                    "{header}",
                    &level_editor_forms::format_bytes(&level.layer1.header.encoded()),
                ),
        );
        let mut stage_header = false;
        ui.collapsing(text(catalog, Key::NativeAssetsLevelHeader), |ui| {
            egui::Grid::new("installed-native-level-header").show(ui, |ui| {
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsLevelMode),
                    &mut self.header.level_mode,
                    31,
                );
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsBackgroundPalette),
                    &mut self.header.background_palette,
                    7,
                );
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsLastScreen),
                    &mut self.header.last_screen,
                    31,
                );
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsBackgroundColor),
                    &mut self.header.background_color,
                    7,
                );
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsSpriteTileset),
                    &mut self.header.sprite_tileset,
                    15,
                );
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsDefaultMusic),
                    &mut self.header.default_music_selector,
                    7,
                );
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsTimeLimit),
                    &mut self.header.time_limit_selector,
                    3,
                );
                ui.label(text(catalog, Key::NativeAssetsCustomTimeBypass));
                ui.checkbox(
                    &mut self.header.custom_time_enabled,
                    text(catalog, Key::NativeAssetsEnabled),
                );
                ui.end_row();
                ui.label(text(catalog, Key::NativeAssetsCustomTimeHex));
                ui.add_enabled(
                    self.header.custom_time_enabled,
                    egui::DragValue::new(&mut self.header.custom_time_value)
                        .range(0..=lm_level::CustomTimeSettings::MAX_VALUE)
                        .hexadecimal(3, false, true),
                );
                ui.end_row();
                ui.label(text(catalog, Key::NativeAssetsForceTimeReset));
                ui.add_enabled(
                    self.header.custom_time_enabled,
                    egui::Checkbox::without_text(&mut self.header.force_time_reset),
                );
                ui.end_row();
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsForegroundPalette),
                    &mut self.header.foreground_palette,
                    7,
                );
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsSpritePalette),
                    &mut self.header.sprite_palette,
                    7,
                );
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsObjectTileset),
                    &mut self.header.object_tileset,
                    15,
                );
                header_row(
                    ui,
                    &text(catalog, Key::NativeAssetsLayer1VerticalScroll),
                    &mut self.header.layer1_vertical_scroll,
                    3,
                );
            });
            ui.horizontal(|ui| {
                if ui
                    .button(text(catalog, Key::NativeAssetsStageHeader))
                    .clicked()
                {
                    stage_header = true;
                }
                if ui
                    .button(text(catalog, Key::NativeAssetsResetHeader))
                    .clicked()
                {
                    self.header = native_level_document_form::NativeLevelHeaderForm::load(level);
                }
            });
        });
        if stage_header {
            return Some(
                self.header
                    .edits()
                    .map(|edits| NativeLevelAssetsControllerEdit::Level(edits))
                    .map_err(|error| error.to_string()),
            );
        }
        ui.heading(
            text(catalog, Key::NativeLevelDocumentObjectsFormat)
                .replace("{count}", &level.layer1.objects.records.len().to_string()),
        );
        index(
            ui,
            &mut self.object_index,
            level.layer1.objects.records.len(),
            catalog,
        );
        self.sync_level_object_form(level, false);
        ui.text_edit_singleline(&mut self.level_record.object);
        object_semantic_fields(ui, &mut self.level_record, catalog);
        let mut action = None;
        let mut apply_object_fields = false;
        let mut move_object = None;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, Key::NativeLevelDocumentLoadSelected))
                .clicked()
            {
                self.sync_level_object_form(level, true);
            }
            for (label, value) in [
                (Key::NativeLevelDocumentInsert, 0),
                (Key::NativeLevelDocumentReplace, 1),
                (Key::NativeLevelDocumentRemove, 2),
            ] {
                if ui.button(text(catalog, label)).clicked() {
                    action = Some(value);
                }
            }
            if ui
                .add_enabled(
                    self.level_record.object_fields_loaded
                        && self.object_index < level.layer1.objects.records.len(),
                    egui::Button::new(text(catalog, Key::NativeLevelDocumentApplyObjectFields)),
                )
                .clicked()
            {
                apply_object_fields = true;
            }
            if ui
                .add_enabled(
                    self.object_index > 0,
                    egui::Button::new(text(catalog, Key::NativeAssetsMoveUp)),
                )
                .clicked()
            {
                move_object = move_before_indexes(
                    self.object_index,
                    level.layer1.objects.records.len(),
                    false,
                );
            }
            if ui
                .add_enabled(
                    self.object_index.saturating_add(1) < level.layer1.objects.records.len(),
                    egui::Button::new(text(catalog, Key::NativeAssetsMoveDown)),
                )
                .clicked()
            {
                move_object = move_before_indexes(
                    self.object_index,
                    level.layer1.objects.records.len(),
                    true,
                );
            }
            if ui
                .add_enabled(
                    self.object_index < level.layer1.objects.records.len(),
                    egui::Button::new(text(catalog, Key::NativeLevelDocumentCopy)),
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
            return Some(Err(error));
        }
        if apply_object_fields {
            let edit = self.level_record.object_field_edit(self.object_index);
            if let Ok(NativeLevelEdit::Objects(edits)) = &edit
                && let [ObjectEdit::SetOrdinaryFields { index, fields }] = edits.as_slice()
            {
                let mut predicted = level.layer1.objects.clone();
                match predicted.set_ordinary_fields(*index, *fields) {
                    Ok(selected) => {
                        self.pending_selection_move = Some(PendingSelectionMove::Object(selected));
                    }
                    Err(error) => return Some(Err(error.to_string())),
                }
            }
            return Some(edit.map(|edit| NativeLevelAssetsControllerEdit::Level(vec![edit])));
        }
        if let Some((before, selected)) = move_object {
            self.pending_selection_move = Some(PendingSelectionMove::Object(selected));
            return Some(Ok(NativeLevelAssetsControllerEdit::Level(vec![
                NativeLevelEdit::Objects(vec![ObjectEdit::MoveBefore {
                    from: self.object_index,
                    before,
                }]),
            ])));
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
                _ => level_editor_forms::parse_object(&self.level_record.object).map(|record| {
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
        self.sprite_panel(ui, level, sprite_lengths, catalog)
    }

    fn sprite_panel(
        &mut self,
        ui: &mut egui::Ui,
        level: &LoadedLevelSlot,
        sprite_lengths: &SpriteLengthTable,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        ui.heading(
            text(catalog, Key::NativeLevelDocumentSpriteTokensFormat)
                .replace("{count}", &level.sprites.tokens.len().to_string()),
        );
        index(
            ui,
            &mut self.sprite_index,
            level.sprites.tokens.len(),
            catalog,
        );
        self.sync_sprite_form(level, false);
        native_level_document_form::show_localized_sprite_header_form(
            ui,
            "installed-native-sprite-header",
            &mut self.sprite_header,
            catalog,
        );
        let mut apply_spawn_settings = false;
        ui.add_enabled_ui(self.sprite_spawn_available, |ui| {
            ui.add(
                egui::Slider::new(&mut self.sprite_vertical_spawn_range, 0..=3)
                    .text(text(catalog, Key::NativeAssetsVerticalSpawnRange)),
            );
            ui.checkbox(
                &mut self.sprite_smart_spawn,
                text(catalog, Key::NativeAssetsSmartSpawn),
            );
            apply_spawn_settings = ui
                .button(text(catalog, Key::NativeAssetsApplySpawn))
                .clicked();
        });
        if !self.sprite_spawn_available {
            ui.small(text(catalog, Key::NativeAssetsSpawnUnavailable));
        }
        if apply_spawn_settings {
            return Some(spawn_settings_edit(
                self.sprite_vertical_spawn_range,
                self.sprite_smart_spawn,
            ));
        }
        ui.text_edit_singleline(&mut self.level_record.sprite);
        sprite_semantic_fields(ui, &mut self.level_record, catalog);
        let mut action = None;
        let mut apply_sprite_fields = false;
        let mut move_sprite = None;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, Key::NativeLevelDocumentLoadRecord))
                .clicked()
            {
                self.sync_sprite_form(level, true);
            }
            for (label, value) in [
                (Key::NativeAssetsApplyHeader, 0),
                (Key::NativeLevelDocumentInsertRecord, 1),
                (Key::NativeLevelDocumentReplaceRecord, 2),
                (Key::NativeLevelDocumentRemoveToken, 3),
            ] {
                if ui.button(text(catalog, label)).clicked() {
                    action = Some(value);
                }
            }
            if ui
                .add_enabled(
                    self.level_record.sprite_fields_loaded
                        && self.sprite_index < level.sprites.tokens.len(),
                    egui::Button::new(text(catalog, Key::NativeLevelDocumentApplySpriteFields)),
                )
                .clicked()
            {
                apply_sprite_fields = true;
            }
            if ui
                .add_enabled(
                    self.sprite_index > 0,
                    egui::Button::new(text(catalog, Key::NativeAssetsMoveUp)),
                )
                .clicked()
            {
                move_sprite =
                    move_before_indexes(self.sprite_index, level.sprites.tokens.len(), false);
            }
            if ui
                .add_enabled(
                    self.sprite_index.saturating_add(1) < level.sprites.tokens.len(),
                    egui::Button::new(text(catalog, Key::NativeAssetsMoveDown)),
                )
                .clicked()
            {
                move_sprite =
                    move_before_indexes(self.sprite_index, level.sprites.tokens.len(), true);
            }
            let selected = level.sprites.tokens.get(self.sprite_index);
            if ui
                .add_enabled(
                    matches!(selected, Some(SpriteToken::Record(_))),
                    egui::Button::new(text(catalog, Key::NativeLevelDocumentCopyRecord)),
                )
                .clicked()
                && let Some(SpriteToken::Record(record)) = selected
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
            return Some(Err(error));
        }
        if apply_sprite_fields {
            let edit = self.level_record.sprite_field_edit(
                self.sprite_index,
                level.sprites.tokens.get(self.sprite_index),
                sprite_lengths,
            );
            if let Ok(NativeLevelEdit::SetSpriteFields { index, fields }) = &edit {
                let vertical =
                    lm_profile::smw_us_v1_level_mode(level.layer1.header.level_mode()).vertical;
                let mut predicted = level.sprites.clone();
                match predicted.set_record_fields(*index, *fields, vertical, sprite_lengths) {
                    Ok(selected) => {
                        self.pending_selection_move = Some(PendingSelectionMove::Sprite(selected));
                    }
                    Err(error) => return Some(Err(error.to_string())),
                }
            }
            return Some(edit.map(|edit| NativeLevelAssetsControllerEdit::Level(vec![edit])));
        }
        if let Some((before, selected)) = move_sprite {
            self.pending_selection_move = Some(PendingSelectionMove::Sprite(selected));
            return Some(Ok(NativeLevelAssetsControllerEdit::Level(vec![
                NativeLevelEdit::MoveSpriteBefore {
                    from: self.sprite_index,
                    before,
                },
            ])));
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
                0 => self.sprite_header.edit(),
                3 => Ok(NativeLevelEdit::RemoveSprite {
                    index: self.sprite_index,
                }),
                _ => native_level_document_form::parse_sprite_token(&self.level_record.sprite).map(
                    |token| {
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
                    },
                ),
            };
            edit.map(|edit| NativeLevelAssetsControllerEdit::Level(vec![edit]))
        })
    }
}

fn spawn_settings_edit(
    vertical_range: u8,
    smart_spawn: bool,
) -> Result<NativeLevelAssetsControllerEdit, String> {
    if vertical_range > lm_level::SpriteSpawnSettings::RANGE_MASK {
        return Err(lm_level::SpriteSpawnRangeError(vertical_range).to_string());
    }
    Ok(NativeLevelAssetsControllerEdit::SpriteSpawnProperties {
        vertical_range,
        smart_spawn,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_aggregate_level_panel_has_no_literal_widget_text() {
        let source = include_str!("level.rs");
        for literal_widget in [
            "ui.heading(\"",
            "ui.label(\"",
            "ui.button(\"",
            "Button::new(\"",
            ".text(\"",
            "ui.collapsing(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "aggregate Level panel regressed to fixed widget text: {literal_widget}"
            );
        }
    }

    #[test]
    fn installed_spawn_form_emits_bounded_semantic_intent() {
        assert!(matches!(
            spawn_settings_edit(3, true).unwrap(),
            NativeLevelAssetsControllerEdit::SpriteSpawnProperties {
                vertical_range: 3,
                smart_spawn: true,
            }
        ));
        assert_eq!(
            spawn_settings_edit(4, false).unwrap_err(),
            "sprite vertical spawn range must be in 0..=3, got 4"
        );
    }
}

fn semantic_field_row(ui: &mut egui::Ui, label: &str, value: &mut u8, maximum: u8) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).range(0..=maximum));
    ui.end_row();
}

pub(super) fn object_semantic_fields(
    ui: &mut egui::Ui,
    form: &mut native_level_document_form::NativeLevelRecordForm,
    catalog: Option<&LocalizationCatalog>,
) {
    egui::Grid::new("native-assets-object-semantic-fields").show(ui, |ui| {
        semantic_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentObjectCommand),
            &mut form.object_command,
            0x3f,
        );
        semantic_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentObjectParameter),
            &mut form.object_parameter,
            0xff,
        );
        semantic_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentObjectFirstCoordinate),
            &mut form.object_first,
            0x0f,
        );
        semantic_field_row(
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

pub(super) fn object_stream_screen(objects: &lm_level::ObjectStream, index: usize) -> Option<u16> {
    objects
        .native_placements()
        .into_iter()
        .find(|placement| placement.record_index == index)
        .map(|placement| placement.screen)
}

fn sprite_semantic_fields(
    ui: &mut egui::Ui,
    form: &mut native_level_document_form::NativeLevelRecordForm,
    catalog: Option<&LocalizationCatalog>,
) {
    egui::Grid::new("native-assets-sprite-semantic-fields").show(ui, |ui| {
        semantic_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentSpriteNumber),
            &mut form.sprite_number,
            0xff,
        );
        semantic_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentScreen),
            &mut form.sprite_screen,
            0x1f,
        );
        semantic_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentSpriteX),
            &mut form.sprite_x,
            0x0f,
        );
        semantic_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentSpriteYLow),
            &mut form.sprite_y_low,
            0x1f,
        );
        semantic_field_row(
            ui,
            &text(catalog, Key::NativeLevelDocumentSpriteExtraBits),
            &mut form.sprite_extra_bits,
            3,
        );
    });
}

fn header_row(ui: &mut egui::Ui, label: &str, value: &mut u8, maximum: u8) {
    ui.label(label);
    ui.add(
        egui::DragValue::new(value)
            .range(0..=maximum)
            .hexadecimal(2, false, true),
    );
    ui.end_row();
}
