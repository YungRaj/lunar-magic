use lm_app::{AppState, Command};
use lm_overworld::{OverworldMessage, encode_native_overworld_message_file};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED,
    SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET, SmwUsV1OverworldMessageStorage,
    load_smw_us_v1_overworld_messages, smw_us_v1_overworld_message_allocation_policy,
    smw_us_v1_overworld_message_installation_plan, smw_us_v1_overworld_message_patch_locator,
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

    pub(super) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        self.is_dirty().then(|| {
            let content_revision = self
                .current
                .iter()
                .flat_map(|message| message.0)
                .fold(0x4f56_4552_4d53_4753_u64, |revision, byte| {
                    revision.rotate_left(5) ^ u64::from(byte)
                });
            app.project_revision().wrapping_mul(0xd6e8_feb8_6659_fd93)
                ^ self.revision.rotate_left(31)
                ^ content_revision
                ^ (self.current.len() as u64).rotate_left(17)
        })
    }

    pub(super) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        if self.is_stale(app.project_revision()) {
            return Err("stale overworld-message workspace cannot be recovered".into());
        }
        if !self.is_dirty() {
            return Ok(app.recovery_snapshot());
        }
        self.validate()?;
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        let hook = staged
            .rom
            .read(
                SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET,
                SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED.len(),
            )
            .map_err(|error| error.to_string())?;
        if hook == SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED {
            let plan = smw_us_v1_overworld_message_installation_plan(&self.current)
                .map_err(|error| error.to_string())?;
            staged
                .install_relocatable_patch(&plan)
                .map_err(|error| error.to_string())?;
        } else if hook.first() == Some(&0x22) {
            let loaded = staged
                .load_expanded_overworld_messages_detected(
                    smw_us_v1_overworld_message_patch_locator(),
                )
                .map_err(|error| error.to_string())?;
            staged
                .save_installed_overworld_messages(
                    &self.current,
                    &loaded.storage,
                    smw_us_v1_overworld_message_patch_locator(),
                    &smw_us_v1_overworld_message_allocation_policy(),
                    SMW_US_V1_CHECKSUM_FIELD,
                    0xff,
                )
                .map_err(|error| error.to_string())?;
        } else {
            return Err("overworld-message runtime hook is not recognized".into());
        }
        if staged
            .load_expanded_overworld_messages_detected(smw_us_v1_overworld_message_patch_locator())
            .map_err(|error| error.to_string())?
            .messages
            != self.current
        {
            return Err("recovered overworld messages did not reopen exactly".into());
        }
        app.recovery_snapshot_with_current_rom(staged.save_snapshot(), app.current_level())
            .map_err(|error| error.to_string())
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
