use eframe::egui;
use lm_app::RevisionProfile;
use lm_level::{LegacyLevelHeader, SpriteLengthTable};
use lm_rom::{Region, RomImage, SupportedGame};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LevelGraphicsExportMode {
    ChooseNewDirectory,
    ReplaceExtracted,
    ReplaceJoined,
}

pub(crate) fn take_level_graphics_export_shortcut(ui: &mut egui::Ui) -> bool {
    ui.input_mut(|input| {
        !input.modifiers.any() && input.consume_key(egui::Modifiers::NONE, egui::Key::F9)
    })
}

pub(crate) struct ExtractedGraphicsPaths {
    pub(crate) standard_directory: PathBuf,
    pub(crate) exgraphics_directory: PathBuf,
    pub(crate) required_existing: Vec<PathBuf>,
}

pub(crate) struct ExtractedJoinedGraphicsPaths {
    pub(crate) all_gfx_path: PathBuf,
    pub(crate) exgraphics_directory: PathBuf,
    pub(crate) required_existing: Vec<PathBuf>,
}

pub(crate) struct CurrentLevelGraphicsAssignments {
    pub(crate) foreground_background: Vec<usize>,
    pub(crate) sprites: Vec<usize>,
    pub(crate) super_graphics_bypass_enabled: bool,
}

pub(crate) const LUNAR_MAGIC_ALL_GFX_FILE_SIZES: [usize; 0x34] = {
    let mut sizes = [0x1000; 0x34];
    sizes[0x27] = 0x0c00;
    sizes[0x28] = 0x0800;
    sizes[0x29] = 0x0800;
    sizes[0x2a] = 0x0800;
    sizes[0x2b] = 0x0800;
    sizes[0x2f] = 0x0400;
    sizes[0x30] = 0x0800;
    sizes[0x31] = 0x0800;
    sizes[0x32] = 0x5d00;
    sizes[0x33] = 0x3000;
    sizes
};

pub(crate) fn extracted_graphics_paths(
    rom_path: &Path,
    file_numbers: &[usize],
) -> Result<ExtractedGraphicsPaths, String> {
    let parent = rom_path
        .parent()
        .ok_or("the open ROM path has no parent directory")?;
    let directory = parent.join("Graphics");
    let metadata = std::fs::symlink_metadata(&directory).map_err(|error| {
        format!(
            "the extracted Graphics directory {} is unavailable: {error}",
            directory.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(format!(
            "the extracted Graphics path must be a directory: {}",
            directory.display()
        ));
    }
    let mut required_existing = (0..=0x33)
        .map(|file| directory.join(format!("GFX{file:02X}.bin")))
        .collect::<Vec<_>>();
    let exgraphics_directory = parent.join("ExGraphics");
    if file_numbers
        .iter()
        .any(|file| *file >= 0x34 && *file != 0x7f)
    {
        let metadata = std::fs::symlink_metadata(&exgraphics_directory).map_err(|error| {
            format!(
                "the extracted ExGraphics directory {} is unavailable: {error}",
                exgraphics_directory.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(format!(
                "the extracted ExGraphics path must be a directory: {}",
                exgraphics_directory.display()
            ));
        }
        for file in file_numbers
            .iter()
            .copied()
            .filter(|file| *file >= 0x34 && *file != 0x7f)
        {
            let width = if file < 0x100 { 2 } else { 3 };
            let path = exgraphics_directory.join(format!("ExGFX{file:0width$X}.bin"));
            if !required_existing.contains(&path) {
                required_existing.push(path);
            }
        }
    }
    Ok(ExtractedGraphicsPaths {
        standard_directory: directory,
        exgraphics_directory,
        required_existing,
    })
}

pub(crate) fn extracted_joined_graphics_paths(
    rom_path: &Path,
    file_numbers: &[usize],
) -> Result<ExtractedJoinedGraphicsPaths, String> {
    let parent = rom_path
        .parent()
        .ok_or("the open ROM path has no parent directory")?;
    let graphics_directory = parent.join("Graphics");
    let metadata = std::fs::symlink_metadata(&graphics_directory).map_err(|error| {
        format!(
            "the extracted Graphics directory {} is unavailable: {error}",
            graphics_directory.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(format!(
            "the extracted Graphics path must be a directory: {}",
            graphics_directory.display()
        ));
    }
    let all_gfx_path = graphics_directory.join("AllGFX.bin");
    let exgraphics_directory = parent.join("ExGraphics");
    let mut required_existing = vec![all_gfx_path.clone()];
    if file_numbers
        .iter()
        .any(|file| *file >= 0x34 && *file != 0x7f)
    {
        let metadata = std::fs::symlink_metadata(&exgraphics_directory).map_err(|error| {
            format!(
                "the extracted ExGraphics directory {} is unavailable: {error}",
                exgraphics_directory.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(format!(
                "the extracted ExGraphics path must be a directory: {}",
                exgraphics_directory.display()
            ));
        }
        for file in file_numbers
            .iter()
            .copied()
            .filter(|file| *file >= 0x34 && *file != 0x7f)
        {
            let width = if file < 0x100 { 2 } else { 3 };
            let path = exgraphics_directory.join(format!("ExGFX{file:0width$X}.bin"));
            if !required_existing.contains(&path) {
                required_existing.push(path);
            }
        }
    }
    Ok(ExtractedJoinedGraphicsPaths {
        all_gfx_path,
        exgraphics_directory,
        required_existing,
    })
}

pub(crate) fn current_level_graphics_files(
    image: &RomImage,
    profile: &RevisionProfile,
    level: u16,
    special_world_passed: bool,
) -> Result<Vec<usize>, String> {
    let assignments =
        current_level_graphics_assignments(image, profile, level, special_world_passed)?;
    Ok(collapse_duplicate_files(
        assignments
            .foreground_background
            .into_iter()
            .chain(assignments.sprites)
            .filter(|file| *file != 0x7f)
            .collect(),
    ))
}

pub(crate) fn current_level_graphics_assignments(
    image: &RomImage,
    profile: &RevisionProfile,
    level: u16,
    special_world_passed: bool,
) -> Result<CurrentLevelGraphicsAssignments, String> {
    let project = lm_project::Project::new(image.clone());
    let level_layout = profile
        .level_layout_for_rom(image)
        .map_err(|error| error.to_string())?;
    let loaded_level = project
        .load_level_slot(usize::from(level), level_layout, &profile.sprite_lengths)
        .map_err(|error| format!("cannot load level {level:03X} graphics settings: {error}"))?;
    let (foreground_background, sprites, super_graphics_bypass_enabled) =
        if let Some(settings_layout) = profile.expanded_settings {
            let settings = project
                .load_expanded_level_settings(usize::from(level), settings_layout)
                .map_err(|error| {
                    format!("cannot load level {level:03X} expanded graphics settings: {error}")
                })?;
            let selection = lm_level::ExpandedLevelHeader::from(&settings).super_graphics_bypass();
            if selection.enabled {
                let mut sprites = selection.sprites.map(usize::from).to_vec();
                if special_world_passed {
                    sprites[1] = 0x7f;
                }
                (
                    selection.foreground_background.map(usize::from).to_vec(),
                    sprites,
                    true,
                )
            } else {
                let (foreground, sprites) = legacy_level_graphics_assignments(
                    image,
                    profile,
                    loaded_level.layer1.header,
                    special_world_passed,
                )?;
                (foreground, sprites, false)
            }
        } else {
            let (foreground, sprites) = legacy_level_graphics_assignments(
                image,
                profile,
                loaded_level.layer1.header,
                special_world_passed,
            )?;
            (foreground, sprites, false)
        };
    Ok(CurrentLevelGraphicsAssignments {
        foreground_background,
        sprites,
        super_graphics_bypass_enabled,
    })
}

fn legacy_level_graphics_assignments(
    image: &RomImage,
    profile: &RevisionProfile,
    header: LegacyLevelHeader,
    special_world_passed: bool,
) -> Result<(Vec<usize>, Vec<usize>), String> {
    if profile.game != SupportedGame::SuperMarioWorld
        || profile.region != Region::NorthAmerica
        || profile.revision != 0
    {
        return Err(format!(
            "legacy graphics assignment tables are not recovered for profile {}",
            profile.name
        ));
    }
    let mut foreground = lm_profile::smw_us_v1_object_tileset_graphics_files(
        image,
        usize::from(header.object_tileset()),
    )
    .map_err(|error| error.to_string())?
    .to_vec();
    foreground.extend([0x7f, 0x7f]);
    let mut sprites = lm_profile::smw_us_v1_sprite_tileset_graphics_files(
        image,
        usize::from(header.sprite_tileset()),
    )
    .map_err(|error| error.to_string())?
    .to_vec();
    if special_world_passed {
        sprites[1] = 0x7f;
    }
    Ok((foreground, sprites))
}

pub(crate) fn pristine_current_level_graphics_files(
    image: &RomImage,
    level: u16,
    special_world_passed: bool,
) -> Result<Vec<usize>, String> {
    let assignments =
        pristine_current_level_graphics_assignments(image, level, special_world_passed)?;
    Ok(collapse_duplicate_files(
        assignments
            .foreground_background
            .into_iter()
            .chain(assignments.sprites)
            .collect(),
    ))
}

pub(crate) fn pristine_current_level_graphics_assignments(
    image: &RomImage,
    level: u16,
    special_world_passed: bool,
) -> Result<CurrentLevelGraphicsAssignments, String> {
    let project = lm_project::Project::new(image.clone());
    let loaded_level = project
        .load_level_slot(
            usize::from(level),
            lm_profile::smw_us_v1_vanilla_level_layout(),
            &SpriteLengthTable::standard(),
        )
        .map_err(|error| format!("cannot load level {level:03X} graphics settings: {error}"))?;
    let mut foreground_background = lm_profile::smw_us_v1_object_tileset_graphics_files(
        image,
        usize::from(loaded_level.layer1.header.object_tileset()),
    )
    .map_err(|error| error.to_string())?
    .to_vec();
    foreground_background.extend([0x7f, 0x7f]);
    let mut sprites = lm_profile::smw_us_v1_sprite_tileset_graphics_files(
        image,
        usize::from(loaded_level.layer1.header.sprite_tileset()),
    )
    .map_err(|error| error.to_string())?
    .to_vec();
    if special_world_passed {
        sprites[1] = 0x7f;
    }
    Ok(CurrentLevelGraphicsAssignments {
        foreground_background,
        sprites,
        super_graphics_bypass_enabled: false,
    })
}

fn collapse_duplicate_files(files: Vec<usize>) -> Vec<usize> {
    let mut seen = std::collections::HashSet::with_capacity(files.len());
    files
        .into_iter()
        .filter(|file| *file != 0x7f && seen.insert(*file))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn original_level_graphics_publication_shortcut_is_unmodified_f9() {
        for (modifiers, expected) in [
            (egui::Modifiers::NONE, true),
            (egui::Modifiers::CTRL, false),
            (egui::Modifiers::SHIFT, false),
            (egui::Modifiers::ALT, false),
        ] {
            let context = egui::Context::default();
            let mut taken = false;
            let _ = context.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::F9,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    modifiers,
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        taken = take_level_graphics_export_shortcut(ui);
                    });
                },
            );
            assert_eq!(taken, expected, "unexpected F9 routing for {modifiers:?}");
        }
    }

    #[test]
    fn extracted_graphics_paths_use_the_rom_sibling_directory_and_complete_standard_set() {
        let root = std::env::temp_dir().join(format!(
            "lm-level-gfx-paths-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        let graphics = root.join("Graphics");
        std::fs::create_dir_all(&graphics).unwrap();
        let paths = extracted_graphics_paths(&root.join("game.smc"), &[]).unwrap();
        assert_eq!(paths.standard_directory, graphics);
        assert_eq!(paths.required_existing.len(), 0x34);
        assert_eq!(
            paths.required_existing.first().unwrap(),
            &graphics.join("GFX00.bin")
        );
        assert_eq!(
            paths.required_existing.last().unwrap(),
            &graphics.join("GFX33.bin")
        );
        assert_eq!(paths.exgraphics_directory, root.join("ExGraphics"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracted_bypass_files_use_the_sibling_exgraphics_directory_and_canonical_widths() {
        let root = std::env::temp_dir().join(format!(
            "lm-level-exgfx-paths-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("Graphics")).unwrap();
        std::fs::create_dir_all(root.join("ExGraphics")).unwrap();
        let paths =
            extracted_graphics_paths(&root.join("game.smc"), &[0x14, 0x80, 0x123, 0x80, 0x7f])
                .unwrap();
        assert_eq!(paths.required_existing.len(), 0x36);
        assert_eq!(
            &paths.required_existing[0x34..],
            &[
                root.join("ExGraphics/ExGFX80.bin"),
                root.join("ExGraphics/ExGFX123.bin")
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn joined_paths_require_allgfx_and_only_selected_extended_files() {
        let root = std::env::temp_dir().join(format!(
            "lm-level-allgfx-paths-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("Graphics")).unwrap();
        std::fs::create_dir_all(root.join("ExGraphics")).unwrap();
        let paths =
            extracted_joined_graphics_paths(&root.join("game.smc"), &[0x14, 0x80, 0x123, 0x7f])
                .unwrap();
        assert_eq!(
            paths.required_existing,
            [
                root.join("Graphics/AllGFX.bin"),
                root.join("ExGraphics/ExGFX80.bin"),
                root.join("ExGraphics/ExGFX123.bin")
            ]
        );
        assert_eq!(LUNAR_MAGIC_ALL_GFX_FILE_SIZES.len(), 0x34);
        assert_eq!(LUNAR_MAGIC_ALL_GFX_FILE_SIZES[0x27], 0x0c00);
        assert_eq!(LUNAR_MAGIC_ALL_GFX_FILE_SIZES[0x32], 0x5d00);
        assert_eq!(
            LUNAR_MAGIC_ALL_GFX_FILE_SIZES.iter().sum::<usize>(),
            0x36d00
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pristine_level_105_resolves_the_exact_vanilla_fg_bg_and_sprite_set() {
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        assert_eq!(
            pristine_current_level_graphics_files(&image, 0x105, false).unwrap(),
            [0x14, 0x17, 0x1b, 0x15, 0x00, 0x01, 0x13, 0x20]
        );
    }

    #[test]
    fn special_world_view_omits_the_normal_sp2_assignment_before_publication() {
        let image = RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap();
        assert_eq!(
            pristine_current_level_graphics_files(&image, 0x105, true).unwrap(),
            [0x14, 0x17, 0x1b, 0x15, 0x00, 0x13, 0x20]
        );
        let assignments = pristine_current_level_graphics_assignments(&image, 0x105, true).unwrap();
        assert_eq!(
            assignments.foreground_background,
            [0x14, 0x17, 0x1b, 0x15, 0x7f, 0x7f]
        );
        assert_eq!(assignments.sprites, [0x00, 0x7f, 0x13, 0x20]);
        assert!(!assignments.super_graphics_bypass_enabled);
    }

    #[test]
    fn legacy_assignments_retain_six_foreground_and_four_sprite_source_slots() {
        let mut profile = lm_profile::test_support::profile();
        profile.game = SupportedGame::SuperMarioWorld;
        profile.region = Region::NorthAmerica;
        profile.revision = 0;
        let mut bytes = vec![0; 0x8000];
        bytes[lm_profile::SMW_US_V1_OBJECT_TILESET_GRAPHICS_OFFSET
            ..lm_profile::SMW_US_V1_OBJECT_TILESET_GRAPHICS_OFFSET + 4]
            .copy_from_slice(&[0x14, 0x17, 0x1b, 0x15]);
        bytes[lm_profile::SMW_US_V1_SPRITE_TILESET_GRAPHICS_OFFSET
            ..lm_profile::SMW_US_V1_SPRITE_TILESET_GRAPHICS_OFFSET + 4]
            .copy_from_slice(&[0x00, 0x01, 0x13, 0x20]);
        let image = RomImage::from_bytes(bytes).unwrap();

        let (foreground, sprites) = legacy_level_graphics_assignments(
            &image,
            &profile,
            LegacyLevelHeader::default(),
            false,
        )
        .unwrap();
        assert_eq!(foreground, [0x14, 0x17, 0x1b, 0x15, 0x7f, 0x7f]);
        assert_eq!(sprites, [0x00, 0x01, 0x13, 0x20]);

        let (_, special_world_sprites) =
            legacy_level_graphics_assignments(&image, &profile, LegacyLevelHeader::default(), true)
                .unwrap();
        assert_eq!(special_world_sprites, [0x00, 0x7f, 0x13, 0x20]);
    }

    #[test]
    fn duplicate_assignments_collapse_at_first_occurrence_for_file_publication() {
        assert_eq!(
            collapse_duplicate_files(vec![0x14, 0x17, 0x14, 0x00, 0x17]),
            [0x14, 0x17, 0x00]
        );
    }

    #[test]
    fn internal_file_7f_is_ignored_before_publication_like_the_original_writer() {
        assert_eq!(collapse_duplicate_files(vec![0x7f, 0x14, 0x7f]), [0x14]);
    }

    #[test]
    fn retained_lunar_magic_f9_oracle_binds_separate_and_joined_publication() {
        let fixture =
            include_str!("../../../docs/oracle-work/lm363/pristine-us/level-gfx-f9/oracle.tsv");
        let fields = fixture
            .lines()
            .skip(1)
            .map(|line| line.split_once('\t').expect("oracle row has two columns"))
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(fields["level"], "105");
        assert_eq!(fields["graphics_editor_command"], "232A");
        assert_eq!(fields["shortcut_virtual_key"], "78");
        assert_eq!(
            fields["confirmation_title"],
            "Save level GFX to Graphics folder?"
        );
        assert_eq!(
            fields["separate_assignment_order"],
            "14,17,1B,15,00,01,13,20"
        );
        assert_eq!(fields["separate_unchanged_file_count"], "44");
        assert_eq!(fields["separate_missing_file"], "GFX33.bin");
        assert_eq!(fields["separate_missing_changed_file_count"], "0");
        assert_eq!(fields["joined_toggle_command"], "24BD");
        assert_eq!(fields["joined_total_size"], "36D00");
        assert_eq!(
            usize::from_str_radix(fields["joined_total_size"], 16).unwrap(),
            LUNAR_MAGIC_ALL_GFX_FILE_SIZES.iter().sum::<usize>()
        );

        let mut offset = 0usize;
        let mut recovered_ranges = Vec::new();
        for (file, size) in LUNAR_MAGIC_ALL_GFX_FILE_SIZES.iter().copied().enumerate() {
            if [0x00, 0x01, 0x13, 0x14, 0x15, 0x17, 0x1b, 0x20].contains(&file) {
                recovered_ranges.push(format!("{file:02X}:{offset:05X}:{size:05X}"));
            }
            offset += size;
        }
        assert_eq!(recovered_ranges.join(","), fields["joined_changed_ranges"]);
        assert_eq!(
            fields["joined_observed_sha256"],
            fields["joined_expected_sha256"]
        );
    }
}
