mod form;
mod workspace;

use eframe::egui;
use form::BossTileForm;
use lm_app::{AppState, Command};
use workspace::BossSequenceWorkspace;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Default)]
pub(crate) struct RomBossSequenceEditor {
    workspace: Option<BossSequenceWorkspace>,
    form: BossTileForm,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomBossSequenceEditor {
    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        self.workspace.as_ref()?.staged_recovery_generation(app)
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "boss-sequence workspace is closed".to_owned())?
            .staged_recovery_snapshot(app)
    }

    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        match BossSequenceWorkspace::load(app) {
            Ok(workspace) => {
                self.form = BossTileForm::default();
                self.form.load(&workspace).ok();
                self.workspace = Some(workspace);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if !workspace.is_dirty() {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Editor
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new("ROM Boss-Sequence Messages")
                .default_size([520.0, 340.0])
                .show(context, |ui| command = self.contents(ui, project_revision));
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.is_stale(project_revision);
        let dirty = workspace.is_dirty();
        ui.label("Seven lossless 24×8 tile-index messages. All fields are hexadecimal.");
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after these messages were opened. Reopen before committing.",
            );
        }
        egui::Grid::new("rom-boss-sequence-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Message (00–06)");
                if ui.text_edit_singleline(&mut self.form.message).changed() {
                    self.form.selection_changed();
                }
                ui.end_row();
                ui.label("Row (00–07)");
                if ui.text_edit_singleline(&mut self.form.row).changed() {
                    self.form.selection_changed();
                }
                ui.end_row();
                ui.label("Column (00–17)");
                if ui.text_edit_singleline(&mut self.form.column).changed() {
                    self.form.selection_changed();
                }
                ui.end_row();
                ui.label("Tile value");
                ui.text_edit_singleline(&mut self.form.value);
                ui.end_row();
            });
        let mut command = None;
        ui.horizontal(|ui| {
            if ui.button("Load tile").clicked()
                && let Err(error) = self.load_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(!stale, egui::Button::new("Apply tile"))
                .clicked()
                && let Err(error) = self.apply_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(dirty && !stale, egui::Button::new("Commit messages to ROM"))
                .clicked()
            {
                match self.prepare_commit(project_revision) {
                    Ok(prepared) => command = prepared,
                    Err(error) => self.error = Some(error),
                }
            }
            ui.label(if dirty { "Staged" } else { "Unchanged" });
        });
        command
    }

    fn load_selected(&mut self) -> Result<(), String> {
        self.form.load(
            self.workspace
                .as_ref()
                .ok_or_else(|| "boss-sequence workspace is closed".to_owned())?,
        )
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        self.form.apply(
            self.workspace
                .as_mut()
                .ok_or_else(|| "boss-sequence workspace is closed".to_owned())?,
        )
    }

    fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "boss-sequence workspace is closed".to_owned())?
            .prepare_commit(project_revision)
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard boss-message changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged boss-sequence messages have not been committed.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_close = None;
                    }
                    if ui.button("Discard").clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("Boss-sequence editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.form.clear_selection();
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_profile::smw_us_v1_boss_sequence_locator;
    use std::path::PathBuf;

    fn pristine_app() -> AppState {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app
    }

    #[test]
    fn staged_pristine_boss_sequence_is_recovered_as_complete_table() {
        let app = pristine_app();
        let mut editor = RomBossSequenceEditor::default();
        editor.open(&app);
        editor
            .workspace
            .as_mut()
            .unwrap()
            .set_tile((6, 7, 23), 0xa5)
            .unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = reopened
            .project()
            .unwrap()
            .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())
            .unwrap()
            .table;
        assert_eq!(table.messages[6].0[191], 0xa5);
    }

    #[test]
    fn staged_installed_boss_sequence_update_is_recovered_exactly() {
        let mut installer = pristine_app();
        let mut table = installer
            .project()
            .unwrap()
            .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())
            .unwrap()
            .table;
        table.messages[0].0[0] = 0x11;
        installer
            .dispatch(Command::ReplaceNativeOverworldBossSequence {
                rev: 0,
                table: Box::new(table),
            })
            .unwrap();
        let installed = installer.project().unwrap().save_snapshot();
        let mut app = AppState::default();
        app.load_rom(installed).unwrap();
        let mut editor = RomBossSequenceEditor::default();
        editor.open(&app);
        editor
            .workspace
            .as_mut()
            .unwrap()
            .set_tile((1, 0, 0), 0x22)
            .unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = reopened
            .project()
            .unwrap()
            .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())
            .unwrap()
            .table;
        assert_eq!(table.messages[0].0[0], 0x11);
        assert_eq!(table.messages[1].0[0], 0x22);
    }

    #[test]
    fn pristine_rom_commit_reopens_exact_staged_tile_and_closes_editor() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original).unwrap();
        let mut editor = RomBossSequenceEditor::default();
        editor.open(&app);
        editor.form.value = "A5".into();
        editor.apply_selected().unwrap();
        let command = editor
            .prepare_commit(app.project_revision())
            .unwrap()
            .unwrap();
        app.dispatch(command).unwrap();
        editor.commit_succeeded();
        assert!(!editor.is_open());
        let reopened = app
            .project()
            .unwrap()
            .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())
            .unwrap();
        assert_eq!(reopened.table.messages[0].0[0], 0xa5);
    }
}
