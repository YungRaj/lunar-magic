#[cfg(test)]
use crate::Command;
use crate::{AppError, AppState, FrontendEffect};
use lm_profile::{SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_lunar_magic_metadata_layout};
use lm_rats::AllocationPolicy;
use lm_rom::{Mapper, Region, SupportedGame};

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

    pub(crate) fn apply_fastrom_patch(
        &mut self,
        expected_revision: u64,
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
            return Err(AppError::FastRomPatchIdentityMismatch);
        }
        match lm_profile::detect_smw_us_v1_fastrom_patch(project.rom.logical_bytes())? {
            lm_profile::SmwUsV1FastRomPatchState::Installed => {
                return Err(AppError::FastRomPatchAlreadyInstalled);
            }
            lm_profile::SmwUsV1FastRomPatchState::Absent => {}
        }
        let metadata = project
            .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())?
            .ok_or(AppError::FastRomPatchIdentityMismatch)?;
        if !metadata.use_fastrom_addressing() || !metadata.fastrom_ever_enabled() {
            return Err(AppError::FastRomPatchIdentityMismatch);
        }

        let bytes = project.rom.logical_bytes();
        let allocation_start = lm_rats::scan(bytes)
            .into_iter()
            .filter(|block| block.header_offset >= 0x08_0000)
            .map(|block| block.payload.end)
            .max()
            .unwrap_or(0x08_0000)
            .min(bytes.len());
        let plan = lm_profile::smw_us_v1_fastrom_patch_plan(
            bytes,
            AllocationPolicy::lorom(allocation_start..bytes.len()),
            SMW_US_V1_CHECKSUM_FIELD,
        )?;
        project.install_relocatable_patch_with_expansion_retry(&plan, 0x40_0000)?;
        self.advance_project_revision()?;
        let description = "Apply FastROM speed patch".to_owned();
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

    #[test]
    fn fastrom_patch_installs_reopens_and_undoes_after_the_addressing_option() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        assert!(app.dispatch(Command::ApplyFastRomPatch { rev: 0 }).is_err());
        app.dispatch(Command::SetUseFastRomAddressing {
            rev: 0,
            enabled: true,
        })
        .unwrap();
        app.dispatch(Command::ApplyFastRomPatch { rev: 1 }).unwrap();
        assert_eq!(
            lm_profile::detect_smw_us_v1_fastrom_patch(app.project().unwrap().rom.logical_bytes()),
            Ok(lm_profile::SmwUsV1FastRomPatchState::Installed)
        );
        let patched = metadata(&app);
        assert!(patched.use_fastrom_addressing());
        assert!(patched.fastrom_ever_enabled());
        assert!(patched.fastrom_speed_patch_applied());
        let reopened = lm_project::Project::open_supported(
            lm_rom::RomImage::from_bytes(app.project().unwrap().save_snapshot()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            lm_profile::detect_smw_us_v1_fastrom_patch(reopened.rom.logical_bytes()),
            Ok(lm_profile::SmwUsV1FastRomPatchState::Installed)
        );
        app.dispatch(Command::Undo).unwrap();
        assert!(metadata(&app).use_fastrom_addressing());
        assert_eq!(
            lm_profile::detect_smw_us_v1_fastrom_patch(app.project().unwrap().rom.logical_bytes()),
            Ok(lm_profile::SmwUsV1FastRomPatchState::Absent)
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
