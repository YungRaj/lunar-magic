use crate::atomic_output::write_new;
use lm_graphics::{CompactExAnimationFile, GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_profile::RevisionProfile;
use lm_project::{InstalledAsset, Project};
use std::path::Path;

pub(super) fn graphics(
    project: &Project,
    profile: &RevisionProfile,
    slot: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let graphics = project.load_graphics_file(slot, profile.graphics)?;
    write_new(
        output,
        GraphicsInterchangeFile {
            source_slot: u16::try_from(slot).map_err(|_| "graphics slot exceeds file format")?,
            graphics,
        }
        .encode()?,
    )?;
    println!("exported-graphics: {slot:#04x}");
    Ok(())
}

pub(super) fn palette(
    project: &Project,
    profile: &RevisionProfile,
    slot: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let palette = project
        .load_installed_palette(slot, profile.palette_installation)?
        .ok_or("per-level palette subsystem is not installed in this ROM")?;
    write_new(
        output,
        PaletteInterchangeFile {
            source_palette: u16::try_from(slot).map_err(|_| "palette slot exceeds file format")?,
            palette,
        }
        .encode()?,
    )?;
    println!("exported-palette: {slot:#05x}");
    Ok(())
}

pub(super) fn exanimation(
    project: &Project,
    profile: &RevisionProfile,
    slot: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let animation = match project.load_installed_exanimation(
        slot,
        profile.exanimation_installation,
        &profile.exanimation_double_size_modes,
    )? {
        InstalledAsset::Present(animation) => animation,
        InstalledAsset::SubsystemAbsent => {
            return Err("per-level ExAnimation subsystem is not installed in this ROM".into());
        }
        InstalledAsset::SlotEmpty => {
            return Err("selected level has no per-level ExAnimation payload".into());
        }
    };
    write_new(
        output,
        CompactExAnimationFile {
            source_slot: u16::try_from(slot).map_err(|_| "ExAnimation slot exceeds file format")?,
            animation,
        }
        .encode(&profile.exanimation_double_size_modes)?,
    )?;
    println!("exported-exanimation: {slot:#05x}");
    Ok(())
}
