use crate::{AppError, AppState, FrontendEffect};
use lm_level::{ExpandedLevelSettingsRecord, ExpandedOverworldSettings};
use lm_overworld::OverworldLayer3SettingsTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
    load_smw_us_v1_overworld_settings,
    smw_us_v1_expanded_settings_installation_plan_with_overworld_settings,
    smw_us_v1_expanded_settings_layout,
};
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_overworld_layer3_settings(
        &mut self,
        expected_revision: u64,
        settings: &OverworldLayer3SettingsTable,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        let records = std::array::from_fn(|index| {
            ExpandedLevelSettingsRecord::from_encoded(*settings.maps[index].encoded())
        });
        self.replace_native_overworld_settings(
            expected_revision,
            &ExpandedOverworldSettings { records },
        )
    }

    pub(crate) fn replace_native_overworld_settings(
        &mut self,
        expected_revision: u64,
        settings: &ExpandedOverworldSettings,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        self.ensure_project_revision_capacity()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::NativeOverworldSettingsIdentityMismatch);
        }
        let installed = load_smw_us_v1_overworld_settings(project)
            .map_err(|error| AppError::NativeOverworldSettingsStorage(error.to_string()))?
            .installed;
        if installed {
            project.save_expanded_overworld_settings(
                SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
                settings,
                smw_us_v1_expanded_settings_layout(),
                SMW_US_V1_CHECKSUM_FIELD,
            )?;
        } else {
            project.install_relocatable_patch(
                &smw_us_v1_expanded_settings_installation_plan_with_overworld_settings(Some(
                    settings,
                ))?,
            )?;
        }
        let reopened = project.load_expanded_overworld_settings(
            SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
            smw_us_v1_expanded_settings_layout(),
        )?;
        if reopened != *settings {
            return Err(AppError::NativeOverworldSettingsReopenMismatch);
        }
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld settings".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use lm_overworld::{
        NativeOverworldLevelNameTable, OverworldLevelName, OverworldMessage, Submap,
    };
    use lm_profile::{
        smw_us_v1_default_special_expanded_settings_record, smw_us_v1_overworld_level_name_locator,
        smw_us_v1_overworld_level_name_runtime, smw_us_v1_overworld_message_patch_locator,
        smw_us_v1_overworld_player_start_layout,
    };
    use lm_rom::{RomImage, detect_identity};
    use std::path::PathBuf;

    #[test]
    fn install_is_one_application_revision_and_undo_step() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut settings = ExpandedOverworldSettings {
            records: std::array::from_fn(|_| smw_us_v1_default_special_expanded_settings_record()),
        };
        settings.records[6].set_word(11, 0x4567).unwrap();
        app.dispatch(Command::ReplaceNativeOverworldSettings {
            rev: 0,
            settings: Box::new(settings.clone()),
        })
        .unwrap();
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        assert_eq!(
            app.project()
                .unwrap()
                .load_expanded_overworld_settings(
                    SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
                    smw_us_v1_expanded_settings_layout()
                )
                .unwrap(),
            settings
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn semantic_layer3_command_installs_and_reopens_exact_records() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let defaults = ExpandedOverworldSettings {
            records: std::array::from_fn(|_| smw_us_v1_default_special_expanded_settings_record()),
        };
        let mut bytes = [0; OverworldLayer3SettingsTable::ENCODED_LEN];
        for (index, record) in defaults.records.iter().enumerate() {
            let start = index * record.encoded().len();
            bytes[start..start + record.encoded().len()].copy_from_slice(record.encoded());
        }
        let mut settings = OverworldLayer3SettingsTable::decode(&bytes).unwrap();
        settings.maps[4].set_uses_custom_tilemap(true);
        settings.maps[4].set_tilemap_file(0x345).unwrap();
        app.dispatch(Command::ReplaceNativeOverworldLayer3Settings {
            rev: 0,
            settings: Box::new(settings.clone()),
        })
        .unwrap();
        assert_eq!(
            app.project()
                .unwrap()
                .load_overworld_layer3_settings(
                    lm_profile::smw_us_v1_overworld_layer3_settings_layout()
                )
                .unwrap(),
            settings
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn complete_metadata_workflow_updates_installed_runtimes_across_header_variants() {
        let physical = crate::test_support::pristine_smw_us_rom_bytes();
        let physical_image = RomImage::from_bytes(physical.clone()).unwrap();
        let variants = [physical, physical_image.logical_bytes().to_vec()];
        let mut logical_results = Vec::new();

        for original in variants {
            let original_image = RomImage::from_bytes(original.clone()).unwrap();
            let original_header = original_image.copier_header_bytes().map(<[u8]>::to_vec);
            let mut app = AppState::default();
            app.load_rom(original.clone()).unwrap();

            let mut settings = ExpandedOverworldSettings {
                records: std::array::from_fn(|_| {
                    smw_us_v1_default_special_expanded_settings_record()
                }),
            };
            settings.records[2].set_word(9, 0x1234).unwrap();
            app.dispatch(Command::ReplaceNativeOverworldSettings {
                rev: 0,
                settings: Box::new(settings.clone()),
            })
            .unwrap();

            let mut names = NativeOverworldLevelNameTable {
                names: (0..100)
                    .map(|slot| OverworldLevelName {
                        level: NativeOverworldLevelNameTable::level_for_slot(slot).unwrap(),
                        tiles: [u8::try_from(slot).unwrap(); OverworldLevelName::TILE_COUNT],
                        raw_flags: 0,
                    })
                    .collect(),
            };
            app.dispatch(Command::ReplaceNativeOverworldLevelNames {
                rev: 1,
                table: Box::new(names.clone()),
            })
            .unwrap();

            let mut messages = vec![OverworldMessage([0x1f; 144]); 200];
            app.dispatch(Command::ReplaceNativeOverworldMessages {
                rev: 2,
                messages: messages.clone(),
            })
            .unwrap();

            let mut starts = app
                .project()
                .unwrap()
                .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())
                .unwrap();
            starts.starts[1].submap = Submap::StarWorld;
            starts.starts[1].x = 0x98;
            starts.starts[1].y = 0xb8;
            app.dispatch(Command::ReplaceNativeOverworldPlayerStarts {
                rev: 3,
                starts: Box::new(starts.clone()),
            })
            .unwrap();

            settings.records[6].set_word(11, 0x4567).unwrap();
            app.dispatch(Command::ReplaceNativeOverworldSettings {
                rev: 4,
                settings: Box::new(settings.clone()),
            })
            .unwrap();
            names.names[0].tiles[0] ^= 0x3f;
            app.dispatch(Command::ReplaceNativeOverworldLevelNames {
                rev: 5,
                table: Box::new(names.clone()),
            })
            .unwrap();
            messages[199].0[143] = 0x20;
            app.dispatch(Command::ReplaceNativeOverworldMessages {
                rev: 6,
                messages: messages.clone(),
            })
            .unwrap();

            let project = app.project().unwrap();
            assert_eq!(
                project
                    .load_expanded_overworld_settings(
                        SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
                        smw_us_v1_expanded_settings_layout()
                    )
                    .unwrap(),
                settings
            );
            assert_eq!(
                project
                    .load_overworld_level_names_detected(
                        smw_us_v1_overworld_level_name_locator(),
                        smw_us_v1_overworld_level_name_runtime()
                    )
                    .unwrap()
                    .table,
                names
            );
            assert_eq!(
                project
                    .load_expanded_overworld_messages_detected(
                        smw_us_v1_overworld_message_patch_locator()
                    )
                    .unwrap()
                    .messages,
                messages
            );
            assert_eq!(
                project
                    .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())
                    .unwrap(),
                starts
            );
            let result = RomImage::from_bytes(project.save_snapshot()).unwrap();
            assert_eq!(
                result.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(detect_identity(&result).unwrap().checksum_matches());
            logical_results.push(result.logical_bytes().to_vec());

            for _ in 0..7 {
                app.dispatch(Command::Undo).unwrap();
            }
            assert_eq!(app.project().unwrap().save_snapshot(), original);
        }
        assert_eq!(logical_results[0], logical_results[1]);
    }
}
