use crate::{AppError, AppState, FrontendEffect};
use lm_overworld::ExpandedLayerTilemap;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_title_tilemap_allocation_policy,
    smw_us_v1_title_tilemap_locator,
};
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_title_tilemap(
        &mut self,
        expected_revision: u64,
        tilemap: &ExpandedLayerTilemap,
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
            return Err(AppError::TitleTilemapIdentityMismatch);
        }
        let locator = smw_us_v1_title_tilemap_locator();
        if project.load_title_tilemap_detected(locator)?.tilemap == *tilemap {
            return Ok(Vec::new());
        }
        let allocation = smw_us_v1_title_tilemap_allocation_policy(project.rom.logical_len());
        project.save_title_tilemap_detected(
            tilemap,
            locator,
            &allocation,
            SMW_US_V1_CHECKSUM_FIELD,
            0xff,
        )?;
        self.advance_project_revision()?;
        let description = "Replace native SMW title-screen tilemap".to_owned();
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
    use lm_project::{Project, TitleTilemapStorage};
    use lm_rom::{RomImage, detect_identity};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn title_edit_is_undoable_to_the_exact_pristine_rom() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        let mut tilemap = app
            .project()
            .unwrap()
            .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())
            .unwrap()
            .tilemap;
        tilemap.primary_bytes_mut()[0] ^= 1;
        app.dispatch(Command::ReplaceNativeTitleTilemap {
            rev: 0,
            tilemap: Box::new(tilemap),
        })
        .unwrap();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn title_tilemap_install_and_update_match_across_copier_header_variants() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let physical = fs::read(
            root.join("oracle-work/lm363/pristine-us/title-screen-transfer-positive/before.smc"),
        )
        .unwrap();
        let physical_image = RomImage::from_bytes(physical.clone()).unwrap();
        let variants = [physical, physical_image.logical_bytes().to_vec()];
        let mut logical_results = Vec::new();

        for original in variants {
            let original_image = RomImage::from_bytes(original.clone()).unwrap();
            let original_header = original_image.copier_header_bytes().map(<[u8]>::to_vec);
            let mut app = AppState::default();
            app.load_rom(original.clone()).unwrap();
            let mut tilemap = app
                .project()
                .unwrap()
                .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())
                .unwrap()
                .tilemap;
            tilemap.primary_bytes_mut()[0] ^= 1;
            app.dispatch(Command::ReplaceNativeTitleTilemap {
                rev: 0,
                tilemap: Box::new(tilemap.clone()),
            })
            .unwrap();

            let installed = RomImage::from_bytes(app.project().unwrap().save_snapshot()).unwrap();
            assert_eq!(
                installed.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(detect_identity(&installed).unwrap().checksum_matches());
            let reopened = Project::open_supported(installed).unwrap();
            let loaded = reopened
                .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())
                .unwrap();
            assert_eq!(loaded.tilemap, tilemap);
            assert!(matches!(loaded.storage, TitleTilemapStorage::Expanded(_)));

            tilemap.secondary_bytes_mut()[0] ^= 0x55;
            app.dispatch(Command::ReplaceNativeTitleTilemap {
                rev: 1,
                tilemap: Box::new(tilemap.clone()),
            })
            .unwrap();
            let updated = RomImage::from_bytes(app.project().unwrap().save_snapshot()).unwrap();
            assert_eq!(
                updated.copier_header_bytes().map(<[u8]>::to_vec),
                original_header
            );
            assert!(detect_identity(&updated).unwrap().checksum_matches());
            assert_eq!(
                Project::open_supported(updated.clone())
                    .unwrap()
                    .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())
                    .unwrap()
                    .tilemap,
                tilemap
            );
            logical_results.push(updated.logical_bytes().to_vec());

            app.dispatch(Command::Undo).unwrap();
            app.dispatch(Command::Undo).unwrap();
            assert_eq!(app.project().unwrap().save_snapshot(), original);
        }
        assert_eq!(logical_results[0], logical_results[1]);
    }
}
