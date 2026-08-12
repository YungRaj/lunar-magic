use super::{PasteTarget, RomExAnimationEditor};
use crate::native_clipboard;
use eframe::egui;
use lm_app::ExAnimationControllerEdit;
use lm_app::LocalizationCatalog;
use lm_graphics::{ExAnimationFrameEdit, ExAnimationRecord};

impl RomExAnimationEditor {
    pub(super) fn frame_clipboard(
        &mut self,
        ui: &mut egui::Ui,
        stale: bool,
        record_exists: bool,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let frames = self
            .workspace
            .as_ref()
            .and_then(|workspace| {
                workspace
                    .controller
                    .record_frames(self.selected_record)
                    .ok()
            })
            .unwrap_or_default();
        self.selected_frame = self.selected_frame.min(frames.len().saturating_sub(1));
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.selected_frame)
                    .range(0..=frames.len().saturating_sub(1))
                    .prefix(super::text(
                        catalog,
                        lm_app::ExtendedUiTextKey::NativeAssetsAnimationFramePrefix,
                    )),
            );
            if ui
                .add_enabled(
                    self.record_editable && record_exists && !frames.is_empty(),
                    egui::Button::new(super::text(
                        catalog,
                        lm_app::ExtendedUiTextKey::NativeAssetsAnimationCopyFrame,
                    )),
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
                    !stale && self.record_editable && record_exists && !frames.is_empty(),
                    egui::Button::new(super::text(
                        catalog,
                        lm_app::ExtendedUiTextKey::NativeAssetsAnimationPasteFrame,
                    )),
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
            Ok(record) => self.apply(&[ExAnimationControllerEdit::ReplaceRecord {
                index: self.selected_record,
                record,
            }]),
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn paste_frame(&mut self, text: &str) {
        match native_clipboard::decode_exanimation_frame(text) {
            Ok(frame) => self.apply(&[ExAnimationControllerEdit::EditRecordFrames {
                record: self.selected_record,
                edits: vec![ExAnimationFrameEdit::Replace {
                    index: self.selected_frame,
                    frame,
                }],
            }]),
            Err(error) => self.error = Some(error),
        }
    }

    pub(super) fn current_record(&self) -> Option<&ExAnimationRecord> {
        self.workspace
            .as_ref()?
            .controller
            .animation()
            .records
            .get(self.selected_record)
    }
}
