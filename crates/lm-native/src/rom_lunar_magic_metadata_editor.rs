mod form;
mod workspace;

use eframe::egui;
use form::MetadataByteForm;
use lm_app::{AppState, Command};
use workspace::{LunarMagicMetadataWorkspace, MetadataRegion};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Default)]
pub(crate) struct RomLunarMagicMetadataEditor {
    workspace: Option<LunarMagicMetadataWorkspace>,
    form: MetadataByteForm,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomLunarMagicMetadataEditor {
    pub(crate) fn stage_recovery_on_project(
        &self,
        app: &AppState,
        staged: &mut lm_project::Project,
    ) -> Result<(), String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("Lunar Magic metadata workspace is closed")?;
        if workspace.is_stale(app.project_revision()) {
            return Err("stale Lunar Magic metadata workspace cannot be recovered".into());
        }
        if !workspace.is_dirty() {
            return Err("Lunar Magic metadata workspace has no staged recovery edit".into());
        }
        lm_app::save_lunar_magic_rom_metadata_to_project(staged, workspace.metadata())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        if !workspace.is_dirty() {
            return None;
        }
        let metadata = workspace.metadata();
        let content_revision = metadata
            .attribution()
            .iter()
            .copied()
            .chain(std::iter::once(metadata.vram_version()))
            .chain(metadata.feature_record().iter().copied())
            .fold(0x4c4d_4d45_5441_4441_u64, |revision, byte| {
                revision.rotate_left(5) ^ u64::from(byte)
            });
        Some(
            app.project_revision().wrapping_mul(0xa24b_aed4_963e_e407)
                ^ content_revision.rotate_left(31),
        )
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or_else(|| "Lunar Magic metadata workspace is closed".to_owned())?;
        if workspace.is_stale(app.project_revision()) {
            return Err("stale Lunar Magic metadata workspace cannot be recovered".into());
        }
        if !workspace.is_dirty() {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        self.stage_recovery_on_project(app, &mut staged)?;
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
        match LunarMagicMetadataWorkspace::load(app) {
            Ok(workspace) => {
                self.form = MetadataByteForm::default();
                self.form.load(&workspace).ok();
                self.workspace = Some(workspace);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if !workspace.is_dirty() {
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
            egui::Window::new("Lunar Magic ROM Metadata")
                .default_size([570.0, 370.0])
                .show(context, |ui| command = self.contents(ui, project_revision));
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.is_stale(project_revision);
        let dirty = workspace.is_dirty();
        let metadata = workspace.metadata();
        ui.label("Lossless fixed LM metadata. Unknown bytes remain deliberately opaque.");
        ui.monospace(format!(
            "features={:08X}  compression={:02X}  mapping={:02X}  checksum-status={:02X}",
            metadata.feature_bits(),
            metadata.compression_configuration(),
            metadata.mapping_configuration(),
            metadata.checksum_status()
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this metadata was opened. Reopen before committing.",
            );
        }
        self.byte_form(ui);
        let mut command = None;
        ui.horizontal(|ui| {
            if ui.button("Load byte").clicked()
                && let Err(error) = self.load_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(!stale, egui::Button::new("Apply byte"))
                .clicked()
                && let Err(error) = self.apply_selected()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(dirty && !stale, egui::Button::new("Commit metadata to ROM"))
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

    fn byte_form(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("rom-lunar-magic-metadata-form")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Region");
                let before = self.form.region;
                egui::ComboBox::from_id_salt("lm-metadata-region")
                    .selected_text(region_name(self.form.region))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.form.region,
                            MetadataRegion::Attribution,
                            "Attribution (00–9F)",
                        );
                        ui.selectable_value(
                            &mut self.form.region,
                            MetadataRegion::VramVersion,
                            "VRAM version (00)",
                        );
                        ui.selectable_value(
                            &mut self.form.region,
                            MetadataRegion::FeatureRecord,
                            "Feature record (00–18)",
                        );
                    });
                if before != self.form.region {
                    self.form.selection_changed();
                }
                ui.end_row();
                ui.label("Byte index");
                if ui.text_edit_singleline(&mut self.form.index).changed() {
                    self.form.selection_changed();
                }
                ui.end_row();
                ui.label("Byte value");
                ui.text_edit_singleline(&mut self.form.value);
                ui.end_row();
            });
    }

    fn load_selected(&mut self) -> Result<(), String> {
        self.form.load(
            self.workspace
                .as_ref()
                .ok_or_else(|| "Lunar Magic metadata workspace is closed".to_owned())?,
        )
    }

    fn apply_selected(&mut self) -> Result<(), String> {
        self.form.apply(
            self.workspace
                .as_mut()
                .ok_or_else(|| "Lunar Magic metadata workspace is closed".to_owned())?,
        )
    }

    fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        self.workspace
            .as_ref()
            .ok_or_else(|| "Lunar Magic metadata workspace is closed".to_owned())?
            .prepare_commit(project_revision)
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard Lunar Magic metadata changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The staged fixed metadata has not been committed.");
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
            egui::Window::new("Lunar Magic metadata editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.form.clear_selection();
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

const fn region_name(region: MetadataRegion) -> &'static str {
    match region {
        MetadataRegion::Attribution => "Attribution",
        MetadataRegion::VramVersion => "VRAM version",
        MetadataRegion::FeatureRecord => "Feature record",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_profile::smw_us_v1_lunar_magic_metadata_layout;
    use std::{fs, path::PathBuf};

    #[test]
    fn real_lm363_metadata_edit_commits_reopens_and_closes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(fixture).unwrap();
        let mut editor = RomLunarMagicMetadataEditor::default();
        editor.open(&app);
        editor.form.index = "9F".into();
        editor.load_selected().unwrap();
        editor.form.value = "A5".into();
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
            .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())
            .unwrap()
            .unwrap();
        assert_eq!(reopened.attribution()[0x9f], 0xa5);
    }

    #[test]
    fn staged_real_metadata_recovers_all_owned_regions_without_mutating_live_project() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(fixture).unwrap();
        let mut editor = RomLunarMagicMetadataEditor::default();
        editor.open(&app);
        for (region, index, value) in [
            (MetadataRegion::Attribution, "9F", "A5"),
            (MetadataRegion::VramVersion, "00", "34"),
            (MetadataRegion::FeatureRecord, "17", "5A"),
        ] {
            editor.form.region = region;
            editor.form.index = index.into();
            editor.form.selection_changed();
            editor.load_selected().unwrap();
            editor.form.value = value.into();
            editor.apply_selected().unwrap();
        }

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let metadata = reopened
            .project()
            .unwrap()
            .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())
            .unwrap()
            .unwrap();
        assert_eq!(metadata.attribution()[0x9f], 0xa5);
        assert_eq!(metadata.vram_version(), 0x34);
        assert_eq!(metadata.feature_record()[0x17], 0x5a);
    }
}
