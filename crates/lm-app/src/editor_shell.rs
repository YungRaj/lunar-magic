use crate::{
    exanimation_edit_script, graphics_edit_script, level_edit_script, map16_edit_script,
    overworld_edit_script, palette_edit_script, shell_command,
};
use lm_app::{AppState, Map16ControllerEdit, NativeLevelEdit, RevisionProfileControllers};
use lm_level::LegacyHeaderEdit;
use lm_project::{
    CompleteOverworldSaveOptions, ExAnimationSaveOptions, GraphicsSaveOptions, LevelSaveOptions,
    Map16SetSaveOptions, PaletteSaveOptions,
};
use lm_rom::RomImage;
use std::ops::Range;
use std::path::Path;

mod expanded_settings;
mod graphics_migration;
mod owned;

pub(crate) use expanded_settings::{edit_expanded_settings, edit_expanded_settings_word};
pub(crate) use graphics_migration::migrate_graphics_compression;
pub(crate) use owned::execute_owned_editor_script;

pub(crate) fn edit_level_header(
    app: &mut AppState,
    field: shell_command::LevelHeaderField,
    value: u8,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let edit = match field {
        shell_command::LevelHeaderField::BackgroundPalette => {
            LegacyHeaderEdit::BackgroundPalette(value)
        }
        shell_command::LevelHeaderField::LevelMode => LegacyHeaderEdit::LevelMode(value),
        shell_command::LevelHeaderField::BackgroundColor => {
            LegacyHeaderEdit::BackgroundColor(value)
        }
        shell_command::LevelHeaderField::SpriteTileset => LegacyHeaderEdit::SpriteTileset(value),
        shell_command::LevelHeaderField::SpritePalette => LegacyHeaderEdit::SpritePalette(value),
        shell_command::LevelHeaderField::ForegroundPalette => {
            LegacyHeaderEdit::ForegroundPalette(value)
        }
        shell_command::LevelHeaderField::ObjectTileset => LegacyHeaderEdit::ObjectTileset(value),
    };
    commit_level_edits(
        app,
        &[NativeLevelEdit::LegacyHeader(edit)],
        search,
        "Edit native level header",
    )
}

pub(crate) fn execute_editor_script(
    app: &mut AppState,
    editor: shell_command::ScriptEditor,
    path: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    match editor {
        shell_command::ScriptEditor::NativeAssets => {
            execute_native_assets_script(app, path, search)
        }
        shell_command::ScriptEditor::ExAnimation => execute_exanimation_script(app, path, search),
        shell_command::ScriptEditor::Graphics => execute_graphics_script(app, path, search),
        shell_command::ScriptEditor::Level => execute_level_script(app, path, search),
        shell_command::ScriptEditor::Map16 => execute_map16_script(app, path, search),
        shell_command::ScriptEditor::Overworld => execute_overworld_script(app, path, search),
        shell_command::ScriptEditor::Palette => execute_palette_script(app, path, search),
    }
}

pub(crate) fn execute_native_assets_script(
    app: &mut AppState,
    path: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let loaded = crate::native_assets_edit_loader::load(path)?;
    commit_native_assets_edits(app, &loaded.edits, loaded.palette_ownership, search)?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn commit_native_assets_edits(
    app: &mut AppState,
    edits: &[lm_app::NativeLevelAssetsControllerEdit],
    palette_ownership: Option<lm_graphics::PaletteOwnership>,
    search: Range<usize>,
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
    let prepared =
        controller.prepare_commit("Apply native aggregate level-assets edit", &options)?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

pub(crate) fn execute_exanimation_script(
    app: &mut AppState,
    path: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        exanimation_edit_script::MAX_SCRIPT_LEN,
        "ExAnimation edit",
    )?;
    let edits = exanimation_edit_script::parse(&text)?;
    commit_exanimation_edits(app, &edits, search)?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn execute_graphics_script(
    app: &mut AppState,
    path: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(path, graphics_edit_script::MAX_SCRIPT_LEN, "graphics edit")?;
    let script = graphics_edit_script::parse(&text)?;
    commit_graphics_edits(app, &script, search)?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn execute_level_script(
    app: &mut AppState,
    path: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(path, level_edit_script::MAX_SCRIPT_LEN, "level-edit")?;
    let edits = level_edit_script::parse(&text)?;
    commit_level_edits(app, &edits, search, "Apply native level-edit script")?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn execute_map16_script(
    app: &mut AppState,
    path: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(path, map16_edit_script::MAX_SCRIPT_LEN, "Map16 edit")?;
    let edits = map16_edit_script::parse(&text)?;
    commit_map16_edits(app, &edits, search)?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn execute_palette_script(
    app: &mut AppState,
    path: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(path, palette_edit_script::MAX_SCRIPT_LEN, "palette edit")?;
    let script = palette_edit_script::parse(&text)?;
    commit_palette_edits(app, &script, search)?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn execute_overworld_script(
    app: &mut AppState,
    path: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        overworld_edit_script::MAX_SCRIPT_LEN,
        "overworld edit",
    )?;
    let script = overworld_edit_script::parse(&text)?;
    commit_overworld_edits(app, &script, search)?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn read_bounded_utf8(
    path: &Path,
    maximum: usize,
    kind: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = crate::read_bounded_bytes(path, maximum, kind)?;
    Ok(String::from_utf8(bytes)?)
}

pub(crate) fn commit_level_edits(
    app: &mut AppState,
    edits: &[NativeLevelEdit],
    search: Range<usize>,
    description: &str,
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
    let prepared = controller.prepare_commit(description, &options)?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

pub(crate) fn commit_map16_edits(
    app: &mut AppState,
    edits: &[Map16ControllerEdit],
    search: Range<usize>,
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
    let prepared = controller.prepare_commit("Apply native Map16 edit script", &options)?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

pub(crate) fn commit_palette_edits(
    app: &mut AppState,
    script: &palette_edit_script::PaletteEditScript,
    search: Range<usize>,
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
    let prepared = controller.prepare_commit(
        "Apply native palette edit script",
        &PaletteSaveOptions {
            allocation: policy,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

pub(crate) fn commit_graphics_edits(
    app: &mut AppState,
    script: &graphics_edit_script::GraphicsEditScript,
    search: Range<usize>,
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
    let prepared = controller.prepare_commit(
        "Apply native graphics edit script",
        &GraphicsSaveOptions {
            allocation: policy,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

pub(crate) fn commit_exanimation_edits(
    app: &mut AppState,
    edits: &[lm_app::ExAnimationControllerEdit],
    search: Range<usize>,
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
    let prepared = controller.prepare_commit(
        "Apply native ExAnimation edit script",
        &ExAnimationSaveOptions {
            allocation: policy,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}

pub(crate) fn commit_overworld_edits(
    app: &mut AppState,
    script: &overworld_edit_script::OverworldEditScript,
    search: Range<usize>,
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
    let prepared = controller.prepare_commit(
        "Apply native complete-overworld edit script",
        &CompleteOverworldSaveOptions::uniform_allocation(policy),
    )?;
    app.dispatch(prepared.into_command())?;
    Ok(())
}
