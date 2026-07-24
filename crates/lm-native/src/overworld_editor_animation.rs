use crate::{
    exanimation_form::{GlobalForm, RecordForm},
    native_clipboard,
};
use eframe::egui;
use lm_app::{ExAnimationControllerEdit, OverworldControllerEdit};
use lm_graphics::{CompactExAnimation, ExAnimationFrameEdit, exanimation_frames};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    Record,
    Frame,
}

#[derive(Default)]
pub(crate) struct OverworldAnimationPanel {
    loaded_revision: Option<u64>,
    loaded_record: Option<usize>,
    selected: usize,
    selected_frame: usize,
    global: GlobalForm,
    record: RecordForm,
    record_editable: bool,
    trigger: usize,
    trigger_enabled: bool,
    trigger_value: u8,
    paste_target: Option<PasteTarget>,
}

impl OverworldAnimationPanel {
    pub(crate) fn invalidate(&mut self) {
        self.loaded_revision = None;
        self.loaded_record = None;
        self.paste_target = None;
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
        modes: &[bool; 256],
        revision: u64,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        self.load(animation, modes, revision);
        ui.horizontal(|ui| {
            ui.label("Setting (hex)");
            ui.text_edit_singleline(&mut self.global.setting);
        });
        ui.horizontal(|ui| {
            ui.label("Header (hex)");
            ui.text_edit_singleline(&mut self.global.header);
        });
        if ui.button("Apply animation globals").clicked() {
            return Some(self.global.parse().map(|(setting, header)| {
                OverworldControllerEdit::Animation(vec![
                    ExAnimationControllerEdit::SetSetting(setting),
                    ExAnimationControllerEdit::SetHeaderValue(header),
                ])
            }));
        }
        ui.separator();
        self.trigger_ui(ui, animation);
        ui.separator();
        self.record_ui(ui, animation, modes)
    }

    fn trigger_ui(&mut self, ui: &mut egui::Ui, animation: &CompactExAnimation) {
        if ui
            .add(egui::Slider::new(&mut self.trigger, 0..=15).text("Trigger"))
            .changed()
        {
            self.load_trigger(animation);
        }
        ui.checkbox(&mut self.trigger_enabled, "Enabled");
        ui.add_enabled(
            self.trigger_enabled,
            egui::Slider::new(&mut self.trigger_value, 0..=u8::MAX).text("Value"),
        );
    }

    fn record_ui(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
        modes: &[bool; 256],
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let records = &animation.records;
        if !records.is_empty() {
            self.selected = self.selected.min(records.len() - 1);
        }
        if ui.button("Apply trigger").clicked() {
            return Some(Ok(OverworldControllerEdit::Animation(vec![
                ExAnimationControllerEdit::SetTrigger {
                    trigger: self.trigger,
                    value: self.trigger_enabled.then_some(self.trigger_value),
                },
            ])));
        }
        ui.add(
            egui::Slider::new(&mut self.selected, 0..=records.len().saturating_sub(1))
                .text("Record"),
        );
        if self.loaded_record != Some(self.selected) {
            self.load_record(animation, modes);
        }
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
        ui.add(egui::TextEdit::multiline(&mut self.record.frames).desired_rows(6));
        if !self.record_editable && !records.is_empty() {
            ui.label("This special transfer kind has no ordinary frame payload.");
        }
        if let Some(edit) = self.clipboard_ui(ui, animation, modes) {
            return Some(edit);
        }
        let mut operation = None;
        ui.horizontal(|ui| {
            if ui.button("Append").clicked() {
                operation = Some(RecordOperation::Append);
            }
            if ui
                .add_enabled(
                    !records.is_empty() && self.record_editable,
                    egui::Button::new("Replace"),
                )
                .clicked()
            {
                operation = Some(RecordOperation::Replace);
            }
            if ui
                .add_enabled(!records.is_empty(), egui::Button::new("Remove"))
                .clicked()
            {
                operation = Some(RecordOperation::Remove);
            }
        });
        operation.map(|operation| self.record_edit(operation, records.len(), modes))
    }

    fn clipboard_ui(
        &mut self,
        ui: &mut egui::Ui,
        animation: &CompactExAnimation,
        modes: &[bool; 256],
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let record = animation.records.get(self.selected);
        let frames = record
            .and_then(|record| {
                exanimation_frames(record, modes[usize::from(record.size_mode())]).ok()
            })
            .unwrap_or_default();
        self.selected_frame = self.selected_frame.min(frames.len().saturating_sub(1));
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
                self.paste_target = Some(PasteTarget::Record);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            ui.add(
                egui::DragValue::new(&mut self.selected_frame)
                    .range(0..=frames.len().saturating_sub(1))
                    .prefix("Frame "),
            );
            if ui
                .add_enabled(!frames.is_empty(), egui::Button::new("Copy frame"))
                .clicked()
            {
                copy_result = Some(native_clipboard::encode_exanimation_frame(
                    &frames[self.selected_frame],
                ));
            }
            if ui
                .add_enabled(!frames.is_empty(), egui::Button::new("Paste frame"))
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Frame);
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
        let edit = match self.paste_target.take()? {
            PasteTarget::Record => {
                native_clipboard::decode_exanimation_record(&text).map(|record| {
                    ExAnimationControllerEdit::ReplaceRecord {
                        index: self.selected,
                        record,
                    }
                })
            }
            PasteTarget::Frame => native_clipboard::decode_exanimation_frame(&text).map(|frame| {
                ExAnimationControllerEdit::EditRecordFrames {
                    record: self.selected,
                    edits: vec![ExAnimationFrameEdit::Replace {
                        index: self.selected_frame,
                        frame,
                    }],
                }
            }),
        };
        Some(edit.map(|edit| OverworldControllerEdit::Animation(vec![edit])))
    }

    fn record_edit(
        &self,
        operation: RecordOperation,
        len: usize,
        modes: &[bool; 256],
    ) -> Result<OverworldControllerEdit, String> {
        let edit = match operation {
            RecordOperation::Append => ExAnimationControllerEdit::InsertRecord {
                index: len,
                record: self.record.parse(modes)?,
            },
            RecordOperation::Replace => ExAnimationControllerEdit::ReplaceRecord {
                index: self.selected,
                record: self.record.parse(modes)?,
            },
            RecordOperation::Remove => ExAnimationControllerEdit::RemoveRecord {
                index: self.selected,
            },
        };
        Ok(OverworldControllerEdit::Animation(vec![edit]))
    }

    fn load(&mut self, animation: &CompactExAnimation, modes: &[bool; 256], revision: u64) {
        if self.loaded_revision == Some(revision) {
            return;
        }
        self.global = GlobalForm::load(animation.setting, animation.header_value);
        self.loaded_revision = Some(revision);
        self.loaded_record = None;
        self.load_trigger(animation);
        self.load_record(animation, modes);
    }

    fn load_trigger(&mut self, animation: &CompactExAnimation) {
        self.trigger_enabled = animation.trigger_mask & (1 << self.trigger) != 0;
        self.trigger_value = animation.trigger_values[self.trigger];
    }

    fn load_record(&mut self, animation: &CompactExAnimation, modes: &[bool; 256]) {
        if animation.records.is_empty() {
            self.record = RecordForm::default();
            self.record_editable = true;
            self.loaded_record = Some(0);
            return;
        }
        self.selected = self.selected.min(animation.records.len() - 1);
        let record = &animation.records[self.selected];
        if let Ok(frames) = exanimation_frames(record, modes[usize::from(record.size_mode())]) {
            self.record = RecordForm::load(record, &frames);
            self.record_editable = true;
        } else {
            self.record = RecordForm::load(record, &[]);
            self.record_editable = false;
        }
        self.loaded_record = Some(self.selected);
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

#[derive(Clone, Copy)]
enum RecordOperation {
    Append,
    Replace,
    Remove,
}
