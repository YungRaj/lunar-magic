use crate::{AppError, AppState, FrontendEffect};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_title_recording_allocation_policy,
    smw_us_v1_title_recording_locator,
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
            0xff,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use std::{fs, path::PathBuf};

    #[test]
    fn recording_install_is_undoable_to_the_exact_pristine_rom() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let recording = TitleScreenRecording::from_bytes(vec![0x12, 0x34, 0x56, 0xff]).unwrap();
        app.dispatch(Command::ReplaceNativeTitleRecording { rev: 0, recording })
            .unwrap();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
