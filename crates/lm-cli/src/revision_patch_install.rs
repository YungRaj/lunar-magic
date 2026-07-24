use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_profile::{RevisionPatchTemplate, RevisionProfile};
use lm_project::Project;
use lm_rats::parse_at;
use lm_rom::RomImage;
use std::fs;
use std::ops::Range;
use std::path::Path;

pub(crate) fn execute(
    input_rom: &Path,
    output_rom: &Path,
    profile_path: &Path,
    template_path: &Path,
    search: Range<usize>,
    fill: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    if [input_rom, profile_path, template_path].contains(&output_rom) {
        return Err("revision patch output must differ from every input".into());
    }
    let profile = RevisionProfile::read_from(fs::File::open(profile_path)?)?;
    let template = RevisionPatchTemplate::decode(&read_bounded(
        template_path,
        RevisionPatchTemplate::MAX_FILE_LEN,
    )?)?;
    let mut project = Project::open_supported(RomImage::from_bytes(read_rom(input_rom)?)?)?;
    let identity = project
        .identity
        .as_ref()
        .ok_or("opened project has no detected identity")?
        .clone();
    profile.ensure_identity(&identity)?;
    profile.audit_rom(&project.rom)?;
    let plan = template.installation_plan(
        &profile,
        &project.rom,
        search,
        identity.internal_header_offset,
        identity.internal_header_offset + 0x1c,
        fill,
    )?;
    let result = project.install_relocatable_patch(&plan)?;
    let snapshot = project.save_snapshot();
    for block in &result.blocks {
        let reopened = parse_at(&snapshot, block.header_offset)
            .map_err(|error| format!("installed revision patch has invalid tag: {error:?}"))?;
        if reopened != *block {
            return Err("installed revision patch failed tagged-block reopen".into());
        }
    }
    write_new(output_rom, snapshot)?;
    println!(
        "installed-revision-patch: {} payloads={} writes={}",
        template.name,
        result.blocks.len(),
        template.writes.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_rejected_before_file_access() {
        let same = Path::new("same");
        assert!(
            execute(
                same,
                same,
                Path::new("profile"),
                Path::new("patch"),
                0..1,
                0xff
            )
            .is_err()
        );
        assert!(execute(Path::new("rom"), same, same, Path::new("patch"), 0..1, 0xff).is_err());
        assert!(
            execute(
                Path::new("rom"),
                same,
                Path::new("profile"),
                same,
                0..1,
                0xff
            )
            .is_err()
        );
    }
}
