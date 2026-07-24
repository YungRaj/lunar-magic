use super::common::ImportContext;
use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_graphics::{CompactExAnimationFile, GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_project::{
    ExAnimationSaveOptions, GraphicsSaveOptions, PaletteSaveOptions, PayloadReadPolicy, Project,
};
use lm_rom::RomImage;
use std::path::Path;

pub(super) fn graphics(
    mut context: ImportContext,
    slot: usize,
    asset: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = context.profile.graphics;
    let graphics = GraphicsInterchangeFile::decode(&read_bounded(
        asset,
        GraphicsInterchangeFile::MAX_FILE_LEN,
    )?)?
    .graphics;
    let previous_block = context
        .project
        .load_payload(
            layout.pointers.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: layout.maximum_compressed_len,
                bank_size: Some(0x8000),
            },
        )?
        .block;
    let allocation = context.allocation()?;
    context.project.save_graphics_file_with_checksum(
        slot,
        &graphics,
        layout,
        context.checksum_field,
        &GraphicsSaveOptions {
            allocation,
            previous_block,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    )?;
    let snapshot = context.project.save_snapshot();
    if Project::new(RomImage::from_bytes(snapshot.clone())?).load_graphics_file(slot, layout)?
        != graphics
    {
        return Err("profile-imported graphics failed semantic reopen verification".into());
    }
    write_new(output, snapshot)?;
    println!("imported-graphics: {slot:#04x}");
    Ok(())
}

pub(super) fn palette(
    mut context: ImportContext,
    slot: usize,
    asset: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = context
        .profile
        .palette_installation
        .resolve(&context.project.rom)?
        .ok_or("per-level palette subsystem is not installed in this ROM")?;
    let palette = PaletteInterchangeFile::decode(&read_bounded(
        asset,
        PaletteInterchangeFile::MAX_FILE_LEN,
    )?)?
    .palette;
    if palette.colors.len() != layout.colors_per_palette {
        return Err("palette file color count does not match the profile".into());
    }
    let encoded_len = layout
        .colors_per_palette
        .checked_mul(2)
        .ok_or("palette size overflow")?;
    let previous_block = context
        .project
        .load_payload(
            layout.pointers.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: encoded_len },
        )?
        .block;
    let allocation = context.allocation()?;
    context.project.save_palette_with_checksum(
        slot,
        &palette,
        layout,
        context.checksum_field,
        &PaletteSaveOptions {
            allocation,
            previous_block,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    )?;
    let snapshot = context.project.save_snapshot();
    if Project::new(RomImage::from_bytes(snapshot.clone())?).load_palette(slot, layout)? != palette
    {
        return Err("profile-imported palette failed semantic reopen verification".into());
    }
    write_new(output, snapshot)?;
    println!("imported-palette: {slot:#05x}");
    Ok(())
}

pub(super) fn exanimation(
    mut context: ImportContext,
    slot: usize,
    asset: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = context
        .profile
        .exanimation_installation
        .resolve(&context.project.rom)?
        .ok_or("per-level ExAnimation subsystem is not installed in this ROM")?
        .resolve(&context.project.rom)?
        .payload;
    let modes = context.profile.exanimation_double_size_modes;
    let animation = CompactExAnimationFile::decode(
        &read_bounded(asset, CompactExAnimationFile::MAX_FILE_LEN)?,
        layout.maximum_records,
        &modes,
    )?
    .animation;
    let previous_block = context
        .project
        .load_payload(
            layout.pointers.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: layout.maximum_encoded_len,
                bank_size: Some(0x8000),
            },
        )?
        .block;
    let allocation = context.allocation()?;
    context.project.save_exanimation_with_checksum(
        slot,
        &animation,
        layout,
        &modes,
        context.checksum_field,
        &ExAnimationSaveOptions {
            allocation,
            previous_block,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    )?;
    let snapshot = context.project.save_snapshot();
    if Project::new(RomImage::from_bytes(snapshot.clone())?)
        .load_exanimation(slot, layout, &modes)?
        != animation
    {
        return Err("profile-imported ExAnimation failed semantic reopen verification".into());
    }
    write_new(output, snapshot)?;
    println!("imported-exanimation: {slot:#05x}");
    Ok(())
}
