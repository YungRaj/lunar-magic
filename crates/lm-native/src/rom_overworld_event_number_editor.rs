use crate::level_editor_forms::parse_hex_u8;
use eframe::egui;
use lm_app::{AppState, Command};
use lm_overworld::EventNumberMap;
use lm_profile::smw_us_v1_overworld_event_number_map_locator;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    revision: u64,
    original: EventNumberMap,
    current: EventNumberMap,
}

#[derive(Default)]
pub(crate) struct RomOverworldEventNumberEditor {
    workspace: Option<Workspace>,
    event: String,
    mapped: String,
    loaded_event: Option<u8>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomOverworldEventNumberEditor {
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
                    .load_overworld_event_number_map_detected(
                        smw_us_v1_overworld_event_number_map_locator(),
                    )
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(loaded) => {
                self.event = "00".into();
                self.mapped = format!("{:02X}", loaded.map.get(0));
                self.loaded_event = Some(0);
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: loaded.map.clone(),
                    current: loaded.map,
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
            egui::Window::new("ROM Overworld Event-Number Map")
                .default_size([500.0, 300.0])
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
        ui.label("Complete 256-entry event-number mapping. Values are hexadecimal bytes.");
        ui.label(format!(
            "Current native stored length: {:02X}",
            workspace.current.stored_len()
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this map was opened. Reopen before committing.",
            );
        }
        egui::Grid::new("rom-overworld-event-number-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Event");
                if ui.text_edit_singleline(&mut self.event).changed() {
                    self.loaded_event = None;
                }
                ui.end_row();
                ui.label("Mapped event");
                ui.text_edit_singleline(&mut self.mapped);
                ui.end_row();
            });
        let mut command = None;
        ui.horizontal(|ui| {
            if ui.button("Load entry").clicked()
                && let Err(error) = self.load_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(!stale, egui::Button::new("Apply entry"))
                .clicked()
                && let Err(error) = self.apply_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(dirty && !stale, egui::Button::new("Commit map to ROM"))
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

    fn selected_event(&self) -> Result<u8, String> {
        parse_hex_u8(&self.event, "event number")
    }

    fn load_selected(&mut self) -> Result<(), String> {
        let event = self.selected_event()?;
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "event-number workspace is closed".to_owned())?;
        self.mapped = format!("{:02X}", workspace.current.get(event));
        self.loaded_event = Some(event);
        Ok(())
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        let event = self.selected_event()?;
        if self.loaded_event != Some(event) {
            return Err("load the selected event before applying it".into());
        }
        let mapped = parse_hex_u8(&self.mapped, "mapped event number")?;
        self.workspace
            .as_mut()
            .ok_or_else(|| "event-number workspace is closed".to_owned())?
            .current
            .set(event, mapped);
        Ok(())
    }

    fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "event-number workspace is closed".to_owned())?;
        if workspace.revision != project_revision {
            return Err("stale event-number workspace cannot be committed".into());
        }
        if workspace.current == workspace.original {
            return Ok(None);
        }
        Ok(Some(Command::ReplaceNativeOverworldEventNumberMap {
            rev: workspace.revision,
            map: Box::new(workspace.current.clone()),
        }))
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard event-number changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged mapping has not been committed to the ROM.");
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
            egui::Window::new("Event-number editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.loaded_event = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn pristine_app() -> AppState {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut app = AppState::default();
        app.load_rom(fs::read(root.join("Super Mario World (USA).sfc")).unwrap())
            .unwrap();
        app
    }

    #[test]
    fn high_event_edit_installs_full_map_and_reopens() {
        let mut app = pristine_app();
        let mut editor = RomOverworldEventNumberEditor::default();
        editor.open(&app);
        editor.event = "FF".into();
        editor.loaded_event = None;
        editor.load_selected().unwrap();
        editor.mapped = "7E".into();
        editor.apply_selected().unwrap();
        let command = editor.prepare_commit(0).unwrap().unwrap();
        app.dispatch(command).unwrap();
        let reopened =
            app.project()
                .unwrap()
                .load_overworld_event_number_map_detected(
                    smw_us_v1_overworld_event_number_map_locator(),
                )
                .unwrap()
                .map;
        assert_eq!(reopened.get(0xff), 0x7e);
        assert_eq!(reopened.stored_len(), EventNumberMap::ENTRY_COUNT);
    }

    #[test]
    fn malformed_unloaded_stale_and_dirty_states_are_retained() {
        let app = pristine_app();
        let mut editor = RomOverworldEventNumberEditor::default();
        editor.open(&app);
        editor.event = "100".into();
        assert!(editor.load_selected().is_err());
        editor.event = "01".into();
        editor.loaded_event = None;
        assert!(editor.apply_selected().is_err());
        editor.load_selected().unwrap();
        editor.mapped = "A5".into();
        editor.apply_selected().unwrap();
        assert!(editor.prepare_commit(1).is_err());
        assert!(!editor.request_close(false));
        assert!(editor.is_open());
    }
}
