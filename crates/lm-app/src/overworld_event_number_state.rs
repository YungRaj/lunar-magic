use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::EventNumberMap;
use lm_profile::{SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_event_number_map_locator};
use lm_project::Project;
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_overworld_event_number_map(
        &mut self,
        expected_revision: u64,
        map: &EventNumberMap,
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
        if !save_native_overworld_event_number_map_to_project(project, map)? {
            return Ok(Vec::new());
        }
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld event-number map".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }
}

/// Applies the detected native event-number mapping persistence used by the application command.
pub fn save_native_overworld_event_number_map_to_project(
    project: &mut Project,
    map: &EventNumberMap,
) -> Result<bool, AppError> {
    let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
    if identity.game != SupportedGame::SuperMarioWorld
        || identity.region != Region::NorthAmerica
        || identity.revision != 0
        || identity.mapper != Mapper::LoRom
    {
        return Err(AppError::NativeOverworldEventMapIdentityMismatch);
    }
    if project
        .load_overworld_event_number_map_detected(smw_us_v1_overworld_event_number_map_locator())?
        .map
        == *map
    {
        return Ok(false);
    }
    project.save_overworld_event_number_map_detected(
        map,
        smw_us_v1_overworld_event_number_map_locator(),
        SMW_US_V1_CHECKSUM_FIELD,
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use std::path::PathBuf;

    #[test]
    fn replacement_is_revisioned_semantic_and_exactly_undoable() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut map =
            app.project()
                .unwrap()
                .load_overworld_event_number_map_detected(
                    smw_us_v1_overworld_event_number_map_locator(),
                )
                .unwrap()
                .map;
        map.set(0xff, 0x7e);
        app.dispatch(Command::ReplaceNativeOverworldEventNumberMap {
            rev: 0,
            map: Box::new(map.clone()),
        })
        .unwrap();
        assert_eq!(
            app.project()
                .unwrap()
                .load_overworld_event_number_map_detected(
                    smw_us_v1_overworld_event_number_map_locator()
                )
                .unwrap()
                .map,
            map
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
