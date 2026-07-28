use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::OverworldMessage;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED,
    SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET, smw_us_v1_overworld_message_allocation_policy,
    smw_us_v1_overworld_message_installation_plan, smw_us_v1_overworld_message_patch_locator,
};
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_overworld_messages(
        &mut self,
        expected_revision: u64,
        messages: &[OverworldMessage],
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
            return Err(AppError::NativeOverworldMessageIdentityMismatch);
        }
        let hook = project.rom.read(
            SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET,
            SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED.len(),
        )?;
        let changed = if hook == SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED {
            project.install_relocatable_patch(&smw_us_v1_overworld_message_installation_plan(
                messages,
            )?)?;
            true
        } else if hook.first() == Some(&0x22) {
            let loaded = project.load_expanded_overworld_messages_detected(
                smw_us_v1_overworld_message_patch_locator(),
            )?;
            project.save_installed_overworld_messages(
                messages,
                &loaded.storage,
                smw_us_v1_overworld_message_patch_locator(),
                &smw_us_v1_overworld_message_allocation_policy(),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )?
        } else {
            return Err(AppError::NativeOverworldMessageHookMismatch);
        };
        if !changed {
            return Ok(Vec::new());
        }
        if project
            .load_expanded_overworld_messages_detected(smw_us_v1_overworld_message_patch_locator())?
            .messages
            != messages
        {
            return Err(AppError::NativeOverworldMessageReopenMismatch);
        }
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld messages".to_owned();
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
    fn install_grow_and_undo_are_revisioned() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        let original = app.project().unwrap().save_snapshot();
        let first = vec![OverworldMessage([0x1f; 144]); 200];
        app.dispatch(Command::ReplaceNativeOverworldMessages {
            rev: 0,
            messages: first,
        })
        .unwrap();
        let grown = vec![OverworldMessage([0x20; 144]); 400];
        app.dispatch(Command::ReplaceNativeOverworldMessages {
            rev: 1,
            messages: grown.clone(),
        })
        .unwrap();
        assert_eq!(
            app.project()
                .unwrap()
                .load_expanded_overworld_messages_detected(
                    smw_us_v1_overworld_message_patch_locator()
                )
                .unwrap()
                .messages,
            grown
        );
        app.dispatch(Command::Undo).unwrap();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
