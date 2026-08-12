use crate::{
    level_editor_forms::{parse_hex_u8, parse_hex_u16},
    overworld_editor_forms::RevealForm,
};
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog, UiTextKey};
use lm_overworld::SpecialEventRevealTable;
use lm_profile::smw_us_v1_special_event_reveal_locator;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    revision: u64,
    original: SpecialEventRevealTable,
    current: SpecialEventRevealTable,
}

#[derive(Default)]
pub(crate) struct RomOverworldSpecialEventEditor {
    workspace: Option<Workspace>,
    index: String,
    loaded_index: Option<usize>,
    reveal: RevealForm,
    direction: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomOverworldSpecialEventEditor {
    pub(crate) fn staged_recovery_table<'a>(
        &'a self,
        app: &AppState,
    ) -> Result<Option<&'a SpecialEventRevealTable>, String> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Ok(None);
        };
        if workspace.revision != app.project_revision() {
            return Err("stale special-event workspace cannot be recovered".into());
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
            .reveals
            .iter()
            .flat_map(|reveal| {
                reveal
                    .source_tile
                    .to_le_bytes()
                    .into_iter()
                    .chain(reveal.destination_tile.to_le_bytes())
            })
            .chain(workspace.current.directions)
            .fold(0x5350_4543_4556_454e_u64, |revision, byte| {
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
            .ok_or_else(|| "special-event workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale special-event workspace cannot be recovered".into());
        }
        if workspace.current == workspace.original {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        lm_app::save_native_special_event_reveals_to_project(&mut staged, &workspace.current)
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
                    .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(loaded) => {
                self.index = "00".into();
                self.loaded_index = Some(0);
                self.reveal = RevealForm::load(loaded.table.reveals[0]);
                self.direction = format!("{:02X}", loaded.table.directions[0]);
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
                ExtendedUiTextKey::SpecialEventEditorTitle,
            ))
                .default_size([520.0, 340.0])
                .show(context, |ui| command = self.contents(ui, project_revision, catalog));
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
            ExtendedUiTextKey::SpecialEventDescription,
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::SpecialEventStaleNotice,
                ),
            );
        }
        egui::Grid::new("rom-special-event-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::SpecialEventIndex,
                ));
                if ui.text_edit_singleline(&mut self.index).changed() {
                    self.loaded_index = None;
                }
                ui.end_row();
                ui.label(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::SpecialEventSourceTile,
                ));
                ui.text_edit_singleline(&mut self.reveal.source);
                ui.end_row();
                ui.label(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::SpecialEventDestinationTile,
                ));
                ui.text_edit_singleline(&mut self.reveal.destination);
                ui.end_row();
                ui.label(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::SpecialEventDirection,
                ));
                ui.text_edit_singleline(&mut self.direction);
                ui.end_row();
            });
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .button(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::SpecialEventLoadEntry,
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
                        ExtendedUiTextKey::SpecialEventApplyEntry,
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
                        ExtendedUiTextKey::SpecialEventCommit,
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
                    ExtendedUiTextKey::SpecialEventStaged
                } else {
                    ExtendedUiTextKey::SpecialEventUnchanged
                },
            ));
        });
        command
    }

    fn selected_index(&self) -> Result<usize, String> {
        let index = usize::from(parse_hex_u16(&self.index, "special-event index")?);
        if index >= SpecialEventRevealTable::ENTRY_COUNT {
            return Err(format!(
                "special-event index {index:#x} exceeds {:#x}",
                SpecialEventRevealTable::ENTRY_COUNT - 1
            ));
        }
        Ok(index)
    }

    fn load_selected(&mut self) -> Result<(), String> {
        let index = self.selected_index()?;
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "special-event workspace is closed".to_owned())?;
        self.reveal = RevealForm::load(workspace.current.reveals[index]);
        self.direction = format!("{:02X}", workspace.current.directions[index]);
        self.loaded_index = Some(index);
        Ok(())
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        let index = self.selected_index()?;
        if self.loaded_index != Some(index) {
            return Err("load the selected special event before applying it".into());
        }
        let reveal = self.reveal.parse()?;
        let direction = parse_hex_u8(&self.direction, "special-event direction")?;
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "special-event workspace is closed".to_owned())?;
        let mut edited = workspace.current.clone();
        edited.reveals[index] = reveal;
        edited.directions[index] = direction;
        edited.encode().map_err(|error| error.to_string())?;
        workspace.current = edited;
        Ok(())
    }

    fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "special-event workspace is closed".to_owned())?;
        if workspace.revision != project_revision {
            return Err("stale special-event workspace cannot be committed".into());
        }
        if workspace.current == workspace.original {
            return Ok(None);
        }
        Ok(Some(Command::ReplaceNativeSpecialEventReveals {
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
            ExtendedUiTextKey::SpecialEventDiscardTitle,
        ))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::SpecialEventUnsavedNotice,
                ));
                ui.horizontal(|ui| {
                    if ui.button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::CommonCancel,
                    )).clicked() {
                        self.pending_close = None;
                    }
                    if ui.button(crate::frontend_ui::localized_text(
                        catalog,
                        UiTextKey::UnsavedDiscard,
                    )).clicked() {
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
                ExtendedUiTextKey::SpecialEventErrorTitle,
            )).show(context, |ui| {
                ui.label(error);
                if ui.button(crate::frontend_ui::localized_text(
                    catalog,
                    UiTextKey::CommonOk,
                )).clicked() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn special_event_editor_surface_has_no_literal_widget_text() {
        let source = include_str!("rom_overworld_special_event_editor.rs");
        for literal_widget in [
            "egui::Window::new(\"",
            "ui.button(\"",
            "egui::Button::new(\"",
            "ui.label(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "special-event editor bypasses typed localization with {literal_widget}"
            );
        }
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("SpecialEvent"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
    }

    fn pristine_app() -> AppState {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app
    }

    #[test]
    fn pristine_special_event_edit_installs_and_reopens_all_planes() {
        let mut app = pristine_app();
        let mut editor = RomOverworldSpecialEventEditor::default();
        editor.open(&app);
        editor.index = "17".into();
        editor.loaded_index = None;
        editor.load_selected().unwrap();
        editor.reveal.source = "0123".into();
        editor.reveal.destination = "0456".into();
        editor.direction = "87".into();
        editor.apply_selected().unwrap();
        let command = editor.prepare_commit(0).unwrap().unwrap();
        app.dispatch(command).unwrap();
        let reopened = app
            .project()
            .unwrap()
            .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
            .unwrap()
            .table;
        assert_eq!(reopened.reveals[23].source_tile, 0x123);
        assert_eq!(reopened.reveals[23].destination_tile, 0x456);
        assert_eq!(reopened.directions[23], 0x87);
    }

    #[test]
    fn invalid_source_unloaded_stale_and_dirty_states_are_retained() {
        let app = pristine_app();
        let mut editor = RomOverworldSpecialEventEditor::default();
        editor.open(&app);
        editor.reveal.source = "0800".into();
        assert!(editor.apply_selected().is_err());
        editor.index = "01".into();
        editor.loaded_index = None;
        assert!(editor.apply_selected().is_err());
        editor.load_selected().unwrap();
        editor.reveal.destination = "1234".into();
        editor.apply_selected().unwrap();
        assert!(editor.prepare_commit(1).is_err());
        assert!(!editor.request_close(false));
        assert!(editor.is_open());
    }

    #[test]
    fn staged_pristine_special_events_recover_all_three_complete_planes() {
        let app = pristine_app();
        let mut editor = RomOverworldSpecialEventEditor::default();
        editor.open(&app);
        editor.reveal.source = "0111".into();
        editor.reveal.destination = "0222".into();
        editor.direction = "33".into();
        editor.apply_selected().unwrap();
        editor.index = "17".into();
        editor.loaded_index = None;
        editor.load_selected().unwrap();
        editor.reveal.source = "0444".into();
        editor.reveal.destination = "0555".into();
        editor.direction = "66".into();
        editor.apply_selected().unwrap();
        let expected = editor.workspace.as_ref().unwrap().current.clone();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = reopened
            .project()
            .unwrap()
            .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
            .unwrap()
            .table;
        assert_eq!(table, expected);
    }

    #[test]
    fn staged_installed_special_event_update_preserves_prior_record() {
        let mut installer = pristine_app();
        let mut first = RomOverworldSpecialEventEditor::default();
        first.open(&installer);
        first.reveal.source = "0123".into();
        first.reveal.destination = "0234".into();
        first.direction = "45".into();
        first.apply_selected().unwrap();
        installer
            .dispatch(first.prepare_commit(0).unwrap().unwrap())
            .unwrap();

        let mut app = AppState::default();
        app.load_rom(installer.project().unwrap().save_snapshot())
            .unwrap();
        let mut editor = RomOverworldSpecialEventEditor::default();
        editor.open(&app);
        editor.index = "17".into();
        editor.loaded_index = None;
        editor.load_selected().unwrap();
        editor.reveal.source = "0345".into();
        editor.reveal.destination = "0456".into();
        editor.direction = "67".into();
        editor.apply_selected().unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = reopened
            .project()
            .unwrap()
            .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
            .unwrap()
            .table;
        assert_eq!(table.reveals[0].source_tile, 0x123);
        assert_eq!(table.reveals[0].destination_tile, 0x234);
        assert_eq!(table.directions[0], 0x45);
        assert_eq!(table.reveals[23].source_tile, 0x345);
        assert_eq!(table.reveals[23].destination_tile, 0x456);
        assert_eq!(table.directions[23], 0x67);
    }
}
