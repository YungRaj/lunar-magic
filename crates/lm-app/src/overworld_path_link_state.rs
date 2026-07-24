use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::OverworldPathLinkTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_path_installation_plan,
    smw_us_v1_overworld_path_link_layout, smw_us_v1_overworld_path_patch_locator,
    smw_us_v1_overworld_path_update_policy,
};
use lm_project::OverworldPathLinkStorage;
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_path_links(
        &mut self,
        expected_revision: u64,
        table: &OverworldPathLinkTable,
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
            return Err(AppError::NativeOverworldPathIdentityMismatch);
        }
        let loaded =
            project.load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())?;
        let changed = match loaded.storage {
            OverworldPathLinkStorage::Fixed if table.links.len() == 14 => project
                .save_overworld_path_links(
                    table,
                    smw_us_v1_overworld_path_link_layout(),
                    SMW_US_V1_CHECKSUM_FIELD,
                )?,
            OverworldPathLinkStorage::Fixed => {
                project.install_relocatable_patch(&smw_us_v1_overworld_path_installation_plan(
                    table,
                )?)?;
                true
            }
            storage @ OverworldPathLinkStorage::CurrentPatch { .. } => {
                let allocation = smw_us_v1_overworld_path_update_policy(project.rom.logical_len());
                project.save_installed_overworld_path_links(
                    table,
                    storage,
                    smw_us_v1_overworld_path_patch_locator(),
                    &allocation,
                    SMW_US_V1_CHECKSUM_FIELD,
                    0xff,
                )?
            }
        };
        if !changed {
            return Ok(Vec::new());
        }
        if project
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())?
            .table
            != *table
        {
            return Err(AppError::NativeOverworldPathReopenMismatch);
        }
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld path links".to_owned();
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
    use lm_overworld::{OverworldEndpoint, OverworldPathLink, OverworldPathTarget};
    use std::{fs, path::PathBuf};

    #[test]
    fn native_path_replacement_is_revisioned_and_undoable() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        let before = app.project().unwrap().save_snapshot();
        let mut table = app
            .project()
            .unwrap()
            .load_overworld_path_links(smw_us_v1_overworld_path_link_layout())
            .unwrap();
        table.links[0].target.x_tile ^= 1;
        app.dispatch(Command::ReplaceNativeOverworldPathLinks {
            rev: 0,
            table: Box::new(table),
        })
        .unwrap();
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), before);
    }

    #[test]
    fn expanded_path_install_is_one_application_revision_and_undo_step() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut app = AppState::default();
        app.load_rom(bytes.clone()).unwrap();
        let table = OverworldPathLinkTable {
            links: (0_u16..20)
                .map(|value| OverworldPathLink {
                    source: OverworldEndpoint {
                        x: value,
                        y: value + 1,
                        submap: u8::try_from(value % 7).unwrap(),
                    },
                    destination: OverworldEndpoint {
                        x: value + 2,
                        y: value + 3,
                        submap: u8::try_from((value + 1) % 7).unwrap(),
                    },
                    target: OverworldPathTarget {
                        y_tile: u8::try_from(value).unwrap(),
                        x_tile: u8::try_from(value + 1).unwrap(),
                    },
                })
                .collect(),
        };
        app.dispatch(Command::ReplaceNativeOverworldPathLinks {
            rev: 0,
            table: Box::new(table.clone()),
        })
        .unwrap();
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        assert_eq!(
            app.project()
                .unwrap()
                .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
                .unwrap()
                .table,
            table
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), bytes);
    }
}
