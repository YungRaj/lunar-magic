use super::common::ImportContext;
use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_project::{NativeLevelAssetsFile, Project};
use lm_rom::RomImage;
use std::path::Path;

pub(super) fn import(
    mut context: ImportContext,
    slot: usize,
    asset: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let modes = context.profile.exanimation_double_size_modes;
    let mut file = NativeLevelAssetsFile::decode(
        &read_bounded(asset, NativeLevelAssetsFile::MAX_FILE_LEN)?,
        &context.profile.sprite_lengths,
        context.profile.exanimation.maximum_records,
        &modes,
    )?;
    if file.assets.level.sprites.expanded != context.profile.level.expanded_sprites {
        return Err("native-assets sprite format does not match the profile".into());
    }
    if file.assets.palette.colors.len() != context.profile.palette.colors_per_palette {
        return Err("native-assets palette size does not match the profile".into());
    }
    if file.assets.expanded_settings.is_some() != context.profile.expanded_settings.is_some() {
        return Err("native-assets expanded-settings presence does not match the profile".into());
    }
    file.assets.level.number = slot;
    let (layout, options) = context.profile.native_level_assets_save_plan_for_rom(
        context.search.clone(),
        &context.project.rom,
        context.checksum_field - 0x1c,
    )?;
    context.project.save_native_level_assets(
        file.assets.as_save_assets(),
        layout,
        &context.profile.sprite_lengths,
        &modes,
        context.checksum_field,
        &options,
    )?;
    let snapshot = context.project.save_snapshot();
    let reopened = Project::new(RomImage::from_bytes(snapshot.clone())?).load_native_level_assets(
        slot,
        layout,
        &context.profile.sprite_lengths,
        &modes,
    )?;
    if reopened != file.assets {
        return Err("profile-imported native assets failed semantic reopen verification".into());
    }
    write_new(output, snapshot)?;
    println!("imported-native-assets: {slot:#05x}");
    Ok(())
}
