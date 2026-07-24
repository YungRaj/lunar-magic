use super::{Command, RomMap16Editor, Workspace};
use crate::rom_allocation::parse_search_range;
use lm_project::Map16SetSaveOptions;

impl RomMap16Editor {
    pub(super) fn prepare_commit(&self) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let options = self.save_options(workspace)?;
        workspace
            .controller
            .prepare_commit("Edit complete native Map16 set", &options)
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
                "Edit and reclaim complete native Map16 set",
                &options,
                manifest,
            )
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    fn save_options(&self, workspace: &Workspace) -> Result<Map16SetSaveOptions, String> {
        let search = parse_search_range(&self.search_start, &self.search_end)?;
        let allocation = workspace
            .profile
            .allocation_policy_for_rom(search, &workspace.image, workspace.internal_header)
            .map_err(|error| error.to_string())?;
        let pages = workspace.controller.set().pages.len();
        Ok(Map16SetSaveOptions {
            graphics_allocation: allocation.clone(),
            acts_like_allocation: allocation,
            previous_graphics: vec![None; pages],
            previous_acts_like: vec![None; pages],
            reuse_identical: true,
            erase_fill: 0xff,
        })
    }
}
