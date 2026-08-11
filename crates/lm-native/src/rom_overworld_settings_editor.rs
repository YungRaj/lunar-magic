use crate::expanded_settings_editor_form::ExpandedSettingsForm;
use eframe::egui;
use lm_app::{AppState, Command};
use lm_level::ExpandedOverworldSettings;
use lm_overworld::{OverworldLayer3SettingsRecord, OverworldLayer3SettingsTable};
use lm_profile::load_smw_us_v1_overworld_settings;

mod layer3_form;

use layer3_form::Layer3Form;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    revision: u64,
    installed: bool,
    original: ExpandedOverworldSettings,
    current: ExpandedOverworldSettings,
}

#[derive(Default)]
pub(crate) struct RomOverworldSettingsEditor {
    workspace: Option<Workspace>,
    submap: usize,
    loaded_submap: Option<usize>,
    form: ExpandedSettingsForm,
    layer3_form: Layer3Form,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomOverworldSettingsEditor {
    pub(crate) fn staged_recovery_settings<'a>(
        &'a self,
        app: &AppState,
    ) -> Result<Option<&'a ExpandedOverworldSettings>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "overworld-settings workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale overworld-settings workspace cannot be recovered".into());
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
            .records
            .iter()
            .flat_map(|record| record.encoded().iter().copied())
            .fold(0x4f57_5345_5454_494e_u64, |revision, byte| {
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
            .ok_or_else(|| "overworld-settings workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale overworld-settings workspace cannot be recovered".into());
        }
        if workspace.current == workspace.original {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        lm_app::save_native_overworld_settings_to_project(&mut staged, &workspace.current)
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
                load_smw_us_v1_overworld_settings(project).map_err(|error| error.to_string())
            });
        match result {
            Ok(loaded) => {
                self.submap = 0;
                self.loaded_submap = Some(0);
                self.form = ExpandedSettingsForm::load(&loaded.settings.records[0]);
                self.layer3_form = Layer3Form::load(&OverworldLayer3SettingsRecord::from_bytes(
                    *loaded.settings.records[0].encoded(),
                ));
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    installed: loaded.installed,
                    original: loaded.settings.clone(),
                    current: loaded.settings,
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
            egui::Window::new("ROM Overworld Global Settings")
                .default_size([620.0, 600.0])
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
        ui.label("Seven lossless 16-word special settings records. Values are hexadecimal.");
        ui.label(if workspace.installed {
            "Expanded settings are installed."
        } else {
            "Pristine defaults; committing installs the recovered expanded-settings runtime."
        });
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after these settings were opened. Reopen before committing.",
            );
        }
        ui.horizontal(|ui| {
            ui.label("Submap record");
            egui::ComboBox::from_id_salt("rom-overworld-settings-submap")
                .selected_text(format!("{}", self.submap))
                .show_ui(ui, |ui| {
                    for index in 0..ExpandedOverworldSettings::SUBMAP_COUNT {
                        if ui
                            .selectable_value(&mut self.submap, index, format!("{index}"))
                            .clicked()
                        {
                            self.loaded_submap = None;
                        }
                    }
                });
            if ui.button("Load").clicked()
                && let Err(error) = self.load_selected()
            {
                self.error = Some(error);
            }
        });
        egui::Grid::new("rom-overworld-settings-words")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                for index in 0..self.form.words.len() {
                    ui.label(format!("Word {index:X}"));
                    ui.text_edit_singleline(&mut self.form.words[index]);
                    if index % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        self.show_layer3_form(ui, stale);
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale, egui::Button::new("Apply record"))
                .clicked()
                && let Err(error) = self.apply_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(dirty && !stale, egui::Button::new("Commit settings to ROM"))
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

    fn show_layer3_form(&mut self, ui: &mut egui::Ui, stale: bool) {
        egui::CollapsingHeader::new("Semantic Layer 3 settings")
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("rom-overworld-layer3-semantic-fields")
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Use custom tilemap");
                        ui.checkbox(&mut self.layer3_form.uses_custom_tilemap, "");
                        ui.end_row();
                        ui.label("Use custom graphics");
                        ui.checkbox(&mut self.layer3_form.uses_custom_graphics, "");
                        ui.end_row();
                        ui.label("Tilemap file");
                        ui.add(
                            egui::DragValue::new(&mut self.layer3_form.tilemap_file)
                                .range(0..=0x0fff)
                                .hexadecimal(3, false, true),
                        );
                        ui.end_row();
                        ui.label("Tilemap size");
                        ui.add(
                            egui::DragValue::new(&mut self.layer3_form.tilemap_size).range(0..=3),
                        );
                        ui.end_row();
                        ui.label("Tilemap position");
                        ui.add(
                            egui::DragValue::new(&mut self.layer3_form.tilemap_position)
                                .range(0..=3),
                        );
                        ui.end_row();
                    });
                ui.label("Address-layout words");
                egui::Grid::new("rom-overworld-layer3-layout-words")
                    .num_columns(4)
                    .show(ui, |ui| {
                        for (index, value) in
                            self.layer3_form.layout_words.iter_mut().enumerate()
                        {
                            ui.label(format!("{index}"));
                            ui.add(
                                egui::DragValue::new(value).hexadecimal(4, false, true),
                            );
                            if index % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                ui.label("Graphics files");
                ui.horizontal(|ui| {
                    for (index, value) in
                        self.layer3_form.graphics_files.iter_mut().enumerate()
                    {
                        ui.label(format!("GFX {index}"));
                        ui.add(
                            egui::DragValue::new(value)
                                .range(0..=0x0fff)
                                .hexadecimal(3, false, true),
                        );
                    }
                });
                if ui
                    .add_enabled(!stale, egui::Button::new("Apply Layer 3 fields"))
                    .clicked()
                    && let Err(error) = self.apply_layer3_selected()
                {
                    self.error = Some(error);
                }
                ui.small(
                    "Semantic edits preserve opaque feature bits, reserved bytes, and high graphics-word nibbles.",
                );
            });
    }

    fn load_selected(&mut self) -> Result<(), String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "overworld-settings workspace is closed".to_owned())?;
        let record = workspace
            .current
            .records
            .get(self.submap)
            .ok_or_else(|| "invalid overworld-settings submap".to_owned())?;
        self.form = ExpandedSettingsForm::load(record);
        self.layer3_form = Layer3Form::load(&OverworldLayer3SettingsRecord::from_bytes(
            *record.encoded(),
        ));
        self.loaded_submap = Some(self.submap);
        Ok(())
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        if self.loaded_submap != Some(self.submap) {
            return Err("load the selected submap record before applying it".into());
        }
        let edits = self.form.edits()?;
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "overworld-settings workspace is closed".to_owned())?;
        let record = workspace
            .current
            .records
            .get_mut(self.submap)
            .ok_or_else(|| "invalid overworld-settings submap".to_owned())?;
        for (index, value) in edits {
            record
                .set_word(index, value)
                .map_err(|error| error.to_string())?;
        }
        self.layer3_form = Layer3Form::load(&OverworldLayer3SettingsRecord::from_bytes(
            *record.encoded(),
        ));
        Ok(())
    }

    fn apply_layer3_selected(&mut self) -> Result<(), String> {
        if self.loaded_submap != Some(self.submap) {
            return Err("load the selected submap record before applying it".into());
        }
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "overworld-settings workspace is closed".to_owned())?;
        let record = workspace
            .current
            .records
            .get_mut(self.submap)
            .ok_or_else(|| "invalid overworld-settings submap".to_owned())?;
        let source = OverworldLayer3SettingsRecord::from_bytes(*record.encoded());
        let edited = self.layer3_form.apply(&source)?;
        *record = lm_level::ExpandedLevelSettingsRecord::from_encoded(*edited.encoded());
        self.form = ExpandedSettingsForm::load(record);
        Ok(())
    }

    fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "overworld-settings workspace is closed".to_owned())?;
        if workspace.revision != project_revision {
            return Err("stale overworld-settings workspace cannot be committed".into());
        }
        if workspace.current == workspace.original {
            return Ok(None);
        }
        let mut bytes = [0; OverworldLayer3SettingsTable::ENCODED_LEN];
        for (index, record) in workspace.current.records.iter().enumerate() {
            let start = index * record.encoded().len();
            bytes[start..start + record.encoded().len()].copy_from_slice(record.encoded());
        }
        let settings =
            OverworldLayer3SettingsTable::decode(&bytes).map_err(|error| error.to_string())?;
        Ok(Some(Command::ReplaceNativeOverworldLayer3Settings {
            rev: workspace.revision,
            settings: Box::new(settings),
        }))
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard overworld-settings changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged settings have not been committed to the ROM.");
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
            egui::Window::new("Overworld-settings editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.loaded_submap = None;
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
    fn pristine_edit_installs_and_reopens_all_seven_records() {
        let mut app = pristine_app();
        let mut editor = RomOverworldSettingsEditor::default();
        editor.open(&app);
        assert!(!editor.workspace.as_ref().unwrap().installed);
        editor.submap = 6;
        editor.loaded_submap = None;
        editor.load_selected().unwrap();
        editor.form.words[11] = "4567".into();
        editor.apply_selected().unwrap();
        let expected = editor.workspace.as_ref().unwrap().current.clone();
        let command = editor.prepare_commit(0).unwrap().unwrap();
        app.dispatch(command).unwrap();
        let reopened = load_smw_us_v1_overworld_settings(app.project().unwrap()).unwrap();
        assert!(reopened.installed);
        assert_eq!(reopened.settings, expected);
    }

    #[test]
    fn malformed_unloaded_stale_and_dirty_states_are_retained() {
        let app = pristine_app();
        let mut editor = RomOverworldSettingsEditor::default();
        editor.open(&app);
        editor.form.words[15] = "10000".into();
        assert!(editor.apply_selected().is_err());
        editor.form.words[15] = "0028".into();
        editor.submap = 1;
        editor.loaded_submap = None;
        assert!(editor.apply_selected().is_err());
        editor.load_selected().unwrap();
        editor.form.words[0] = "1234".into();
        editor.apply_selected().unwrap();
        assert!(editor.prepare_commit(1).is_err());
        assert!(!editor.request_close(false));
        assert!(editor.is_open());
    }

    #[test]
    fn semantic_layer3_gui_installs_reopens_preserves_opaque_bytes_and_undoes() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut editor = RomOverworldSettingsEditor::default();
        editor.open(&app);
        editor.submap = 4;
        editor.loaded_submap = None;
        editor.load_selected().unwrap();
        let before = OverworldLayer3SettingsRecord::from_bytes(
            *editor.workspace.as_ref().unwrap().current.records[4].encoded(),
        );
        editor.layer3_form.uses_custom_tilemap = true;
        editor.layer3_form.uses_custom_graphics = true;
        editor.layer3_form.tilemap_file = 0x345;
        editor.layer3_form.tilemap_size = 3;
        editor.layer3_form.tilemap_position = 2;
        editor.layer3_form.graphics_files[2] = 0x678;
        editor.apply_layer3_selected().unwrap();
        let after = OverworldLayer3SettingsRecord::from_bytes(
            *editor.workspace.as_ref().unwrap().current.records[4].encoded(),
        );
        assert_eq!(after.preserved_bytes(), before.preserved_bytes());
        assert_eq!(
            after.feature_flags() & !0x6000,
            before.feature_flags() & !0x6000
        );
        let command = editor.prepare_commit(0).unwrap().unwrap();
        assert!(matches!(
            &command,
            Command::ReplaceNativeOverworldLayer3Settings { .. }
        ));
        app.dispatch(command).unwrap();
        let reopened =
            app.project()
                .unwrap()
                .load_overworld_layer3_settings(
                    lm_profile::smw_us_v1_overworld_layer3_settings_layout(),
                )
                .unwrap();
        assert_eq!(reopened.maps[4], after);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn staged_pristine_overworld_settings_recover_all_seven_records() {
        let app = pristine_app();
        let mut editor = RomOverworldSettingsEditor::default();
        editor.open(&app);
        editor.form.words[0] = "1234".into();
        editor.apply_selected().unwrap();
        editor.submap = 6;
        editor.loaded_submap = None;
        editor.load_selected().unwrap();
        editor.form.words[15] = "ABCD".into();
        editor.apply_selected().unwrap();
        let expected = editor.workspace.as_ref().unwrap().current.clone();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let loaded = load_smw_us_v1_overworld_settings(reopened.project().unwrap()).unwrap();
        assert!(loaded.installed);
        assert_eq!(loaded.settings, expected);
        assert_eq!(loaded.settings.records[0].word(0).unwrap(), 0x1234);
        assert_eq!(loaded.settings.records[6].word(15).unwrap(), 0xabcd);
    }

    #[test]
    fn staged_installed_overworld_settings_preserve_prior_submap_edits() {
        let mut installer = pristine_app();
        let mut first = RomOverworldSettingsEditor::default();
        first.open(&installer);
        first.submap = 2;
        first.loaded_submap = None;
        first.load_selected().unwrap();
        first.form.words[9] = "3456".into();
        first.apply_selected().unwrap();
        installer
            .dispatch(first.prepare_commit(0).unwrap().unwrap())
            .unwrap();

        let mut app = AppState::default();
        app.load_rom(installer.project().unwrap().save_snapshot())
            .unwrap();
        let mut editor = RomOverworldSettingsEditor::default();
        editor.open(&app);
        assert!(editor.workspace.as_ref().unwrap().installed);
        editor.submap = 6;
        editor.loaded_submap = None;
        editor.load_selected().unwrap();
        editor.form.words[11] = "5678".into();
        editor.apply_selected().unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let settings = load_smw_us_v1_overworld_settings(reopened.project().unwrap())
            .unwrap()
            .settings;
        assert_eq!(settings.records[2].word(9).unwrap(), 0x3456);
        assert_eq!(settings.records[6].word(11).unwrap(), 0x5678);
    }
}
