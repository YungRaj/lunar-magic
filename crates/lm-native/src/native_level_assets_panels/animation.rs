use super::{AggregatePanels, PasteTarget, pasted_text};
use crate::{exanimation_form, native_clipboard};
use eframe::egui;
use lm_app::{ExAnimationControllerEdit, NativeLevelAssetsControllerEdit};
use lm_graphics::ExAnimationFrameEdit;
use lm_project::NativeLevelAssetsFile;

impl AggregatePanels {
    pub(super) fn animation_panel(
        &mut self,
        ui: &mut egui::Ui,
        file: &NativeLevelAssetsFile,
        modes: &[bool; 256],
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        if let Some(edit) = self.animation_header(ui) {
            return Some(edit);
        }
        ui.separator();
        let len = file.assets.exanimation.records.len();
        ui.heading(format!("Records ({len})"));
        if ui
            .add(egui::DragValue::new(&mut self.record_index).range(0..=len))
            .changed()
        {
            let records = &file.assets.exanimation.records;
            self.record_index = self.record_index.min(records.len().saturating_sub(1));
            if let Some(record) = records.get(self.record_index) {
                let frames =
                    lm_graphics::exanimation_frames(record, modes[usize::from(record.size_mode())])
                        .unwrap_or_default();
                self.record = exanimation_form::RecordForm::load(record, &frames);
            }
        }
        for (label, field) in [
            ("Kind", &mut self.record.kind),
            ("Size mode", &mut self.record.size_mode),
            ("Destination", &mut self.record.destination),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.text_edit_singleline(field);
            });
        }
        ui.checkbox(&mut self.record.destination_flag, "Destination flag");
        ui.label("Source words, one frame per line");
        ui.add(egui::TextEdit::multiline(&mut self.record.frames).desired_rows(6));
        if let Some(edit) = self.animation_clipboard(ui, file, modes) {
            return Some(edit);
        }
        let mut action = None;
        ui.horizontal(|ui| {
            if ui.button("Append").clicked() {
                action = Some(0);
            }
            if ui
                .add_enabled(self.record_index < len, egui::Button::new("Replace"))
                .clicked()
            {
                action = Some(1);
            }
            if ui
                .add_enabled(self.record_index < len, egui::Button::new("Remove"))
                .clicked()
            {
                action = Some(2);
            }
        });
        action.map(|action| {
            let edit = if action == 2 {
                Ok(ExAnimationControllerEdit::RemoveRecord {
                    index: self.record_index,
                })
            } else {
                self.record.parse(modes).map(|record| {
                    if action == 0 {
                        ExAnimationControllerEdit::InsertRecord { index: len, record }
                    } else {
                        ExAnimationControllerEdit::ReplaceRecord {
                            index: self.record_index,
                            record,
                        }
                    }
                })
            };
            edit.map(|edit| NativeLevelAssetsControllerEdit::ExAnimation(vec![edit]))
        })
    }

    fn animation_header(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        ui.horizontal(|ui| {
            ui.label("Setting");
            ui.text_edit_singleline(&mut self.global.setting);
        });
        ui.horizontal(|ui| {
            ui.label("Header");
            ui.text_edit_singleline(&mut self.global.header);
        });
        if ui.button("Apply slot settings").clicked() {
            return Some(self.global.parse().map(|(setting, header)| {
                NativeLevelAssetsControllerEdit::ExAnimation(vec![
                    ExAnimationControllerEdit::SetSetting(setting),
                    ExAnimationControllerEdit::SetHeaderValue(header),
                ])
            }));
        }
        ui.separator();
        if ui
            .add(egui::Slider::new(&mut self.trigger_index, 0..=15).text("Trigger"))
            .changed()
        {
            self.loaded_revision = None;
        }
        ui.checkbox(&mut self.trigger_enabled, "Enabled");
        ui.add_enabled(
            self.trigger_enabled,
            egui::TextEdit::singleline(&mut self.trigger_value),
        );
        ui.button("Apply trigger").clicked().then(|| {
            let value = if self.trigger_enabled {
                exanimation_form::hex_u8(&self.trigger_value, "trigger").map(Some)
            } else {
                Ok(None)
            };
            value.map(|value| {
                NativeLevelAssetsControllerEdit::ExAnimation(vec![
                    ExAnimationControllerEdit::SetTrigger {
                        trigger: self.trigger_index,
                        value,
                    },
                ])
            })
        })
    }

    fn animation_clipboard(
        &mut self,
        ui: &mut egui::Ui,
        file: &NativeLevelAssetsFile,
        modes: &[bool; 256],
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let record = file.assets.exanimation.records.get(self.record_index);
        let frames = record
            .and_then(|record| {
                lm_graphics::exanimation_frames(record, modes[usize::from(record.size_mode())]).ok()
            })
            .unwrap_or_default();
        self.frame_index = self.frame_index.min(frames.len().saturating_sub(1));
        let mut copy_result = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(record.is_some(), egui::Button::new("Copy record"))
                .clicked()
                && let Some(record) = record
            {
                copy_result = Some(native_clipboard::encode_exanimation_record(record));
            }
            if ui
                .add_enabled(record.is_some(), egui::Button::new("Paste record"))
                .clicked()
            {
                self.paste_target = Some(PasteTarget::AnimationRecord);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            ui.add(
                egui::DragValue::new(&mut self.frame_index)
                    .range(0..=frames.len().saturating_sub(1))
                    .prefix("Frame "),
            );
            if ui
                .add_enabled(!frames.is_empty(), egui::Button::new("Copy frame"))
                .clicked()
            {
                copy_result = Some(native_clipboard::encode_exanimation_frame(
                    &frames[self.frame_index],
                ));
            }
            if ui
                .add_enabled(!frames.is_empty(), egui::Button::new("Paste frame"))
                .clicked()
            {
                self.paste_target = Some(PasteTarget::AnimationFrame);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(result) = copy_result {
            match result {
                Ok(text) => ui.ctx().copy_text(text),
                Err(error) => return Some(Err(error)),
            }
        }
        let text = pasted_text(ui)?;
        let edit =
            match self.paste_target.take()? {
                PasteTarget::AnimationRecord => native_clipboard::decode_exanimation_record(&text)
                    .map(|record| ExAnimationControllerEdit::ReplaceRecord {
                        index: self.record_index,
                        record,
                    }),
                PasteTarget::AnimationFrame => native_clipboard::decode_exanimation_frame(&text)
                    .map(|frame| ExAnimationControllerEdit::EditRecordFrames {
                        record: self.record_index,
                        edits: vec![ExAnimationFrameEdit::Replace {
                            index: self.frame_index,
                            frame,
                        }],
                    }),
                PasteTarget::Object
                | PasteTarget::Layer2Object
                | PasteTarget::Layer2Tilemap
                | PasteTarget::Sprite
                | PasteTarget::PaletteColor => {
                    return None;
                }
            };
        Some(edit.map(|edit| NativeLevelAssetsControllerEdit::ExAnimation(vec![edit])))
    }
}
