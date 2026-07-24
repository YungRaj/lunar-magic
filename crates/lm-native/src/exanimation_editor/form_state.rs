use super::ExAnimationEditor;
use crate::exanimation_form::{GlobalForm, RecordForm};
use lm_app::ExAnimationControllerEdit;

impl ExAnimationEditor {
    pub(super) fn apply_global(&mut self) {
        match self.global.parse() {
            Ok((setting, header)) => self.apply_edits(&[
                ExAnimationControllerEdit::SetSetting(setting),
                ExAnimationControllerEdit::SetHeaderValue(header),
            ]),
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn apply_record(&mut self, append: bool) {
        let Some(document) = &self.document else {
            return;
        };
        match self.record.parse(&document.modes) {
            Ok(record) => {
                let edit = if append {
                    ExAnimationControllerEdit::InsertRecord {
                        index: document.controller.value().animation.records.len(),
                        record,
                    }
                } else {
                    ExAnimationControllerEdit::ReplaceRecord {
                        index: self.selected_record,
                        record,
                    }
                };
                self.apply_edits(std::slice::from_ref(&edit));
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn apply_edits(&mut self, edits: &[ExAnimationControllerEdit]) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if let Err(error) = document
            .controller
            .apply_edits(document.controller.revision(), edits)
        {
            self.error = Some(error.to_string());
        } else {
            self.invalidate_forms();
        }
    }

    pub(super) fn load_forms(&mut self) {
        let Some(document) = &self.document else {
            return;
        };
        let revision = document.controller.revision();
        if self.loaded_revision != Some(revision) {
            let animation = &document.controller.value().animation;
            self.global = GlobalForm::load(animation.setting, animation.header_value);
            self.loaded_revision = Some(revision);
            self.loaded_record = None;
            self.load_trigger();
        }
        if self.loaded_record != Some(self.selected_record) {
            self.load_record();
        }
    }

    pub(super) fn load_trigger(&mut self) {
        let Some(document) = &self.document else {
            return;
        };
        let animation = &document.controller.value().animation;
        self.trigger_enabled = animation.trigger_mask & (1 << self.trigger_index) != 0;
        self.trigger_value = format!("{:02X}", animation.trigger_values[self.trigger_index]);
    }

    fn load_record(&mut self) {
        let Some(document) = &self.document else {
            return;
        };
        let records = &document.controller.value().animation.records;
        if records.is_empty() {
            self.record = RecordForm::default();
            self.record_editable = true;
            self.loaded_record = Some(0);
            return;
        }
        self.selected_record = self.selected_record.min(records.len() - 1);
        match document.controller.record_frames(self.selected_record) {
            Ok(frames) => {
                self.record = RecordForm::load(&records[self.selected_record], &frames);
                self.record_editable = true;
                self.loaded_record = Some(self.selected_record);
            }
            Err(error) => {
                self.record = RecordForm::load(&records[self.selected_record], &[]);
                self.record_editable = false;
                self.error = Some(error.to_string());
                self.loaded_record = Some(self.selected_record);
            }
        }
    }

    pub(super) fn invalidate_forms(&mut self) {
        self.loaded_revision = None;
        self.loaded_record = None;
    }
}
