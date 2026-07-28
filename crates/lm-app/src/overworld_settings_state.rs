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
    use lm_profile::smw_us_v1_default_special_expanded_settings_record;
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
}
