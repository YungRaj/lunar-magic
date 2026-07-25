use crate::level_editor_forms;
use eframe::egui;
use lm_app::MwlDocumentController;
use lm_level::{LegacyLevelHeader, LevelObjectData, ObjectEdit, ObjectRecord};

#[derive(Default)]
pub(super) struct MwlObjectPanel {
    loaded_revision: Option<u64>,
    layer1: Option<LevelObjectData>,
    selected: usize,
    header: String,
    record: String,
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
    ) -> Result<bool, String> {
        ui.collapsing("Typed Layer 1 objects", |ui| {
            self.ensure_loaded(controller)?;
            self.contents(ui, controller)
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
    ) -> Result<bool, String> {
        let count = self
            .layer1
            .as_ref()
            .expect("loaded Layer 1")
            .objects
            .records
            .len();
        ui.label(format!(
            "{count} ordered standard/extended/custom object records"
        ));
        ui.label("Exact five-byte legacy level header:");
        ui.text_edit_singleline(&mut self.header);
        if ui.button("Stage exact header").clicked() {
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

        ui.label("Object record (3–8 hexadecimal bytes):");
        ui.text_edit_singleline(&mut self.record);
        self.record_controls(ui)?;
        self.reorder_controls(ui)?;

        let mut committed = false;
        if ui.button("Commit typed Layer 1 objects").clicked() {
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

    fn record_controls(&mut self, ui: &mut egui::Ui) -> Result<(), String> {
        ui.horizontal(|ui| -> Result<(), String> {
            if ui.button("Insert before").clicked() {
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
            if ui.button("Replace").clicked() {
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
            if ui.button("Delete").clicked() {
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

    fn reorder_controls(&mut self, ui: &mut egui::Ui) -> Result<(), String> {
        ui.horizontal(|ui| -> Result<(), String> {
            if ui.button("Move up").clicked() && self.selected > 0 {
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
            if ui.button("Move down").clicked() && self.selected + 1 < len {
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
    }
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
}
