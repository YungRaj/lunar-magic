use crate::atomic_output::write_new;
use lm_level::{Map16PageFile, NativeLevelFile};
use lm_profile::RevisionProfile;
use lm_project::Project;
use std::path::Path;

pub(super) fn level(
    project: &Project,
    profile: &RevisionProfile,
    slot: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let loaded = project.load_level_slot(slot, profile.level, &profile.sprite_lengths)?;
    write_new(
        output,
        NativeLevelFile {
            source_level: u16::try_from(slot).map_err(|_| "level slot exceeds file format")?,
            layer1: loaded.layer1,
            sprites: loaded.sprites,
        }
        .encode()?,
    )?;
    println!("exported-level: {slot:#05x}");
    Ok(())
}

pub(super) fn map16(
    project: &Project,
    profile: &RevisionProfile,
    slot: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let page = project.load_map16_page(slot, profile.map16)?;
    write_new(
        output,
        Map16PageFile {
            source_page: u16::try_from(slot).map_err(|_| "Map16 page exceeds file format")?,
            page,
        }
        .encode()?,
    )?;
    println!("exported-map16: {slot:#04x}");
    Ok(())
}

pub(super) fn layer2(
    project: &Project,
    profile: &RevisionProfile,
    slot: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = profile
        .layer2
        .ok_or("revision profile does not declare an installed Layer 2 pointer table")?;
    let level = project.load_level_slot(slot, profile.level, &profile.sprite_lengths)?;
    let layer2 = project.load_level_layer2(slot, level.layer1.header.level_mode(), layout)?;
    write_new(output, layer2.encode_mwl()?)?;
    println!("exported-layer2: {slot:#05x}");
    Ok(())
}
