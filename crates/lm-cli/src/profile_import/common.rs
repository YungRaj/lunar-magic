use crate::oracle_input::read_rom;
use lm_profile::RevisionProfile;
use lm_project::Project;
use lm_rats::AllocationPolicy;
use lm_rom::RomImage;
use std::fs;
use std::ops::Range;
use std::path::Path;

pub(super) struct ImportContext {
    pub project: Project,
    pub profile: RevisionProfile,
    pub checksum_field: usize,
    internal_header_offset: usize,
    pub search: Range<usize>,
}

impl ImportContext {
    pub(super) fn load(
        input_rom: &Path,
        output_rom: &Path,
        profile_path: &Path,
        asset_path: &Path,
        search: Range<usize>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if [input_rom, profile_path, asset_path].contains(&output_rom) {
            return Err("refusing to overwrite an import input".into());
        }
        let profile = RevisionProfile::read_from(fs::File::open(profile_path)?)?;
        let project = Project::open_supported(RomImage::from_bytes(read_rom(input_rom)?)?)?;
        let identity = project
            .identity
            .as_ref()
            .ok_or("opened project has no detected identity")?;
        profile.ensure_identity(identity)?;
        profile.audit_rom(&project.rom)?;
        if search.start >= search.end || search.end > project.rom.logical_len() {
            return Err(
                "allocation search range must be nonempty and inside the logical ROM".into(),
            );
        }
        Ok(Self {
            checksum_field: identity.internal_header_offset + 0x1c,
            internal_header_offset: identity.internal_header_offset,
            project,
            profile,
            search,
        })
    }

    pub(super) fn allocation(&self) -> Result<AllocationPolicy, Box<dyn std::error::Error>> {
        Ok(self.profile.allocation_policy_for_rom(
            self.search.clone(),
            &self.project.rom,
            self.internal_header_offset,
        )?)
    }
}
