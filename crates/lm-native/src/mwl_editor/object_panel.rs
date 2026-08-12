use super::OptionalCatalogText;
use crate::level_editor_forms;
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog, MwlDocumentController};
use lm_level::{
    LegacyLevelHeader, LevelObjectData, ObjectCoordinateNibbles, ObjectEdit, ObjectRecord,
};

#[derive(Default)]
pub(super) struct MwlObjectPanel {
    loaded_revision: Option<u64>,
    layer1: Option<LevelObjectData>,
    selected: usize,
    header: String,
    record: String,
    command_id: String,
    parameter: String,
    first_coordinate: String,
    second_coordinate: String,
    advances_screen: bool,
    jump_target: String,
}

impl MwlObjectPanel {
    pub(super) fn invalidate(&mut self) {
        self.loaded_revision = None;
        self.layer1 = None;
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        controller: &mut MwlDocumentController,
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<bool, String> {
        ui.collapsing(catalog.extended_text(Key::MwlObjectHeading), |ui| {
            self.ensure_loaded(controller)?;
            self.contents(ui, controller, catalog)
        })
        .body_returned
        .transpose()
        .map(Option::unwrap_or_default)
    }

    fn ensure_loaded(&mut self, controller: &MwlDocumentController) -> Result<(), String> {
        if self.loaded_revision == Some(controller.revision()) {
            return Ok(());
        }
        self.layer1 = Some(controller.layer1().map_err(|error| error.to_string())?);
        self.loaded_revision = Some(controller.revision());
        self.selected = 0;
        self.load_form();
        Ok(())
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        controller: &mut MwlDocumentController,
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<bool, String> {
        let count = self
            .layer1
            .as_ref()
            .expect("loaded Layer 1")
            .objects
            .records
            .len();
        ui.label(
            catalog
                .extended_text(Key::MwlObjectCountFormat)
                .replace("{count}", &count.to_string()),
        );
        ui.label(catalog.extended_text(Key::MwlObjectHeader));
        ui.text_edit_singleline(&mut self.header);
        if ui
            .button(catalog.extended_text(Key::MwlObjectStageHeader))
            .clicked()
        {
            self.stage_header()?;
        }

        let mut new_selection = None;
        egui::ScrollArea::vertical()
            .id_salt("mwl-layer1-objects")
            .max_height(180.0)
            .show(ui, |ui| {
                let records = &self
                    .layer1
                    .as_ref()
                    .expect("loaded Layer 1")
                    .objects
                    .records;
                for (index, record) in records.iter().enumerate() {
                    let label = format!(
                        "{index:03}: {}",
                        level_editor_forms::format_bytes(record.encoded())
                    );
                    if ui.selectable_label(self.selected == index, label).clicked() {
                        new_selection = Some(index);
                    }
                }
            });
        if let Some(index) = new_selection {
            self.selected = index;
            self.load_form();
        }

        ui.label(catalog.extended_text(Key::MwlObjectRecord));
        ui.text_edit_singleline(&mut self.record);
        self.record_controls(ui, catalog)?;
        self.reorder_controls(ui, catalog)?;
        self.semantic_controls(ui, catalog)?;

        let mut committed = false;
        if ui
            .button(catalog.extended_text(Key::MwlObjectCommit))
            .clicked()
        {
            controller
                .replace_layer1(
                    controller.revision(),
                    self.layer1.as_ref().expect("loaded Layer 1"),
                )
                .map_err(|error| error.to_string())?;
            self.invalidate();
            committed = true;
        }
        Ok(committed)
    }

    fn semantic_controls(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<(), String> {
        ui.label(catalog.extended_text(Key::MwlObjectRecoveredFields));
        egui::Grid::new("mwl-object-semantic-fields")
            .num_columns(2)
            .show(ui, |ui| {
                field(
                    ui,
                    catalog.extended_text(Key::MwlObjectCommandId),
                    &mut self.command_id,
                );
                field(
                    ui,
                    catalog.extended_text(Key::MwlObjectParameter),
                    &mut self.parameter,
                );
                field(
                    ui,
                    catalog.extended_text(Key::MwlObjectFirstCoordinate),
                    &mut self.first_coordinate,
                );
                field(
                    ui,
                    catalog.extended_text(Key::MwlObjectSecondCoordinate),
                    &mut self.second_coordinate,
                );
            });
        ui.checkbox(
            &mut self.advances_screen,
            catalog.extended_text(Key::MwlObjectAdvancesScreen),
        );
        if ui
            .button(catalog.extended_text(Key::MwlObjectStageFields))
            .clicked()
        {
            self.stage_semantic_fields()?;
        }
        let jump = self
            .layer1
            .as_ref()
            .and_then(|layer| layer.objects.records.get(self.selected))
            .and_then(ObjectRecord::screen_jump);
        if let Some(jump) = jump {
            ui.label(
                catalog
                    .extended_text(Key::MwlObjectJumpEncodingFormat)
                    .replace("{encoding}", &format!("{:?}", jump.encoding)),
            );
            let suffix = if jump.resolved_screen() <= 0x1f {
                ""
            } else {
                catalog.extended_text(Key::MwlObjectOutsideScreenSuffix)
            };
            ui.label(
                catalog
                    .extended_text(Key::MwlObjectResolvedScreenFormat)
                    .replace("{screen}", &format!("{:02X}", jump.resolved_screen()))
                    .replace("{suffix}", suffix),
            );
            ui.horizontal(|ui| {
                ui.label(catalog.extended_text(Key::MwlObjectJumpTarget));
                ui.text_edit_singleline(&mut self.jump_target);
            });
            if ui
                .button(catalog.extended_text(Key::MwlObjectStageJumpTarget))
                .clicked()
            {
                let packed_target =
                    level_editor_forms::parse_hex_u16(&self.jump_target, "screen-jump target")?;
                self.apply_object_edit(ObjectEdit::SetScreenJumpTarget {
                    index: self.selected,
                    packed_target,
                })?;
                self.load_form();
            }
        }
        Ok(())
    }

    fn stage_semantic_fields(&mut self) -> Result<(), String> {
        let edits = [
            ObjectEdit::SetCommandId {
                index: self.selected,
                command_id: level_editor_forms::parse_hex_u8(
                    &self.command_id,
                    "object command ID",
                )?,
            },
            ObjectEdit::SetParameter {
                index: self.selected,
                parameter: level_editor_forms::parse_hex_u8(&self.parameter, "object parameter")?,
            },
            ObjectEdit::SetCoordinateNibbles {
                index: self.selected,
                coordinates: ObjectCoordinateNibbles {
                    first: level_editor_forms::parse_hex_u8(
                        &self.first_coordinate,
                        "first object coordinate",
                    )?,
                    second: level_editor_forms::parse_hex_u8(
                        &self.second_coordinate,
                        "second object coordinate",
                    )?,
                },
            },
            ObjectEdit::SetAdvancesScreen {
                index: self.selected,
                advances: self.advances_screen,
            },
        ];
        self.layer1
            .as_mut()
            .expect("loaded Layer 1")
            .objects
            .apply_edits(&edits)
            .map_err(|error| error.to_string())?;
        self.load_form();
        Ok(())
    }

    fn apply_object_edit(&mut self, edit: ObjectEdit) -> Result<(), String> {
        self.layer1
            .as_mut()
            .expect("loaded Layer 1")
            .objects
            .apply_edits(&[edit])
            .map_err(|error| error.to_string())
    }

    fn record_controls(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<(), String> {
        ui.horizontal(|ui| -> Result<(), String> {
            if ui
                .button(catalog.extended_text(Key::MwlInsertBefore))
                .clicked()
            {
                let record = self.form_record()?;
                let layer1 = self.layer1.as_mut().expect("loaded Layer 1");
                let index = self.selected.min(layer1.objects.records.len());
                layer1
                    .objects
                    .apply_edits(&[ObjectEdit::Insert { index, record }])
                    .map_err(|error| error.to_string())?;
                self.selected = index;
                self.load_form();
            }
            if ui.button(catalog.extended_text(Key::MwlReplace)).clicked() {
                let record = self.form_record()?;
                let layer1 = self.layer1.as_mut().expect("loaded Layer 1");
                if self.selected >= layer1.objects.records.len() {
                    return Err("select an existing Layer 1 object to replace".into());
                }
                layer1
                    .objects
                    .apply_edits(&[ObjectEdit::Replace {
                        index: self.selected,
                        record,
                    }])
                    .map_err(|error| error.to_string())?;
                self.load_form();
            }
            if ui.button(catalog.extended_text(Key::MwlDelete)).clicked() {
                let layer1 = self.layer1.as_mut().expect("loaded Layer 1");
                layer1
                    .objects
                    .apply_edits(&[ObjectEdit::Remove {
                        index: self.selected,
                    }])
                    .map_err(|error| error.to_string())?;
                self.selected = self
                    .selected
                    .min(layer1.objects.records.len().saturating_sub(1));
                self.load_form();
            }
            Ok(())
        })
        .inner
    }

    fn reorder_controls(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) -> Result<(), String> {
        ui.horizontal(|ui| -> Result<(), String> {
            if ui.button(catalog.extended_text(Key::MwlMoveUp)).clicked() && self.selected > 0 {
                let before = self.selected - 1;
                self.layer1
                    .as_mut()
                    .expect("loaded Layer 1")
                    .objects
                    .apply_edits(&[ObjectEdit::MoveBefore {
                        from: self.selected,
                        before,
                    }])
                    .map_err(|error| error.to_string())?;
                self.selected = before;
            }
            let len = self
                .layer1
                .as_ref()
                .expect("loaded Layer 1")
                .objects
                .records
                .len();
            if ui.button(catalog.extended_text(Key::MwlMoveDown)).clicked()
                && self.selected + 1 < len
            {
                self.layer1
                    .as_mut()
                    .expect("loaded Layer 1")
                    .objects
                    .apply_edits(&[ObjectEdit::MoveBefore {
                        from: self.selected,
                        before: self.selected + 2,
                    }])
                    .map_err(|error| error.to_string())?;
                self.selected += 1;
            }
            Ok(())
        })
        .inner
    }

    fn stage_header(&mut self) -> Result<(), String> {
        let bytes = level_editor_forms::parse_bytes(&self.header, "legacy level-header byte")?;
        let header = LegacyLevelHeader::decode(&bytes)
            .map_err(|error| format!("invalid five-byte legacy level header: {error}"))?;
        self.layer1.as_mut().expect("loaded Layer 1").header = header;
        Ok(())
    }

    fn form_record(&self) -> Result<ObjectRecord, String> {
        ObjectRecord::new(level_editor_forms::parse_bytes(
            &self.record,
            "Layer 1 object byte",
        )?)
        .map_err(|error| error.to_string())
    }

    fn load_form(&mut self) {
        let Some(layer1) = self.layer1.as_ref() else {
            return;
        };
        self.header = level_editor_forms::format_bytes(&layer1.header.encoded());
        self.record = layer1
            .objects
            .records
            .get(self.selected)
            .map_or_else(String::new, |record| {
                level_editor_forms::format_bytes(record.encoded())
            });
        if let Some(record) = layer1.objects.records.get(self.selected) {
            let coordinates = record.coordinate_nibbles();
            self.command_id = format!("{:02X}", record.command_id());
            self.parameter = format!("{:02X}", record.parameter());
            self.first_coordinate = format!("{:X}", coordinates.first);
            self.second_coordinate = format!("{:X}", coordinates.second);
            self.advances_screen = record.advances_screen();
            self.jump_target = record
                .screen_jump()
                .map_or_else(String::new, |jump| format!("{:04X}", jump.packed_target));
        } else {
            self.command_id.clear();
            self.parameter.clear();
            self.first_coordinate.clear();
            self.second_coordinate.clear();
            self.advances_screen = false;
            self.jump_target.clear();
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.text_edit_singleline(value);
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_form_accepts_every_supported_width_and_rejects_terminators() {
        for len in 3..=8 {
            let panel = MwlObjectPanel {
                record: std::iter::repeat_n("12", len).collect::<Vec<_>>().join(" "),
                ..MwlObjectPanel::default()
            };
            assert_eq!(panel.form_record().unwrap().encoded().len(), len);
        }
        for record in ["01 02", "FF 02 03", "01 02 03 04 05 06 07 08 09"] {
            let panel = MwlObjectPanel {
                record: record.into(),
                ..MwlObjectPanel::default()
            };
            assert!(panel.form_record().is_err());
        }
    }

    #[test]
    fn exact_header_form_changes_no_object_bytes() {
        let mut panel = MwlObjectPanel {
            layer1: Some(LevelObjectData::parse(&[1, 2, 3, 4, 5, 0x11, 0x22, 0x33, 0xff]).unwrap()),
            header: "AA BB CC DD EE".into(),
            ..MwlObjectPanel::default()
        };
        let records = panel.layer1.as_ref().unwrap().objects.clone();
        panel.stage_header().unwrap();
        assert_eq!(
            panel.layer1.as_ref().unwrap().header.encoded(),
            [0xaa, 0xbb, 0xcc, 0xdd, 0xee]
        );
        assert_eq!(panel.layer1.as_ref().unwrap().objects, records);
        panel.header = "00 01".into();
        assert!(panel.stage_header().is_err());
    }

    #[test]
    fn recovered_field_form_preserves_extension_bytes_and_rejects_shape_changes() {
        let mut panel = MwlObjectPanel {
            layer1: Some(LevelObjectData {
                header: LegacyLevelHeader::decode(&[0; 5]).unwrap(),
                objects: lm_level::ObjectStream {
                    records: vec![ObjectRecord::new(vec![0x9f, 0x0a, 1, 0xaa]).unwrap()],
                },
            }),
            selected: 0,
            ..MwlObjectPanel::default()
        };
        panel.load_form();
        panel.command_id = "22".into();
        panel.parameter = "0F".into();
        panel.first_coordinate = "3".into();
        panel.second_coordinate = "4".into();
        panel.advances_screen = true;
        panel.stage_semantic_fields().unwrap();
        let record = &panel.layer1.as_ref().unwrap().objects.records[0];
        assert_eq!(record.command_id(), 0x22);
        assert_eq!(record.parameter(), 0x0f);
        assert_eq!(record.encoded()[3], 0xaa);

        let before = record.clone();
        panel.command_id = "01".into();
        assert!(panel.stage_semantic_fields().is_err());
        assert_eq!(panel.layer1.as_ref().unwrap().objects.records[0], before);
    }
}
