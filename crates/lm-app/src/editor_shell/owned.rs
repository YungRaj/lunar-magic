//! Ownership-backed editor script execution and atomic reclamation commits.

use super::read_bounded_utf8;
use crate::{
    exanimation_edit_script, graphics_edit_script, level_edit_script, map16_edit_script,
    overworld_edit_script, palette_edit_script, shell_command,
};
use lm_app::{AppState, Map16ControllerEdit, NativeLevelEdit, RevisionProfileControllers};
use lm_project::{
    CompleteOverworldSaveOptions, ExAnimationSaveOptions, GraphicsSaveOptions, LevelSaveOptions,
    Map16SetSaveOptions, PaletteSaveOptions, RatsOwnershipManifest, RatsOwnershipManifestFile,
};
use lm_rom::RomImage;
use std::ops::Range;
use std::path::Path;

pub(crate) fn execute_owned_editor_script(
    app: &mut AppState,
    editor: shell_command::ScriptEditor,
    path: &Path,
    ownership_manifest: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ownership = RatsOwnershipManifestFile::decode(&crate::read_bounded_bytes(
        ownership_manifest,
        RatsOwnershipManifestFile::MAX_FILE_LEN,
        "RATS ownership manifest",
    )?)?
    .0;
    match editor {
        shell_command::ScriptEditor::NativeAssets => {
            let loaded = crate::native_assets_edit_loader::load(path)?;
            if !loaded.map16_edits.is_empty() {
                return Err(
                    "aggregate Map16 edits do not yet support reclamation manifests".into(),
                );
            }
            commit_native_assets(
                app,
                &loaded.edits,
                loaded.palette_ownership,
                search,
                &ownership,
            )?;
        }
        shell_command::ScriptEditor::ExAnimation => {
            let text = read_bounded_utf8(
                path,
                exanimation_edit_script::MAX_SCRIPT_LEN,
                "ExAnimation edit",
            )?;
            commit_exanimation(
                app,
                &exanimation_edit_script::parse(&text)?,
                search,
                &ownership,
            )?;
        }
        shell_command::ScriptEditor::Graphics => {
            let text =
                read_bounded_utf8(path, graphics_edit_script::MAX_SCRIPT_LEN, "graphics edit")?;
            commit_graphics(
                app,
                &graphics_edit_script::parse(&text)?,
                search,
                &ownership,
            )?;
        }
        shell_command::ScriptEditor::Level => {
            let text = read_bounded_utf8(path, level_edit_script::MAX_SCRIPT_LEN, "level-edit")?;
            commit_level(app, &level_edit_script::parse(&text)?, search, &ownership)?;
        }
        shell_command::ScriptEditor::Map16 => {
            let text = read_bounded_utf8(path, map16_edit_script::MAX_SCRIPT_LEN, "Map16 edit")?;
            commit_map16(app, &map16_edit_script::parse(&text)?, search, &ownership)?;
        }
        shell_command::ScriptEditor::Overworld => {
            let text = read_bounded_utf8(
                path,
                overworld_edit_script::MAX_SCRIPT_LEN,
                "overworld edit",
            )?;
            commit_overworld(
                app,
                &overworld_edit_script::parse(&text)?,
                search,
                &ownership,
            )?;
        }
        shell_command::ScriptEditor::Palette => {
            let text =
                read_bounded_utf8(path, palette_edit_script::MAX_SCRIPT_LEN, "palette edit")?;
            commit_palette(app, &palette_edit_script::parse(&text)?, search, &ownership)?;
        }
    }
    println!("{}", app.status);
    Ok(())
}

fn commit_native_assets(
    app: &mut AppState,
    edits: &[lm_app::NativeLevelAssetsControllerEdit],
    palette_ownership: Option<lm_graphics::PaletteOwnership>,
    search: Range<usize>,
    manifest: &RatsOwnershipManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    if edits.is_empty() {
        return Err("native-assets edit specification contains no domains".into());
    }
    let profiled = app.profiled_controller_snapshot()?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())?;
    let (_, options) = profiled.profile.native_level_assets_save_plan_for_rom(
        search,
        &image,
        profiled.snapshot.identity.internal_header_offset,
    )?;
    let ownership = palette_ownership.unwrap_or_else(|| {
        lm_graphics::PaletteOwnership::editable(profiled.profile.palette.colors_per_palette)
    });
    let mut controller = profiled
        .profile
        .decode_native_level_assets(&profiled.snapshot, ownership)?;
    controller.apply_edits(edits)?;
    let prepared = controller.prepare_commit_with_reclamation(
        "Apply owned native aggregate level-assets edit",
        &options,
        manifest,
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

fn commit_level(
    app: &mut AppState,
    edits: &[NativeLevelEdit],
    search: Range<usize>,
    manifest: &RatsOwnershipManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let profiled = app.profiled_controller_snapshot()?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())?;
    let policy = profiled.profile.allocation_policy_for_rom(
        search,
        &image,
        profiled.snapshot.identity.internal_header_offset,
    )?;
    let mut controller = profiled.profile.decode_level(&profiled.snapshot)?;
    controller.apply_edits(edits)?;
    let options = LevelSaveOptions {
        layer1_allocation: policy.clone(),
        sprite_allocation: policy,
        previous_layer1: None,
        previous_sprites: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    let prepared = controller.prepare_commit_with_reclamation(
        "Apply owned native level-edit script",
        &options,
        manifest,
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

fn commit_map16(
    app: &mut AppState,
    edits: &[Map16ControllerEdit],
    search: Range<usize>,
    manifest: &RatsOwnershipManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let profiled = app.profiled_controller_snapshot()?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())?;
    let policy = profiled.profile.allocation_policy_for_rom(
        search,
        &image,
        profiled.snapshot.identity.internal_header_offset,
    )?;
    let mut controller = profiled.profile.decode_map16(&profiled.snapshot)?;
    controller.apply_edits(edits)?;
    let options = Map16SetSaveOptions {
        graphics_allocation: policy.clone(),
        acts_like_allocation: policy,
        previous_graphics: Vec::new(),
        previous_acts_like: Vec::new(),
        reuse_identical: true,
        erase_fill: 0xff,
    };
    let prepared = controller.prepare_commit_with_reclamation(
        "Apply owned native Map16 edit script",
        &options,
        manifest,
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

fn commit_palette(
    app: &mut AppState,
    script: &palette_edit_script::PaletteEditScript,
    search: Range<usize>,
    manifest: &RatsOwnershipManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let profiled = app.profiled_controller_snapshot()?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())?;
    let policy = profiled.profile.allocation_policy_for_rom(
        search,
        &image,
        profiled.snapshot.identity.internal_header_offset,
    )?;
    let mut controller = profiled
        .profile
        .decode_palette(&profiled.snapshot, script.ownership.clone())?;
    controller.apply_edits(&script.edits)?;
    let prepared = controller.prepare_commit_with_reclamation(
        "Apply owned native palette edit script",
        &PaletteSaveOptions {
            allocation: policy,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        manifest,
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

fn commit_graphics(
    app: &mut AppState,
    script: &graphics_edit_script::GraphicsEditScript,
    search: Range<usize>,
    manifest: &RatsOwnershipManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let profiled = app.profiled_controller_snapshot()?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())?;
    let policy = profiled.profile.allocation_policy_for_rom(
        search,
        &image,
        profiled.snapshot.identity.internal_header_offset,
    )?;
    let mut controller = profiled
        .profile
        .decode_graphics(&profiled.snapshot, script.ownership.clone())?;
    controller.apply_edits(&script.edits)?;
    let prepared = controller.prepare_commit_with_reclamation(
        "Apply owned native graphics edit script",
        &GraphicsSaveOptions {
            allocation: policy,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        manifest,
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

fn commit_exanimation(
    app: &mut AppState,
    edits: &[lm_app::ExAnimationControllerEdit],
    search: Range<usize>,
    manifest: &RatsOwnershipManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let profiled = app.profiled_controller_snapshot()?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())?;
    let policy = profiled.profile.allocation_policy_for_rom(
        search,
        &image,
        profiled.snapshot.identity.internal_header_offset,
    )?;
    let mut controller = profiled.profile.decode_exanimation(&profiled.snapshot)?;
    controller.apply_edits(edits)?;
    let prepared = controller.prepare_commit_with_reclamation(
        "Apply owned native ExAnimation edit script",
        &ExAnimationSaveOptions {
            allocation: policy,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        manifest,
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

fn commit_overworld(
    app: &mut AppState,
    script: &overworld_edit_script::OverworldEditScript,
    search: Range<usize>,
    manifest: &RatsOwnershipManifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let profiled = app.profiled_controller_snapshot()?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())?;
    let policy = profiled.profile.allocation_policy_for_rom(
        search,
        &image,
        profiled.snapshot.identity.internal_header_offset,
    )?;
    let mut controller = profiled.profile.decode_overworld(
        &profiled.snapshot,
        script.slot,
        script.palette_ownership.clone(),
    )?;
    controller.apply_edits(&script.edits)?;
    let options = CompleteOverworldSaveOptions::uniform_allocation(policy);
    let prepared = controller.prepare_commit_with_reclamation(
        "Apply owned complete overworld edit script",
        &options,
        manifest,
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}
