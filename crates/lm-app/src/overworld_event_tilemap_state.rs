use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::EventTilemapBuffers;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SmwUsV1EventTilemapStorage, load_smw_us_v1_event_tilemaps,
    smw_us_v1_event_tilemap_installation_plan, smw_us_v1_event_tilemap_locator,
    smw_us_v1_event_tilemap_update_policy,
};
use lm_project::EventTilemapCompression;
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_event_tilemaps(
        &mut self,
        expected_revision: u64,
        buffers: &EventTilemapBuffers,
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
            return Err(AppError::NativeEventTilemapIdentityMismatch);
        }
        let locator = smw_us_v1_event_tilemap_locator();
        let loaded = load_smw_us_v1_event_tilemaps(project)?;
        match loaded.storage {
            SmwUsV1EventTilemapStorage::Installed(compression) => {
                if loaded.buffers == *buffers {
                    return Ok(Vec::new());
                }
                let update = smw_us_v1_event_tilemap_update_policy(project.rom.logical_len());
                project.save_event_tilemap_buffers_detected(
                    buffers,
                    locator,
                    compression,
                    &update,
                    SMW_US_V1_CHECKSUM_FIELD,
                    0xff,
                )?;
            }
            SmwUsV1EventTilemapStorage::Pristine => {
                if loaded.buffers == *buffers {
                    return Ok(Vec::new());
                }
                let compression = EventTilemapCompression::Lz2;
                let plan = smw_us_v1_event_tilemap_installation_plan(buffers, compression);
                project.install_event_tilemap_buffers(buffers, locator, compression, &plan)?;
            }
        }
        if load_smw_us_v1_event_tilemaps(project)?.buffers != *buffers {
            return Err(AppError::NativeEventTilemapReopenMismatch);
        }
        self.advance_project_revision()?;
        let description = "Replace native SMW overworld event tilemaps".to_owned();
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
    fn install_update_and_two_undos_restore_the_pristine_rom() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        for (revision, value) in [(0, 0x12), (1, 0x34)] {
            let mut buffers = EventTilemapBuffers::default();
            buffers.primary_bytes_mut()[7] = value;
            buffers.secondary_high_bytes_mut()[9] = value.wrapping_add(1);
            app.dispatch(Command::ReplaceNativeOverworldEventTilemaps {
                rev: revision,
                buffers: Box::new(buffers),
            })
            .unwrap();
        }
        app.dispatch(Command::Undo).unwrap();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
