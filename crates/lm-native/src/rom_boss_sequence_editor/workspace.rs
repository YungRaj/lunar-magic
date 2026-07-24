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
