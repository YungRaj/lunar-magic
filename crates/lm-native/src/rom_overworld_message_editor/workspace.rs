use lm_app::{AppState, Command};
use lm_overworld::{OverworldMessage, encode_native_overworld_message_file};
use lm_profile::{
    SmwUsV1OverworldMessageStorage, load_smw_us_v1_overworld_messages,
    smw_us_v1_overworld_message_installation_plan,
};

pub(super) struct OverworldMessageWorkspace {
    revision: u64,
    original: Vec<OverworldMessage>,
    current: Vec<OverworldMessage>,
    pub(super) storage: SmwUsV1OverworldMessageStorage,
}

impl OverworldMessageWorkspace {
    pub(super) fn load(app: &AppState) -> Result<Self, String> {
        let loaded = load_smw_us_v1_overworld_messages(
            app.project()
                .ok_or_else(|| "open a supported ROM first".to_owned())?,
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            revision: app.project_revision(),
            original: loaded.messages.clone(),
            current: loaded.messages,
            storage: loaded.storage,
        })
    }

    #[cfg(test)]
    pub(super) fn for_test(count: usize) -> Self {
        let messages = vec![OverworldMessage([0x1f; OverworldMessage::ENCODED_LEN]); count];
        Self {
            revision: 0,
            original: messages.clone(),
            current: messages,
            storage: SmwUsV1OverworldMessageStorage::Pristine,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.current.len()
    }

    pub(super) fn storage_label(&self) -> &'static str {
        match self.storage {
            SmwUsV1OverworldMessageStorage::Pristine => "pristine selector table",
            SmwUsV1OverworldMessageStorage::Expanded(_) => "expanded Lunar Magic runtime",
        }
    }

    pub(super) fn tile(&self, (message, row, column): (usize, usize, usize)) -> u8 {
        self.current[message].0[row * OverworldMessage::COLUMNS + column]
    }

    pub(super) fn set_tile(
        &mut self,
        selection @ (message, row, column): (usize, usize, usize),
        value: u8,
    ) -> Result<(), String> {
        if value == 0xfe {
            return Err("tile value FE is reserved as the native string terminator".into());
        }
        self.current[message].0[row * OverworldMessage::COLUMNS + column] = value;
        self.validate()?;
        debug_assert_eq!(self.tile(selection), value);
        Ok(())
    }

    pub(super) fn resize(&mut self, count: usize) -> Result<(), String> {
        if !(194..=512).contains(&count) || count % 2 != 0 {
            return Err("message count must be even and between 0C2 and 200".into());
        }
        self.current.resize(
            count,
            OverworldMessage([0x1f; OverworldMessage::ENCODED_LEN]),
        );
        self.validate()
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.current != self.original
    }

    pub(super) const fn is_stale(&self, revision: u64) -> bool {
        self.revision != revision
    }

    pub(super) fn prepare_commit(&self, revision: u64) -> Result<Option<Command>, String> {
        if self.is_stale(revision) {
            return Err("stale overworld-message workspace cannot be committed".into());
        }
        if !self.is_dirty() {
            return Ok(None);
        }
        self.validate()?;
        Ok(Some(Command::ReplaceNativeOverworldMessages {
            rev: self.revision,
            messages: self.current.clone(),
        }))
    }

    fn validate(&self) -> Result<(), String> {
        encode_native_overworld_message_file(&self.current).map_err(|error| error.to_string())?;
        smw_us_v1_overworld_message_installation_plan(&self.current)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_bounds_terminator_and_stale_commit_are_rejected() {
        let mut workspace = OverworldMessageWorkspace::for_test(194);
        assert!(workspace.resize(193).is_err());
        assert!(workspace.resize(513).is_err());
        workspace.resize(200).unwrap();
        assert!(workspace.set_tile((199, 7, 17), 0xfe).is_err());
        workspace.set_tile((199, 7, 17), 0xab).unwrap();
        assert!(workspace.prepare_commit(1).is_err());
        assert!(workspace.prepare_commit(0).unwrap().is_some());
    }
}
