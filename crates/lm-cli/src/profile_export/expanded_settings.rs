use crate::atomic_output::write_new;
use lm_profile::RevisionProfile;
use lm_project::Project;
use std::path::Path;

pub(super) fn export(
    project: &Project,
    profile: &RevisionProfile,
    slot: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = profile
        .expanded_settings
        .ok_or("profile does not declare an installed expanded-settings table")?;
    let record = project.load_expanded_level_settings(slot, layout)?;
    write_new(output, record.encoded())?;
    println!("exported-expanded-settings: {slot:#05x}");
    Ok(())
}
