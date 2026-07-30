use eframe::egui;
use lm_app::{AppState, Command};
use lm_project::LevelAccessRestrictionKeys;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub(crate) struct LevelAccessRestrictionDialog {
    open: bool,
    title: String,
    acknowledged: bool,
    error: Option<String>,
}

impl LevelAccessRestrictionDialog {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.acknowledged = false;
        self.error = None;
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) -> Option<Command> {
        let mut command = None;
        if self.open {
            egui::Window::new("Restrict Level Access by Lunar Magic (Version 1.1)")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(
                        "This is a relatively weak form of protection. It only prevents casual \
                         examination of your levels.",
                    );
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "This operation is permanent in Lunar Magic. Keep an unmodified backup.",
                    );
                    ui.label(
                        "After restriction, performing additional editing operations on the \
                         locked ROM is not recommended.",
                    );
                    ui.separator();
                    ui.label("ROM Title (21 char Max, ASCII)");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.title)
                            .char_limit(21)
                            .desired_width(260.0),
                    );
                    ui.checkbox(
                        &mut self.acknowledged,
                        "I understand that the original tool cannot reverse this operation.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.open = false;
                        }
                        if ui
                            .add_enabled(
                                self.acknowledged,
                                egui::Button::new("Restrict Level Access"),
                            )
                            .clicked()
                        {
                            if !self.title.is_ascii() {
                                self.error = Some("the ROM title must contain only ASCII".into());
                            } else {
                                let title = if self.title.is_empty() {
                                    "Super Peachy World".to_owned()
                                } else {
                                    self.title.clone()
                                };
                                command = Some(Command::RestrictLevelAccess {
                                    rev: app.project_revision(),
                                    title,
                                    keys: fresh_keys(),
                                });
                            }
                        }
                    });
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("Level access restriction error")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
        command
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.open = false;
    }
}

fn fresh_keys() -> LevelAccessRestrictionKeys {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let mut state = duration.as_secs() ^ u64::from(duration.subsec_nanos()).rotate_left(19);
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    LevelAccessRestrictionKeys {
        per_save_low: next().to_le_bytes()[0] & 0x7f,
        per_save_high: next().to_le_bytes()[0],
        graphics: u16::from_le_bytes(next().to_le_bytes()[..2].try_into().unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_material_respects_lunar_magics_seven_bit_first_key() {
        for _ in 0..16 {
            assert_eq!(fresh_keys().per_save_low & 0x80, 0);
        }
    }
}
