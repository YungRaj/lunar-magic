use crate::rom_allocation::parse_search_range;
use lm_app::{
    AppState, Command, ExAnimationController, RevisionProfile, RevisionProfileControllers,
};
use lm_project::ExAnimationSaveOptions;
use lm_rom::RomImage;

pub(super) struct Workspace {
    pub(super) controller: ExAnimationController,
    pub(super) profile: RevisionProfile,
    pub(super) modes: [bool; 256],
    pub(super) slot: u16,
    pub(super) image: RomImage,
    pub(super) internal_header: usize,
}

impl Workspace {
    pub(super) fn prepare_commit(
        &self,
        search_start: &str,
        search_end: &str,
    ) -> Result<Command, String> {
        let options = self.save_options(search_start, search_end)?;
        self.controller
            .prepare_commit(
                format!("Edit native ExAnimation {:03X}", self.slot),
                &options,
            )
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    pub(super) fn prepare_commit_with_reclamation(
        &self,
        search_start: &str,
        search_end: &str,
        manifest: &lm_project::RatsOwnershipManifest,
    ) -> Result<Command, String> {
        let options = self.save_options(search_start, search_end)?;
        self.controller
            .prepare_commit_with_reclamation(
                format!("Edit and reclaim native ExAnimation {:03X}", self.slot),
                &options,
                manifest,
            )
            .map(lm_app::PreparedRomCommit::into_command)
            .map_err(|error| error.to_string())
    }

    fn save_options(
        &self,
        search_start: &str,
        search_end: &str,
    ) -> Result<ExAnimationSaveOptions, String> {
        let range = parse_search_range(search_start, search_end)?;
        let allocation = self
            .profile
            .allocation_policy_for_rom(range, &self.image, self.internal_header)
            .map_err(|error| error.to_string())?;
        Ok(ExAnimationSaveOptions {
            allocation,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        })
    }
}

pub(super) fn decode(app: &AppState) -> Result<Workspace, String> {
    let profiled = app
        .profiled_controller_snapshot()
        .map_err(|error| error.to_string())?;
    let lm_app::EditorMode::ExAnimation(slot) = profiled.snapshot.mode else {
        return Err("select an ExAnimation slot before opening the ROM editor".into());
    };
    let controller = profiled
        .profile
        .decode_exanimation(&profiled.snapshot)
        .map_err(|error| error.to_string())?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let modes = profiled.profile.exanimation_double_size_modes;
    Ok(Workspace {
        controller,
        profile: profiled.profile,
        modes,
        slot,
        image,
        internal_header: profiled.snapshot.identity.internal_header_offset,
    })
}
