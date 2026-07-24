use super::{Command, RomOverworldEditor, Workspace};
use crate::rom_allocation::parse_search_range;
use lm_project::CompleteOverworldSaveOptions;

impl RomOverworldEditor {
    pub(super) fn prepare_commit(&self) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let options = self.save_options(workspace)?;
        workspace
            .controller
            .prepare_commit(
                format!("Edit complete native overworld slot {:03X}", workspace.slot),
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
                format!(
                    "Edit and reclaim complete native overworld slot {:03X}",
                    workspace.slot
                ),
                &options,
                manifest,
            )
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    fn save_options(&self, workspace: &Workspace) -> Result<CompleteOverworldSaveOptions, String> {
        let range = parse_search_range(&self.search_start, &self.search_end)?;
        workspace
            .profiled
            .profile
            .allocation_policy_for_rom(
                range,
                &workspace.image,
                workspace.profiled.snapshot.identity.internal_header_offset,
            )
            .map(CompleteOverworldSaveOptions::uniform_allocation)
            .map_err(|error| error.to_string())
    }
}
