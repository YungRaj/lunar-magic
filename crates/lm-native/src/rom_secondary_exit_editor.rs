use crate::level_editor_forms::{SecondaryExitForm, parse_hex_u16};
use eframe::egui;
use lm_app::{AppState, Command};
use lm_level::SecondaryExitTable;
use lm_profile::smw_us_v1_secondary_exit_locator;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    revision: u64,
    original: SecondaryExitTable,
    table: SecondaryExitTable,
}

#[derive(Default)]
pub(crate) struct RomSecondaryExitEditor {
    workspace: Option<Workspace>,
    index: String,
    loaded_index: Option<usize>,
    form: SecondaryExitForm,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomSecondaryExitEditor {
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
                    .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(loaded) => {
                self.index = "0000".into();
                self.form = SecondaryExitForm::load(loaded.table.entries[0]);
                self.loaded_index = Some(0);
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: loaded.table.clone(),
                    table: loaded.table,
                });
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if workspace.table == workspace.original {
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
            self.load_selected();
            egui::Window::new("ROM Secondary Exit Table")
                .default_size([520.0, 570.0])
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
        let modified = workspace.table != workspace.original;
        ui.label("Global 8,192-entry native table. Values are hexadecimal.");
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this table was opened. Reopen before committing.",
            );
        }
        ui.horizontal(|ui| {
            ui.label("Entry");
            if ui.text_edit_singleline(&mut self.index).changed() {
                self.loaded_index = None;
            }
            if ui.button("Load").clicked() {
                self.load_selected();
            }
        });
        secondary_fields(ui, &mut self.form);
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale, egui::Button::new("Apply entry"))
                .clicked()
            {
                if let Err(error) = self.apply_selected() {
                    self.error = Some(error);
                }
            }
            if ui
                .add_enabled(modified && !stale, egui::Button::new("Commit table to ROM"))
                .clicked()
            {
                match self.prepare_commit(project_revision) {
                    Ok(prepared) => command = prepared,
                    Err(error) => self.error = Some(error),
                }
            }
            ui.label(if modified { "Staged" } else { "Unchanged" });
        });
        command
    }

    fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "secondary-exit workspace is closed".to_owned())?;
        if workspace.revision != project_revision {
            return Err("stale secondary-exit workspace cannot be committed".into());
        }
        if workspace.table == workspace.original {
            return Ok(None);
        }
        Ok(Some(Command::ReplaceNativeSecondaryExits {
            rev: workspace.revision,
            table: Box::new(workspace.table.clone()),
        }))
    }

    fn selected_index(&self) -> Result<usize, String> {
        let index = usize::from(parse_hex_u16(&self.index, "secondary-exit index")?);
        if index >= SecondaryExitTable::ENTRY_COUNT {
            return Err(format!(
                "secondary-exit index {index:#x} exceeds {:#x}",
                SecondaryExitTable::ENTRY_COUNT - 1
            ));
        }
        Ok(index)
    }

    fn load_selected(&mut self) {
        let result = self.selected_index().and_then(|index| {
            let workspace = self
                .workspace
                .as_ref()
                .ok_or_else(|| "secondary-exit workspace is closed".to_owned())?;
            Ok((
                index,
                SecondaryExitForm::load(workspace.table.entries[index]),
            ))
        });
        match result {
            Ok((index, form)) => {
                self.loaded_index = Some(index);
                self.form = form;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        let index = self.selected_index()?;
        if self.loaded_index != Some(index) {
            return Err("load the selected entry before applying it".into());
        }
        let value = self.form.parse()?;
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "secondary-exit workspace is closed".to_owned())?;
        workspace.table.entries[index] = value;
        Ok(())
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged secondary exits?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged global table has not been committed to the ROM.");
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
            egui::Window::new("Secondary-exit editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.loaded_index = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn secondary_fields(ui: &mut egui::Ui, form: &mut SecondaryExitForm) {
    egui::Grid::new("rom-secondary-exit-fields")
        .striped(true)
        .show(ui, |ui| {
            for (label, value) in [
                ("Destination", &mut form.destination),
                ("Position/method", &mut form.position),
                ("Screen", &mut form.screen),
                ("X", &mut form.x),
                ("Y", &mut form.y),
                ("Destination flags", &mut form.destination_flags),
                ("X/overworld flags", &mut form.x_flags),
                ("Additional flags", &mut form.additional),
            ] {
                ui.label(label);
                ui.text_edit_singleline(value);
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_table_stages_a_global_entry_and_emits_one_profile_command() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(original).unwrap();
        let mut editor = RomSecondaryExitEditor::default();
        editor.open(&app);
        assert!(editor.is_open());
        editor.index = "0400".into();
        editor.loaded_index = None;
        editor.load_selected();
        editor.form.destination = "0105".into();
        editor.apply_selected().unwrap();
        assert_eq!(
            editor.workspace.as_ref().unwrap().table.entries[0x400].destination_level,
            0x105
        );
        let command = editor.prepare_commit(0).unwrap().unwrap();
        app.dispatch(command).unwrap();
        assert_eq!(app.project_revision(), 1);
        assert!(editor.prepare_commit(1).is_err());
    }

    #[test]
    fn selection_must_be_loaded_and_close_guards_staged_changes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image =
            RomImage::from_bytes(fs::read(root.join("Super Mario World (USA).sfc")).unwrap())
                .unwrap();
        let mut app = AppState::default();
        app.load_rom(image.as_file_bytes().to_vec()).unwrap();
        let mut editor = RomSecondaryExitEditor::default();
        editor.open(&app);
        editor.index = "2000".into();
        assert!(editor.apply_selected().is_err());
        editor.index = "0001".into();
        editor.loaded_index = None;
        assert!(editor.apply_selected().is_err());
        editor.load_selected();
        editor.form.additional = "01".into();
        editor.apply_selected().unwrap();
        assert!(!editor.request_close(false));
        assert_eq!(editor.pending_close, Some(PendingClose::Editor));
    }
}
