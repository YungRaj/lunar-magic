use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::BossSequenceMessageTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_boss_sequence_locator,
    smw_us_v1_boss_sequence_update_policy,
};
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_boss_sequence_messages(
        &mut self,
        expected_revision: u64,
        table: &BossSequenceMessageTable,
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
            return Err(AppError::NativeBossSequenceIdentityMismatch);
        }
        let locator = smw_us_v1_boss_sequence_locator();
        if project.load_boss_sequence_messages_detected(locator)?.table == *table {
            return Ok(Vec::new());
        }
        let update = smw_us_v1_boss_sequence_update_policy(project.rom.logical_len());
        project.save_boss_sequence_messages_detected(
            table,
            locator,
            &update,
            SMW_US_V1_CHECKSUM_FIELD,
            0xff,
        )?;
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld boss-sequence messages".to_owned();
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
    fn replacement_is_undoable_to_the_exact_pristine_rom() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut table = app
            .project()
            .unwrap()
            .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())
            .unwrap()
            .table;
        table.messages[0].0[0] ^= 1;
        app.dispatch(Command::ReplaceNativeOverworldBossSequence {
            rev: 0,
            table: Box::new(table),
        })
        .unwrap();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
