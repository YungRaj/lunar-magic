use eframe::egui;
use lm_app::{AppState, Command, UiTextKey};

#[derive(Default)]
pub(crate) struct LevelDeletionDialog {
    level: Option<u16>,
}

impl LevelDeletionDialog {
    pub(crate) fn open(&mut self, app: &AppState) {
        self.level = app
            .current_level_deletion_available()
            .then(|| app.current_level())
            .flatten();
    }

    pub(crate) fn is_open(&self) -> bool {
        self.level.is_some()
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        app: &AppState,
    ) -> Option<(u16, Command)> {
        let level = self.level?;
        let localize = |key: UiTextKey| {
            app.localization().map_or_else(
                || key.english().to_owned(),
                |catalog| catalog.text(key).to_owned(),
            )
        };
        let title = localize(UiTextKey::DeleteLevelWindowTitle);
        let question =
            localize(UiTextKey::DeleteLevelQuestion).replace("{level}", &format!("{level:03X}"));
        let delete = localize(UiTextKey::CommonDelete);
        let cancel = localize(UiTextKey::CommonCancel);
        let mut command = None;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(context, |ui| {
                ui.label(question);
                ui.horizontal(|ui| {
                    if ui.button(delete).clicked() {
                        command = Some((
                            level,
                            Command::DeleteCurrentLevel {
                                rev: app.project_revision(),
                            },
                        ));
                    }
                    if ui.button(cancel).clicked() {
                        self.level = None;
                    }
                });
            });
        if command.is_some() {
            self.level = None;
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_dialog_is_inert() {
        let mut dialog = LevelDeletionDialog::default();
        assert!(!dialog.is_open());
        assert!(
            dialog
                .show(&egui::Context::default(), &AppState::default())
                .is_none()
        );
    }
}
