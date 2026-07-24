use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::NativeOverworldLevelNameTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_level_name_allocation_policy,
    smw_us_v1_overworld_level_name_installation_plan, smw_us_v1_overworld_level_name_locator,
    smw_us_v1_overworld_level_name_runtime,
};
use lm_project::OverworldLevelNameStorage;
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_level_names(
        &mut self,
        expected_revision: u64,
        table: &NativeOverworldLevelNameTable,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        self.ensure_project_revision_capacity()?;
        table.encode()?;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
        if identity.game != SupportedGame::SuperMarioWorld
            || identity.region != Region::NorthAmerica
            || identity.revision != 0
            || identity.mapper != Mapper::LoRom
        {
            return Err(AppError::NativeOverworldLevelNameIdentityMismatch);
        }
        let loaded = project.load_overworld_level_names_detected(
            smw_us_v1_overworld_level_name_locator(),
            smw_us_v1_overworld_level_name_runtime(),
        )?;
        let changed = match loaded.storage {
            OverworldLevelNameStorage::Vanilla => {
                project.install_relocatable_patch(
                    &smw_us_v1_overworld_level_name_installation_plan(table)?,
                )?;
                true
            }
            storage @ OverworldLevelNameStorage::Expanded { .. } => project
                .save_installed_overworld_level_names(
                    table,
                    storage,
                    Mapper::LoRom,
                    &smw_us_v1_overworld_level_name_allocation_policy(),
                    SMW_US_V1_CHECKSUM_FIELD,
                    0xff,
                )?,
        };
        if !changed {
            return Ok(Vec::new());
        }
        let reopened = project
            .load_overworld_level_names_detected(
                smw_us_v1_overworld_level_name_locator(),
                smw_us_v1_overworld_level_name_runtime(),
            )?
            .table;
        if reopened != *table {
            return Err(AppError::NativeOverworldLevelNameReopenMismatch);
        }
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld level names".to_owned();
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
    use lm_overworld::OverworldLevelName;
    use std::{fs, path::PathBuf};

    #[test]
    fn install_is_one_application_revision_and_undo_step() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let table = NativeOverworldLevelNameTable {
            names: (0..100)
                .map(|slot| OverworldLevelName {
                    level: NativeOverworldLevelNameTable::level_for_slot(slot).unwrap(),
                    tiles: [u8::try_from(slot).unwrap(); OverworldLevelName::TILE_COUNT],
                    raw_flags: 0,
                })
                .collect(),
        };
        app.dispatch(Command::ReplaceNativeOverworldLevelNames {
            rev: 0,
            table: Box::new(table.clone()),
        })
        .unwrap();
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        assert_eq!(
            app.project()
                .unwrap()
                .load_overworld_level_names_detected(
                    smw_us_v1_overworld_level_name_locator(),
                    smw_us_v1_overworld_level_name_runtime(),
                )
                .unwrap()
                .table,
            table
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
