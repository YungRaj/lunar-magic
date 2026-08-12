use crate::level_editor_forms::{parse_hex_u8, parse_hex_u16};
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog, UiTextKey};
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
    pub(crate) fn staged_recovery_table<'a>(
        &'a self,
        app: &AppState,
    ) -> Result<Option<&'a NativeOverworldLevelNameTable>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "level-name workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale level-name workspace cannot be recovered".into());
        }
        Ok((workspace.current != workspace.original).then_some(&workspace.current))
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        if workspace.current == workspace.original {
            return None;
        }
        let content_revision = workspace.current.names.iter().fold(
            0x4c45_5645_4c4e_414d_u64 ^ workspace.current.names.len() as u64,
            |mut revision, name| {
                for byte in name
                    .level
                    .to_le_bytes()
                    .into_iter()
                    .chain(name.tiles)
                    .chain([name.raw_flags])
                {
                    revision = revision.rotate_left(5) ^ u64::from(byte);
                }
                revision
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
            .ok_or_else(|| "level-name workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale level-name workspace cannot be recovered".into());
        }
        if workspace.current == workspace.original {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        lm_app::save_native_overworld_level_names_to_project(&mut staged, &workspace.current)
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
        catalog: Option<&LocalizationCatalog>,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new(crate::frontend_ui::extended_localized_text(
                catalog,
                ExtendedUiTextKey::LevelNameEditorTitle,
            ))
            .default_size([520.0, 320.0])
            .show(context, |ui| {
                command = self.contents(ui, project_revision, catalog)
            });
        }
        let approved = self.close_confirmation(context, catalog);
        self.show_error(context, catalog);
        (approved, command)
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        project_revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.revision != project_revision;
        let dirty = workspace.current != workspace.original;
        ui.label(crate::frontend_ui::extended_localized_text(
            catalog,
            ExtendedUiTextKey::LevelNameDescription,
        ));
        ui.label(
            crate::frontend_ui::extended_localized_text(
                catalog,
                ExtendedUiTextKey::LevelNameCountFormat,
            )
            .replace("{count}", &workspace.current.names.len().to_string()),
        );
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::LevelNameStaleNotice,
                ),
            );
        }
        egui::Grid::new("rom-overworld-level-name-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::LevelNameLevel,
                ));
                if ui.text_edit_singleline(&mut self.level).changed() {
                    self.loaded = None;
                }
                ui.end_row();
                ui.label(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::LevelNameTile,
                ));
                if ui.text_edit_singleline(&mut self.tile).changed() {
                    self.loaded = None;
                }
                ui.end_row();
                ui.label(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::LevelNameTileValue,
                ));
                ui.text_edit_singleline(&mut self.value);
                ui.end_row();
            });
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .button(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::LevelNameLoadTile,
                ))
                .clicked()
                && let Err(error) = self.load_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    !stale,
                    egui::Button::new(crate::frontend_ui::extended_localized_text(
                        catalog,
                        ExtendedUiTextKey::LevelNameApplyTile,
                    )),
                )
                .clicked()
                && let Err(error) = self.apply_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    dirty && !stale,
                    egui::Button::new(crate::frontend_ui::extended_localized_text(
                        catalog,
                        ExtendedUiTextKey::LevelNameCommit,
                    )),
                )
                .clicked()
            {
                match self.prepare_commit(project_revision) {
                    Ok(prepared) => command = prepared,
                    Err(error) => self.error = Some(error),
                }
            }
            ui.label(crate::frontend_ui::extended_localized_text(
                catalog,
                if dirty {
                    ExtendedUiTextKey::LevelNameStaged
                } else {
                    ExtendedUiTextKey::LevelNameUnchanged
                },
            ));
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

    fn close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(crate::frontend_ui::extended_localized_text(
            catalog,
            ExtendedUiTextKey::LevelNameDiscardTitle,
        ))
        .collapsible(false)
        .resizable(false)
        .show(context, |ui| {
            ui.label(crate::frontend_ui::extended_localized_text(
                catalog,
                ExtendedUiTextKey::LevelNameUnsavedNotice,
            ));
            ui.horizontal(|ui| {
                if ui
                    .button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::CommonCancel,
                    ))
                    .clicked()
                {
                    self.pending_close = None;
                }
                if ui
                    .button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::UnsavedDiscard,
                    ))
                    .clicked()
                {
                    self.clear();
                    approved = pending == PendingClose::Application;
                }
            });
        });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(crate::frontend_ui::extended_localized_text(
                catalog,
                ExtendedUiTextKey::LevelNameErrorTitle,
            ))
            .show(context, |ui| {
                ui.label(error);
                if ui
                    .button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::CommonOk,
                    ))
                    .clicked()
                {
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
    use std::path::PathBuf;

    #[test]
    fn level_name_editor_surface_has_no_literal_widget_text() {
        let source = include_str!("rom_overworld_level_name_editor.rs");
        for literal_widget in [
            "egui::Window::new(\"",
            "ui.button(\"",
            "egui::Button::new(\"",
            "ui.label(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "level-name editor bypasses typed localization with {literal_widget}"
            );
        }
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("LevelName"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
    }

    fn pristine_app() -> AppState {
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app
    }

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
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
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

    #[test]
    fn staged_pristine_level_names_recover_the_complete_maximum_table() {
        let app = pristine_app();
        let mut editor = RomOverworldLevelNameEditor::default();
        editor.open(&app);
        editor.level = "1DB".into();
        editor.tile = "12".into();
        editor.loaded = None;
        editor.load_selected().unwrap();
        editor.value = "A5".into();
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
            .load_overworld_level_names_detected(
                smw_us_v1_overworld_level_name_locator(),
                smw_us_v1_overworld_level_name_runtime(),
            )
            .unwrap();
        assert!(matches!(
            loaded.storage,
            lm_project::OverworldLevelNameStorage::Expanded { .. }
        ));
        assert_eq!(loaded.table.names.len(), 0x100);
        assert_eq!(loaded.table.names[0xff].level, 0x1db);
        assert_eq!(loaded.table.names[0xff].tiles[0x12], 0xa5);
    }

    #[test]
    fn staged_installed_level_name_growth_preserves_prior_name() {
        let mut installer = pristine_app();
        let mut first = RomOverworldLevelNameEditor::default();
        first.open(&installer);
        first.value = "5A".into();
        first.apply_selected().unwrap();
        installer
            .dispatch(first.prepare_commit(0).unwrap().unwrap())
            .unwrap();

        let mut app = AppState::default();
        app.load_rom(installer.project().unwrap().save_snapshot())
            .unwrap();
        let mut editor = RomOverworldLevelNameEditor::default();
        editor.open(&app);
        editor.level = "1DB".into();
        editor.tile = "12".into();
        editor.loaded = None;
        editor.load_selected().unwrap();
        editor.value = "C3".into();
        editor.apply_selected().unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = reopened
            .project()
            .unwrap()
            .load_overworld_level_names_detected(
                smw_us_v1_overworld_level_name_locator(),
                smw_us_v1_overworld_level_name_runtime(),
            )
            .unwrap()
            .table;
        assert_eq!(table.names[0].tiles[0], 0x5a);
        assert_eq!(table.names.len(), 0x100);
        assert_eq!(table.names[0xff].tiles[0x12], 0xc3);
    }
}
