use crate::{AppError, AppState, FrontendEffect};
use lm_level::SecondaryExitTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_builtin_secondary_exit_installation_plan_from_source,
    smw_us_v1_secondary_exit_allocation_policy, smw_us_v1_secondary_exit_locator,
};
use lm_project::SecondaryExitStorage;
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_secondary_exits(
        &mut self,
        expected_revision: u64,
        table: &SecondaryExitTable,
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
            return Err(AppError::SecondaryExitIdentityMismatch);
        }
        let locator = smw_us_v1_secondary_exit_locator();
        let loaded = project.load_secondary_exit_table_detected(locator)?;
        if loaded.table == *table
            && matches!(loaded.storage, SecondaryExitStorage::Installed { .. })
        {
            return Ok(Vec::new());
        }
        match loaded.storage {
            SecondaryExitStorage::Pristine => {
                project.install_relocatable_patch(
                    &smw_us_v1_builtin_secondary_exit_installation_plan_from_source(
                        &loaded.table,
                        table,
                    )?,
                )?;
            }
            SecondaryExitStorage::Installed { .. } => {
                project.save_installed_secondary_exit_table(
                    table,
                    locator,
                    &smw_us_v1_secondary_exit_allocation_policy(project.rom.logical_len()),
                    SMW_US_V1_CHECKSUM_FIELD,
                    0xff,
                )?;
            }
        }
        self.advance_project_revision()?;
        let description = "Replace native secondary-exit table".to_owned();
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
    use lm_project::Project;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn real_installed_table_edit_is_one_undoable_application_revision() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let source =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let mut table = source
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap()
            .table;
        table.entries[0x123].destination_level = 0x105;
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::ReplaceNativeSecondaryExits {
            rev: 0,
            table: Box::new(table),
        })
        .unwrap();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn pristine_install_is_one_application_revision_and_undo_step() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let source =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let mut table = source
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap()
            .table;
        table.entries[0x400].destination_level = 0x105;
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::ReplaceNativeSecondaryExits {
            rev: 0,
            table: Box::new(table),
        })
        .unwrap();
        assert_eq!(app.project_revision(), 1);
        assert!(matches!(
            app.project()
                .unwrap()
                .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
                .unwrap()
                .storage,
            SecondaryExitStorage::Installed {
                fixed_prefix_planes: 0,
                ..
            }
        ));
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
