use crate::{
    EditorMode, PreparedRomCommit, ProfiledControllerSnapshot, RevisionProfileControllers,
};
use lm_level::MwlFile;
use lm_project::MwlNativeLevel;
use lm_rom::RomImage;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Visible regular MWLs selected from one directory plus Lunar Magic's hidden-file skip count.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlDirectoryListing {
    pub paths: Vec<PathBuf>,
    pub hidden_skipped: usize,
}

/// Enumerates the files accepted by Lunar Magic's multi-level insert command.
///
/// # Errors
///
/// Returns an I/O error when the directory or one of its entries cannot be inspected.
pub fn discover_mwl_directory(directory: &Path) -> std::io::Result<MwlDirectoryListing> {
    let mut paths = Vec::new();
    let mut hidden_skipped = 0;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mwl"))
        {
            continue;
        }
        if !entry.file_type()?.is_file() {
            continue;
        }
        if mwl_path_is_hidden(&path, &entry.metadata()?) {
            hidden_skipped += 1;
            continue;
        }
        paths.push(path);
    }
    paths.sort_by(|left, right| {
        left.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase()
            .cmp(
                &right
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_ascii_lowercase(),
            )
            .then_with(|| left.cmp(right))
    });
    Ok(MwlDirectoryListing {
        paths,
        hidden_skipped,
    })
}

#[cfg(windows)]
fn mwl_path_is_hidden(_path: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x2 != 0
}

#[cfg(not(windows))]
fn mwl_path_is_hidden(path: &Path, _metadata: &fs::Metadata) -> bool {
    path.file_name()
        .is_some_and(|name| name.as_encoded_bytes().starts_with(b"."))
}

/// Decodes one MWL and prepares an atomic import into the level declared by its header.
///
/// The returned commit remains bound to `profiled.snapshot.revision`; a caller must dispatch it
/// before preparing the next directory entry from a fresh application snapshot.
///
/// # Errors
///
/// Rejects malformed or oversized MWL framing, an out-of-profile target, unavailable installed
/// layouts, and any cross-domain save preflight failure.
pub fn prepare_declared_mwl_import(
    profiled: &ProfiledControllerSnapshot,
    bytes: &[u8],
    search: Range<usize>,
) -> Result<(u16, PreparedRomCommit), String> {
    if bytes.len() > MwlFile::MAX_FILE_BYTES {
        return Err(format!(
            "binary MWL level exceeds {} bytes",
            MwlFile::MAX_FILE_BYTES
        ));
    }
    let file = MwlFile::decode(bytes).map_err(|error| error.to_string())?;
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let (layout, options) = profiled
        .profile
        .native_level_assets_save_plan_for_rom(
            search.clone(),
            &image,
            profiled.snapshot.identity.internal_header_offset,
        )
        .map_err(|error| error.to_string())?;
    let Some((_, layer2_options)) = profiled
        .profile
        .level_layer2_save_plan(
            search,
            image.logical_len(),
            profiled.snapshot.identity.internal_header_offset,
        )
        .map_err(|error| error.to_string())?
    else {
        return Err("active revision profile has no native Layer 2 layout".into());
    };
    let source = MwlNativeLevel::decode(
        &file,
        &profiled.profile.sprite_lengths,
        layout.exanimation.maximum_records,
        &profiled.profile.exanimation_double_size_modes,
    )
    .map_err(|error| error.to_string())?;
    let level = source.header.level_number();
    if usize::from(level) >= profiled.profile.level.layer1.entries {
        return Err(format!(
            "MWL target level {level:03X} is outside the active profile"
        ));
    }
    let ownership = lm_graphics::PaletteOwnership::editable(layout.palette.colors_per_palette);
    let mut snapshot = profiled.snapshot.clone();
    snapshot.mode = EditorMode::Level(level);
    let controller = profiled
        .profile
        .decode_native_level_assets(&snapshot, ownership)
        .map_err(|error| error.to_string())?;
    let prepared = controller
        .prepare_smw_us_v1_installed_mwl_import(&source, &options, &layer2_options)
        .map_err(|error| error.to_string())?;
    Ok((level, prepared))
}

#[cfg(test)]
mod tests {
    use super::discover_mwl_directory;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn directory_selection_matches_visible_file_contract() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "lm-batch-mwl-directory-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("Level 001.mwl"), b"one").unwrap();
        fs::write(directory.join("Level 000.MWL"), b"zero").unwrap();
        fs::write(directory.join(".Level 002.mwl"), b"hidden").unwrap();
        fs::write(directory.join("notes.txt"), b"ignored").unwrap();
        fs::create_dir(directory.join("Level 003.mwl")).unwrap();

        let listing = discover_mwl_directory(&directory).unwrap();
        assert_eq!(
            listing
                .paths
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["Level 000.MWL", "Level 001.mwl"]
        );
        assert_eq!(listing.hidden_skipped, 1);
        fs::remove_dir_all(directory).unwrap();
    }
}
