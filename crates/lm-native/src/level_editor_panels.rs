use crate::{
    level_editor_advanced::LevelAdvancedPanelState,
    level_editor_auxiliary::LevelAuxiliaryPanelState,
    level_editor_forms::{self, EntranceForm},
    native_clipboard,
};
use eframe::egui;
use lm_app::CompleteLevelDocumentEdit;
use lm_app::LocalizationCatalog;
use lm_level::{
    CompleteLevelFile, LevelAuxiliaryEdit, LevelLayer, ObjectEdit, SequenceEdit, SpriteEdit,
};

mod entrance_form;
mod header;

use entrance_form::entrance_fields;
use header::show_header;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Panel {
    #[default]
    Header,
    Objects,
    Sprites,
    Entrances,
    Auxiliary,
    Advanced,
}

#[derive(Default)]
pub(crate) struct LevelPanelState {
    panel: Panel,
    object_layer: usize,
    object_index: usize,
    object_bytes: String,
    object_key: Option<(u64, usize, usize)>,
    sprite_index: usize,
    sprite_bytes: String,
    sprite_key: Option<(u64, usize)>,
    entrance_index: usize,
    entrance: EntranceForm,
    entrance_key: Option<(u64, usize)>,
    auxiliary: LevelAuxiliaryPanelState,
    advanced: LevelAdvancedPanelState,
}

impl LevelPanelState {
    pub(crate) fn invalidate(&mut self) {
        self.object_key = None;
        self.sprite_key = None;
        self.entrance_key = None;
        self.auxiliary.invalidate();
        self.advanced.invalidate();
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.panel, Panel::Header, "Header");
            ui.selectable_value(&mut self.panel, Panel::Objects, "Objects");
            ui.selectable_value(&mut self.panel, Panel::Sprites, "Sprites");
            ui.selectable_value(&mut self.panel, Panel::Entrances, "Entrances");
            ui.selectable_value(&mut self.panel, Panel::Auxiliary, "Exits/Map16");
            ui.selectable_value(&mut self.panel, Panel::Advanced, "Advanced");
        });
        ui.separator();
        match self.panel {
            Panel::Header => show_header(ui, level),
            Panel::Objects => self.show_objects(ui, level, revision),
            Panel::Sprites => self.show_sprites(ui, level, revision),
            Panel::Entrances => self.show_entrances(ui, level, revision),
            Panel::Auxiliary => self.auxiliary.show(ui, level, revision, catalog),
            Panel::Advanced => self.advanced.show(ui, level, revision),
        }
    }

    fn show_objects(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.object_layer, 0, "Layer 1");
            ui.selectable_value(&mut self.object_layer, 1, "Layer 2");
        });
        let records = if self.object_layer == 0 {
            &level.0.layer1.objects.records
        } else {
            &level.0.layer2.objects.records
        };
        if !records.is_empty() {
            self.object_index = self.object_index.min(records.len() - 1);
        }
        ui.add(
            egui::Slider::new(&mut self.object_index, 0..=records.len().saturating_sub(1))
                .text("Record"),
        );
        let key = (revision, self.object_layer, self.object_index);
        if self.object_key != Some(key) {
            self.object_bytes = records
                .get(self.object_index)
                .map_or_else(String::new, |record| {
                    level_editor_forms::format_bytes(record.encoded())
                });
            self.object_key = Some(key);
        }
        ui.label("Lossless encoded bytes (3–8 bytes):");
        ui.text_edit_singleline(&mut self.object_bytes);
        let mut action = None;
        let mut remove = false;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Append").clicked() {
                action = Some(true);
            }
            if ui
                .add_enabled(!records.is_empty(), egui::Button::new("Replace"))
                .clicked()
            {
                action = Some(false);
            }
            if ui
                .add_enabled(!records.is_empty(), egui::Button::new("Remove"))
                .clicked()
            {
                remove = true;
            }
            if ui
                .add_enabled(!records.is_empty(), egui::Button::new("Copy"))
                .clicked()
            {
                match native_clipboard::encode_level_object(&records[self.object_index]) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui.button("Paste").clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(error) = copy_error {
            return Some(Err(error));
        }
        if let Some(text) = pasted_text(ui) {
            return Some(native_clipboard::decode_level_object(&text).map(|record| {
                vec![self.object_edit(ObjectEdit::Replace {
                    index: self.object_index,
                    record,
                })]
            }));
        }
        if remove {
            let edit = self.object_edit(ObjectEdit::Remove {
                index: self.object_index,
            });
            return Some(Ok(vec![edit]));
        }
        action.map(|append| {
            level_editor_forms::parse_object(&self.object_bytes).map(|record| {
                let edit = if append {
                    ObjectEdit::Insert {
                        index: records.len(),
                        record,
                    }
                } else {
                    ObjectEdit::Replace {
                        index: self.object_index,
                        record,
                    }
                };
                vec![self.object_edit(edit)]
            })
        })
    }

    fn object_edit(&self, edit: ObjectEdit) -> CompleteLevelDocumentEdit {
        CompleteLevelDocumentEdit::LayerObject {
            layer: if self.object_layer == 0 {
                LevelLayer::Layer1
            } else {
                LevelLayer::Layer2
            },
            edit,
        }
    }

    fn show_sprites(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let records = &level.0.sprites.records;
        if !records.is_empty() {
            self.sprite_index = self.sprite_index.min(records.len() - 1);
        }
        ui.label(format!("Stream header: {:02X}", level.0.sprites.header));
        ui.add(
            egui::Slider::new(&mut self.sprite_index, 0..=records.len().saturating_sub(1))
                .text("Record"),
        );
        let key = (revision, self.sprite_index);
        if self.sprite_key != Some(key) {
            self.sprite_bytes = records
                .get(self.sprite_index)
                .map_or_else(String::new, |record| {
                    level_editor_forms::format_bytes(&record.encoded)
                });
            self.sprite_key = Some(key);
        }
        ui.label("Lossless revision-sized encoded bytes:");
        ui.text_edit_singleline(&mut self.sprite_bytes);
        let mut operation = None;
        let mut remove = false;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Append").clicked() {
                operation = Some(true);
            }
            if ui
                .add_enabled(!records.is_empty(), egui::Button::new("Replace"))
                .clicked()
            {
                operation = Some(false);
            }
            if ui
                .add_enabled(!records.is_empty(), egui::Button::new("Remove"))
                .clicked()
            {
                remove = true;
            }
            if ui
                .add_enabled(!records.is_empty(), egui::Button::new("Copy"))
                .clicked()
            {
                match native_clipboard::encode_level_sprite(&records[self.sprite_index]) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui.button("Paste").clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(error) = copy_error {
            return Some(Err(error));
        }
        if let Some(text) = pasted_text(ui) {
            return Some(native_clipboard::decode_level_sprite(&text).map(|record| {
                vec![CompleteLevelDocumentEdit::Sprite(SpriteEdit::Replace {
                    index: self.sprite_index,
                    record,
                })]
            }));
        }
        if remove {
            return Some(Ok(vec![CompleteLevelDocumentEdit::Sprite(
                SpriteEdit::Remove {
                    index: self.sprite_index,
                },
            )]));
        }
        operation.map(|append| {
            level_editor_forms::parse_sprite(&self.sprite_bytes).map(|record| {
                vec![CompleteLevelDocumentEdit::Sprite(if append {
                    SpriteEdit::Insert {
                        index: records.len(),
                        record,
                    }
                } else {
                    SpriteEdit::Replace {
                        index: self.sprite_index,
                        record,
                    }
                })]
            })
        })
    }

    fn show_entrances(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let values = &level.0.entrances;
        if !values.is_empty() {
            self.entrance_index = self.entrance_index.min(values.len() - 1);
        }
        ui.add(
            egui::Slider::new(&mut self.entrance_index, 0..=values.len().saturating_sub(1))
                .text("Entrance"),
        );
        let key = (revision, self.entrance_index);
        if self.entrance_key != Some(key) {
            self.entrance = values
                .get(self.entrance_index)
                .copied()
                .map_or_else(EntranceForm::default, EntranceForm::load);
            self.entrance_key = Some(key);
        }
        entrance_fields(ui, &mut self.entrance);
        let mut operation = None;
        let mut remove = false;
        ui.horizontal(|ui| {
            if ui.button("Append").clicked() {
                operation = Some(true);
            }
            if ui
                .add_enabled(!values.is_empty(), egui::Button::new("Replace"))
                .clicked()
            {
                operation = Some(false);
            }
            if ui
                .add_enabled(!values.is_empty(), egui::Button::new("Remove"))
                .clicked()
            {
                remove = true;
            }
        });
        if remove {
            return Some(Ok(vec![CompleteLevelDocumentEdit::Auxiliary(
                LevelAuxiliaryEdit::Entrance(SequenceEdit::Remove {
                    index: self.entrance_index,
                }),
            )]));
        }
        operation.map(|append| {
            self.entrance.parse().map(|value| {
                vec![CompleteLevelDocumentEdit::Auxiliary(
                    LevelAuxiliaryEdit::Entrance(if append {
                        SequenceEdit::Insert {
                            index: values.len(),
                            value,
                        }
                    } else {
                        SequenceEdit::Replace {
                            index: self.entrance_index,
                            value,
                        }
                    }),
                )]
            })
        })
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
