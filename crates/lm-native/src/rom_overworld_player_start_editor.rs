use crate::{
    level_editor_forms::{format_bytes, parse_hex_u16},
    overworld_editor_forms::SUBMAP_NAMES,
};
use eframe::egui;
use lm_app::{AppState, Command};
use lm_overworld::{NativeOverworldPlayerStarts, Submap};
use lm_profile::smw_us_v1_overworld_player_start_layout;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Clone, Debug, Default)]
struct PlayerStartForm {
    x: String,
    y: String,
    submap: usize,
}

impl PlayerStartForm {
    fn load(starts: &NativeOverworldPlayerStarts, player: usize) -> Self {
        let start = starts.starts[player];
        Self {
            x: format!("{:04X}", start.x),
            y: format!("{:04X}", start.y),
            submap: usize::from(start.submap.encoded()),
        }
    }

    fn apply(
        &self,
        starts: &NativeOverworldPlayerStarts,
        player: usize,
    ) -> Result<NativeOverworldPlayerStarts, String> {
        let mut edited = starts.clone();
        let start = edited
            .starts
            .get_mut(player)
            .ok_or_else(|| "player-start index is outside the native table".to_owned())?;
        start.x = parse_hex_u16(&self.x, "player-start X")?;
        start.y = parse_hex_u16(&self.y, "player-start Y")?;
        start.submap = Submap::decode(u8::try_from(self.submap).unwrap_or(u8::MAX))
            .ok_or_else(|| "invalid overworld submap".to_owned())?;
        edited.encode().map_err(|error| error.to_string())?;
        Ok(edited)
    }
}

struct Workspace {
    revision: u64,
    original: NativeOverworldPlayerStarts,
    current: NativeOverworldPlayerStarts,
}

#[derive(Default)]
pub(crate) struct RomOverworldPlayerStartEditor {
    workspace: Option<Workspace>,
    player: usize,
    loaded_player: Option<usize>,
    form: PlayerStartForm,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomOverworldPlayerStartEditor {
    pub(crate) fn staged_recovery_starts<'a>(
        &'a self,
        app: &AppState,
    ) -> Result<Option<&'a NativeOverworldPlayerStarts>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "player-start workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale player-start workspace cannot be recovered".into());
        }
        Ok((workspace.current != workspace.original).then_some(&workspace.current))
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        if workspace.current == workspace.original {
            return None;
        }
        let content_revision = workspace
            .current
            .starts
            .iter()
            .flat_map(|start| {
                let x = start.x.to_le_bytes();
                let y = start.y.to_le_bytes();
                [
                    start.player,
                    x[0],
                    x[1],
                    y[0],
                    y[1],
                    start.submap.encoded(),
                    start.raw_flags,
                ]
            })
            .chain(workspace.current.reserved)
            .fold(0x504c_4159_5354_4152_u64, |revision, byte| {
                revision.rotate_left(5) ^ u64::from(byte)
            });
        Some(
            app.project_revision().wrapping_mul(0xa24b_aed4_963e_e407)
                ^ workspace.revision.rotate_left(31)
                ^ content_revision,
        )
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "player-start workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale player-start workspace cannot be recovered".into());
        }
        if workspace.current == workspace.original {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        lm_app::save_native_overworld_player_starts_to_project(&mut staged, &workspace.current)
            .map_err(|error| error.to_string())?;
        app.recovery_snapshot_with_current_rom(staged.save_snapshot(), app.current_level())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        let result = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())
            .and_then(|project| {
                project
                    .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(starts) => {
                self.player = 0;
                self.loaded_player = Some(0);
                self.form = PlayerStartForm::load(&starts, 0);
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: starts.clone(),
                    current: starts,
                });
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if workspace.current == workspace.original {
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
            egui::Window::new("ROM Overworld Player Starts")
                .default_size([500.0, 340.0])
                .show(context, |ui| command = self.contents(ui, project_revision));
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.revision != project_revision;
        let dirty = workspace.current != workspace.original;
        ui.label("Exact two-player native start records. Coordinates are hexadecimal.");
        ui.label(format!(
            "Preserved adjacent option bytes: {}",
            format_bytes(&workspace.current.reserved)
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after these starts were opened. Reopen before committing.",
            );
        }
        ui.horizontal(|ui| {
            ui.label("Player");
            if ui.selectable_value(&mut self.player, 0, "Mario").clicked()
                | ui.selectable_value(&mut self.player, 1, "Luigi").clicked()
            {
                self.loaded_player = None;
            }
            if ui.button("Load").clicked()
                && let Err(error) = self.load_selected()
            {
                self.error = Some(error);
            }
        });
        egui::Grid::new("rom-overworld-player-start-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label("X");
                ui.text_edit_singleline(&mut self.form.x);
                ui.end_row();
                ui.label("Y");
                ui.text_edit_singleline(&mut self.form.y);
                ui.end_row();
                ui.label("Submap");
                egui::ComboBox::from_id_salt("rom-player-start-submap")
                    .selected_text(
                        SUBMAP_NAMES
                            .get(self.form.submap)
                            .copied()
                            .unwrap_or("Invalid"),
                    )
                    .show_ui(ui, |ui| {
                        for (index, name) in SUBMAP_NAMES.iter().enumerate() {
                            ui.selectable_value(&mut self.form.submap, index, *name);
                        }
                    });
                ui.end_row();
            });
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale, egui::Button::new("Apply player"))
                .clicked()
                && let Err(error) = self.apply_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(dirty && !stale, egui::Button::new("Commit starts to ROM"))
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
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "player-start workspace is closed".to_owned())?;
        if self.player >= workspace.current.starts.len() {
            return Err("invalid player-start index".into());
        }
        self.form = PlayerStartForm::load(&workspace.current, self.player);
        self.loaded_player = Some(self.player);
        Ok(())
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        if self.loaded_player != Some(self.player) {
            return Err("load the selected player before applying it".into());
        }
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "player-start workspace is closed".to_owned())?;
        workspace.current = self.form.apply(&workspace.current, self.player)?;
        Ok(())
    }

    fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "player-start workspace is closed".to_owned())?;
        if workspace.revision != project_revision {
            return Err("stale player-start workspace cannot be committed".into());
        }
        if workspace.current == workspace.original {
            return Ok(None);
        }
        Ok(Some(Command::ReplaceNativeOverworldPlayerStarts {
            rev: workspace.revision,
            starts: Box::new(workspace.current.clone()),
        }))
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard player-start changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged start records have not been committed to the ROM.");
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
            egui::Window::new("Player-start editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.loaded_player = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pristine_app() -> AppState {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app
    }

    #[test]
    fn player_edit_dispatches_reopens_and_preserves_reserved_options() {
        let mut app = pristine_app();
        let mut editor = RomOverworldPlayerStartEditor::default();
        editor.open(&app);
        let reserved = editor.workspace.as_ref().unwrap().current.reserved;
        editor.form.x = "0098".into();
        editor.form.y = "00B8".into();
        editor.form.submap = usize::from(Submap::StarWorld.encoded());
        editor.apply_selected().unwrap();
        let command = editor.prepare_commit(0).unwrap().unwrap();
        app.dispatch(command).unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())
            .unwrap();
        assert_eq!(reopened.starts[0].submap, Submap::StarWorld);
        assert_eq!(reopened.starts[0].x, 0x98);
        assert_eq!(reopened.reserved, reserved);
    }

    #[test]
    fn unaligned_stale_and_unloaded_edits_preserve_workspace() {
        let app = pristine_app();
        let mut editor = RomOverworldPlayerStartEditor::default();
        editor.open(&app);
        editor.form.x = "0069".into();
        assert!(editor.apply_selected().is_err());
        editor.form.x = "0088".into();
        editor.player = 1;
        editor.loaded_player = None;
        assert!(editor.apply_selected().is_err());
        editor.load_selected().unwrap();
        editor.form.x = "0088".into();
        editor.apply_selected().unwrap();
        assert!(editor.prepare_commit(1).is_err());
        assert!(!editor.request_close(false));
        assert!(editor.is_open());
    }

    #[test]
    fn staged_player_starts_recover_both_players_and_reserved_options_exactly() {
        let app = pristine_app();
        let mut editor = RomOverworldPlayerStartEditor::default();
        editor.open(&app);
        let reserved = editor.workspace.as_ref().unwrap().current.reserved;

        editor.form.x = "0098".into();
        editor.form.y = "00B8".into();
        editor.form.submap = usize::from(Submap::StarWorld.encoded());
        editor.apply_selected().unwrap();
        editor.player = 1;
        editor.loaded_player = None;
        editor.load_selected().unwrap();
        editor.form.x = "0128".into();
        editor.form.y = "0068".into();
        editor.form.submap = usize::from(Submap::ForestOfIllusion.encoded());
        editor.apply_selected().unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let starts = reopened
            .project()
            .unwrap()
            .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())
            .unwrap();
        assert_eq!(starts.starts[0].submap, Submap::StarWorld);
        assert_eq!((starts.starts[0].x, starts.starts[0].y), (0x98, 0xb8));
        assert_eq!(starts.starts[1].submap, Submap::ForestOfIllusion);
        assert_eq!((starts.starts[1].x, starts.starts[1].y), (0x128, 0x68));
        assert_eq!(starts.reserved, reserved);
    }
}
