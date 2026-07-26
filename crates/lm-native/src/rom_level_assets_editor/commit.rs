use super::{Command, RomLevelAssetsEditor, Workspace};
use crate::rom_allocation::parse_search_range;
use lm_project::{LevelLayer2SaveOptions, NativeLevelAssetsSaveOptions};

impl RomLevelAssetsEditor {
    pub(super) fn prepare_commit(&self) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let (options, layer2_options) = self.save_options(workspace)?;
        let description = format!("Edit native assets for level {:03X}", workspace.source_slot);
        let prepared = if let Some(layer2_options) = layer2_options.as_ref() {
            workspace
                .controller
                .prepare_commit_with_layer2(description, &options, layer2_options)
        } else {
            workspace.controller.prepare_commit(description, &options)
        };
        prepared
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    pub(super) fn prepare_commit_with_reclamation(
        &self,
        manifest: &lm_project::RatsOwnershipManifest,
    ) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let (options, layer2_options) = self.save_options(workspace)?;
        let description = format!(
            "Edit and reclaim native assets for level {:03X}",
            workspace.source_slot
        );
        let prepared = if let Some(layer2_options) = layer2_options.as_ref() {
            workspace
                .controller
                .prepare_commit_with_layer2_and_reclamation(
                    description,
                    &options,
                    layer2_options,
                    manifest,
                )
        } else {
            workspace
                .controller
                .prepare_commit_with_reclamation(description, &options, manifest)
        };
        prepared
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    fn save_options(
        &self,
        workspace: &Workspace,
    ) -> Result<(NativeLevelAssetsSaveOptions, Option<LevelLayer2SaveOptions>), String> {
        let search = parse_search_range(&self.search_start, &self.search_end)?;
        let (_, core) = workspace
            .profile
            .native_level_assets_save_plan_for_rom(
                search.clone(),
                &workspace.image,
                workspace.internal_header,
            )
            .map_err(|error| error.to_string())?;
        let layer2 = workspace
            .profile
            .level_layer2_save_plan(
                search,
                workspace.image.logical_len(),
                workspace.internal_header,
            )
            .map_err(|error| error.to_string())?
            .map(|(_, options)| options);
        Ok((core, layer2))
    }
}
