use crate::{AppError, AppState, FrontendEffect};
use lm_profile::{SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_lunar_magic_metadata_layout};
use lm_rom::{LunarMagicRomMetadata, Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_lunar_magic_rom_metadata(
        &mut self,
        expected_revision: u64,
        metadata: &LunarMagicRomMetadata,
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
            return Err(AppError::LunarMagicMetadataIdentityMismatch);
        }
        let layout = smw_us_v1_lunar_magic_metadata_layout();
        if project.load_lunar_magic_rom_metadata(layout)?.as_ref() == Some(metadata) {
            return Ok(Vec::new());
        }
        project.save_lunar_magic_rom_metadata(metadata, layout, SMW_US_V1_CHECKSUM_FIELD)?;
        self.advance_project_revision()?;
        let description = "Replace Lunar Magic ROM metadata".to_owned();
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
    fn real_metadata_install_is_revision_checked_and_exactly_undoable() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let fixture =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let fixture = Project::open_supported(RomImage::from_bytes(fixture).unwrap()).unwrap();
        let metadata = fixture
            .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())
            .unwrap()
            .unwrap();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::ReplaceLunarMagicRomMetadata {
            rev: 0,
            metadata: Box::new(metadata),
        })
        .unwrap();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
