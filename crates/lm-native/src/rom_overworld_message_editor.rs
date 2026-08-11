mod form;
mod workspace;

use crate::level_editor_forms::parse_hex_u16;
use eframe::egui;
use form::MessageTileForm;
use lm_app::{AppState, Command};
use workspace::OverworldMessageWorkspace;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Default)]
pub(crate) struct RomOverworldMessageEditor {
    workspace: Option<OverworldMessageWorkspace>,
    form: MessageTileForm,
    count: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomOverworldMessageEditor {
    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        self.workspace.as_ref()?.staged_recovery_generation(app)
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "overworld-message workspace is closed".to_owned())?
            .staged_recovery_snapshot(app)
    }

    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        match OverworldMessageWorkspace::load(app) {
            Ok(workspace) => {
                self.count = format!("{:03X}", workspace.len());
                self.form = MessageTileForm::default();
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
            egui::Window::new("ROM Overworld Messages")
                .default_size([560.0, 390.0])
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
        ui.label("Complete variable 8×18 message table. All numeric fields are hexadecimal.");
        ui.label(format!(
            "Loaded storage: {}; staged messages: {}",
            workspace.storage_label(),
            workspace.len()
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after these messages were opened. Reopen before committing.",
            );
        }
        self.message_form(ui);
        ui.horizontal(|ui| {
            ui.label("Table count (0C2–200, even)");
            ui.text_edit_singleline(&mut self.count);
            if ui
                .add_enabled(!stale, egui::Button::new("Resize table"))
                .clicked()
                && let Err(error) = self.resize()
            {
                self.error = Some(error);
            }
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

    fn message_form(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("rom-overworld-message-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Message");
                if ui.text_edit_singleline(&mut self.form.message).changed() {
                    self.form.selection_changed();
                }
                ui.end_row();
                ui.label("Row (00–07)");
                if ui.text_edit_singleline(&mut self.form.row).changed() {
                    self.form.selection_changed();
                }
                ui.end_row();
                ui.label("Column (00–11)");
                if ui.text_edit_singleline(&mut self.form.column).changed() {
                    self.form.selection_changed();
                }
                ui.end_row();
                ui.label("Tile value (FE is reserved)");
                ui.text_edit_singleline(&mut self.form.value);
                ui.end_row();
            });
    }

    fn resize(&mut self) -> Result<(), String> {
        let count = usize::from(parse_hex_u16(&self.count, "message count")?);
        self.workspace
            .as_mut()
            .ok_or_else(|| "overworld-message workspace is closed".to_owned())?
            .resize(count)?;
        self.form.selection_changed();
        Ok(())
    }

    fn load_selected(&mut self) -> Result<(), String> {
        self.form.load(
            self.workspace
                .as_ref()
                .ok_or_else(|| "overworld-message workspace is closed".to_owned())?,
        )
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        self.form.apply(
            self.workspace
                .as_mut()
                .ok_or_else(|| "overworld-message workspace is closed".to_owned())?,
        )
    }

    fn prepare_commit(&self, revision: u64) -> Result<Option<Command>, String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "overworld-message workspace is closed".to_owned())?
            .prepare_commit(revision)
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard overworld-message changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged message table has not been committed to the ROM.");
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
            egui::Window::new("Overworld-message editor error").show(context, |ui| {
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
    use lm_overworld::OverworldMessage;
    use lm_profile::load_smw_us_v1_overworld_messages;
    use std::path::PathBuf;

    fn pristine_app() -> AppState {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app
    }

    #[test]
    fn staged_pristine_overworld_messages_are_recovered_as_complete_installed_table() {
        let app = pristine_app();
        let mut editor = RomOverworldMessageEditor::default();
        editor.open(&app);
        editor.workspace.as_mut().unwrap().resize(200).unwrap();
        editor
            .workspace
            .as_mut()
            .unwrap()
            .set_tile((199, 7, 17), 0xa5)
            .unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let loaded = load_smw_us_v1_overworld_messages(reopened.project().unwrap()).unwrap();
        assert_eq!(loaded.messages.len(), 200);
        assert_eq!(loaded.messages[199].0[143], 0xa5);
    }

    #[test]
    fn staged_installed_overworld_message_update_is_recovered_exactly() {
        let mut installer = pristine_app();
        installer
            .dispatch(Command::ReplaceNativeOverworldMessages {
                rev: 0,
                messages: vec![OverworldMessage([0x1f; OverworldMessage::ENCODED_LEN]); 200],
            })
            .unwrap();
        let installed = installer.project().unwrap().save_snapshot();
        let mut app = AppState::default();
        app.load_rom(installed).unwrap();
        let mut editor = RomOverworldMessageEditor::default();
        editor.open(&app);
        assert!(matches!(
            editor.workspace.as_ref().unwrap().storage,
            lm_profile::SmwUsV1OverworldMessageStorage::Expanded(_)
        ));
        editor
            .workspace
            .as_mut()
            .unwrap()
            .set_tile((0, 0, 0), 0x7b)
            .unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let loaded = load_smw_us_v1_overworld_messages(reopened.project().unwrap()).unwrap();
        assert_eq!(loaded.messages.len(), 200);
        assert_eq!(loaded.messages[0].0[0], 0x7b);
    }

    #[test]
    fn pristine_table_grows_commits_and_semantically_reopens() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original).unwrap();
        let mut editor = RomOverworldMessageEditor::default();
        editor.open(&app);
        assert_eq!(editor.workspace.as_ref().unwrap().len(), 194);
        editor.count = "0C8".into();
        editor.resize().unwrap();
        editor.form.message = "0C7".into();
        editor.form.row = "07".into();
        editor.form.column = "11".into();
        editor.load_selected().unwrap();
        editor.form.value = "A5".into();
        editor.apply_selected().unwrap();
        let command = editor
            .prepare_commit(app.project_revision())
            .unwrap()
            .unwrap();
        app.dispatch(command).unwrap();
        editor.commit_succeeded();
        let reopened = load_smw_us_v1_overworld_messages(app.project().unwrap()).unwrap();
        assert_eq!(reopened.messages.len(), 200);
        assert_eq!(reopened.messages[199].0[143], 0xa5);
    }
}
