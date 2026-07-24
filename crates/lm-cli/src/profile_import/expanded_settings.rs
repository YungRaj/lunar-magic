use super::common::ImportContext;
use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_level::ExpandedLevelSettingsRecord;
use lm_project::Project;
use lm_rom::RomImage;
use std::path::Path;

pub(super) fn import(
    mut context: ImportContext,
    slot: usize,
    asset: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let record = ExpandedLevelSettingsRecord::decode(&read_bounded(
        asset,
        ExpandedLevelSettingsRecord::ENCODED_LEN,
    )?)?;
    let layout = context
        .profile
        .expanded_settings
        .ok_or("profile does not declare an installed expanded-settings table")?;
    context
        .project
        .save_expanded_level_settings(slot, &record, layout, context.checksum_field)?;
    let snapshot = context.project.save_snapshot();
    if Project::new(RomImage::from_bytes(snapshot.clone())?)
        .load_expanded_level_settings(slot, layout)?
        != record
    {
        return Err(
            "profile-imported expanded settings failed semantic reopen verification".into(),
        );
    }
    write_new(output, snapshot)?;
    println!("imported-expanded-settings: {slot:#05x}");
    Ok(())
}
