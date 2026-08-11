use crate::{AppError, AppState, EditorMode, FrontendEffect};
use lm_project::{NativeLevelAssetsLayout, Project, RomMutation};

const ORIGINAL_TEST_LEVEL_SOURCE: usize = 0x19;
const LEVEL_DELETION_CHECKSUM_FIELD: usize = 0x7fdc;
const LEVEL_DELETION_COMPENSATION_RANGE: std::ops::Range<usize> = 0x7efc0..0x7f0a0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelDeletionPartition {
    pub modified: Vec<u16>,
    pub unmodified: Vec<u16>,
}

impl AppState {
    pub fn level_deletion_partition(&self) -> Result<LevelDeletionPartition, AppError> {
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let profile = self
            .revision_profile
            .as_ref()
            .ok_or(AppError::NoRevisionProfile)?;
        if profile.game != lm_rom::SupportedGame::SuperMarioWorld
            || profile.region != lm_rom::Region::NorthAmerica
            || profile.revision != 0
            || profile.mapper != lm_rom::Mapper::LoRom
        {
            return Err(AppError::LevelDeletion(
                "native multi-level deletion currently requires the authenticated SMW-US revision-0 LoROM family"
                    .into(),
            ));
        }
        let layout = profile
            .level_layout_for_rom(&project.rom)
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
        let mut partition = LevelDeletionPartition {
            modified: Vec::new(),
            unmodified: Vec::new(),
        };
        for level in 0..layout.layer1.entries {
            let expanded = crate::native_level_is_in_expanded_area(
                &project.rom,
                layout.mapper,
                layout.layer1,
                level,
            )
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
            let level = u16::try_from(level).map_err(|_| {
                AppError::LevelDeletion(
                    "standard level table exceeds the level-number domain".into(),
                )
            })?;
            if expanded {
                partition.modified.push(level);
            } else {
                partition.unmodified.push(level);
            }
        }
        Ok(partition)
    }

    #[must_use]
    pub fn current_level_deletion_available(&self) -> bool {
        let (Some(project), Some(profile), EditorMode::Level(level)) =
            (&self.project, &self.revision_profile, self.mode)
        else {
            return false;
        };
        if profile.game != lm_rom::SupportedGame::SuperMarioWorld
            || profile.region != lm_rom::Region::NorthAmerica
            || profile.revision != 0
            || profile.mapper != lm_rom::Mapper::LoRom
        {
            return false;
        }
        profile
            .level_layout_for_rom(&project.rom)
            .ok()
            .and_then(|layout| {
                crate::native_level_is_in_expanded_area(
                    &project.rom,
                    layout.mapper,
                    layout.layer1,
                    usize::from(level),
                )
                .ok()
            })
            .unwrap_or(false)
    }

    pub(crate) fn delete_current_level(
        &mut self,
        expected_revision: u64,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        let EditorMode::Level(level) = self.mode else {
            return Err(AppError::NoLevelView);
        };
        self.delete_levels(expected_revision, &[level], false)
    }

    pub(crate) fn delete_levels(
        &mut self,
        expected_revision: u64,
        requested_levels: &[u16],
        clear_original_level_area: bool,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        let mut levels = requested_levels.to_vec();
        levels.sort_unstable();
        levels.dedup();
        if levels.is_empty() {
            return Err(AppError::LevelDeletion(
                "select at least one expanded-area level to delete".into(),
            ));
        }
        let profile = self
            .revision_profile
            .clone()
            .ok_or(AppError::NoRevisionProfile)?;
        if profile.game != lm_rom::SupportedGame::SuperMarioWorld
            || profile.region != lm_rom::Region::NorthAmerica
            || profile.revision != 0
            || profile.mapper != lm_rom::Mapper::LoRom
        {
            return Err(AppError::LevelDeletion(
                "native level deletion currently requires the authenticated SMW-US revision-0 LoROM family"
                    .into(),
            ));
        }
        let source = self
            .project
            .as_ref()
            .ok_or(AppError::NoProject)?
            .rom
            .clone();
        let level_layout = profile
            .level_layout_for_rom(&source)
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
        let palette = profile
            .palette_installation
            .resolve(&source)
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?
            .ok_or_else(|| {
                AppError::LevelDeletion("per-level palette runtime is unavailable".into())
            })?;
        let exanimation = profile
            .exanimation_installation
            .resolve(&source)
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?
            .ok_or_else(|| {
                AppError::LevelDeletion("per-level ExAnimation runtime is unavailable".into())
            })?
            .resolve(&source)
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?
            .payload;
        let source_project = Project::new(source.clone());
        let expanded_settings =
            lm_profile::smw_us_v1_installed_expanded_settings_layout(&source_project)
                .map_err(|error| AppError::LevelDeletion(error.to_string()))?
                .or(profile.expanded_settings);
        let layer2 = lm_profile::smw_us_v1_layer2_layout(&source)
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
        let lfix3 = lm_profile::detect_smw_us_v1_current_lfix3_runtime(source.logical_bytes())
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?
            .is_some()
            .then(lm_profile::smw_us_v1_lfix3_level_fields_layout);
        let layout = NativeLevelAssetsLayout {
            level: level_layout,
            palette,
            exanimation,
            expanded_settings,
        };

        for &level in &levels {
            if usize::from(level) >= level_layout.layer1.entries {
                return Err(AppError::LevelDeletion(format!(
                    "level {level:03X} is outside the standard level table"
                )));
            }
        }
        let selected_levels = levels
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let mut deletes_all_unmodified_levels = true;
        for level in 0..level_layout.layer1.entries {
            let expanded = crate::native_level_is_in_expanded_area(
                &source,
                level_layout.mapper,
                level_layout.layer1,
                level,
            )
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
            if !expanded
                && !selected_levels.contains(&u16::try_from(level).map_err(|_| {
                    AppError::LevelDeletion(
                        "standard level table exceeds the level-number domain".into(),
                    )
                })?)
            {
                deletes_all_unmodified_levels = false;
                break;
            }
        }

        let before = source.logical_bytes().to_vec();
        let preserved_checksum = source
            .read(LEVEL_DELETION_CHECKSUM_FIELD, 4)
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?
            .to_vec();
        let mut staged = Project::new(source);
        for &level in &levels {
            staged
                .delete_native_level_assets_to_original_source(
                    format!("Delete level {level:03X}"),
                    layout,
                    Some(layer2),
                    Some(lm_profile::smw_us_v1_vanilla_entrance_layout()),
                    lfix3,
                    usize::from(level),
                    ORIGINAL_TEST_LEVEL_SOURCE,
                    0x7fdc,
                    0x00,
                )
                .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
        }
        let mut secondary_exits = staged
            .load_secondary_exit_table_detected(lm_profile::smw_us_v1_secondary_exit_locator())
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?
            .table;
        if clear_original_level_area {
            secondary_exits.entries.fill(Default::default());
            staged
                .save_installed_secondary_exit_table(
                    &secondary_exits,
                    lm_profile::smw_us_v1_secondary_exit_locator(),
                    &lm_profile::smw_us_v1_secondary_exit_allocation_policy(
                        staged.rom.logical_len(),
                    ),
                    0x7fdc,
                    0x00,
                )
                .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
            staged
                .clear_original_level_data_area("Clear original level data area", 0x7fdc)
                .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
        } else {
            if deletes_all_unmodified_levels {
                secondary_exits.entries.fill(Default::default());
            } else {
                for exit in &mut secondary_exits.entries {
                    if selected_levels.contains(&exit.destination_level) {
                        *exit = Default::default();
                    }
                }
            }
            let locator = lm_profile::smw_us_v1_secondary_exit_locator();
            let allocation =
                lm_profile::smw_us_v1_secondary_exit_allocation_policy(staged.rom.logical_len());
            if deletes_all_unmodified_levels {
                staged
                    .save_installed_secondary_exit_table(
                        &secondary_exits,
                        locator,
                        &allocation,
                        0x7fdc,
                        0x00,
                    )
                    .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
            } else {
                staged
                    .repack_installed_secondary_exit_table(
                        &secondary_exits,
                        locator,
                        &allocation,
                        0x7fdc,
                        0x00,
                    )
                    .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
            }
            preserve_level_deletion_checksum(&mut staged, &preserved_checksum)?;
        }
        let description = if clear_original_level_area {
            format!(
                "Delete {} {} and clear original level data area",
                levels.len(),
                if levels.len() == 1 { "level" } else { "levels" }
            )
        } else if levels.len() == 1 {
            format!("Delete level {:03X}", levels[0])
        } else {
            format!("Delete {} levels", levels.len())
        };
        let mutation = RomMutation::between(profile.mapper, &before, staged.rom.logical_bytes())?;
        self.commit_rom_mutation(expected_revision, description, &mutation)
    }
}

fn preserve_level_deletion_checksum(
    staged: &mut Project,
    preserved_checksum: &[u8],
) -> Result<(), AppError> {
    staged
        .rom
        .write(LEVEL_DELETION_CHECKSUM_FIELD, preserved_checksum)
        .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
    staged
        .rom
        .write(
            LEVEL_DELETION_COMPENSATION_RANGE.start,
            &vec![0; LEVEL_DELETION_COMPENSATION_RANGE.len()],
        )
        .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
    let target =
        lm_rom::SnesChecksum::decode(staged.rom.logical_bytes(), LEVEL_DELETION_CHECKSUM_FIELD)
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
    let baseline =
        lm_rom::compute_snes_checksum(staged.rom.logical_bytes(), LEVEL_DELETION_CHECKSUM_FIELD)
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
    let required = target.checksum.wrapping_sub(baseline.checksum);
    let full_bytes = usize::from(required / u16::from(u8::MAX));
    let remainder =
        u8::try_from(required % u16::from(u8::MAX)).expect("checksum remainder is below 255");
    if full_bytes >= LEVEL_DELETION_COMPENSATION_RANGE.len() {
        let computed = lm_rom::compute_snes_checksum(
            staged.rom.logical_bytes(),
            LEVEL_DELETION_CHECKSUM_FIELD,
        )
        .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
        staged
            .rom
            .write(LEVEL_DELETION_CHECKSUM_FIELD, &computed.encoded())
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
        return Ok(());
    }
    let mut compensation = vec![0; LEVEL_DELETION_COMPENSATION_RANGE.len()];
    compensation[..full_bytes].fill(0xff);
    compensation[full_bytes] = remainder;
    staged
        .rom
        .write(LEVEL_DELETION_COMPENSATION_RANGE.start, &compensation)
        .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
    let computed =
        lm_rom::compute_snes_checksum(staged.rom.logical_bytes(), LEVEL_DELETION_CHECKSUM_FIELD)
            .map_err(|error| AppError::LevelDeletion(error.to_string()))?;
    if computed != target {
        return Err(AppError::LevelDeletion(format!(
            "level-deletion checksum compensation mismatch: expected {target:?}, computed {computed:?}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use lm_project::{
        ExAnimationRomLayout, InstalledExAnimationRomLayout, InstalledLayout, LevelPointerTable,
    };
    use lm_rom::{Mapper, RomImage};
    use std::fs;

    fn installed_app() -> (AppState, Vec<u8>) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let image = RomImage::from_bytes(bytes.clone()).unwrap();
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
        let mut app = AppState::default();
        app.load_rom(bytes.clone()).unwrap();
        app.revision_profile = Some(profile);
        app.dispatch(Command::SelectLevel(0)).unwrap();
        (app, bytes)
    }

    #[test]
    fn application_delete_is_one_revision_and_undo_restores_every_byte() {
        let (mut app, before) = installed_app();
        assert!(app.current_level_deletion_available());
        let effects = app
            .dispatch(Command::DeleteCurrentLevel { rev: 0 })
            .unwrap();
        assert_eq!(app.project_revision(), 1);
        assert_eq!(app.status, "Delete level 000");
        assert!(matches!(
            effects.as_slice(),
            [FrontendEffect::ProjectChanged {
                description,
                mode: EditorMode::Level(0),
                revision: 1,
            }] if description == "Delete level 000"
        ));
        let layout = lm_profile::smw_us_v1_vanilla_level_layout();
        assert_eq!(
            layout
                .layer1
                .read_snes_pointer(&app.project.as_ref().unwrap().rom, 0)
                .unwrap()
                .get(),
            0x068000
        );
        let reopened = RomImage::from_bytes(app.project.as_ref().unwrap().save_snapshot()).unwrap();
        assert_eq!(
            layout.layer1.read_snes_pointer(&reopened, 0).unwrap().get(),
            0x068000
        );
        assert_eq!(
            lm_rom::SnesChecksum::decode(reopened.logical_bytes(), 0x7fdc).unwrap(),
            lm_rom::compute_snes_checksum(reopened.logical_bytes(), 0x7fdc).unwrap()
        );
        assert!(!app.current_level_deletion_available());
        assert!(app.dispatch(Command::Undo).unwrap().len() == 1);
        assert_eq!(app.project.as_ref().unwrap().rom.as_file_bytes(), before);
        assert!(app.current_level_deletion_available());
    }

    #[test]
    fn stale_delete_rejects_before_mutation() {
        let (mut app, before) = installed_app();
        assert!(matches!(
            app.dispatch(Command::DeleteCurrentLevel { rev: 1 }),
            Err(AppError::StaleProjectRevision { .. })
        ));
        assert_eq!(app.project.as_ref().unwrap().rom.as_file_bytes(), before);
        assert_eq!(app.project_revision(), 0);
    }

    #[test]
    fn batch_delete_redirects_modified_and_original_levels_in_one_revision() {
        let (mut app, before) = installed_app();
        let effects = app
            .dispatch(Command::DeleteLevels {
                rev: 0,
                levels: vec![1, 0, 1],
                clear_original_level_area: false,
            })
            .unwrap();
        assert_eq!(app.project_revision(), 1);
        assert_eq!(app.status, "Delete 2 levels");
        assert!(matches!(
            effects.as_slice(),
            [FrontendEffect::ProjectChanged {
                description,
                revision: 1,
                ..
            }] if description == "Delete 2 levels"
        ));

        let layout = lm_profile::smw_us_v1_vanilla_level_layout();
        let replacement = layout
            .layer1
            .read_snes_pointer(
                &app.project.as_ref().unwrap().rom,
                ORIGINAL_TEST_LEVEL_SOURCE,
            )
            .unwrap();
        for level in [0, 1] {
            assert_eq!(
                layout
                    .layer1
                    .read_snes_pointer(&app.project.as_ref().unwrap().rom, level)
                    .unwrap(),
                replacement
            );
        }
        let reopened = RomImage::from_bytes(app.project.as_ref().unwrap().save_snapshot()).unwrap();
        assert_eq!(
            lm_rom::SnesChecksum::decode(reopened.logical_bytes(), 0x7fdc).unwrap(),
            lm_rom::compute_snes_checksum(reopened.logical_bytes(), 0x7fdc).unwrap()
        );
        if let Some(path) = std::env::var_os("LM_DELETE_PAIR_DIAGNOSTIC_OUTPUT") {
            std::fs::write(path, app.project.as_ref().unwrap().rom.as_file_bytes()).unwrap();
        }
        assert_eq!(
            lm_oracle::sha256_hex(app.project.as_ref().unwrap().rom.as_file_bytes()),
            "5a675ad29e8e85ede57ff55efe47d86c3d18651242ccd19a075579aa77003596"
        );
        assert_eq!(app.dispatch(Command::Undo).unwrap().len(), 1);
        assert_eq!(app.project.as_ref().unwrap().rom.as_file_bytes(), before);
    }

    #[test]
    fn deletion_partition_covers_every_standard_slot_exactly_once() {
        let (app, _) = installed_app();
        let partition = app.level_deletion_partition().unwrap();
        assert_eq!(partition.modified, vec![0]);
        assert_eq!(partition.unmodified.len(), 0x1ff);
        assert!(!partition.unmodified.contains(&0));
        let mut complete = partition.modified;
        complete.extend(partition.unmodified);
        complete.sort_unstable();
        assert_eq!(complete, (0..=0x1ff).collect::<Vec<_>>());
    }

    #[test]
    fn batch_delete_rejects_empty_and_out_of_range_sets_atomically() {
        let (mut app, before) = installed_app();
        for levels in [Vec::new(), vec![0x200]] {
            assert!(matches!(
                app.dispatch(Command::DeleteLevels {
                    rev: 0,
                    levels,
                    clear_original_level_area: false,
                }),
                Err(AppError::LevelDeletion(_))
            ));
            assert_eq!(app.project_revision(), 0);
            assert_eq!(app.project.as_ref().unwrap().rom.as_file_bytes(), before);
        }
    }

    #[test]
    fn unmodified_batch_with_clear_matches_the_complete_lunar_magic_oracle() {
        let (mut app, before) = installed_app();
        let effects = app
            .dispatch(Command::DeleteLevels {
                rev: 0,
                levels: (1..=0x1ff).collect(),
                clear_original_level_area: true,
            })
            .unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(app.project_revision(), 1);
        if let Some(path) = std::env::var_os("LM_DELETE_CLEAR_DIAGNOSTIC_OUTPUT") {
            std::fs::write(path, app.project.as_ref().unwrap().rom.as_file_bytes()).unwrap();
        }
        assert_eq!(
            lm_oracle::sha256_hex(app.project.as_ref().unwrap().rom.as_file_bytes()),
            "72b5bceeba2f764c2cf996c5133f84bb90433637743092e8b03daa44243a96d1"
        );
        assert_eq!(app.dispatch(Command::Undo).unwrap().len(), 1);
        assert_eq!(app.project.as_ref().unwrap().rom.as_file_bytes(), before);
    }

    #[test]
    fn modified_and_all_batch_modes_match_complete_lunar_magic_oracles() {
        for (levels, expected, diagnostic_name) in [
            (
                vec![0],
                "08b427c9547c1881085a042d3ef341b6642d8139a912a099d6d9726815213ee3",
                "modified.smc",
            ),
            (
                (0..=0x1ff).collect(),
                "03af33fa60385f81d09f89a442866e7c4e2dcde2fc83393f0d46d701f423d7ad",
                "all.smc",
            ),
            (
                (1..=0x1ff).collect(),
                "659d06f8662e716a98950eb82c0e84f66a89d521f63ee39e46b10d8ed31a819a",
                "unmodified.smc",
            ),
        ] {
            let (mut app, before) = installed_app();
            app.dispatch(Command::DeleteLevels {
                rev: 0,
                levels,
                clear_original_level_area: false,
            })
            .unwrap();
            if let Some(directory) = std::env::var_os("LM_DELETE_DIAGNOSTIC_DIRECTORY") {
                std::fs::write(
                    std::path::PathBuf::from(directory).join(diagnostic_name),
                    app.project.as_ref().unwrap().rom.as_file_bytes(),
                )
                .unwrap();
            }
            assert_eq!(
                lm_oracle::sha256_hex(app.project.as_ref().unwrap().rom.as_file_bytes()),
                expected
            );
            assert_eq!(app.dispatch(Command::Undo).unwrap().len(), 1);
            assert_eq!(app.project.as_ref().unwrap().rom.as_file_bytes(), before);
        }
    }
}
