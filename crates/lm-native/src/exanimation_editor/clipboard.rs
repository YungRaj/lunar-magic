use super::ExAnimationEditor;
use crate::native_clipboard;
use eframe::egui;
use lm_app::ExAnimationControllerEdit;
use lm_graphics::ExAnimationFrameEdit;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PasteTarget {
    Record,
    Frame,
}

impl ExAnimationEditor {
    pub(super) fn frame_clipboard(&mut self, ui: &mut egui::Ui, record_exists: bool) {
        let frames = self
            .document
            .as_ref()
            .and_then(|document| document.controller.record_frames(self.selected_record).ok())
            .unwrap_or_default();
        self.selected_frame = self.selected_frame.min(frames.len().saturating_sub(1));
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.selected_frame)
                    .range(0..=frames.len().saturating_sub(1))
                    .prefix("Frame "),
            );
            if ui
                .add_enabled(
                    self.record_editable && record_exists && !frames.is_empty(),
                    egui::Button::new("Copy frame"),
                )
                .clicked()
            {
                match native_clipboard::encode_exanimation_frame(&frames[self.selected_frame]) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(
                    self.record_editable && record_exists && !frames.is_empty(),
                    egui::Button::new("Paste frame"),
                )
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Frame);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
    }

    pub(super) fn paste_record(&mut self, text: &str) {
        match native_clipboard::decode_exanimation_record(text) {
            Ok(record) => self.apply_edits(&[ExAnimationControllerEdit::ReplaceRecord {
                index: self.selected_record,
                record,
            }]),
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn paste_frame(&mut self, text: &str) {
        match native_clipboard::decode_exanimation_frame(text) {
            Ok(frame) => self.apply_edits(&[ExAnimationControllerEdit::EditRecordFrames {
                record: self.selected_record,
                edits: vec![ExAnimationFrameEdit::Replace {
                    index: self.selected_frame,
                    frame,
                }],
            }]),
            Err(error) => self.error = Some(error),
        }
    }
}
