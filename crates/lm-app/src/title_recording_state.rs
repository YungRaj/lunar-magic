use crate::{AppError, AppState, FrontendEffect};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_TITLE_RECORDING_RECLAIM_FILL,
    smw_us_v1_title_recording_allocation_policy, smw_us_v1_title_recording_locator,
    smw_us_v1_title_recording_recorder_allocation_policy,
    smw_us_v1_title_recording_recorder_locator,
};
use lm_rom::{Mapper, Region, SupportedGame};
use lm_title::TitleScreenRecording;

impl AppState {
    pub(crate) fn replace_title_recording(
        &mut self,
        expected_revision: u64,
        recording: &TitleScreenRecording,
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
            return Err(AppError::TitleRecordingIdentityMismatch);
        }
        let locator = smw_us_v1_title_recording_locator();
        if project
            .load_title_recording_detected(&locator)?
            .recording
            .as_ref()
            == Some(recording)
        {
            return Ok(Vec::new());
        }
        let allocation = smw_us_v1_title_recording_allocation_policy(project.rom.logical_len());
        project.save_title_recording_detected(
            recording,
            &locator,
            &allocation,
            SMW_US_V1_CHECKSUM_FIELD,
            SMW_US_V1_TITLE_RECORDING_RECLAIM_FILL,
        )?;
        self.advance_project_revision()?;
        let description = "Replace native SMW title-screen recording".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn set_title_recording_recorder(
        &mut self,
        expected_revision: u64,
        install: bool,
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
            return Err(AppError::TitleRecordingIdentityMismatch);
        }
        let locator = smw_us_v1_title_recording_recorder_locator();
        let allocation =
            smw_us_v1_title_recording_recorder_allocation_policy(project.rom.logical_len());
        let changed = if install {
            project.install_title_recording_recorder(&locator, &allocation)?
        } else {
            project.uninstall_title_recording_recorder(&locator, &allocation)?
        };
        if !changed {
            return Ok(Vec::new());
        }
        self.advance_project_revision()?;
        let description = if install {
            "Install native SMW title-movement joypad recorder"
        } else {
            "Uninstall native SMW title-movement joypad recorder"
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
    use crate::Command;
    use std::path::PathBuf;

    #[test]
    fn recording_install_is_undoable_to_the_exact_pristine_rom() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let recording = TitleScreenRecording::from_bytes(vec![0x12, 0x34, 0x56, 0xff]).unwrap();
        app.dispatch(Command::ReplaceNativeTitleRecording { rev: 0, recording })
            .unwrap();
        assert_eq!(app.project().unwrap().rom.logical_len(), 0x10_0000);
        assert_eq!(app.project().unwrap().rom.logical_bytes()[0x7fd7], 0x0a);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn recorder_commands_are_revision_checked_and_undoable() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(original) = std::fs::read(root.join("Super Mario World (USA).sfc")) else {
            return;
        };
        if original.len() != 0x20_0200 {
            return;
        }
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        assert!(matches!(
            app.dispatch(Command::InstallNativeTitleRecordingRecorder { rev: 1 }),
            Err(AppError::StaleProjectRevision { .. })
        ));
        app.dispatch(Command::InstallNativeTitleRecordingRecorder { rev: 0 })
            .unwrap();
        assert_eq!(app.project_revision(), 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
        app.dispatch(Command::Redo).unwrap();
        app.dispatch(Command::UninstallNativeTitleRecordingRecorder { rev: 3 })
            .unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
