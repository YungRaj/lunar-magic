use crate::level_editor_forms::{parse_hex_u8, parse_hex_u16};
use eframe::egui;
use lm_app::{AppState, Command};
use lm_overworld::{NativeOverworldLevelNameTable, OverworldLevelName};
use lm_profile::{smw_us_v1_overworld_level_name_locator, smw_us_v1_overworld_level_name_runtime};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    revision: u64,
    original: NativeOverworldLevelNameTable,
    current: NativeOverworldLevelNameTable,
}

#[derive(Default)]
pub(crate) struct RomOverworldLevelNameEditor {
    workspace: Option<Workspace>,
    level: String,
    tile: String,
    value: String,
    loaded: Option<(usize, usize)>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomOverworldLevelNameEditor {
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
                    .load_overworld_level_names_detected(
                        smw_us_v1_overworld_level_name_locator(),
                        smw_us_v1_overworld_level_name_runtime(),
                    )
                    .map_err(|error| error.to_string())
            });
        match loaded {
            Ok(loaded) => {
                self.level = "000".into();
                self.tile = "00".into();
                self.value = format!("{:02X}", loaded.table.names[0].tiles[0]);
                self.loaded = Some((0, 0));
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: loaded.table.clone(),
                    current: loaded.table,
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
            egui::Window::new("ROM Overworld Level Names")
                .default_size([520.0, 320.0])
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
        ui.label(
            "Lossless 19-tile level-name records. Level, tile index, and value are hexadecimal.",
        );
        ui.label(format!(
            "Staged name records: {}",
            workspace.current.names.len()
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this table was opened. Reopen before committing.",
            );
        }
        egui::Grid::new("rom-overworld-level-name-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Level");
                if ui.text_edit_singleline(&mut self.level).changed() {
                    self.loaded = None;
                }
                ui.end_row();
                ui.label("Tile (00–12)");
                if ui.text_edit_singleline(&mut self.tile).changed() {
                    self.loaded = None;
                }
                ui.end_row();
                ui.label("Tile value");
                ui.text_edit_singleline(&mut self.value);
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
                .add_enabled(dirty && !stale, egui::Button::new("Commit names to ROM"))
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

    fn selection(&self) -> Result<(usize, usize), String> {
        let level = parse_hex_u16(&self.level, "level number")?;
        let slot = if level <= 0x24 {
            usize::from(level)
        } else if (0x101..=0x1db).contains(&level) {
            usize::from(level - 0xdc)
        } else {
            return Err("level must be 000–024 or 101–1DB".into());
        };
        let tile = usize::from(parse_hex_u8(&self.tile, "tile index")?);
        if tile >= OverworldLevelName::TILE_COUNT {
            return Err("tile index must be 00–12".into());
        }
        Ok((slot, tile))
    }

    fn load_selected(&mut self) -> Result<(), String> {
        let selection = self.selection()?;
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "level-name workspace is closed".to_owned())?;
        self.value = format!(
            "{:02X}",
            workspace
                .current
                .names
                .get(selection.0)
                .map_or(0x1f, |name| name.tiles[selection.1])
        );
        self.loaded = Some(selection);
        Ok(())
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        let selection = self.selection()?;
        if self.loaded != Some(selection) {
            return Err("load the selected tile before applying it".into());
        }
        let value = parse_hex_u8(&self.value, "tile value")?;
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "level-name workspace is closed".to_owned())?;
        extend_names_through(&mut workspace.current, selection.0);
        workspace.current.names[selection.0].tiles[selection.1] = value;
        workspace
            .current
            .encode()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "level-name workspace is closed".to_owned())?;
        if workspace.revision != project_revision {
            return Err("stale level-name workspace cannot be committed".into());
        }
        if workspace.current == workspace.original {
            return Ok(None);
        }
        workspace
            .current
            .encode()
            .map_err(|error| error.to_string())?;
        Ok(Some(Command::ReplaceNativeOverworldLevelNames {
            rev: workspace.revision,
            table: Box::new(workspace.current.clone()),
        }))
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard level-name changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged level names have not been committed to the ROM.");
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
            egui::Window::new("Level-name editor error").show(context, |ui| {
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

fn extend_names_through(table: &mut NativeOverworldLevelNameTable, slot: usize) {
    while table.names.len() <= slot {
        let next = table.names.len();
        table.names.push(OverworldLevelName {
            level: NativeOverworldLevelNameTable::level_for_slot(next).unwrap_or(0),
            tiles: [0x1f; OverworldLevelName::TILE_COUNT],
            raw_flags: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn editor() -> RomOverworldLevelNameEditor {
        let table = NativeOverworldLevelNameTable {
            names: vec![OverworldLevelName {
                level: 0,
                tiles: [0x1f; OverworldLevelName::TILE_COUNT],
                raw_flags: 0,
            }],
        };
        RomOverworldLevelNameEditor {
            workspace: Some(Workspace {
                revision: 4,
                original: table.clone(),
                current: table,
            }),
            level: "101".into(),
            tile: "12".into(),
            value: "AB".into(),
            loaded: Some((0x25, 0x12)),
            ..Default::default()
        }
    }

    #[test]
    fn apply_expands_canonical_prefix_across_level_number_gap() {
        let mut editor = editor();
        editor.apply_selected().unwrap();
        let current = &editor.workspace.as_ref().unwrap().current;
        assert_eq!(current.names.len(), 0x26);
        assert_eq!(current.names[0x25].level, 0x101);
        assert_eq!(current.names[0x25].tiles[0x12], 0xab);
        assert!(current.encode().is_ok());
    }

    #[test]
    fn stale_and_changed_selection_are_rejected() {
        let mut editor = editor();
        assert!(editor.prepare_commit(5).is_err());
        editor.tile = "11".into();
        assert!(editor.apply_selected().is_err());
    }

    #[test]
    fn pristine_rom_install_reopens_with_the_staged_name() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(original).unwrap();
        let mut editor = RomOverworldLevelNameEditor::default();
        editor.open(&app);
        editor.value = "5A".into();
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
            .load_overworld_level_names_detected(
                smw_us_v1_overworld_level_name_locator(),
                smw_us_v1_overworld_level_name_runtime(),
            )
            .unwrap();
        assert_eq!(reopened.table.names[0].tiles[0], 0x5a);
    }
}
