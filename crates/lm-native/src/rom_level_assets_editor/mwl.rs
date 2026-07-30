use super::{Command, RomLevelAssetsEditor};
use crate::document_loader::{BoundedRead, LoadedDocument};
use lm_level::MwlFile;
use lm_project::MwlNativeLevel;

impl RomLevelAssetsEditor {
    pub(super) fn show_mwl_actions(
        &mut self,
        ui: &mut eframe::egui::Ui,
        stale: bool,
        modified: bool,
    ) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !stale && !self.mwl_loader.is_running(),
                    eframe::egui::Button::new("Export complete MWL…"),
                )
                .clicked()
                && let Err(error) = self.export_mwl()
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(
                    !modified
                        && !stale
                        && !self.mwl_loader.is_running()
                        && !self.manifest_loader.is_running(),
                    eframe::egui::Button::new("Import complete MWL…"),
                )
                .clicked()
                && let Err(error) = self.choose_mwl_import()
            {
                self.error = Some(error);
            }
        });
    }

    pub(super) fn export_mwl(&mut self) -> Result<(), String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let Some(destination) = crate::dialogs::choose_mwl_save_path(workspace.source_slot) else {
            return Ok(());
        };
        let semantic = workspace
            .controller
            .export_smw_us_v1_installed_mwl()
            .map_err(|error| error.to_string())?;
        let bytes = semantic
            .encode(
                &workspace.profile.sprite_lengths,
                &workspace.profile.exanimation_double_size_modes,
            )
            .and_then(|file| file.encode().map_err(Into::into))
            .map_err(|error: lm_project::MwlNativeLevelError| error.to_string())?;
        lm_app::file_persistence::write_new(&destination, &bytes).map_err(|error| error.to_string())
    }

    pub(super) fn choose_mwl_import(&mut self) -> Result<(), String> {
        let Some(path) = crate::dialogs::choose_mwl_document() else {
            return Ok(());
        };
        self.mwl_loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(MwlFile::MAX_FILE_BYTES).unwrap_or(u64::MAX),
            "complete MWL level",
        )])
    }

    pub(super) fn finish_mwl_import(
        &self,
        result: Result<LoadedDocument, String>,
        project_revision: u64,
    ) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        if workspace.controller.revision() != project_revision {
            return Err("the ROM changed while the MWL was loading".into());
        }
        let [(_, bytes)] = result?.into_exact::<1>("complete MWL import")?;
        let file = MwlFile::decode(&bytes).map_err(|error| error.to_string())?;
        let mut source = MwlNativeLevel::decode(
            &file,
            &workspace.profile.sprite_lengths,
            workspace.profile.exanimation.maximum_records,
            &workspace.profile.exanimation_double_size_modes,
        )
        .map_err(|error| error.to_string())?;
        source
            .retarget(workspace.source_slot)
            .map_err(|error| error.to_string())?;
        let (options, layer2) = self.save_options(workspace)?;
        let layer2 = layer2
            .ok_or_else(|| "active revision profile has no native Layer 2 layout".to_string())?;
        workspace
            .controller
            .prepare_smw_us_v1_installed_mwl_import(&source, &options, &layer2)
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }
}
