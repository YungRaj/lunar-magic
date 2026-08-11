#[cfg(test)]
use crate::Command;
use crate::{AppError, AppState, FrontendEffect};
use lm_profile::{SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_lunar_magic_metadata_layout};

impl AppState {
    pub(crate) fn set_use_fastrom_addressing(
        &mut self,
        expected_revision: u64,
        enabled: bool,
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
        let layout = smw_us_v1_lunar_magic_metadata_layout();
        let metadata = project
            .load_lunar_magic_rom_metadata(layout)?
            .ok_or(AppError::LunarMagicMetadataIdentityMismatch)?;
        if metadata.use_fastrom_addressing() == enabled {
            return Ok(Vec::new());
        }
        project.save_lunar_magic_rom_metadata(
            &metadata.with_use_fastrom_addressing(enabled),
            layout,
            SMW_US_V1_CHECKSUM_FIELD,
        )?;
        self.advance_project_revision()?;
        let description = if enabled {
            "Enable FastROM addressing"
        } else {
            "Disable FastROM addressing"
        }
        .to_owned();
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
    use std::{fs, path::PathBuf};

    fn metadata(app: &AppState) -> lm_rom::LunarMagicRomMetadata {
        app.project()
            .unwrap()
            .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())
            .unwrap()
            .unwrap()
    }

    #[test]
    fn rom_scoped_fastrom_option_preserves_the_permanent_lock_and_undoes_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();

        let before = metadata(&app);
        assert!(!before.use_fastrom_addressing());
        assert!(!before.fastrom_ever_enabled());
        assert!(!before.fastrom_speed_patch_applied());

        app.dispatch(Command::SetUseFastRomAddressing {
            rev: 0,
            enabled: true,
        })
        .unwrap();
        let enabled = metadata(&app);
        assert!(enabled.use_fastrom_addressing());
        assert!(enabled.fastrom_ever_enabled());
        assert!(!enabled.fastrom_speed_patch_applied());

        app.dispatch(Command::SetUseFastRomAddressing {
            rev: 1,
            enabled: false,
        })
        .unwrap();
        let disabled = metadata(&app);
        assert!(!disabled.use_fastrom_addressing());
        assert!(disabled.fastrom_ever_enabled());
        assert!(!disabled.fastrom_speed_patch_applied());

        app.dispatch(Command::Undo).unwrap();
        assert_eq!(metadata(&app), enabled);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn fastrom_option_is_revision_checked_and_idempotent() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        assert!(
            app.dispatch(Command::SetUseFastRomAddressing {
                rev: 1,
                enabled: true,
            })
            .is_err()
        );
        assert!(
            app.dispatch(Command::SetUseFastRomAddressing {
                rev: 0,
                enabled: false,
            })
            .unwrap()
            .is_empty()
        );
        assert_eq!(app.controller_snapshot().unwrap().revision, 0);
    }
}
