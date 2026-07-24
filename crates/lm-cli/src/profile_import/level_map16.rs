use super::common::ImportContext;
use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_exact},
};
use lm_level::{Map16PageFile, NativeLevelFile};
use lm_project::{LevelSaveOptions, LoadedLevelSlot, Map16SaveOptions, PayloadReadPolicy, Project};
use lm_rom::RomImage;
use std::path::Path;

pub(super) fn level(
    mut context: ImportContext,
    slot: usize,
    asset: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = context.profile.level;
    let file = NativeLevelFile::decode(
        &read_bounded(asset, NativeLevelFile::MAX_FILE_LEN)?,
        &context.profile.sprite_lengths,
    )?;
    if file.sprites.expanded != layout.expanded_sprites {
        return Err("level file sprite format does not match the profile".into());
    }
    let layer_pointer = layout.layer1.pointer_offset(slot)?;
    let sprite_pointer = layout
        .sprites
        .read_snes_pointer(&context.project.rom, slot)?;
    let previous_layer1 = context
        .project
        .load_payload(
            layer_pointer,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrTerminated {
                terminator: vec![0xff],
                maximum_len: 0x8000,
                bank_size: Some(0x8000),
            },
        )?
        .block;
    let previous_sprites = context
        .project
        .load_payload_from_pointer(
            sprite_pointer,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrTerminated {
                terminator: if layout.expanded_sprites {
                    vec![0xff, 0xfe]
                } else {
                    vec![0xff]
                },
                maximum_len: 0x8000,
                bank_size: Some(0x8000),
            },
        )?
        .block;
    let allocation = context.allocation()?;
    let expected = LoadedLevelSlot {
        number: slot,
        layer1: file.layer1,
        sprites: file.sprites,
    };
    context.project.save_level_slot_with_checksum(
        layout,
        &expected,
        &context.profile.sprite_lengths,
        context.checksum_field,
        &LevelSaveOptions {
            layer1_allocation: allocation.clone(),
            sprite_allocation: allocation,
            previous_layer1,
            previous_sprites,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    )?;
    let snapshot = context.project.save_snapshot();
    let reopened = Project::new(RomImage::from_bytes(snapshot.clone())?);
    if reopened.load_level_slot(slot, layout, &context.profile.sprite_lengths)? != expected {
        return Err("profile-imported level failed semantic reopen verification".into());
    }
    write_new(output, snapshot)?;
    println!("imported-level: {slot:#05x}");
    Ok(())
}

pub(super) fn map16(
    mut context: ImportContext,
    slot: usize,
    asset: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = context.profile.map16;
    let page = Map16PageFile::decode(&read_exact(
        asset,
        Map16PageFile::ENCODED_LEN,
        "Map16 page",
    )?)?
    .page;
    let previous_graphics = context
        .project
        .load_payload(
            layout.graphics.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: 0x800 },
        )?
        .block;
    let previous_acts_like = context
        .project
        .load_payload(
            layout.acts_like.pointer_offset(slot)?,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrFixed { len: 0x200 },
        )?
        .block;
    let allocation = context.allocation()?;
    context.project.save_map16_page_with_checksum(
        slot,
        &page,
        layout,
        context.checksum_field,
        &Map16SaveOptions {
            graphics_allocation: allocation.clone(),
            acts_like_allocation: allocation,
            previous_graphics,
            previous_acts_like,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    )?;
    let snapshot = context.project.save_snapshot();
    let reopened = Project::new(RomImage::from_bytes(snapshot.clone())?);
    if reopened.load_map16_page(slot, layout)? != page {
        return Err("profile-imported Map16 page failed semantic reopen verification".into());
    }
    write_new(output, snapshot)?;
    println!("imported-map16: {slot:#04x}");
    Ok(())
}
