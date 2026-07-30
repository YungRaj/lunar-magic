use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

pub(crate) const MAX_ROM_FILE_LEN: u64 = 32 * 1024 * 1024 + 512;

pub(crate) fn choose_rom() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("SNES ROM", &["smc", "sfc"])
        .pick_file()
}

pub(crate) fn choose_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("SNES ROM", &["smc", "sfc"])
        .set_file_name("edited.smc")
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

pub(crate) fn choose_palette_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic palette", &["lmpal"])
        .pick_file()
}

pub(crate) fn choose_palette_ownership() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Palette ownership evidence", &["lmpalown"])
        .pick_file()
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

pub(crate) fn choose_complete_overworld_document() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Portable Lunar Magic complete overworld", &["lmow"])
        .pick_file()
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
        .pick_file()
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

#[cfg(test)]
#[path = "dialogs_tests.rs"]
mod tests;
