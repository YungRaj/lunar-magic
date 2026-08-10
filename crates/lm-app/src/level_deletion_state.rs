use crate::{AppError, AppState, EditorMode, FrontendEffect};
use lm_project::{NativeLevelAssetsLayout, Project, RomMutation};

const ORIGINAL_TEST_LEVEL_SOURCE: usize = 0x19;

impl AppState {
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
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        let EditorMode::Level(level) = self.mode else {
            return Err(AppError::NoLevelView);
        };
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

        let before = source.logical_bytes().to_vec();
        let mut staged = Project::new(source);
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
        let mutation = RomMutation::between(profile.mapper, &before, staged.rom.logical_bytes())?;
        self.commit_rom_mutation(
            expected_revision,
            format!("Delete level {level:03X}"),
            &mutation,
        )
    }
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
}
