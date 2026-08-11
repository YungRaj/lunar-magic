use crate::level_editor_forms::{SecondaryExitForm, parse_hex_u16};
use eframe::egui;
use lm_app::{AppState, Command, LocalizationCatalog};
use lm_level::{SecondaryExit, SecondaryExitTable};
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
    pending_clear_all: bool,
}

impl RomSecondaryExitEditor {
    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        (workspace.table != workspace.original).then(|| {
            let content_revision = workspace
                .table
                .entries
                .iter()
                .flat_map(|entry| {
                    let destination = entry.destination_level.to_le_bytes();
                    [
                        destination[0],
                        destination[1],
                        entry.position_and_method,
                        entry.screen,
                        entry.x,
                        entry.y,
                        entry.destination_flags,
                        entry.x_and_overworld_flags,
                        entry.additional_flags,
                    ]
                })
                .fold(0x5345_434f_4e44_4152_u64, |revision, byte| {
                    revision.rotate_left(5) ^ u64::from(byte)
                });
            app.project_revision().wrapping_mul(0xa24b_aed4_963e_e407)
                ^ workspace.revision.rotate_left(31)
                ^ content_revision
        })
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "secondary-exit workspace is closed".to_owned())?;
        if workspace.revision != app.project_revision() {
            return Err("stale secondary-exit workspace cannot be recovered".into());
        }
        if workspace.table == workspace.original {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        lm_app::save_native_secondary_exits_to_project(&mut staged, &workspace.table)
            .map_err(|error| error.to_string())?;
        if staged
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .map_err(|error| error.to_string())?
            .table
            != workspace.table
        {
            return Err("recovered secondary-exit table did not reopen exactly".into());
        }
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
        catalog: Option<&LocalizationCatalog>,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.workspace.is_some() {
            self.load_selected();
            egui::Window::new(dialog_title(catalog))
                .default_size([520.0, 570.0])
                .show(context, |ui| {
                    command = self.contents(ui, project_revision, catalog);
                });
        }
        let approved = self.close_confirmation(context);
        self.clear_all_confirmation(context, project_revision);
        self.show_error(context);
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
        secondary_fields(ui, &mut self.form, catalog);
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
                .add_enabled(
                    !stale,
                    egui::Button::new(dialog_control_text(catalog, 0x66, "Clear entry")),
                )
                .clicked()
                && let Err(error) = self.clear_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    !stale,
                    egui::Button::new(dialog_control_text(catalog, 0x65, "Clear all…")),
                )
                .clicked()
            {
                self.pending_clear_all = true;
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

    fn clear_selected(&mut self) -> Result<(), String> {
        let index = self.selected_index()?;
        if self.loaded_index != Some(index) {
            return Err("load the selected entry before clearing it".into());
        }
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "secondary-exit workspace is closed".to_owned())?;
        workspace.table.entries[index] = SecondaryExit::default();
        self.form = SecondaryExitForm::load(SecondaryExit::default());
        Ok(())
    }

    fn clear_all(&mut self, project_revision: u64) -> Result<(), String> {
        let workspace = self
            .workspace
            .as_mut()
            .ok_or_else(|| "secondary-exit workspace is closed".to_owned())?;
        if workspace.revision != project_revision {
            return Err("stale secondary-exit workspace cannot be cleared".into());
        }
        workspace.table.entries.fill(SecondaryExit::default());
        if let Some(index) = self.loaded_index {
            self.form = SecondaryExitForm::load(workspace.table.entries[index]);
        }
        self.pending_clear_all = false;
        Ok(())
    }

    fn clear_all_confirmation(&mut self, context: &egui::Context, project_revision: u64) {
        if !self.pending_clear_all {
            return;
        }
        egui::Window::new("Clear all secondary exits?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("This stages 8,192 cleared entries. The ROM is unchanged until commit.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_clear_all = false;
                    }
                    if ui.button("Clear all").clicked()
                        && let Err(error) = self.clear_all(project_revision)
                    {
                        self.error = Some(error);
                    }
                });
            });
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
        self.pending_clear_all = false;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

const ORIGINAL_DIALOG_ID: u16 = 0x03f1;

fn dialog_title(catalog: Option<&LocalizationCatalog>) -> &str {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_DIALOG_ID))
        .unwrap_or("ROM Secondary Exit Table")
}

fn dialog_control_text<'a>(
    catalog: Option<&'a LocalizationCatalog>,
    control_id: u32,
    fallback: &'a str,
) -> &'a str {
    catalog
        .and_then(|catalog| catalog.original_dialog_control_text(ORIGINAL_DIALOG_ID, control_id))
        .unwrap_or(fallback)
}

fn secondary_fields(
    ui: &mut egui::Ui,
    form: &mut SecondaryExitForm,
    catalog: Option<&LocalizationCatalog>,
) {
    egui::Grid::new("rom-secondary-exit-fields")
        .striped(true)
        .show(ui, |ui| {
            for (label, value) in [
                (
                    dialog_control_text(catalog, 0x6c, "Destination"),
                    &mut form.destination,
                ),
                ("Position/method", &mut form.position),
                (
                    dialog_control_text(catalog, 0xdb, "Screen"),
                    &mut form.screen,
                ),
                (dialog_control_text(catalog, 0x67, "X"), &mut form.x),
                (dialog_control_text(catalog, 0x69, "Y"), &mut form.y),
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
    use lm_app::{OriginalDialogTextKey, UiTextKey};
    use lm_rom::RomImage;
    use std::path::PathBuf;

    #[test]
    fn pristine_table_stages_a_global_entry_and_emits_one_profile_command() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
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
    fn staged_pristine_secondary_exits_recover_the_complete_installed_table() {
        let (app, mut editor) = opened_app_and_editor();
        editor.index = "1FFF".into();
        editor.loaded_index = None;
        editor.load_selected();
        editor.form.destination = "0105".into();
        editor.form.position = "02".into();
        editor.form.screen = "1F".into();
        editor.form.x = "0A".into();
        editor.form.y = "07".into();
        editor.form.destination_flags = "03".into();
        editor.form.x_flags = "80".into();
        editor.form.additional = "01".into();
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
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap();
        assert!(matches!(
            loaded.storage,
            lm_project::SecondaryExitStorage::Installed { .. }
        ));
        assert_eq!(loaded.table.entries[0x1fff].destination_level, 0x105);
        assert_eq!(loaded.table.entries[0x1fff].screen, 0x1f);
        assert_eq!(loaded.table.entries[0x1fff].additional_flags, 1);
    }

    #[test]
    fn staged_installed_secondary_exit_update_preserves_prior_entries() {
        let mut installer = AppState::default();
        installer
            .load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut table = installer
            .project()
            .unwrap()
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap()
            .table;
        table.entries[0x123].destination_level = 0x105;
        installer
            .dispatch(Command::ReplaceNativeSecondaryExits {
                rev: 0,
                table: Box::new(table),
            })
            .unwrap();
        let mut app = AppState::default();
        app.load_rom(installer.project().unwrap().save_snapshot())
            .unwrap();
        let mut editor = RomSecondaryExitEditor::default();
        editor.open(&app);
        editor.index = "1FFE".into();
        editor.loaded_index = None;
        editor.load_selected();
        editor.form.destination = "0106".into();
        editor.form.additional = "01".into();
        editor.apply_selected().unwrap();

        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let table = reopened
            .project()
            .unwrap()
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap()
            .table;
        assert_eq!(table.entries[0x123].destination_level, 0x105);
        assert_eq!(table.entries[0x1ffe].destination_level, 0x106);
        assert_eq!(table.entries[0x1ffe].additional_flags, 1);
    }

    #[test]
    fn invalid_staged_secondary_exit_is_reported_without_panicking_recovery_generation() {
        let (app, mut editor) = opened_app_and_editor();
        editor.form.y = "08".into();
        editor.apply_selected().unwrap();

        assert!(editor.staged_recovery_generation(&app).is_some());
        assert!(editor.staged_recovery_snapshot(&app).is_err());
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
    }

    #[test]
    fn selection_must_be_loaded_and_close_guards_staged_changes() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
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

    #[test]
    fn clear_entry_requires_loaded_selection_and_changes_only_that_entry() {
        let mut editor = opened_editor();
        editor.index = "0400".into();
        assert!(editor.clear_selected().is_err());
        editor.loaded_index = None;
        editor.load_selected();
        editor.form.destination = "0105".into();
        editor.apply_selected().unwrap();
        editor.workspace.as_mut().unwrap().table.entries[0x401].destination_level = 0x106;

        editor.clear_selected().unwrap();
        let table = &editor.workspace.as_ref().unwrap().table;
        assert_eq!(table.entries[0x400], SecondaryExit::default());
        assert_eq!(table.entries[0x401].destination_level, 0x106);
        assert_eq!(editor.form.parse().unwrap(), SecondaryExit::default());
    }

    #[test]
    fn clear_all_stages_one_complete_zero_table_and_reloads_selection() {
        let (mut app, mut editor) = opened_app_and_editor();
        editor.index = "0123".into();
        editor.loaded_index = None;
        editor.load_selected();
        editor.form.destination = "0105".into();
        editor.apply_selected().unwrap();
        editor.pending_clear_all = true;

        editor.clear_all(0).unwrap();
        let workspace = editor.workspace.as_ref().unwrap();
        assert_eq!(
            workspace.table.entries.len(),
            SecondaryExitTable::ENTRY_COUNT
        );
        assert!(
            workspace
                .table
                .entries
                .iter()
                .all(|entry| *entry == SecondaryExit::default())
        );
        assert!(!editor.pending_clear_all);
        assert_eq!(editor.form.parse().unwrap(), SecondaryExit::default());
        let command = editor.prepare_commit(0).unwrap().unwrap();
        app.dispatch(command).unwrap();
        let mut reopened = RomSecondaryExitEditor::default();
        reopened.open(&app);
        assert!(
            reopened
                .workspace
                .as_ref()
                .unwrap()
                .table
                .entries
                .iter()
                .all(|entry| *entry == SecondaryExit::default())
        );

        editor.workspace.as_mut().unwrap().table.entries[7].destination_level = 0x107;
        let before_stale = editor.workspace.as_ref().unwrap().table.clone();
        editor.pending_clear_all = true;
        assert!(editor.clear_all(1).is_err());
        assert!(editor.pending_clear_all);
        assert_eq!(editor.workspace.as_ref().unwrap().table, before_stale);
    }

    #[test]
    fn original_dialog_inventory_localizes_exact_secondary_exit_controls_with_fallbacks() {
        let catalog = LocalizationCatalog::new(
            "fr-FR",
            UiTextKey::ALL.map(|key| (key, key.english().into())),
        )
        .unwrap()
        .with_original_dialog_texts([
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Modifier les entrées secondaires".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 4,
                    control_id: 0x66,
                },
                "Effacer l’emplacement".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 5,
                    control_id: 0x65,
                },
                "Tout effacer".into(),
            ),
        ])
        .unwrap();
        assert_eq!(
            dialog_title(Some(&catalog)),
            "Modifier les entrées secondaires"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), 0x66, "Clear entry"),
            "Effacer l’emplacement"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), 0x65, "Clear all…"),
            "Tout effacer"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), 0x6c, "Destination"),
            "Destination"
        );
    }

    fn opened_editor() -> RomSecondaryExitEditor {
        opened_app_and_editor().1
    }

    fn opened_app_and_editor() -> (AppState, RomSecondaryExitEditor) {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        let mut editor = RomSecondaryExitEditor::default();
        editor.open(&app);
        (app, editor)
    }
}
