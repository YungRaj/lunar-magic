use super::{AggregatePanels, PasteTarget, index, pasted_text};
use crate::{level_editor_forms, native_clipboard};
use eframe::egui;
use lm_app::NativeLevelAssetsControllerEdit;
use lm_level::{NativeLayer2Data, ObjectEdit};

impl AggregatePanels {
    pub(super) fn layer2_panel(
        &mut self,
        ui: &mut egui::Ui,
        layer2: &NativeLayer2Data,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        match layer2 {
            NativeLayer2Data::Objects(objects) => self.layer2_objects_panel(ui, objects),
            NativeLayer2Data::Tilemap(bytes) => self.layer2_tilemap_panel(ui, bytes),
        }
    }

    fn layer2_objects_panel(
        &mut self,
        ui: &mut egui::Ui,
        objects: &lm_level::LevelObjectData,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        ui.heading(format!(
            "Layer 2 objects ({})",
            objects.objects.records.len()
        ));
        index(
            ui,
            &mut self.layer2_object_index,
            objects.objects.records.len(),
        );
        ui.text_edit_singleline(&mut self.layer2_object);
        let mut action = None;
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Load").clicked() {
                self.layer2_object = objects
                    .objects
                    .records
                    .get(self.layer2_object_index)
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
                    self.layer2_object_index < objects.objects.records.len(),
                    egui::Button::new("Copy"),
                )
                .clicked()
            {
                let record = &objects.objects.records[self.layer2_object_index];
                match native_clipboard::encode_level_object(record) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui.button("Paste").clicked() {
                self.paste_target = Some(PasteTarget::Layer2Object);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(error) = copy_error {
            return Some(Err(error));
        }
        if self.paste_target == Some(PasteTarget::Layer2Object)
            && let Some(text) = pasted_text(ui)
        {
            self.paste_target = None;
            return Some(native_clipboard::decode_level_object(&text).map(|record| {
                NativeLevelAssetsControllerEdit::Layer2Objects(vec![ObjectEdit::Replace {
                    index: self.layer2_object_index,
                    record,
                }])
            }));
        }
        action.map(|action| {
            let edit = match action {
                2 => Ok(ObjectEdit::Remove {
                    index: self.layer2_object_index,
                }),
                _ => level_editor_forms::parse_object(&self.layer2_object).map(|record| {
                    if action == 0 {
                        ObjectEdit::Insert {
                            index: self.layer2_object_index,
                            record,
                        }
                    } else {
                        ObjectEdit::Replace {
                            index: self.layer2_object_index,
                            record,
                        }
                    }
                }),
            };
            edit.map(|edit| NativeLevelAssetsControllerEdit::Layer2Objects(vec![edit]))
        })
    }

    fn layer2_tilemap_panel(
        &mut self,
        ui: &mut egui::Ui,
        bytes: &[u8],
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let words = bytes.len() / 2;
        ui.heading(format!("Layer 2 tilemap ({words} words)"));
        index(ui, &mut self.layer2_tile_index, words.saturating_sub(1));
        ui.horizontal(|ui| {
            ui.label("16-bit tile word");
            ui.text_edit_singleline(&mut self.layer2_tile);
            if ui.button("Load").clicked()
                && let Some(bytes) =
                    bytes.get(self.layer2_tile_index * 2..self.layer2_tile_index * 2 + 2)
            {
                self.layer2_tile = format!("{:04X}", u16::from_le_bytes([bytes[0], bytes[1]]));
            }
        });
        ui.button("Apply tile").clicked().then(|| {
            level_editor_forms::parse_hex_u16(&self.layer2_tile, "Layer 2 tile").map(|word| {
                NativeLevelAssetsControllerEdit::Layer2TilemapWords(vec![(
                    self.layer2_tile_index,
                    word,
                )])
            })
        })
    }
}
