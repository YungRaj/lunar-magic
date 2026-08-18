use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

pub(crate) const MAX_ROM_FILE_LEN: u64 = 32 * 1024 * 1024 + 512;

pub(crate) fn choose_rom_async() -> impl std::future::Future<Output = Option<PathBuf>> + 'static {
    async {
        rfd::AsyncFileDialog::new()
            .add_filter("SNES ROM", &["smc", "sfc"])
            .pick_file()
            .await
            .map(|handle| handle.path().to_owned())
    }
}

pub(crate) fn choose_map16_bitmap() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Bitmap image", &["png", "bmp"])
        .add_filter("PNG bitmap", &["png"])
        .add_filter("Windows bitmap", &["bmp"])
        .pick_file()
}

pub(crate) fn choose_snes_graphics_set() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select SNES GFX Set")
        .add_filter("SNES GFX Set", &["set", "bin"])
        .pick_file()
}

pub(crate) fn choose_snes_screen_tile_map() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select SNES Screen Tile Map")
        .add_filter("SNES Screen Tile Map", &["map"])
        .pick_file()
}

pub(crate) fn choose_snes_palette_row() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select SNES Palette Row")
        .add_filter("SNES Palette", &["col", "pal"])
        .pick_file()
}

pub(crate) fn choose_snes_palette_row_save_path(row: usize) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export SNES Palette Row")
        .add_filter("SNES Palette", &["col", "pal"])
        .set_file_name(format!("Palette Row {row:X}.col"))
        .save_file()
}

pub(crate) fn choose_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("SNES ROM", &["smc", "sfc"])
        .set_file_name("edited.smc")
        .save_file()
}

pub(crate) fn choose_bypass_list_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Insert Old Bypass List to ROM")
        .add_filter("Lunar Magic bypass list", &["lst"])
        .pick_file()
}

pub(crate) fn choose_bypass_list_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Extract Old Bypass List from ROM")
        .add_filter("Lunar Magic bypass list", &["lst"])
        .set_file_name("Bypass.lst")
        .save_file()
}

pub(crate) fn choose_revision_profile() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic revision profile", &["lmrev"])
        .pick_file()
}

pub(crate) fn choose_revision_patch() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic revision patch", &["lmpatch"])
        .pick_file()
}

pub(crate) fn choose_ips_patch() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("International Patching System patch", &["ips"])
        .pick_file()
}

pub(crate) fn choose_restore_archive() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select Lunar Restore Points")
        .add_filter("Lunar Restore Points", &["lrp"])
        .pick_file()
}

pub(crate) fn choose_new_restore_archive() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Create Lunar Restore Points")
        .add_filter("Lunar Restore Points", &["lrp"])
        .set_file_name("restore-points.lrp")
        .save_file()
}

pub(crate) fn choose_restore_original_rom() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select Original Unmodified ROM with Header")
        .add_filter("SNES ROM", &["smc", "sfc"])
        .pick_file()
}

pub(crate) fn choose_restore_target_rom() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select ROM to Restore")
        .add_filter("SNES ROM", &["smc", "sfc"])
        .pick_file()
}

pub(crate) fn choose_ips_source_rom(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter("SNES ROM", &["smc", "sfc"])
        .pick_file()
}

pub(crate) fn choose_ips_output(suggested_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select IPS File to Save As")
        .add_filter("International Patching System patch", &["ips"])
        .set_file_name(suggested_name)
        .save_file()
}

pub(crate) fn choose_tool_config() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic external tools", &["lmtools"])
        .pick_file()
}

pub(crate) fn choose_frontend_config() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic frontend configuration", &["lmuicfg"])
        .pick_file()
}

pub(crate) fn choose_localization_catalog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic language catalog", &["lmlang"])
        .pick_file()
}

pub(crate) fn choose_palette_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic palette", &["lmpal"])
        .pick_file()
}

pub(crate) fn choose_palette_save_path(slot: u16) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic palette", &["lmpal"])
        .set_file_name(format!("Palette {slot:03X}.lmpal"))
        .save_file()
}

pub(crate) fn choose_palette_ownership() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Palette ownership evidence", &["lmpalown"])
        .pick_file()
}

pub(crate) fn choose_raw_palette_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Raw 257-Color SMW Palette")
        .add_filter("Raw SNES palette", &["pal", "bin"])
        .pick_file()
}

pub(crate) fn choose_raw_palette_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Raw 257-Color SMW Palette")
        .add_filter("Raw SNES palette", &["pal", "bin"])
        .set_file_name("Level Palette.pal")
        .save_file()
}

pub(crate) fn choose_tpl_palette_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Lunar Magic TPL v2 Palette")
        .add_filter("Lunar Magic TPL palette", &["tpl"])
        .pick_file()
}

pub(crate) fn choose_tpl_palette_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Lunar Magic TPL v2 Palette")
        .add_filter("Lunar Magic TPL palette", &["tpl"])
        .set_file_name("Level Palette.tpl")
        .save_file()
}

pub(crate) fn choose_rgb_palette_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import RGB24 Palette")
        .add_filter("RGB24 palette", &["pal"])
        .pick_file()
}

pub(crate) fn choose_rgb_palette_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export RGB24 Palette")
        .add_filter("RGB24 palette", &["pal"])
        .set_file_name("Level Palette RGB.pal")
        .save_file()
}

pub(crate) fn choose_shared_palette_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Native Shared/Custom SMW Palettes")
        .add_filter("Native SMW shared palette", &["smwpal", "pal"])
        .pick_file()
}

pub(crate) fn choose_shared_palette_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Native Shared/Custom SMW Palettes")
        .add_filter("Native SMW shared palette", &["smwpal", "pal"])
        .set_file_name("Shared Palette.smwpal")
        .save_file()
}

pub(crate) fn choose_graphics_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic graphics", &["lmgfx"])
        .pick_file()
}

pub(crate) fn choose_graphics_ownership() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Graphics ownership evidence", &["lmgfxown"])
        .pick_file()
}

pub(crate) fn choose_raw_graphics() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Insert GFX/ExGFX File")
        .add_filter("Raw SNES graphics", &["bin"])
        .pick_file()
}

pub(crate) fn choose_raw_graphics_save_path(slot: u16) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Extract GFX/ExGFX File")
        .add_filter("Raw SNES graphics", &["bin"])
        .set_file_name(raw_graphics_file_name(slot))
        .save_file()
}

pub(crate) fn choose_external_graphics_editor() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose External Graphics Editor Executable or Application")
        .pick_file()
}

pub(crate) fn choose_emulator() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose Emulator Executable or Application")
        .pick_file()
}

pub(crate) fn choose_native_title_recording() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Native Title-Screen Recording")
        .add_filter("Native title recording", &["lmtitle"])
        .pick_file()
}

pub(crate) fn choose_native_title_recording_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Native Title-Screen Recording")
        .add_filter("Native title recording", &["lmtitle"])
        .set_file_name("Title Recording.lmtitle")
        .save_file()
}

pub(crate) fn choose_zsnes_title_recording_state() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import ZSNES Title-Screen Recording State")
        .add_filter("ZSNES save state", &["zst"])
        .pick_file()
}

pub(crate) fn choose_zsnes_title_recording_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export ZSNES Title-Screen Recording State")
        .add_filter("ZSNES save state", &["zst"])
        .set_file_name("Title Recording.zst")
        .save_file()
}

pub(crate) fn choose_snes9x_title_recording_state() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Snes9x Title-Screen Recording State")
        .add_filter("Snes9x save state", &["000", "001", "002", "frz", "gz"])
        .pick_file()
}

pub(crate) fn choose_graphics_directory() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Extract All Standard GFX Files")
        .pick_folder()
}

pub(crate) fn choose_level_graphics_directory() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Save Current Level GFX Files")
        .pick_folder()
}

pub(crate) fn choose_graphics_import_directory() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Insert All Standard GFX Files")
        .pick_folder()
}

pub(crate) fn choose_exgraphics_directory() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Extract Installed ExGFX Files")
        .pick_folder()
}

pub(crate) fn choose_exgraphics_import_directory() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Insert ExGFX Files")
        .pick_folder()
}

pub(crate) fn choose_all_gfx_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Extract Joined Standard GFX")
        .add_filter("Joined standard graphics", &["bin"])
        .set_file_name("AllGFX.bin")
        .save_file()
}

pub(crate) fn choose_all_gfx_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Insert Joined Standard GFX")
        .add_filter("Joined standard graphics", &["bin"])
        .pick_file()
}

pub(crate) fn raw_graphics_file_name(slot: u16) -> String {
    let prefix = if slot <= 0x7f { "GFX" } else { "ExGFX" };
    format!("{prefix}{slot:02X}.bin")
}

pub(crate) fn choose_rats_ownership() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("RATS ownership evidence", &["lmrats"])
        .pick_file()
}

pub(crate) fn choose_map16_page_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic Map16 page", &["lm16page"])
        .pick_file()
}

pub(crate) fn choose_exanimation_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic ExAnimation", &["lmexan"])
        .pick_file()
}

pub(crate) fn choose_exanimation_save_path(slot: u16) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Portable Lunar Magic ExAnimation")
        .set_file_name(format!("overworld-{slot:03X}.lmexan"))
        .add_filter("Portable Lunar Magic ExAnimation", &["lmexan"])
        .save_file()
}

pub(crate) fn choose_exanimation_size_modes() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("ExAnimation size-mode table", &["bin", "dat"])
        .pick_file()
}

pub(crate) fn choose_complete_level_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic complete level", &["lmlevel"])
        .pick_file()
}

pub(crate) fn choose_native_level_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic native level", &["lmlvl"])
        .pick_file()
}

pub(crate) fn choose_native_level_assets_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic native level assets", &["lmnat"])
        .pick_file()
}

pub(crate) fn choose_map16_set_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic Map16 set", &["lm16set"])
        .pick_file()
}

pub(crate) fn choose_complete_map16_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic complete Map16", &["map16"])
        .pick_file()
}

pub(crate) fn choose_complete_map16_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic complete Map16", &["map16"])
        .set_file_name("AllMap16.map16")
        .save_file()
}

pub(crate) fn choose_selected_map16_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Selected Map16 Range")
        .add_filter("Lunar Magic selected Map16", &["map16"])
        .pick_file()
}

pub(crate) fn choose_selected_map16_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Selected Map16 Range")
        .add_filter("Lunar Magic selected Map16", &["map16"])
        .set_file_name("Map16Selection.map16")
        .save_file()
}

pub(crate) fn choose_legacy_map16_page_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Legacy Map16 Page Pair")
        .add_filter("Lunar Magic Map16Page", &["bin"])
        .set_file_name("Map16Page.bin")
        .pick_file()
}

pub(crate) fn choose_legacy_map16_page_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Legacy Map16 Page Pair")
        .add_filter("Lunar Magic Map16Page", &["bin"])
        .set_file_name("Map16Page.bin")
        .save_file()
}

pub(crate) fn choose_legacy_map16_foreground_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Legacy Foreground Map16 Pair")
        .add_filter("Lunar Magic Map16FG", &["bin"])
        .set_file_name("Map16FG.bin")
        .pick_file()
}

pub(crate) fn choose_legacy_map16_foreground_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Legacy Foreground Map16 Pair")
        .add_filter("Lunar Magic Map16FG", &["bin"])
        .set_file_name("Map16FG.bin")
        .save_file()
}

pub(crate) fn choose_legacy_map16_background_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Legacy Background Map16")
        .add_filter("Lunar Magic Map16BG", &["bin"])
        .set_file_name("Map16BG.bin")
        .pick_file()
}

pub(crate) fn choose_legacy_map16_background_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Legacy Background Map16")
        .add_filter("Lunar Magic Map16BG", &["bin"])
        .set_file_name("Map16BG.bin")
        .save_file()
}

pub(crate) fn choose_complete_overworld_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic complete overworld", &["lmow"])
        .pick_file()
}

pub(crate) fn choose_complete_overworld_save_path(slot: u16) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic complete overworld", &["lmow"])
        .set_file_name(format!("Overworld {slot:03X}.lmow"))
        .save_file()
}

pub(crate) fn choose_overworld_path_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic overworld paths", &["lmowpath"])
        .pick_file()
}

pub(crate) fn choose_overworld_metadata_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic overworld metadata", &["lmowmeta"])
        .pick_file()
}

pub(crate) fn choose_entity_appearance_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic entity appearances", &["lmentapp"])
        .pick_file()
}

pub(crate) fn choose_overworld_appearance_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic overworld appearances", &["lmowapp"])
        .add_filter("Lunar Magic native overworld sprite display", &["sscov"])
        .pick_file()
}

pub(crate) fn choose_native_overworld_sprite_sidecar() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Import Native Overworld Sprite Display Sidecars")
        .add_filter("Lunar Magic overworld sprite display", &["sscov"])
        .pick_file()
}

pub(crate) fn choose_native_overworld_sprite_sidecar_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Native Overworld Sprite Display Sidecars")
        .add_filter("Lunar Magic overworld sprite display", &["sscov"])
        .set_file_name("sprites.sscov")
        .save_file()
}

pub(crate) fn choose_layer3_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic Layer 3", &["lmlayer3"])
        .pick_file()
}

pub(crate) fn choose_mwl_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic level", &["mwl"])
        .pick_file()
}

pub(crate) fn choose_mwl_save_path(level: u16) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic level", &["mwl"])
        .set_file_name(format!("Level {level:03X}.mwl"))
        .save_file()
}

pub(crate) fn choose_mwl_batch_template() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic level", &["mwl"])
        .set_file_name("Levels.mwl")
        .save_file()
}

pub(crate) fn choose_level_image_save_path(level: u16) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Level image", &["png", "bmp"])
        .add_filter("PNG image", &["png"])
        .add_filter("Windows bitmap", &["bmp"])
        .set_file_name(format!("Level {level:03X}.png"))
        .save_file()
}

pub(crate) fn choose_level_bitmap_save_path(level: u16) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Level Bitmap")
        .add_filter("Windows bitmap", &["bmp"])
        .set_file_name(format!("Level {level:03X}.bmp"))
        .save_file()
}

pub(crate) fn choose_level_image_batch_template(extension: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export Multiple Level Images")
        .add_filter("Level image", &[extension])
        .set_file_name(format!("Levels.{extension}"))
        .save_file()
}

pub(crate) fn choose_mwl_directory() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

pub(crate) fn choose_expanded_settings_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic expanded settings record", &["bin", "lmexset"])
        .pick_file()
}

pub(crate) fn choose_custom_object_data() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic custom objects", &["mw0"])
        .pick_file()
}

pub(crate) fn choose_custom_object_descriptions() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic custom object descriptions", &["mw0t"])
        .pick_file()
}

pub(crate) fn choose_custom_sprite_data() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic custom sprite placements", &["mw2"])
        .pick_file()
}

pub(crate) fn choose_custom_sprite_descriptions() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic custom sprite descriptions", &["mwt"])
        .pick_file()
}

pub(crate) fn choose_sprite_length_table() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Sprite record length table", &["bin", "dat"])
        .pick_file()
}

pub(crate) fn choose_native_map16_sidecar() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic native Map16 sidecar", &["m16", "s16"])
        .pick_file()
}

pub(crate) fn choose_dsc_sidecar() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic custom display sidecar", &["dsc"])
        .pick_file()
}

pub(crate) fn choose_ssc_sidecar() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic custom sprite metadata", &["ssc"])
        .pick_file()
}

pub(crate) fn choose_osc_sidecar() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lunar Magic custom object metadata", &["osc"])
        .pick_file()
}

/// Reads a startup-supplied ROM before the native event loop exists.
pub(crate) fn read_rom(path: &Path) -> io::Result<Vec<u8>> {
    read_regular_bounded(path, MAX_ROM_FILE_LEN, "selected ROM")
}

pub(crate) fn read_regular_bounded(
    path: &Path,
    maximum: u64,
    description: &str,
) -> io::Result<Vec<u8>> {
    let path_metadata = fs::symlink_metadata(path)?;
    if !path_metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{description} is not a regular file"),
        ));
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{description} is not a regular file"),
        ));
    }
    if metadata.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{description} exceeds its application bound"),
        ));
    }
    let read_limit = maximum.saturating_add(1);
    let mut bytes = Vec::new();
    file.take(read_limit).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{description} exceeds its application bound"),
        ));
    }
    Ok(bytes)
}

pub(crate) fn read_regular_prefix(
    path: &Path,
    maximum: u64,
    description: &str,
) -> io::Result<Vec<u8>> {
    let path_metadata = fs::symlink_metadata(path)?;
    if !path_metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{description} is not a regular file"),
        ));
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{description} is not a regular file"),
        ));
    }
    let mut bytes = Vec::new();
    file.take(maximum).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
#[path = "dialogs_tests.rs"]
mod tests;
