use super::ExAnimationEditor;
use crate::exanimation_form::{self, RecordForm};
use eframe::egui;
use lm_app::ExAnimationControllerEdit;

impl ExAnimationEditor {
    pub(super) fn record_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("Records");
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let labels = document
            .controller
            .value()
            .animation
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| format!("{index:02X}: kind {:02X}", record.kind()))
            .collect::<Vec<_>>();
        let has_records = !labels.is_empty();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (index, label) in labels.into_iter().enumerate() {
                if ui
                    .selectable_value(&mut self.selected_record, index, label)
                    .clicked()
                {
                    self.loaded_record = None;
                    self.selected_frame = 0;
                }
            }
        });
        if ui.button("Append new record").clicked() {
            self.record = RecordForm::default();
            self.record_editable = true;
            self.apply_record(true);
        }
        if ui
            .add_enabled(has_records, egui::Button::new("Remove selected"))
            .clicked()
        {
            self.apply_edits(&[ExAnimationControllerEdit::RemoveRecord {
                index: self.selected_record,
            }]);
            self.selected_record = self.selected_record.saturating_sub(1);
        }
    }

    pub(super) fn properties(&mut self, ui: &mut egui::Ui) {
        ui.heading("Slot settings");
        ui.horizontal(|ui| {
            ui.label("Setting (hex)");
            ui.text_edit_singleline(&mut self.global.setting);
        });
        ui.horizontal(|ui| {
            ui.label("Header (hex)");
            ui.text_edit_singleline(&mut self.global.header);
        });
        if ui.button("Apply slot settings").clicked() {
            self.apply_global();
        }
        self.trigger_properties(ui);
        ui.separator();
        self.record_properties(ui);
    }

    fn trigger_properties(&mut self, ui: &mut egui::Ui) {
        ui.heading("Trigger");
        if ui
            .add(egui::Slider::new(&mut self.trigger_index, 0..=15))
            .changed()
        {
            self.load_trigger();
        }
        ui.checkbox(&mut self.trigger_enabled, "Enabled");
        ui.horizontal(|ui| {
            ui.label("Value (hex)");
            ui.add_enabled(
                self.trigger_enabled,
                egui::TextEdit::singleline(&mut self.trigger_value),
            );
        });
        if ui.button("Apply trigger").clicked() {
            let value = if self.trigger_enabled {
                exanimation_form::hex_u8(&self.trigger_value, "trigger value").map(Some)
            } else {
                Ok(None)
            };
            match value {
                Ok(value) => self.apply_edits(&[ExAnimationControllerEdit::SetTrigger {
                    trigger: self.trigger_index,
                    value,
                }]),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn record_properties(&mut self, ui: &mut egui::Ui) {
        let record_exists = self.document.as_ref().is_some_and(|document| {
            self.selected_record < document.controller.value().animation.records.len()
        });
        ui.heading(format!("Record {:02X}", self.selected_record));
        for (label, field) in [
            ("Kind (hex)", &mut self.record.kind),
            ("Size mode (hex)", &mut self.record.size_mode),
            ("Destination (hex)", &mut self.record.destination),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.text_edit_singleline(field);
            });
        }
        ui.checkbox(&mut self.record.destination_flag, "Destination flag");
        ui.label("Source words, one frame per line:");
        ui.add(egui::TextEdit::multiline(&mut self.record.frames).desired_rows(8));
        if !self.record_editable {
            ui.label("This special transfer kind has no ordinary source-word frame payload.");
        }
        self.frame_clipboard(ui, record_exists);
        if ui
            .add_enabled(
                self.record_editable && record_exists,
                egui::Button::new("Apply record"),
            )
            .clicked()
        {
            self.apply_record(false);
        }
    }
}
