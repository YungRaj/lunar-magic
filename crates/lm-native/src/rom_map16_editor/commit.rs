use super::{Command, Controller, RomMap16Editor, Workspace};
use crate::rom_allocation::parse_search_range;
use lm_profile::SmwUsV1CompleteMap16SaveOptions;
use lm_project::Map16SetSaveOptions;
use lm_rats::{AllocationPolicy, ProtectedRange};

impl RomMap16Editor {
    pub(super) fn prepare_commit(&self) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let prepared = match &workspace.controller {
            Controller::Profile(controller) => controller
                .prepare_commit(
                    "Edit complete native Map16 set",
                    &self.profile_save_options(workspace)?,
                )
                .map_err(|error| error.to_string())?,
            Controller::Smw(controller) => controller
                .prepare_commit(
                    "Edit native SMW Map16 set",
                    &self.smw_save_options(workspace)?,
                )
                .map_err(|error| error.to_string())?,
        };
        Ok(prepared.into_command())
    }

    pub(super) fn prepare_commit_owned(
        &self,
        manifest: &lm_project::RatsOwnershipManifest,
    ) -> Result<Command, String> {
        let workspace = self.workspace.as_ref().ok_or("workspace is closed")?;
        let Controller::Profile(controller) = &workspace.controller else {
            return Err("reclamation is unavailable for the native SMW Map16 workspace".into());
        };
        let options = self.profile_save_options(workspace)?;
        controller
            .prepare_commit_with_reclamation(
                "Edit and reclaim complete native Map16 set",
                &options,
                manifest,
            )
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    pub(super) fn profile_save_options(
        &self,
        workspace: &Workspace,
    ) -> Result<Map16SetSaveOptions, String> {
        let search = parse_search_range(&self.search_start, &self.search_end)?;
        let profile = workspace
            .profile
            .as_ref()
            .ok_or("revision profile is unavailable")?;
        let allocation = profile
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

    pub(super) fn smw_save_options(
        &self,
        workspace: &Workspace,
    ) -> Result<SmwUsV1CompleteMap16SaveOptions, String> {
        let search = parse_search_range(&self.search_start, &self.search_end)?;
        let image_len = workspace.image.logical_len();
        if search.start >= search.end {
            return Err(format!(
                "allocation range must be nonempty (current ROM length is {image_len:X})"
            ));
        }
        let protected = vec![ProtectedRange(
            workspace.internal_header..workspace.internal_header + 0x40,
        )];
        Ok(SmwUsV1CompleteMap16SaveOptions {
            allocation: AllocationPolicy {
                search,
                bank_size: Some(0x8000),
                fill_bytes: vec![0x00, 0xff],
                protected,
            },
            reuse_identical: true,
            erase_fill: 0xff,
        })
    }
}
