use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::SpecialEventRevealTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_special_event_reveal_installation_plan,
    smw_us_v1_special_event_reveal_locator, smw_us_v1_special_event_update_policy,
};
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_special_event_reveals(
        &mut self,
        expected_revision: u64,
        table: &SpecialEventRevealTable,
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
            return Err(AppError::NativeSpecialEventIdentityMismatch);
        }
        if project
            .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())?
            .table
            == *table
        {
            return Ok(Vec::new());
        }
        let plan = smw_us_v1_special_event_reveal_installation_plan(table)?;
        let update = smw_us_v1_special_event_update_policy(project.rom.logical_len());
        project.save_special_event_reveals_detected(
            table,
            smw_us_v1_special_event_reveal_locator(),
            &plan,
            &update,
            SMW_US_V1_CHECKSUM_FIELD,
            0xff,
        )?;
        self.advance_project_revision()?;
        let description = "Replace native SMW special-event reveals".to_owned();
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
    fn install_update_and_two_undos_restore_the_original_rom() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        for (revision, bias) in [(0, 0x100), (1, 0x180)] {
            let mut table = SpecialEventRevealTable::default();
            for index in 0_u16..24 {
                table.reveals[usize::from(index)] = EventReveal {
                    source_tile: index + bias,
                    destination_tile: index + bias + 0x200,
                };
                table.directions[usize::from(index)] = index.to_le_bytes()[0];
            }
            app.dispatch(Command::ReplaceNativeSpecialEventReveals {
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
