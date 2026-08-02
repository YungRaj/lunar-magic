use crate::{EditorMode, ProfiledControllerSnapshot, RevisionProfileControllers};
use lm_project::LevelPointerTable;
use lm_rom::{Mapper, RomImage, snes_to_pc};
use std::path::{Path, PathBuf};

/// Selection mode used by Lunar Magic's multi-level MWL exporter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MwlBatchExportMode {
    All,
    Modified,
}

/// One complete MWL payload and the native level number used in its output name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlBatchExportDocument {
    pub level: u16,
    pub bytes: Vec<u8>,
}

/// Materializes every selected MWL from one immutable profiled ROM snapshot.
///
/// # Errors
///
/// Returns a diagnostic when the installed palette/runtime layout cannot be resolved, a selected
/// level cannot be decoded or encoded, or a Layer 1 pointer is malformed.
pub fn export_smw_us_v1_installed_mwl_batch(
    profiled: &ProfiledControllerSnapshot,
    mode: MwlBatchExportMode,
) -> Result<Vec<MwlBatchExportDocument>, String> {
    export_smw_us_v1_installed_mwl_batch_until(profiled, mode, || false).map(|documents| {
        documents.expect("an export with a false cancellation predicate cannot be cancelled")
    })
}

/// Materializes selected MWLs, stopping between levels when cancellation is requested.
///
/// A `None` result means cancellation won before another level was started. Callers should check
/// the same predicate once more before publishing the returned batch so cancellation cannot race
/// the materialization/publication boundary.
///
/// # Errors
///
/// Returns the same diagnostics as [`export_smw_us_v1_installed_mwl_batch`].
pub fn export_smw_us_v1_installed_mwl_batch_until(
    profiled: &ProfiledControllerSnapshot,
    mode: MwlBatchExportMode,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<Vec<MwlBatchExportDocument>>, String> {
    if cancelled() {
        return Ok(None);
    }
    let image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
        .map_err(|error| error.to_string())?;
    let palette = profiled
        .profile
        .palette_installation
        .resolve(&image)
        .map_err(|error| error.to_string())?
        .ok_or("active revision profile has no installed per-level palette")?;
    let ownership = lm_graphics::PaletteOwnership::editable(palette.colors_per_palette);
    let level_count = profiled.profile.level.layer1.entries;
    let mut documents = Vec::with_capacity(level_count);
    for slot in 0..level_count {
        if cancelled() {
            return Ok(None);
        }
        if mode == MwlBatchExportMode::Modified
            && !native_level_is_in_expanded_area(
                &image,
                profiled.profile.mapper,
                profiled.profile.level.layer1,
                slot,
            )?
        {
            continue;
        }
        let level = u16::try_from(slot).map_err(|error| error.to_string())?;
        let mut snapshot = profiled.snapshot.clone();
        snapshot.mode = EditorMode::Level(level);
        let controller = profiled
            .profile
            .decode_native_level_assets(&snapshot, ownership.clone())
            .map_err(|error| error.to_string())?;
        let bytes = controller
            .export_smw_us_v1_installed_mwl()
            .map_err(|error| error.to_string())?
            .encode(
                &profiled.profile.sprite_lengths,
                &profiled.profile.exanimation_double_size_modes,
            )
            .and_then(|file| file.encode().map_err(Into::into))
            .map_err(|error: lm_project::MwlNativeLevelError| error.to_string())?;
        documents.push(MwlBatchExportDocument { level, bytes });
    }
    Ok(Some(documents))
}

/// Publishes a complete numbered MWL batch without replacing any existing destination.
///
/// # Errors
///
/// Rejects a template without a file name, aliased/colliding destinations, and staging or
/// publication failures. A failed publication rolls back files created by this call.
pub fn publish_mwl_batch_new(
    template: &Path,
    documents: &[MwlBatchExportDocument],
) -> Result<usize, String> {
    if documents.is_empty() {
        return Ok(0);
    }
    let owned = documents
        .iter()
        .map(|document| {
            Ok((
                mwl_batch_output_path(template, document.level)?,
                document.bytes.as_slice(),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let references = owned
        .iter()
        .map(|(path, bytes)| (path.as_path(), *bytes))
        .collect::<Vec<_>>();
    crate::file_persistence::write_new_group(&references).map_err(|error| error.to_string())?;
    Ok(documents.len())
}

/// Applies Lunar Magic's `base %03X.mwl` naming rule to one output slot.
///
/// # Errors
///
/// Rejects a template without a file-name component.
pub fn mwl_batch_output_path(template: &Path, level: u16) -> Result<PathBuf, String> {
    let stem = template
        .file_stem()
        .ok_or("MWL batch export template requires a file name")?;
    let mut name = stem.to_os_string();
    name.push(format!(" {level:03X}.mwl"));
    Ok(template.with_file_name(name))
}

/// Reports whether a level's Layer 1 payload is stored beyond the original 512-KiB ROM area.
///
/// Lunar Magic 3.63 uses this definition for its “modified levels only” batch exporters.
///
/// # Errors
///
/// Returns a diagnostic for an invalid table entry, SNES pointer, or mapper conversion.
pub fn native_level_is_in_expanded_area(
    image: &RomImage,
    mapper: Mapper,
    layer1: LevelPointerTable,
    level: usize,
) -> Result<bool, String> {
    let pointer = layer1
        .read_snes_pointer(image, level)
        .map_err(|error| error.to_string())?;
    let offset = snes_to_pc(mapper, pointer.get()).map_err(|error| error.to_string())?;
    Ok(offset >= lm_profile::SMW_US_V1_ORIGINAL_LOGICAL_LEN)
}

#[cfg(test)]
mod tests {
    use super::{
        MwlBatchExportDocument, mwl_batch_output_path, native_level_is_in_expanded_area,
        publish_mwl_batch_new,
    };
    use lm_rom::RomImage;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lm-mwl-batch-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn numbered_path_matches_lunar_magic_extension_stripping() {
        assert_eq!(
            mwl_batch_output_path(Path::new("/tmp/My Export.mwl"), 0x105).unwrap(),
            Path::new("/tmp/My Export 105.mwl")
        );
        assert_eq!(
            mwl_batch_output_path(Path::new("/tmp/My Export"), 0x00a).unwrap(),
            Path::new("/tmp/My Export 00A.mwl")
        );
        assert!(mwl_batch_output_path(Path::new("/"), 0).is_err());
    }

    #[test]
    fn publication_uses_numbered_paths_and_rolls_back_on_collision() {
        let directory = temporary_directory("publication");
        let template = directory.join("Export.mwl");
        let documents = [
            MwlBatchExportDocument {
                level: 0,
                bytes: b"zero".to_vec(),
            },
            MwlBatchExportDocument {
                level: 1,
                bytes: b"one".to_vec(),
            },
        ];
        fs::write(directory.join("Export 001.mwl"), b"occupied").unwrap();
        assert!(publish_mwl_batch_new(&template, &documents).is_err());
        assert!(!directory.join("Export 000.mwl").exists());
        assert_eq!(
            fs::read(directory.join("Export 001.mwl")).unwrap(),
            b"occupied"
        );
        fs::remove_file(directory.join("Export 001.mwl")).unwrap();
        assert_eq!(publish_mwl_batch_new(&template, &documents).unwrap(), 2);
        assert_eq!(fs::read(directory.join("Export 000.mwl")).unwrap(), b"zero");
        assert_eq!(fs::read(directory.join("Export 001.mwl")).unwrap(), b"one");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn empty_publication_is_a_successful_no_op() {
        let directory = temporary_directory("empty");
        assert_eq!(
            publish_mwl_batch_new(&directory.join("Export.mwl"), &[]).unwrap(),
            0
        );
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn modified_level_predicate_matches_live_lunar_magic_fixture() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = RomImage::from_bytes(
            fs::read(
                root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
            )
            .unwrap(),
        )
        .unwrap();
        let after = RomImage::from_bytes(
            fs::read(
                root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
            )
            .unwrap(),
        )
        .unwrap();
        let layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let pristine = (0..layout.layer1.entries)
            .filter(|level| {
                native_level_is_in_expanded_area(&before, layout.mapper, layout.layer1, *level)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let installed = (0..layout.layer1.entries)
            .filter(|level| {
                native_level_is_in_expanded_area(&after, layout.mapper, layout.layer1, *level)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(pristine.is_empty());
        assert_eq!(installed, [0]);
    }
}
