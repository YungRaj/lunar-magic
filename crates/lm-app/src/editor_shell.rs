use crate::{
    entrance_edit_script, exanimation_edit_script, graphics_edit_script, level_edit_script,
    map16_edit_script, overworld_edit_script, palette_edit_script, shell_command,
};
use lm_app::{
    AppState, Map16ControllerEdit, MwlBatchExportMode, NativeLevelEdit, RevisionProfileControllers,
    VanillaEntranceController, VanillaEntranceEdit, discover_mwl_directory,
    export_smw_us_v1_installed_mwl_batch, prepare_declared_mwl_import, publish_mwl_batch_new,
};
use lm_level::{LegacyHeaderEdit, MwlFile};
use lm_project::{
    CompleteOverworldSaveOptions, ExAnimationSaveOptions, GraphicsSaveOptions, LevelSaveOptions,
    Map16SetSaveOptions, MwlNativeLevel, PaletteSaveOptions,
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
        shell_command::LevelHeaderField::LastScreen => LegacyHeaderEdit::LastScreen(value),
        shell_command::LevelHeaderField::LevelMode => LegacyHeaderEdit::LevelMode(value),
        shell_command::LevelHeaderField::BackgroundColor => {
            LegacyHeaderEdit::BackgroundColor(value)
        }
        shell_command::LevelHeaderField::SpriteTileset => LegacyHeaderEdit::SpriteTileset(value),
        shell_command::LevelHeaderField::DefaultMusicSelector => {
            LegacyHeaderEdit::DefaultMusicSelector(value)
        }
        shell_command::LevelHeaderField::TimeLimitSelector => {
            LegacyHeaderEdit::TimeLimitSelector(value)
        }
        shell_command::LevelHeaderField::SpritePalette => LegacyHeaderEdit::SpritePalette(value),
        shell_command::LevelHeaderField::ForegroundPalette => {
            LegacyHeaderEdit::ForegroundPalette(value)
        }
        shell_command::LevelHeaderField::ObjectTileset => LegacyHeaderEdit::ObjectTileset(value),
        shell_command::LevelHeaderField::Layer1VerticalScroll => {
            if value > 3 {
                return Err(format!("Layer 1 vertical-scroll mode {value} exceeds 3").into());
            }
            LegacyHeaderEdit::Layer1VerticalScroll(lm_level::Layer1VerticalScrollMode::from_raw(
                value,
            ))
        }
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

pub(crate) fn execute_entrance_script(
    app: &mut AppState,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(path, entrance_edit_script::MAX_SCRIPT_LEN, "entrance-edit")?;
    let edits = entrance_edit_script::parse(&text)?;
    if edits.is_empty() {
        return Err("entrance-edit script contains no commands".into());
    }
    let profiled = app.profiled_controller_snapshot()?;
    let mut layout = lm_profile::smw_us_v1_vanilla_entrance_layout();
    layout.mapper = profiled.snapshot.identity.mapper;
    let mut controller = if edits
        .iter()
        .any(|edit| matches!(edit, VanillaEntranceEdit::SetMidway(_)))
    {
        VanillaEntranceController::decode_with_midway(
            &profiled.snapshot,
            layout,
            lm_profile::smw_us_v1_separate_midway_locator(),
        )?
    } else {
        VanillaEntranceController::decode(&profiled.snapshot, layout)?
    };
    controller.apply_edits(&edits)?;
    let prepared = controller.prepare_commit("Apply semantic entrance-edit script")?;
    app.dispatch(prepared.into_command())?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn import_mwl_level(
    app: &mut AppState,
    path: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = crate::read_bounded_bytes(path, MwlFile::MAX_FILE_BYTES, "binary MWL level")?;
    let file = MwlFile::decode(&bytes)?;
    let profiled = app.profiled_controller_snapshot()?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())?;
    let (layout, options) = profiled.profile.native_level_assets_save_plan_for_rom(
        search.clone(),
        &image,
        profiled.snapshot.identity.internal_header_offset,
    )?;
    let Some((_, layer2_options)) = profiled.profile.level_layer2_save_plan(
        search,
        image.logical_len(),
        profiled.snapshot.identity.internal_header_offset,
    )?
    else {
        return Err("active revision profile has no native Layer 2 layout".into());
    };
    let ownership = lm_graphics::PaletteOwnership::editable(layout.palette.colors_per_palette);
    let controller = profiled
        .profile
        .decode_native_level_assets(&profiled.snapshot, ownership)?;
    let mut source = MwlNativeLevel::decode(
        &file,
        &profiled.profile.sprite_lengths,
        layout.exanimation.maximum_records,
        &profiled.profile.exanimation_double_size_modes,
    )?;
    source.retarget(u16::try_from(controller.assets().level.number)?)?;
    let prepared =
        controller.prepare_smw_us_v1_installed_mwl_import(&source, &options, &layer2_options)?;
    app.dispatch(prepared.into_command())?;
    println!("{}", app.status);
    Ok(())
}

pub(crate) fn import_mwl_level_directory(
    app: &mut AppState,
    directory: &Path,
    search: Range<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !directory.is_dir() {
        return Err("MWL batch import path must be a directory".into());
    }
    let listing = discover_mwl_directory(directory)?;
    let mut inserted = 0usize;
    let mut failed = 0usize;
    for path in listing.paths {
        match import_mwl_declared_target(app, &path, search.clone()) {
            Ok(level) => {
                inserted += 1;
                println!("Inserted level {level:03X} from {}", path.display());
            }
            Err(error) => {
                failed += 1;
                eprintln!("Failed to insert {}: {error}", path.display());
            }
        }
    }
    println!("{inserted} levels have been inserted into the ROM.");
    if failed != 0 {
        println!("{failed} levels failed to insert into the ROM.");
    }
    if listing.hidden_skipped != 0 {
        println!(
            "{} levels were skipped (Hidden Attribute).",
            listing.hidden_skipped
        );
    }
    Ok(())
}

fn import_mwl_declared_target(
    app: &mut AppState,
    path: &Path,
    search: Range<usize>,
) -> Result<u16, Box<dyn std::error::Error>> {
    let bytes = crate::read_bounded_bytes(path, MwlFile::MAX_FILE_BYTES, "binary MWL level")?;
    let profiled = app.profiled_controller_snapshot()?;
    let (level, prepared) = prepare_declared_mwl_import(&profiled, &bytes, search)?;
    app.dispatch(prepared.into_command())?;
    Ok(level)
}

pub(crate) fn export_mwl_level(
    app: &AppState,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let profiled = app.profiled_controller_snapshot()?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())?;
    let palette = profiled
        .profile
        .palette_installation
        .resolve(&image)?
        .ok_or("active revision profile has no installed per-level palette")?;
    let ownership = lm_graphics::PaletteOwnership::editable(palette.colors_per_palette);
    let controller = profiled
        .profile
        .decode_native_level_assets(&profiled.snapshot, ownership)?;
    let semantic = controller.export_smw_us_v1_installed_mwl()?;
    let bytes = semantic
        .encode(
            &profiled.profile.sprite_lengths,
            &profiled.profile.exanimation_double_size_modes,
        )?
        .encode()?;
    crate::file_persistence::write_new(path, &bytes)?;
    println!(
        "Exported level {:03X} to {}",
        controller.assets().level.number,
        path.display()
    );
    Ok(())
}

pub(crate) fn export_all_mwl_levels(
    app: &AppState,
    template: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    export_mwl_levels(app, template, MwlBatchExportMode::All)
}

pub(crate) fn export_modified_mwl_levels(
    app: &AppState,
    template: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    export_mwl_levels(app, template, MwlBatchExportMode::Modified)
}

fn export_mwl_levels(
    app: &AppState,
    template: &Path,
    mode: MwlBatchExportMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let profiled = app.profiled_controller_snapshot()?;
    let documents = export_smw_us_v1_installed_mwl_batch(&profiled, mode)?;
    publish_mwl_batch_new(template, &documents)?;
    println!(
        "{} levels have been exported from the ROM using {}",
        documents.len(),
        template.display()
    );
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
