mod form;
mod workspace;

use eframe::egui;
use form::TileForm;
use lm_app::{AppState, Command};
use workspace::{TilemapKind, TilemapWorkspace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct RomTilemapEditor {
    kind: TilemapKind,
    workspace: Option<TilemapWorkspace>,
    form: TileForm,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomTilemapEditor {
    fn new(kind: TilemapKind) -> Self {
        Self {
            kind,
            workspace: None,
            form: TileForm::default(),
            error: None,
            pending_close: None,
        }
    }

    fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        self.workspace.as_ref()?.staged_recovery_generation(app)
    }

    fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "tilemap workspace is closed".to_owned())?
            .staged_recovery_snapshot(app)
    }

    fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        match TilemapWorkspace::open(self.kind, app) {
            Ok(workspace) => {
                self.form = TileForm::default();
                self.workspace = Some(workspace);
                if let Err(error) = self.load_selected() {
                    self.error = Some(error);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn request_close(&mut self, application: bool) -> bool {
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

    fn show(&mut self, context: &egui::Context, project_revision: u64) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new(format!("ROM {}", self.kind.title()))
                .default_size([520.0, 360.0])
                .show(context, |ui| {
                    command = self.contents(ui, project_revision);
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.revision != project_revision;
        let dirty = workspace.is_dirty();
        ui.label(format!(
            "Exact {}×{} native tile words. Coordinates and values are hexadecimal.",
            self.kind.columns(),
            self.kind.rows()
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this tilemap was opened. Reopen before committing.",
            );
        }
        egui::Grid::new(format!("rom-{:?}-tilemap-form", self.kind))
            .striped(true)
            .show(ui, |ui| {
                ui.label("Row");
                if ui.text_edit_singleline(&mut self.form.row).changed() {
                    self.form.selection_changed();
                }
                ui.end_row();
                ui.label("Column");
                if ui.text_edit_singleline(&mut self.form.column).changed() {
                    self.form.selection_changed();
                }
                ui.end_row();
                if self.kind.planes() > 1 {
                    ui.label("Plane");
                    if ui
                        .selectable_value(&mut self.form.plane, 0, "Primary")
                        .clicked()
                        | ui.selectable_value(&mut self.form.plane, 1, "Secondary")
                            .clicked()
                    {
                        self.form.selection_changed();
                    }
                    ui.end_row();
                }
                ui.label("Tile word");
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
                .add_enabled(dirty && !stale, egui::Button::new("Commit tilemap to ROM"))
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
                .ok_or_else(|| "tilemap workspace is closed".to_owned())?,
        )
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        self.form.apply(
            self.workspace
                .as_mut()
                .ok_or_else(|| "tilemap workspace is closed".to_owned())?,
        )
    }

    fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "tilemap workspace is closed".to_owned())?
            .command(project_revision)
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(format!("Discard {} changes?", self.kind.title()))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged tilemap has not been committed to the ROM.");
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
            egui::Window::new(format!("{} editor error", self.kind.title())).show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.pending_close = None;
    }

    fn commit_succeeded(&mut self) {
        self.clear();
    }
}

macro_rules! tilemap_editor_wrapper {
    ($name:ident, $kind:expr) => {
        pub(crate) struct $name(RomTilemapEditor);

        impl Default for $name {
            fn default() -> Self {
                Self(RomTilemapEditor::new($kind))
            }
        }

        impl $name {
            pub(crate) fn is_open(&self) -> bool {
                self.0.is_open()
            }

            pub(crate) fn open(&mut self, app: &AppState) {
                self.0.open(app);
            }

            pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
                self.0.staged_recovery_generation(app)
            }

            pub(crate) fn staged_recovery_snapshot(
                &self,
                app: &AppState,
            ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
                self.0.staged_recovery_snapshot(app)
            }

            pub(crate) fn request_close(&mut self, application: bool) -> bool {
                self.0.request_close(application)
            }

            pub(crate) fn show(
                &mut self,
                context: &egui::Context,
                revision: u64,
            ) -> (bool, Option<Command>) {
                self.0.show(context, revision)
            }

            pub(crate) fn commit_succeeded(&mut self) {
                self.0.commit_succeeded();
            }
        }
    };
}

tilemap_editor_wrapper!(RomTitleTilemapEditor, TilemapKind::Title);
tilemap_editor_wrapper!(RomCreditsTilemapEditor, TilemapKind::Credits);

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::CreditsTilemap;
    use lm_profile::{smw_us_v1_credits_tilemap_locator, smw_us_v1_title_tilemap_locator};
    use std::path::PathBuf;

    fn pristine_app() -> AppState {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app
    }

    #[test]
    fn title_edit_dispatches_installs_and_semantically_reopens() {
        let mut app = pristine_app();
        let mut editor = RomTilemapEditor::new(TilemapKind::Title);
        editor.open(&app);
        editor.form.value = "1234".into();
        editor.apply_selected().unwrap();
        let command = editor
            .prepare_commit(app.project_revision())
            .unwrap()
            .unwrap();
        app.dispatch(command).unwrap();
        let loaded = app
            .project()
            .unwrap()
            .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())
            .unwrap();
        assert_eq!(
            u16::from_le_bytes([
                loaded.tilemap.primary_bytes()[0],
                loaded.tilemap.primary_bytes()[1]
            ]),
            0x1234
        );
        assert_eq!(app.project_revision(), 1);
    }

    #[test]
    fn credits_edit_dispatches_installs_and_semantically_reopens() {
        let mut app = pristine_app();
        let mut editor = RomTilemapEditor::new(TilemapKind::Credits);
        editor.open(&app);
        editor.form.row = "C9".into();
        editor.form.column = "00".into();
        editor.form.selection_changed();
        editor.load_selected().unwrap();
        editor.form.value = "2222".into();
        editor.apply_selected().unwrap();
        let command = editor
            .prepare_commit(app.project_revision())
            .unwrap()
            .unwrap();
        app.dispatch(command).unwrap();
        let loaded = app
            .project()
            .unwrap()
            .load_credits_tilemap_detected(&smw_us_v1_credits_tilemap_locator())
            .unwrap();
        assert_eq!(
            loaded.tilemap.words()[0xc9 * CreditsTilemap::COLUMNS],
            0x2222
        );
    }

    #[test]
    fn stale_revision_and_dirty_close_preserve_staged_tilemap() {
        let app = pristine_app();
        let mut editor = RomTilemapEditor::new(TilemapKind::Title);
        editor.open(&app);
        editor.form.value = "3456".into();
        editor.apply_selected().unwrap();
        assert!(editor.prepare_commit(app.project_revision() + 1).is_err());
        assert!(!editor.request_close(false));
        assert_eq!(editor.pending_close, Some(PendingClose::Editor));
        assert!(editor.is_open());
    }

    #[test]
    fn staged_title_tilemap_edit_is_recovered_without_committing_live_project() {
        let app = pristine_app();
        let mut editor = RomTilemapEditor::new(TilemapKind::Title);
        editor.open(&app);
        editor.form.value = "4567".into();
        editor.apply_selected().unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let loaded = reopened
            .project()
            .unwrap()
            .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())
            .unwrap();
        assert_eq!(
            u16::from_le_bytes([
                loaded.tilemap.primary_bytes()[0],
                loaded.tilemap.primary_bytes()[1]
            ]),
            0x4567
        );
    }

    #[test]
    fn staged_credits_tilemap_edit_is_recovered_without_committing_live_project() {
        let app = pristine_app();
        let mut editor = RomTilemapEditor::new(TilemapKind::Credits);
        editor.open(&app);
        editor.form.row = "C9".into();
        editor.form.column = "00".into();
        editor.form.selection_changed();
        editor.load_selected().unwrap();
        editor.form.value = "5678".into();
        editor.apply_selected().unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let loaded = reopened
            .project()
            .unwrap()
            .load_credits_tilemap_detected(&smw_us_v1_credits_tilemap_locator())
            .unwrap();
        assert_eq!(
            loaded.tilemap.words()[0xc9 * CreditsTilemap::COLUMNS],
            0x5678
        );
    }
}
