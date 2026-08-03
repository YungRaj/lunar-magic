use super::{AggregatePanels, PasteTarget, index, pasted_text};
use crate::{level_editor_forms, native_clipboard, native_level_document_form};
use eframe::egui;
use lm_app::{NativeLevelAssetsControllerEdit, NativeLevelEdit};
use lm_level::{ObjectEdit, SpriteLengthTable, SpriteToken};
use lm_project::{LoadedLevelSlot, NativeLevelAssetsFile};

impl AggregatePanels {
    pub(super) fn level_panel(
        &mut self,
        ui: &mut egui::Ui,
        file: &NativeLevelAssetsFile,
        sprite_lengths: &SpriteLengthTable,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let level = &file.assets.level;
        ui.label(format!(
            "Source slot {:04X}; header {}",
            file.source_slot,
            level_editor_forms::format_bytes(&level.layer1.header.encoded())
        ));
        let mut stage_header = false;
        ui.collapsing("Level header", |ui| {
            egui::Grid::new("installed-native-level-header").show(ui, |ui| {
                header_row(ui, "Level mode", &mut self.header.level_mode, 31);
                header_row(
                    ui,
                    "Background palette",
                    &mut self.header.background_palette,
                    7,
                );
                header_row(ui, "Last screen", &mut self.header.last_screen, 31);
                header_row(ui, "Background color", &mut self.header.background_color, 7);
                header_row(ui, "Sprite tileset", &mut self.header.sprite_tileset, 15);
                header_row(
                    ui,
                    "Default music selector",
                    &mut self.header.default_music_selector,
                    7,
                );
                header_row(
                    ui,
                    "Time limit selector",
                    &mut self.header.time_limit_selector,
                    3,
                );
                ui.label("Custom time bypass");
                ui.checkbox(&mut self.header.custom_time_enabled, "Enabled");
                ui.end_row();
                ui.label("Custom time (hex)");
                ui.add_enabled(
                    self.header.custom_time_enabled,
                    egui::DragValue::new(&mut self.header.custom_time_value)
                        .range(0..=lm_level::CustomTimeSettings::MAX_VALUE)
                        .hexadecimal(3, false, true),
                );
                ui.end_row();
                ui.label("Force time reset");
                ui.add_enabled(
                    self.header.custom_time_enabled,
                    egui::Checkbox::without_text(&mut self.header.force_time_reset),
                );
                ui.end_row();
                header_row(
                    ui,
                    "Foreground palette",
                    &mut self.header.foreground_palette,
                    7,
                );
                header_row(ui, "Sprite palette", &mut self.header.sprite_palette, 7);
                header_row(ui, "Object tileset", &mut self.header.object_tileset, 15);
                header_row(
                    ui,
                    "Layer 1 vertical scroll",
                    &mut self.header.layer1_vertical_scroll,
                    3,
                );
            });
            ui.horizontal(|ui| {
                if ui.button("Stage header changes").clicked() {
                    stage_header = true;
                }
                if ui.button("Reset staged values").clicked() {
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
        ui.heading(format!("Objects ({})", level.layer1.objects.records.len()));
        index(
            ui,
            &mut self.object_index,
            level.layer1.objects.records.len(),
        );
        ui.text_edit_singleline(&mut self.level_record.object);
        object_semantic_fields(ui, &mut self.level_record);
        let mut action = None;
        let mut apply_object_fields = false;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Load").clicked() {
                let screen = object_screen(level, self.object_index);
                self.level_record
                    .load_object(level.layer1.objects.records.get(self.object_index), screen);
            }
            for (label, value) in [("Insert", 0), ("Replace", 1), ("Remove", 2)] {
                if ui.button(label).clicked() {
                    action = Some(value);
                }
            }
            if ui
                .add_enabled(
                    self.level_record.object_fields_loaded
                        && self.object_index < level.layer1.objects.records.len(),
                    egui::Button::new("Apply object fields"),
                )
                .clicked()
            {
                apply_object_fields = true;
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
        if apply_object_fields {
            return Some(
                self.level_record
                    .object_field_edit(self.object_index)
                    .map(|edit| NativeLevelAssetsControllerEdit::Level(vec![edit])),
            );
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
        self.sprite_panel(ui, level, sprite_lengths)
    }

    fn sprite_panel(
        &mut self,
        ui: &mut egui::Ui,
        level: &LoadedLevelSlot,
        sprite_lengths: &SpriteLengthTable,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        ui.heading(format!("Sprite tokens ({})", level.sprites.tokens.len()));
        index(ui, &mut self.sprite_index, level.sprites.tokens.len());
        ui.horizontal(|ui| {
            ui.label("Header");
            ui.text_edit_singleline(&mut self.sprite_header);
        });
        ui.text_edit_singleline(&mut self.level_record.sprite);
        sprite_semantic_fields(ui, &mut self.level_record);
        let mut action = None;
        let mut apply_sprite_fields = false;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Load record").clicked() {
                self.level_record
                    .load_sprite(level.sprites.tokens.get(self.sprite_index));
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
            if ui
                .add_enabled(
                    self.level_record.sprite_fields_loaded
                        && self.sprite_index < level.sprites.tokens.len(),
                    egui::Button::new("Apply sprite fields"),
                )
                .clicked()
            {
                apply_sprite_fields = true;
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
        if apply_sprite_fields {
            return Some(
                self.level_record
                    .sprite_field_edit(
                        self.sprite_index,
                        level.sprites.tokens.get(self.sprite_index),
                        sprite_lengths,
                    )
                    .map(|edit| NativeLevelAssetsControllerEdit::Level(vec![edit])),
            );
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

fn semantic_field_row(ui: &mut egui::Ui, label: &str, value: &mut u8, maximum: u8) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).range(0..=maximum));
    ui.end_row();
}

fn object_semantic_fields(
    ui: &mut egui::Ui,
    form: &mut native_level_document_form::NativeLevelRecordForm,
) {
    egui::Grid::new("native-assets-object-semantic-fields").show(ui, |ui| {
        semantic_field_row(ui, "Command", &mut form.object_command, 0x3f);
        semantic_field_row(ui, "Parameter", &mut form.object_parameter, 0xff);
        semantic_field_row(ui, "First coordinate", &mut form.object_first, 0x0f);
        semantic_field_row(ui, "Second coordinate", &mut form.object_second, 0x0f);
        ui.label("Screen");
        ui.add(egui::DragValue::new(&mut form.object_screen).range(0..=0x1f));
        ui.end_row();
    });
}

fn object_screen(level: &LoadedLevelSlot, index: usize) -> Option<u16> {
    level
        .layer1
        .objects
        .native_placements()
        .into_iter()
        .find(|placement| placement.record_index == index)
        .map(|placement| placement.screen)
}

fn sprite_semantic_fields(
    ui: &mut egui::Ui,
    form: &mut native_level_document_form::NativeLevelRecordForm,
) {
    egui::Grid::new("native-assets-sprite-semantic-fields").show(ui, |ui| {
        semantic_field_row(ui, "Sprite number", &mut form.sprite_number, 0xff);
        semantic_field_row(ui, "Screen", &mut form.sprite_screen, 0x1f);
        semantic_field_row(ui, "X", &mut form.sprite_x, 0x0f);
        semantic_field_row(ui, "Y (low 5 bits)", &mut form.sprite_y_low, 0x1f);
        semantic_field_row(ui, "Extra bits", &mut form.sprite_extra_bits, 3);
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
