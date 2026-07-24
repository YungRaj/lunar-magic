use crate::atomic_output::write_new;
use lm_profile::RevisionProfile;
use lm_project::{NativeLevelAssetsFile, NativeLevelAssetsLayout, Project};
use std::path::Path;

pub(super) fn export(
    project: &Project,
    profile: &RevisionProfile,
    slot: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_slot = u16::try_from(slot).map_err(|_| "native level slot exceeds file format")?;
    let palette = profile
        .palette_installation
        .resolve(&project.rom)?
        .ok_or("per-level palette subsystem is not installed in this ROM")?;
    let exanimation = profile
        .exanimation_installation
        .resolve(&project.rom)?
        .ok_or("per-level ExAnimation subsystem is not installed in this ROM")?
        .resolve(&project.rom)?
        .payload;
    let layout = NativeLevelAssetsLayout {
        level: profile.level,
        palette,
        exanimation,
        expanded_settings: profile.expanded_settings,
    };
    let assets = project.load_native_level_assets(
        slot,
        layout,
        &profile.sprite_lengths,
        &profile.exanimation_double_size_modes,
    )?;
    write_new(
        output,
        NativeLevelAssetsFile {
            source_slot,
            assets,
        }
        .encode(&profile.exanimation_double_size_modes)?,
    )?;
    println!("exported-native-assets: {slot:#05x}");
    Ok(())
}
