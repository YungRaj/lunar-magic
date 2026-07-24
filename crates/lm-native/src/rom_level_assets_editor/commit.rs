use super::{Command, RomLevelAssetsEditor, Workspace};
use crate::rom_allocation::parse_search_range;
use lm_project::NativeLevelAssetsSaveOptions;

impl RomLevelAssetsEditor {
    pub(super) fn prepare_commit(&self) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let options = self.save_options(workspace)?;
        workspace
            .controller
            .prepare_commit(
                format!("Edit native assets for level {:03X}", workspace.source_slot),
                &options,
            )
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    pub(super) fn prepare_commit_with_reclamation(
        &self,
        manifest: &lm_project::RatsOwnershipManifest,
    ) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let options = self.save_options(workspace)?;
        workspace
            .controller
            .prepare_commit_with_reclamation(
                format!(
                    "Edit and reclaim native assets for level {:03X}",
                    workspace.source_slot
                ),
                &options,
                manifest,
            )
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    fn save_options(&self, workspace: &Workspace) -> Result<NativeLevelAssetsSaveOptions, String> {
        let search = parse_search_range(&self.search_start, &self.search_end)?;
        workspace
            .profile
            .native_level_assets_save_plan_for_rom(
                search,
                &workspace.image,
                workspace.internal_header,
            )
            .map(|(_, options)| options)
            .map_err(|error| error.to_string())
    }
}
