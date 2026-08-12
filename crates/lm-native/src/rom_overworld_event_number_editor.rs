use crate::level_editor_forms::parse_hex_u8;
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog, UiTextKey};
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
    pub(crate) fn staged_recovery_map<'a>(
        &'a self,
        app: &AppState,
    ) -> Result<Option<&'a EventNumberMap>, String> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Ok(None);
        };
        if workspace.revision != app.project_revision() {
            return Err("stale event-number workspace cannot be recovered".into());
        }
        Ok((workspace.current != workspace.original).then_some(&workspace.current))
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        if workspace.current == workspace.original {
            return None;
        }
        let content_revision = workspace.current.encode().iter().fold(
            0x4556_454e_544e_554d_u64 ^ workspace.current.stored_len() as u64,
            |revision, byte| revision.rotate_left(5) ^ u64::from(*byte),
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
            .ok_or_else(|| "event-number workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale event-number workspace cannot be recovered".into());
        }
        if workspace.current == workspace.original {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        lm_app::save_native_overworld_event_number_map_to_project(&mut staged, &workspace.current)
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
        catalog: Option<&LocalizationCatalog>,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new(crate::frontend_ui::extended_localized_text(
                catalog,
                ExtendedUiTextKey::EventNumberEditorTitle,
            ))
            .default_size([500.0, 300.0])
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
            ExtendedUiTextKey::EventNumberDescription,
        ));
        ui.label(
            crate::frontend_ui::extended_localized_text(
                catalog,
                ExtendedUiTextKey::EventNumberStoredLengthFormat,
            )
            .replace(
                "{length}",
                &format!("{:02X}", workspace.current.stored_len()),
            ),
        );
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::EventNumberStaleNotice,
                ),
            );
        }
        egui::Grid::new("rom-overworld-event-number-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::EventNumberEvent,
                ));
                if ui.text_edit_singleline(&mut self.event).changed() {
                    self.loaded_event = None;
                }
                ui.end_row();
                ui.label(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::EventNumberMappedEvent,
                ));
                ui.text_edit_singleline(&mut self.mapped);
                ui.end_row();
            });
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .button(crate::frontend_ui::extended_localized_text(
                    catalog,
                    ExtendedUiTextKey::EventNumberLoadEntry,
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
                        ExtendedUiTextKey::EventNumberApplyEntry,
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
                        ExtendedUiTextKey::EventNumberCommit,
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
                    ExtendedUiTextKey::EventNumberStaged
                } else {
                    ExtendedUiTextKey::EventNumberUnchanged
                },
            ));
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
            ExtendedUiTextKey::EventNumberDiscardTitle,
        ))
        .collapsible(false)
        .resizable(false)
        .show(context, |ui| {
            ui.label(crate::frontend_ui::extended_localized_text(
                catalog,
                ExtendedUiTextKey::EventNumberUnsavedNotice,
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
                ExtendedUiTextKey::EventNumberErrorTitle,
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
    use std::path::PathBuf;

    #[test]
    fn event_number_editor_surface_has_no_literal_widget_text() {
        let source = include_str!("rom_overworld_event_number_editor.rs");
        for literal_widget in [
            "egui::Window::new(\"",
            "ui.button(\"",
            "egui::Button::new(\"",
            "ui.label(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "event-number editor bypasses typed localization with {literal_widget}"
            );
        }
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("EventNumber"))
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

    #[test]
    fn staged_pristine_event_number_map_recovers_full_256_byte_storage() {
        let app = pristine_app();
        let mut editor = RomOverworldEventNumberEditor::default();
        editor.open(&app);
        editor.mapped = "21".into();
        editor.apply_selected().unwrap();
        editor.event = "FF".into();
        editor.loaded_event = None;
        editor.load_selected().unwrap();
        editor.mapped = "7E".into();
        editor.apply_selected().unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let map =
            reopened
                .project()
                .unwrap()
                .load_overworld_event_number_map_detected(
                    smw_us_v1_overworld_event_number_map_locator(),
                )
                .unwrap()
                .map;
        assert_eq!(map.stored_len(), EventNumberMap::ENTRY_COUNT);
        assert_eq!(map.get(0), 0x21);
        assert_eq!(map.get(0xff), 0x7e);
    }

    #[test]
    fn staged_installed_event_number_update_preserves_prior_mapping() {
        let mut installer = pristine_app();
        let mut first = RomOverworldEventNumberEditor::default();
        first.open(&installer);
        first.mapped = "34".into();
        first.apply_selected().unwrap();
        installer
            .dispatch(first.prepare_commit(0).unwrap().unwrap())
            .unwrap();

        let mut app = AppState::default();
        app.load_rom(installer.project().unwrap().save_snapshot())
            .unwrap();
        let mut editor = RomOverworldEventNumberEditor::default();
        editor.open(&app);
        editor.event = "FF".into();
        editor.loaded_event = None;
        editor.load_selected().unwrap();
        editor.mapped = "A5".into();
        editor.apply_selected().unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let map =
            reopened
                .project()
                .unwrap()
                .load_overworld_event_number_map_detected(
                    smw_us_v1_overworld_event_number_map_locator(),
                )
                .unwrap()
                .map;
        assert_eq!(map.stored_len(), EventNumberMap::ENTRY_COUNT);
        assert_eq!(map.get(0), 0x34);
        assert_eq!(map.get(0xff), 0xa5);
    }
}
