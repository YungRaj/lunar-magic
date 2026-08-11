use lm_app::{AppState, Command};
use lm_overworld::{BossSequenceMessage, BossSequenceMessageTable};
use lm_profile::smw_us_v1_boss_sequence_locator;

pub(super) struct BossSequenceWorkspace {
    revision: u64,
    original: BossSequenceMessageTable,
    current: BossSequenceMessageTable,
}

impl BossSequenceWorkspace {
    pub(super) fn load(app: &AppState) -> Result<Self, String> {
        let loaded = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())?
            .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())
            .map_err(|error| error.to_string())?;
        Ok(Self {
            revision: app.project_revision(),
            original: loaded.table.clone(),
            current: loaded.table,
        })
    }

    pub(super) fn tile(&self, (message, row, column): (usize, usize, usize)) -> u8 {
        self.current.messages[message].0[row * BossSequenceMessage::COLUMNS + column]
    }

    pub(super) fn set_tile(
        &mut self,
        selection @ (message, row, column): (usize, usize, usize),
        value: u8,
    ) -> Result<(), String> {
        self.current.messages[message].0[row * BossSequenceMessage::COLUMNS + column] = value;
        let encoded = self.current.encode_native_payload();
        BossSequenceMessageTable::decode_native_payload(&encoded)
            .map_err(|error| error.to_string())?;
        debug_assert_eq!(self.tile(selection), value);
        Ok(())
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.current != self.original
    }

    pub(super) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        self.is_dirty().then(|| {
            let content_revision = self
                .current
                .encode_native_payload()
                .iter()
                .fold(0x424f_5353_5345_5155_u64, |revision, byte| {
                    revision.rotate_left(5) ^ u64::from(*byte)
                });
            app.project_revision().wrapping_mul(0xa24b_aed4_963e_e407)
                ^ self.revision.rotate_left(31)
                ^ content_revision
        })
    }

    pub(super) fn staged_recovery_table<'a>(
        &'a self,
        app: &AppState,
    ) -> Result<Option<&'a BossSequenceMessageTable>, String> {
        if self.is_stale(app.project_revision()) {
            return Err("stale boss-sequence workspace cannot be recovered".into());
        }
        Ok(self.is_dirty().then_some(&self.current))
    }

    pub(super) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        if self.is_stale(app.project_revision()) {
            return Err("stale boss-sequence workspace cannot be recovered".into());
        }
        if !self.is_dirty() {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        lm_app::save_native_boss_sequence_to_project(&mut staged, &self.current)
            .map_err(|error| error.to_string())?;
        app.recovery_snapshot_with_current_rom(staged.save_snapshot(), app.current_level())
            .map_err(|error| error.to_string())
    }

    pub(super) const fn is_stale(&self, project_revision: u64) -> bool {
        self.revision != project_revision
    }

    pub(super) fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        if self.is_stale(project_revision) {
            return Err("stale boss-sequence workspace cannot be committed".into());
        }
        if !self.is_dirty() {
            return Ok(None);
        }
        Ok(Some(Command::ReplaceNativeOverworldBossSequence {
            rev: self.revision,
            table: Box::new(self.current.clone()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_tile_edit_is_valid_and_stale_commit_is_rejected() {
        let table = BossSequenceMessageTable::default();
        let mut workspace = BossSequenceWorkspace {
            revision: 9,
            original: table.clone(),
            current: table,
        };
        workspace.set_tile((6, 7, 23), 0xab).unwrap();
        assert_eq!(workspace.tile((6, 7, 23)), 0xab);
        assert!(workspace.prepare_commit(10).is_err());
        assert!(workspace.prepare_commit(9).unwrap().is_some());
    }
}
