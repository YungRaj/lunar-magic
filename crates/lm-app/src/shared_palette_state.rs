use crate::{AppError, AppState, FrontendEffect};
use lm_graphics::{SmwPaletteBackend, SmwPaletteFile};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_expanded_shared_palette_installation_plan,
    smw_us_v1_shared_palette_layout,
};
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn replace_native_shared_palette(
        &mut self,
        expected_revision: u64,
        palette: &SmwPaletteFile,
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
            return Err(AppError::NativeSharedPaletteIdentityMismatch);
        }
        let layout = smw_us_v1_shared_palette_layout();
        let installed = project.load_shared_palette(layout)?.backend();
        if installed == SmwPaletteBackend::Legacy
            && palette.backend() == SmwPaletteBackend::Expanded
        {
            let expected = project
                .rom
                .read(layout.table_offset, SmwPaletteFile::EXPANDED_FILE_LEN)?
                .to_vec();
            let plan = smw_us_v1_expanded_shared_palette_installation_plan(palette, &expected)?;
            project.install_relocatable_patch(&plan)?;
        } else {
            project.save_shared_palette(palette, layout, SMW_US_V1_CHECKSUM_FIELD)?;
        }
        if project.load_shared_palette(smw_us_v1_shared_palette_layout())? != *palette {
            return Err(AppError::NativeSharedPaletteReopenMismatch);
        }
        self.advance_project_revision()?;
        let description = "Replace native shared SMW palettes".to_owned();
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
    use std::{fs, path::PathBuf};

    #[test]
    fn replacement_is_one_revision_and_one_undo_step() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut bytes =
            fs::read(root.join("oracle-work/lm363/pristine-us/palette/shared.pal")).unwrap();
        bytes[0x234] ^= 0x1f;
        let palette = SmwPaletteFile::decode(&bytes).unwrap();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::ReplaceNativeSharedPalette {
            rev: 0,
            palette: Box::new(palette.clone()),
        })
        .unwrap();
        assert_eq!(app.controller_snapshot().unwrap().revision, 1);
        assert_eq!(
            app.project()
                .unwrap()
                .load_shared_palette(smw_us_v1_shared_palette_layout())
                .unwrap(),
            palette
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
