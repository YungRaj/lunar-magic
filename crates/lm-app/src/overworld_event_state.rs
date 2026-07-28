use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::EventRevealTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_event_allocation_policy,
    smw_us_v1_overworld_event_reveal_locator,
};
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_overworld_event_reveals(
        &mut self,
        expected_revision: u64,
        table: &EventRevealTable,
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
            return Err(AppError::NativeOverworldEventIdentityMismatch);
        }
        if project
            .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())?
            .table
            == *table
        {
            return Ok(Vec::new());
        }
        project.save_overworld_event_reveals_detected(
            table,
            smw_us_v1_overworld_event_reveal_locator(),
            &smw_us_v1_overworld_event_allocation_policy(),
            SMW_US_V1_CHECKSUM_FIELD,
            0xff,
        )?;
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld event reveals".to_owned();
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
    use lm_overworld::EventReveal;
    use std::path::PathBuf;

    #[test]
    fn fixed_to_expanded_and_growth_are_revisioned_and_undoable() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        let original = app.project().unwrap().save_snapshot();
        for (revision, count) in [(0, 200), (1, 255)] {
            let table = EventRevealTable {
                entries: (0..count)
                    .map(|index| EventReveal {
                        source_tile: index,
                        destination_tile: index | 0x200,
                    })
                    .collect(),
            };
            app.dispatch(Command::ReplaceNativeOverworldEventReveals {
                rev: revision,
                table: Box::new(table),
            })
            .unwrap();
        }
        app.dispatch(Command::Undo).unwrap();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
