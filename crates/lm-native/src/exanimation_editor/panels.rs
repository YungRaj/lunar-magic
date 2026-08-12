use super::ExAnimationEditor;
use crate::exanimation_form::{self, RecordForm};
use eframe::egui;
use lm_app::{ExAnimationControllerEdit, ExtendedUiTextKey as Key, LocalizationCatalog};

impl ExAnimationEditor {
    pub(super) fn record_list(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        ui.heading(super::text(catalog, Key::ExAnimationDocumentRecords));
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
            .map(|(index, record)| {
                super::text(catalog, Key::ExAnimationDocumentRecordListFormat)
                    .replace("{index}", &format!("{index:02X}"))
                    .replace("{kind}", &format!("{:02X}", record.kind()))
            })
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
        if ui
            .button(super::text(catalog, Key::ExAnimationDocumentAppendRecord))
            .clicked()
        {
            self.record = RecordForm::default();
            self.record_editable = true;
            self.apply_record(true);
        }
        if ui
            .add_enabled(
                has_records,
                egui::Button::new(super::text(catalog, Key::ExAnimationDocumentRemoveSelected)),
            )
            .clicked()
        {
            self.apply_edits(&[ExAnimationControllerEdit::RemoveRecord {
                index: self.selected_record,
            }]);
            self.selected_record = self.selected_record.saturating_sub(1);
        }
    }

    pub(super) fn properties(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        ui.heading(super::text(catalog, Key::ExAnimationDocumentSlotSettings));
        ui.horizontal(|ui| {
            ui.label(super::text(catalog, Key::ExAnimationDocumentSettingHex));
            ui.text_edit_singleline(&mut self.global.setting);
        });
        ui.horizontal(|ui| {
            ui.label(super::text(catalog, Key::ExAnimationDocumentHeaderHex));
            ui.text_edit_singleline(&mut self.global.header);
        });
        if ui
            .button(super::text(catalog, Key::NativeAssetsAnimationApplySlots))
            .clicked()
        {
            self.apply_global();
        }
        self.trigger_properties(ui, catalog);
        ui.separator();
        self.record_properties(ui, catalog);
    }

    fn trigger_properties(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        ui.heading(super::text(catalog, Key::NativeAssetsAnimationTrigger));
        if ui
            .add(egui::Slider::new(&mut self.trigger_index, 0..=15))
            .changed()
        {
            self.load_trigger();
        }
        ui.checkbox(
            &mut self.trigger_enabled,
            super::text(catalog, Key::NativeAssetsAnimationEnabled),
        );
        ui.horizontal(|ui| {
            ui.label(super::text(
                catalog,
                Key::ExAnimationDocumentTriggerValueHex,
            ));
            ui.add_enabled(
                self.trigger_enabled,
                egui::TextEdit::singleline(&mut self.trigger_value),
            );
        });
        if ui
            .button(super::text(catalog, Key::NativeAssetsAnimationApplyTrigger))
            .clicked()
        {
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

    fn record_properties(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let record_exists = self.document.as_ref().is_some_and(|document| {
            self.selected_record < document.controller.value().animation.records.len()
        });
        ui.heading(
            super::text(catalog, Key::ExAnimationDocumentRecordFormat)
                .replace("{index}", &format!("{:02X}", self.selected_record)),
        );
        for (key, field) in [
            (Key::ExAnimationDocumentKindHex, &mut self.record.kind),
            (
                Key::ExAnimationDocumentTriggerHex,
                &mut self.record.size_mode,
            ),
            (
                Key::ExAnimationDocumentDestinationHex,
                &mut self.record.destination,
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(super::text(catalog, key));
                ui.text_edit_singleline(field);
            });
        }
        ui.checkbox(
            &mut self.record.destination_flag,
            super::text(catalog, Key::NativeAssetsAnimationDestinationFlag),
        );
        ui.label(super::text(
            catalog,
            Key::ExAnimationDocumentSourceWordsNotice,
        ));
        ui.add(egui::TextEdit::multiline(&mut self.record.frames).desired_rows(8));
        if !self.record_editable {
            ui.label(super::text(
                catalog,
                Key::ExAnimationDocumentSpecialTransferNotice,
            ));
        }
        self.frame_clipboard(ui, record_exists, catalog);
        if ui
            .add_enabled(
                self.record_editable && record_exists,
                egui::Button::new(super::text(catalog, Key::ExAnimationDocumentApplyRecord)),
            )
            .clicked()
        {
            self.apply_record(false);
        }
    }
}
