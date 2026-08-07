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
    use super::{discover_mwl_directory, prepare_declared_mwl_import};
    use crate::{ControllerSnapshot, EditorMode, ProfiledControllerSnapshot};
    use lm_level::{MwlFile, SpriteLengthTable};
    use lm_project::{
        ExAnimationRomLayout, InstalledExAnimationRomLayout, InstalledLayout, LevelPointerTable,
        MwlNativeLevel, Project,
    };
    use lm_rom::{Mapper, RomImage, detect_identity};
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn installed_profiled(rom_bytes: Vec<u8>, revision: u64) -> ProfiledControllerSnapshot {
        let image = RomImage::from_bytes(rom_bytes.clone()).unwrap();
        let mut profile = lm_profile::test_support::profile();
        profile.mapper = Mapper::LoRom;
        profile.level = lm_profile::smw_us_v1_vanilla_level_layout();
        profile.level.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
        profile.layer2 = Some(lm_profile::smw_us_v1_layer2_layout(&image).unwrap());
        profile.palette = lm_profile::smw_us_v1_custom_palette_layout();
        profile.palette_installation = InstalledLayout::Unconditional(profile.palette);
        profile.exanimation = ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x8138b,
                entries: 0x200,
                stride: 3,
            },
            maximum_records: 32,
            maximum_encoded_len: 0x8000,
        };
        profile.exanimation_installation =
            InstalledLayout::Unconditional(InstalledExAnimationRomLayout {
                payload: profile.exanimation,
                pointer_presence_mask: 0x00ff_0000,
                pointer_locator: None,
            });
        profile.exanimation_feature_installation = InstalledLayout::Absent;
        profile.expanded_settings = Some(lm_profile::smw_us_v1_expanded_settings_layout());
        profile.map16.mapper = Mapper::LoRom;
        profile.graphics.mapper = Mapper::LoRom;
        profile.overworld.layers.mapper = Mapper::LoRom;
        profile.overworld.event_reveals.mapper = Mapper::LoRom;
        profile.overworld.endpoints.mapper = Mapper::LoRom;
        profile.overworld.messages.mapper = Mapper::LoRom;
        profile.overworld.sprites.mapper = Mapper::LoRom;
        profile.overworld.palette.mapper = Mapper::LoRom;
        profile.overworld.animation.mapper = Mapper::LoRom;
        profile.validate().unwrap();
        ProfiledControllerSnapshot {
            snapshot: ControllerSnapshot {
                revision,
                mode: EditorMode::Level(0),
                identity: detect_identity(&image).unwrap(),
                document_path: None,
                rom_bytes,
            },
            profile,
        }
    }

    fn installed_fixture(headered: bool) -> ProfiledControllerSnapshot {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let physical = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let physical_image = RomImage::from_bytes(physical.clone()).unwrap();
        installed_profiled(
            if headered {
                physical
            } else {
                physical_image.logical_bytes().to_vec()
            },
            17,
        )
    }

    fn edited_authentic_mwl(level: u16) -> Vec<u8> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let file = MwlFile::decode(
            &fs::read(root.join(format!(
                "oracle-work/lm363/pristine-us/levels/Level {level:03X}.mwl"
            )))
            .unwrap(),
        )
        .unwrap();
        let mut source =
            MwlNativeLevel::decode(&file, &SpriteLengthTable::standard(), 32, &[false; 256])
                .unwrap();
        let prior_background = source.layer1.header.background_color();
        source
            .layer1
            .header
            .set_background_color((prior_background + 1) & 7)
            .unwrap();
        source
            .encode(&SpriteLengthTable::standard(), &[false; 256])
            .unwrap()
            .encode()
            .unwrap()
    }

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

    #[test]
    fn installed_binary_mwl_import_is_identical_across_copier_header_variants() {
        let encoded = edited_authentic_mwl(0);

        let mut results = Vec::new();
        for headered in [true, false] {
            let profiled = installed_fixture(headered);
            let original_image = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone()).unwrap();
            let original_header = original_image.copier_header_bytes().map(<[u8]>::to_vec);
            let (level, prepared) = prepare_declared_mwl_import(
                &profiled,
                &encoded,
                0x080000..original_image.logical_len(),
            )
            .unwrap();
            assert_eq!(level, 0);
            assert_eq!(prepared.expected_revision, 17);
            assert!(!prepared.mutation.is_empty());

            let mut project = Project::new(original_image);
            project
                .apply_mutation(&prepared.description, &prepared.mutation)
                .unwrap();
            assert_eq!(
                project.rom.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(detect_identity(&project.rom).unwrap().checksum_matches());
            results.push(project.rom.logical_bytes().to_vec());
        }
        assert_eq!(results[0], results[1]);
    }

    #[test]
    fn installed_directory_import_variants_continue_after_failure_and_match_logically() {
        let inputs = [
            edited_authentic_mwl(0),
            b"not an MWL".to_vec(),
            edited_authentic_mwl(1),
        ];
        let mut final_logical = Vec::new();
        for headered in [true, false] {
            let mut profiled = installed_fixture(headered);
            let original_header = RomImage::from_bytes(profiled.snapshot.rom_bytes.clone())
                .unwrap()
                .copier_header_bytes()
                .map(<[u8]>::to_vec);
            let mut inserted = 0;
            let mut failed = 0;
            for bytes in &inputs {
                let before = profiled.snapshot.rom_bytes.clone();
                match prepare_declared_mwl_import(
                    &profiled,
                    bytes,
                    0x080000..RomImage::from_bytes(before.clone()).unwrap().logical_len(),
                ) {
                    Ok((level, prepared)) => {
                        assert_eq!(usize::from(level), inserted);
                        let mut project = Project::new(RomImage::from_bytes(before).unwrap());
                        project
                            .apply_mutation(&prepared.description, &prepared.mutation)
                            .unwrap();
                        let revision = profiled.snapshot.revision + 1;
                        profiled = installed_profiled(project.save_snapshot(), revision);
                        inserted += 1;
                    }
                    Err(_) => {
                        assert_eq!(profiled.snapshot.rom_bytes, before);
                        failed += 1;
                    }
                }
            }
            assert_eq!((inserted, failed), (2, 1));
            let result = RomImage::from_bytes(profiled.snapshot.rom_bytes).unwrap();
            assert_eq!(
                result.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(detect_identity(&result).unwrap().checksum_matches());
            final_logical.push(result.logical_bytes().to_vec());
        }
        assert_eq!(final_logical[0], final_logical[1]);
    }
}
