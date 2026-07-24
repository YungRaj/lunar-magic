use crate::atomic_output::write_new;
use lm_profile::RevisionProfile;
use lm_project::{CompleteOverworldFile, Project};
use std::path::Path;

pub(super) fn export(
    project: &Project,
    profile: &RevisionProfile,
    slot: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = project.load_complete_overworld(
        slot,
        profile.overworld,
        &profile.exanimation_double_size_modes,
    )?;
    write_new(
        output,
        CompleteOverworldFile {
            source_slot: u16::try_from(slot).map_err(|_| "overworld slot exceeds file format")?,
            shape: profile.overworld_shape,
            data,
        }
        .encode(&profile.exanimation_double_size_modes)?,
    )?;
    println!("exported-overworld: {slot:#05x}");
    Ok(())
}
