use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::NativeOverworldPlayerStarts;
use lm_profile::{SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_player_start_layout};
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_overworld_player_starts(
        &mut self,
        expected_revision: u64,
        starts: &NativeOverworldPlayerStarts,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        self.ensure_project_revision_capacity()?;
        starts.encode()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::NativeOverworldPlayerStartIdentityMismatch);
        }
        let changed = project.save_overworld_player_starts(
            starts,
            smw_us_v1_overworld_player_start_layout(),
            SMW_US_V1_CHECKSUM_FIELD,
        )?;
        if !changed {
            return Ok(Vec::new());
        }
        if project.load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())?
            != *starts
        {
            return Err(AppError::NativeOverworldPlayerStartReopenMismatch);
        }
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld player starts".to_owned();
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
    use lm_overworld::Submap;
    use std::{fs, path::PathBuf};

    #[test]
    fn replacement_is_one_application_revision_and_undo_step() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut starts = app
            .project()
            .unwrap()
            .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())
            .unwrap();
        starts.starts[1].submap = Submap::StarWorld;
        starts.starts[1].x = 0x98;
        starts.starts[1].y = 0xb8;
        app.dispatch(Command::ReplaceNativeOverworldPlayerStarts {
            rev: 0,
            starts: Box::new(starts.clone()),
        })
        .unwrap();
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        assert_eq!(
            app.project()
                .unwrap()
                .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())
                .unwrap(),
            starts
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
