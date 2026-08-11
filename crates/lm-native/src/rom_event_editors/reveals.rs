use crate::level_editor_forms::{parse_hex_u8, parse_hex_u16};
use eframe::egui;
use lm_app::{AppState, Command};
use lm_overworld::{EventReveal, EventRevealTable};
use lm_profile::smw_us_v1_overworld_event_reveal_locator;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    revision: u64,
    original: EventRevealTable,
    current: EventRevealTable,
}

#[derive(Default)]
pub(crate) struct RomOverworldEventRevealEditor {
    workspace: Option<Workspace>,
    index: String,
    source: String,
    destination: String,
    count: String,
    loaded: Option<usize>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomOverworldEventRevealEditor {
    pub(crate) fn staged_recovery_table<'a>(
        &'a self,
        app: &AppState,
    ) -> Result<Option<&'a EventRevealTable>, String> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Ok(None);
        };
        if workspace.revision != app.project_revision() {
            return Err("stale event-reveal workspace cannot be recovered".into());
        }
        Ok((workspace.current != workspace.original).then_some(&workspace.current))
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        if workspace.current == workspace.original {
            return None;
        }
        let content_revision = workspace.current.entries.iter().fold(
            0x4556_454e_5452_564c_u64 ^ workspace.current.entries.len() as u64,
            |revision, entry| {
                revision.rotate_left(7)
                    ^ u64::from(entry.source_tile)
                    ^ u64::from(entry.destination_tile).rotate_left(19)
            },
        );
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
            .ok_or_else(|| "event-reveal workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale event-reveal workspace cannot be recovered".into());
        }
        if workspace.current == workspace.original {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        lm_app::save_native_overworld_event_reveals_to_project(&mut staged, &workspace.current)
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
        let loaded = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())
            .and_then(|project| {
                project
                    .load_overworld_event_reveals_detected(
                        smw_us_v1_overworld_event_reveal_locator(),
                    )
                    .map_err(|error| error.to_string())
            });
        match loaded {
            Ok(loaded) => {
                self.count = format!("{:02X}", loaded.table.entries.len());
                self.index = "00".into();
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: loaded.table.clone(),
                    current: loaded.table,
                });
                self.load_selected().ok();
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
        revision: u64,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new("ROM Overworld Event Reveals")
                .default_size([530.0, 330.0])
                .show(context, |ui| command = self.contents(ui, revision));
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.revision != revision;
        let dirty = workspace.current != workspace.original;
        ui.label("Complete mixed-endian source/destination reveal table. Hexadecimal.");
        ui.label(format!(
            "Staged reveal records: {}",
            workspace.current.entries.len()
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this table was opened. Reopen before committing.",
            );
        }
        egui::Grid::new("rom-overworld-event-reveal-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Index");
                if ui.text_edit_singleline(&mut self.index).changed() {
                    self.loaded = None;
                }
                ui.end_row();
                row(ui, "Source tile (000–7FF)", &mut self.source);
                row(ui, "Destination tile", &mut self.destination);
            });
        ui.horizontal(|ui| {
            ui.label("Table count (01–FF)");
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
            if ui.button("Load reveal").clicked()
                && let Err(error) = self.load_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(!stale, egui::Button::new("Apply reveal"))
                .clicked()
                && let Err(error) = self.apply_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(dirty && !stale, egui::Button::new("Commit reveals to ROM"))
                .clicked()
            {
                match self.prepare_commit(revision) {
                    Ok(prepared) => command = prepared,
                    Err(error) => self.error = Some(error),
                }
            }
            ui.label(if dirty { "Staged" } else { "Unchanged" });
        });
        command
    }

    fn selected_index(&self) -> Result<usize, String> {
        let index = usize::from(parse_hex_u8(&self.index, "reveal index")?);
        let len = self
            .workspace
            .as_ref()
            .ok_or_else(|| "event-reveal workspace is closed".to_owned())?
            .current
            .entries
            .len();
        if index >= len {
            return Err(format!("reveal index must be below {len:02X}"));
        }
        Ok(index)
    }

    fn load_selected(&mut self) -> Result<(), String> {
        let index = self.selected_index()?;
        let entry = self.workspace.as_ref().unwrap().current.entries[index];
        self.source = format!("{:03X}", entry.source_tile);
        self.destination = format!("{:04X}", entry.destination_tile);
        self.loaded = Some(index);
        Ok(())
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        let index = self.selected_index()?;
        if self.loaded != Some(index) {
            return Err("load the selected reveal before applying it".into());
        }
        let entry = EventReveal {
            source_tile: parse_hex_u16(&self.source, "source tile")?,
            destination_tile: parse_hex_u16(&self.destination, "destination tile")?,
        };
        let workspace = self.workspace.as_mut().unwrap();
        let mut staged = workspace.current.clone();
        staged.entries[index] = entry;
        staged.validate().map_err(|error| error.to_string())?;
        workspace.current = staged;
        Ok(())
    }

    fn resize(&mut self) -> Result<(), String> {
        let count = usize::from(parse_hex_u8(&self.count, "reveal count")?);
        if count == 0 {
            return Err("reveal count must be between 01 and FF".into());
        }
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "event-reveal workspace is closed".to_owned())?;
        workspace
            .current
            .entries
            .resize(count, EventReveal::default());
        workspace
            .current
            .validate()
            .map_err(|error| error.to_string())?;
        self.loaded = None;
        Ok(())
    }

    fn prepare_commit(&self, revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "event-reveal workspace is closed".to_owned())?;
        if workspace.revision != revision {
            return Err("stale event-reveal workspace cannot be committed".into());
        }
        if workspace.current == workspace.original {
            return Ok(None);
        }
        workspace
            .current
            .validate()
            .map_err(|error| error.to_string())?;
        Ok(Some(Command::ReplaceNativeOverworldEventReveals {
            rev: workspace.revision,
            table: Box::new(workspace.current.clone()),
        }))
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard event-reveal changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged reveal table has not been committed.");
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
            egui::Window::new("Event-reveal editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.loaded = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.text_edit_singleline(value);
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn pristine_table_grows_installs_and_reopens_last_record() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original).unwrap();
        let mut editor = RomOverworldEventRevealEditor::default();
        editor.open(&app);
        assert_eq!(
            editor.workspace.as_ref().unwrap().current.entries.len(),
            112
        );
        editor.count = "C8".into();
        editor.resize().unwrap();
        editor.index = "C7".into();
        editor.load_selected().unwrap();
        editor.source = "7FF".into();
        editor.destination = "ABCD".into();
        editor.apply_selected().unwrap();
        app.dispatch(
            editor
                .prepare_commit(app.project_revision())
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())
            .unwrap();
        assert_eq!(reopened.table.entries.len(), 200);
        assert_eq!(reopened.table.entries[199].source_tile, 0x7ff);
        assert_eq!(reopened.table.entries[199].destination_tile, 0xabcd);
    }

    #[test]
    fn invalid_source_selection_stale_and_dirty_close_are_safe() {
        let table = EventRevealTable {
            entries: vec![EventReveal::default()],
        };
        let mut editor = RomOverworldEventRevealEditor {
            workspace: Some(Workspace {
                revision: 2,
                original: table.clone(),
                current: table,
            }),
            index: "00".into(),
            ..Default::default()
        };
        assert!(editor.apply_selected().is_err());
        editor.load_selected().unwrap();
        editor.source = "800".into();
        assert!(editor.apply_selected().is_err());
        editor.source = "001".into();
        editor.apply_selected().unwrap();
        assert!(editor.prepare_commit(3).is_err());
        assert!(!editor.request_close(true));
        assert!(editor.is_open());
    }

    #[test]
    fn staged_pristine_reveal_growth_recovers_complete_expanded_table() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original).unwrap();
        let mut editor = RomOverworldEventRevealEditor::default();
        editor.open(&app);
        editor.count = "C8".into();
        editor.resize().unwrap();
        editor.index = "C7".into();
        editor.load_selected().unwrap();
        editor.source = "7FF".into();
        editor.destination = "ABCD".into();
        editor.apply_selected().unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = &reopened
            .project()
            .unwrap()
            .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())
            .unwrap()
            .table;
        assert_eq!(table.entries.len(), 200);
        assert_eq!(table.entries[199].source_tile, 0x7ff);
        assert_eq!(table.entries[199].destination_tile, 0xabcd);
    }

    #[test]
    fn staged_installed_reveal_update_preserves_existing_expanded_records() {
        let mut installer = AppState::default();
        installer
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut first = RomOverworldEventRevealEditor::default();
        first.open(&installer);
        first.count = "C8".into();
        first.resize().unwrap();
        first.index = "C7".into();
        first.load_selected().unwrap();
        first.source = "700".into();
        first.destination = "A000".into();
        first.apply_selected().unwrap();
        installer
            .dispatch(first.prepare_commit(0).unwrap().unwrap())
            .unwrap();

        let mut app = AppState::default();
        app.load_rom(installer.project().unwrap().save_snapshot())
            .unwrap();
        let mut editor = RomOverworldEventRevealEditor::default();
        editor.open(&app);
        editor.index = "00".into();
        editor.load_selected().unwrap();
        editor.source = "321".into();
        editor.destination = "BEEF".into();
        editor.apply_selected().unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = &reopened
            .project()
            .unwrap()
            .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())
            .unwrap()
            .table;
        assert_eq!(table.entries.len(), 200);
        assert_eq!(table.entries[0].source_tile, 0x321);
        assert_eq!(table.entries[0].destination_tile, 0xbeef);
        assert_eq!(table.entries[199].source_tile, 0x700);
        assert_eq!(table.entries[199].destination_tile, 0xa000);
    }
}
