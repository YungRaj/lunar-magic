use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::EventRevealTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_event_allocation_policy,
    smw_us_v1_overworld_event_reveal_locator,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_overworld_event_reveals(
        &mut self,
        expected_revision: u64,
        table: &EventRevealTable,
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
        if !save_native_overworld_event_reveals_to_project(project, table)? {
            return Ok(Vec::new());
        }
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld event reveals".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }
}

/// Saves a native SMW-US overworld event-reveal table through the same detected-storage path used
/// by the ordinary application command. Returns `false` when the detected table is already exact.
pub fn save_native_overworld_event_reveals_to_project(
    project: &mut Project,
    table: &EventRevealTable,
) -> Result<bool, AppError> {
    let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
    if identity.game != SupportedGame::SuperMarioWorld
        || identity.region != Region::NorthAmerica
        || identity.revision != 0
        || identity.mapper != Mapper::LoRom
    {
        return Err(AppError::NativeOverworldEventIdentityMismatch);
    }
    if project
        .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())?
        .table
        == *table
    {
        return Ok(false);
    }
    project.save_overworld_event_reveals_detected(
        table,
        smw_us_v1_overworld_event_reveal_locator(),
        &smw_us_v1_overworld_event_allocation_policy(),
        SMW_US_V1_CHECKSUM_FIELD,
        0xff,
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use lm_overworld::{EventNumberMap, EventReveal, EventTilemapBuffers, SpecialEventRevealTable};
    use lm_profile::{
        load_smw_us_v1_event_tilemaps, smw_us_v1_overworld_event_number_map_locator,
        smw_us_v1_special_event_reveal_locator,
    };
    use lm_rom::{RomImage, detect_identity};
    use std::path::PathBuf;

    #[test]
    fn fixed_to_expanded_and_growth_are_revisioned_and_undoable() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(bytes).unwrap();
        let original = app.project().unwrap().save_snapshot();
        for (revision, count) in [(0, 200), (1, 255)] {
            let table = EventRevealTable {
                entries: (0..count)
                    .map(|index| EventReveal {
                        source_tile: index,
                        destination_tile: index | 0x200,
                    })
                    .collect(),
            };
            app.dispatch(Command::ReplaceNativeOverworldEventReveals {
                rev: revision,
                table: Box::new(table),
            })
            .unwrap();
        }
        app.dispatch(Command::Undo).unwrap();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn complete_event_workflow_matches_across_copier_header_variants() {
        let headerless = crate::test_support::pristine_smw_us_rom_bytes();
        let mut headered = vec![0xa5; 0x200];
        headered.extend_from_slice(&headerless);
        let variants = [headerless, headered];
        let mut logical_results = Vec::new();

        for original in variants {
            let original_image = RomImage::from_bytes(original.clone()).unwrap();
            let original_header = original_image.copier_header_bytes().map(<[u8]>::to_vec);
            let mut app = AppState::default();
            app.load_rom(original.clone()).unwrap();

            let mut numbers = EventNumberMap::default();
            numbers.set(0x7f, 0x31);
            numbers.set(0xff, 0x62);
            app.dispatch(Command::ReplaceNativeOverworldEventNumberMap {
                rev: 0,
                map: Box::new(numbers.clone()),
            })
            .unwrap();

            let reveals = EventRevealTable {
                entries: (0_u16..200)
                    .map(|index| EventReveal {
                        source_tile: index,
                        destination_tile: index + 0x300,
                    })
                    .collect(),
            };
            app.dispatch(Command::ReplaceNativeOverworldEventReveals {
                rev: 1,
                table: Box::new(reveals.clone()),
            })
            .unwrap();

            let mut special = SpecialEventRevealTable::default();
            for index in 0_u16..24 {
                special.reveals[usize::from(index)] = EventReveal {
                    source_tile: index + 0x180,
                    destination_tile: index + 0x500,
                };
                special.directions[usize::from(index)] = (index & 3) as u8;
            }
            app.dispatch(Command::ReplaceNativeSpecialEventReveals {
                rev: 2,
                table: Box::new(special.clone()),
            })
            .unwrap();

            let mut tilemaps = EventTilemapBuffers::default();
            tilemaps.primary_bytes_mut()[7] = 0x12;
            tilemaps.secondary_high_bytes_mut()[9] = 0x34;
            app.dispatch(Command::ReplaceNativeOverworldEventTilemaps {
                rev: 3,
                buffers: Box::new(tilemaps.clone()),
            })
            .unwrap();

            let project = app.project().unwrap();
            assert_eq!(
                project
                    .load_overworld_event_number_map_detected(
                        smw_us_v1_overworld_event_number_map_locator()
                    )
                    .unwrap()
                    .map,
                numbers
            );
            assert_eq!(
                project
                    .load_overworld_event_reveals_detected(
                        smw_us_v1_overworld_event_reveal_locator()
                    )
                    .unwrap()
                    .table,
                reveals
            );
            assert_eq!(
                project
                    .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
                    .unwrap()
                    .table,
                special
            );
            assert_eq!(
                load_smw_us_v1_event_tilemaps(project).unwrap().buffers,
                tilemaps
            );
            let result = RomImage::from_bytes(project.save_snapshot()).unwrap();
            assert_eq!(
                result.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(detect_identity(&result).unwrap().checksum_matches());
            logical_results.push(result.logical_bytes().to_vec());

            for _ in 0..4 {
                app.dispatch(Command::Undo).unwrap();
            }
            assert_eq!(app.project().unwrap().save_snapshot(), original);
        }
        assert_eq!(logical_results[0], logical_results[1]);
    }
}
