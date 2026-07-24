use super::{Command, RomGraphicsEditor, Workspace};
use crate::rom_allocation::parse_search_range;
use lm_project::GraphicsSaveOptions;

impl RomGraphicsEditor {
    pub(super) fn prepare_commit(&self) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let options = self.save_options(workspace)?;
        workspace
            .controller
            .prepare_commit(
                format!("Edit native graphics {:03X}", workspace.slot),
                &options,
            )
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    pub(super) fn prepare_commit_owned(
        &self,
        manifest: &lm_project::RatsOwnershipManifest,
    ) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let options = self.save_options(workspace)?;
        workspace
            .controller
            .prepare_commit_with_reclamation(
                format!("Edit and reclaim native graphics {:03X}", workspace.slot),
                &options,
                manifest,
            )
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    fn save_options(&self, workspace: &Workspace) -> Result<GraphicsSaveOptions, String> {
        let search = parse_search_range(&self.search_start, &self.search_end)?;
        let allocation = workspace
            .profile
            .allocation_policy_for_rom(search, &workspace.image, workspace.internal_header)
            .map_err(|error| error.to_string())?;
        Ok(GraphicsSaveOptions {
            allocation,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        })
    }
}
