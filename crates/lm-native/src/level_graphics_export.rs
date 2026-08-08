use eframe::egui;
use lm_app::RevisionProfile;
use lm_level::{LegacyLevelHeader, SpriteLengthTable};
use lm_rom::{Region, RomImage, SupportedGame};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LevelGraphicsExportMode {
    ChooseNewDirectory,
    ReplaceExtracted,
}

pub(crate) fn take_level_graphics_export_shortcut(ui: &mut egui::Ui) -> bool {
    ui.input_mut(|input| {
        !input.modifiers.any() && input.consume_key(egui::Modifiers::NONE, egui::Key::F9)
    })
}

pub(crate) fn extracted_graphics_paths(rom_path: &Path) -> Result<(PathBuf, Vec<PathBuf>), String> {
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
    let required = (0..=0x33)
        .map(|file| directory.join(format!("GFX{file:02X}.bin")))
        .collect::<Vec<_>>();
    Ok((directory, required))
}

pub(crate) fn current_level_graphics_files(
    image: &RomImage,
    profile: &RevisionProfile,
    level: u16,
    special_world_passed: bool,
) -> Result<Vec<usize>, String> {
    let project = lm_project::Project::new(image.clone());
    let level_layout = profile
        .level_layout_for_rom(image)
        .map_err(|error| error.to_string())?;
    let loaded_level = project
        .load_level_slot(usize::from(level), level_layout, &profile.sprite_lengths)
        .map_err(|error| format!("cannot load level {level:03X} graphics settings: {error}"))?;
    let files = if let Some(settings_layout) = profile.expanded_settings {
        let settings = project
            .load_expanded_level_settings(usize::from(level), settings_layout)
            .map_err(|error| {
                format!("cannot load level {level:03X} expanded graphics settings: {error}")
            })?;
        let selection = lm_level::ExpandedLevelHeader::from(&settings).super_graphics_bypass();
        if selection.enabled {
            level_graphics_files(
                selection.foreground_background.map(usize::from),
                selection.sprites.map(usize::from),
                special_world_passed,
            )
        } else {
            legacy_level_graphics_files(
                image,
                profile,
                loaded_level.layer1.header,
                special_world_passed,
            )?
        }
    } else {
        legacy_level_graphics_files(
            image,
            profile,
            loaded_level.layer1.header,
            special_world_passed,
        )?
    };
    Ok(collapse_duplicate_files(files))
}

pub(crate) fn pristine_current_level_graphics_files(
    image: &RomImage,
    level: u16,
    special_world_passed: bool,
) -> Result<Vec<usize>, String> {
    let project = lm_project::Project::new(image.clone());
    let loaded_level = project
        .load_level_slot(
            usize::from(level),
            lm_profile::smw_us_v1_vanilla_level_layout(),
            &SpriteLengthTable::standard(),
        )
        .map_err(|error| format!("cannot load level {level:03X} graphics settings: {error}"))?;
    let foreground = lm_profile::smw_us_v1_object_tileset_graphics_files(
        image,
        usize::from(loaded_level.layer1.header.object_tileset()),
    )
    .map_err(|error| error.to_string())?;
    let sprites = lm_profile::smw_us_v1_sprite_tileset_graphics_files(
        image,
        usize::from(loaded_level.layer1.header.sprite_tileset()),
    )
    .map_err(|error| error.to_string())?;
    Ok(collapse_duplicate_files(level_graphics_files(
        foreground,
        sprites,
        special_world_passed,
    )))
}

pub(crate) fn legacy_level_graphics_files(
    image: &RomImage,
    profile: &RevisionProfile,
    header: LegacyLevelHeader,
    special_world_passed: bool,
) -> Result<Vec<usize>, String> {
    if profile.game != SupportedGame::SuperMarioWorld
        || profile.region != Region::NorthAmerica
        || profile.revision != 0
    {
        return Err(format!(
            "legacy level graphics assignment tables are not recovered for profile {}",
            profile.name
        ));
    }
    let foreground = lm_profile::smw_us_v1_object_tileset_graphics_files(
        image,
        usize::from(header.object_tileset()),
    )
    .map_err(|error| error.to_string())?;
    let sprites = lm_profile::smw_us_v1_sprite_tileset_graphics_files(
        image,
        usize::from(header.sprite_tileset()),
    )
    .map_err(|error| error.to_string())?;
    Ok(level_graphics_files(
        foreground,
        sprites,
        special_world_passed,
    ))
}

fn level_graphics_files<const FOREGROUND: usize>(
    foreground_background: [usize; FOREGROUND],
    sprites: [usize; 4],
    special_world_passed: bool,
) -> Vec<usize> {
    foreground_background
        .into_iter()
        .chain(
            sprites
                .into_iter()
                .enumerate()
                .filter_map(|(slot, file)| (!special_world_passed || slot != 1).then_some(file)),
        )
        .collect()
}

fn collapse_duplicate_files(files: Vec<usize>) -> Vec<usize> {
    let mut seen = std::collections::HashSet::with_capacity(files.len());
    files
        .into_iter()
        .filter(|file| seen.insert(*file))
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
        let (actual_directory, paths) = extracted_graphics_paths(&root.join("game.smc")).unwrap();
        assert_eq!(actual_directory, graphics);
        assert_eq!(paths.len(), 0x34);
        assert_eq!(paths.first().unwrap(), &graphics.join("GFX00.bin"));
        assert_eq!(paths.last().unwrap(), &graphics.join("GFX33.bin"));
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
    }

    #[test]
    fn duplicate_assignments_collapse_at_first_occurrence_for_file_publication() {
        assert_eq!(
            collapse_duplicate_files(vec![0x14, 0x17, 0x14, 0x00, 0x17]),
            [0x14, 0x17, 0x00]
        );
    }
}
