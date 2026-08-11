use crate::{AppError, AppState, FrontendEffect};
use lm_graphics::{SmwPaletteBackend, SmwPaletteFile};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_expanded_shared_palette_installation_plan_for_mapper,
    smw_us_v1_shared_palette_layout_for_mapper,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, SupportedGame};

impl AppState {
    pub(crate) fn install_native_expanded_shared_palettes(
        &mut self,
        expected_revision: u64,
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
            || !matches!(identity.mapper, Mapper::LoRom | Mapper::ExLoRom)
        {
            return Err(AppError::NativeSharedPaletteIdentityMismatch);
        }
        let mapper = identity.mapper;
        let layout = smw_us_v1_shared_palette_layout_for_mapper(mapper);
        let expected = project
            .rom
            .read(layout.table_offset, SmwPaletteFile::EXPANDED_FILE_LEN)?
            .to_vec();
        let palette =
            SmwPaletteFile::expanded(expected[0x10..].to_vec(), expected[..0x10].to_vec())?;
        let plan = smw_us_v1_expanded_shared_palette_installation_plan_for_mapper(
            &palette, &expected, mapper,
        )?;
        project.install_relocatable_patch(&plan)?;
        if project.load_shared_palette(layout)? != palette {
            return Err(AppError::NativeSharedPaletteReopenMismatch);
        }
        self.advance_project_revision()?;
        let description = "Install expanded shared/custom palette runtime".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

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
        save_native_shared_palette_to_project(project, palette)?;
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

pub fn save_native_shared_palette_to_project(
    project: &mut Project,
    palette: &SmwPaletteFile,
) -> Result<(), AppError> {
    let identity = project.identity.as_ref().ok_or(AppError::NoProject)?;
    if identity.game != SupportedGame::SuperMarioWorld
        || identity.region != Region::NorthAmerica
        || identity.revision != 0
        || !matches!(identity.mapper, Mapper::LoRom | Mapper::ExLoRom)
    {
        return Err(AppError::NativeSharedPaletteIdentityMismatch);
    }
    let mapper = identity.mapper;
    let layout = smw_us_v1_shared_palette_layout_for_mapper(mapper);
    let installed = project.load_shared_palette(layout)?.backend();
    if installed == SmwPaletteBackend::Legacy && palette.backend() == SmwPaletteBackend::Expanded {
        let expected = project
            .rom
            .read(layout.table_offset, SmwPaletteFile::EXPANDED_FILE_LEN)?
            .to_vec();
        let plan = smw_us_v1_expanded_shared_palette_installation_plan_for_mapper(
            palette, &expected, mapper,
        )?;
        project.install_relocatable_patch(&plan)?;
    } else {
        project.save_shared_palette(palette, layout, SMW_US_V1_CHECKSUM_FIELD)?;
    }
    if project.load_shared_palette(layout)? != *palette {
        return Err(AppError::NativeSharedPaletteReopenMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use lm_profile::smw_us_v1_custom_palette_installation;
    use std::{fs, path::PathBuf};

    #[test]
    fn replacement_is_one_revision_and_one_undo_step() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
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
                .load_shared_palette(smw_us_v1_shared_palette_layout_for_mapper(Mapper::LoRom))
                .unwrap(),
            palette
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn pristine_runtime_install_enables_custom_palettes_and_undoes_exactly() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        assert!(
            smw_us_v1_custom_palette_installation()
                .resolve(&app.project().unwrap().rom)
                .unwrap()
                .is_none()
        );
        let effects = app
            .dispatch(Command::InstallExpandedSharedPalettes { rev: 0 })
            .unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(app.project_revision(), 1);
        assert!(
            smw_us_v1_custom_palette_installation()
                .resolve(&app.project().unwrap().rom)
                .unwrap()
                .is_some()
        );
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }

    #[test]
    fn converted_exlorom_installs_edits_reopens_and_undoes_expanded_palettes() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut app = AppState::default();
        app.load_rom(original.clone()).unwrap();
        app.dispatch(Command::ConvertRomTo64MbitExLoRom {
            expected_revision: 0,
        })
        .unwrap();
        assert_eq!(
            app.project().unwrap().identity.as_ref().unwrap().mapper,
            Mapper::ExLoRom
        );
        let converted = app.project().unwrap().save_snapshot();

        app.dispatch(Command::InstallExpandedSharedPalettes { rev: 1 })
            .unwrap();
        let layout = smw_us_v1_shared_palette_layout_for_mapper(Mapper::ExLoRom);
        let installed = app.project().unwrap().load_shared_palette(layout).unwrap();
        assert_eq!(installed.backend(), SmwPaletteBackend::Expanded);
        assert!(
            lm_profile::smw_us_v1_custom_palette_installation_for_mapper(Mapper::ExLoRom)
                .resolve(&app.project().unwrap().rom)
                .unwrap()
                .is_some()
        );
        let installed_identity = lm_rom::detect_identity(&app.project().unwrap().rom).unwrap();
        assert!(
            installed_identity.checksum_matches(),
            "installed checksum: stored={:?}, computed={:?}, low-computed={:?}, low={:02x?}, high={:02x?}",
            installed_identity.stored_checksum,
            installed_identity.computed_checksum,
            lm_rom::compute_snes_checksum(
                app.project().unwrap().rom.logical_bytes(),
                SMW_US_V1_CHECKSUM_FIELD,
            )
            .unwrap(),
            app.project()
                .unwrap()
                .rom
                .read(SMW_US_V1_CHECKSUM_FIELD, 4)
                .unwrap(),
            app.project()
                .unwrap()
                .rom
                .read(0x40_0000 + SMW_US_V1_CHECKSUM_FIELD, 4)
                .unwrap(),
        );

        let mut colors = installed.palette_bytes().to_vec();
        colors[0x234] ^= 0x1f;
        let changed =
            SmwPaletteFile::expanded(colors, installed.auxiliary_bytes().to_vec()).unwrap();
        app.dispatch(Command::ReplaceNativeSharedPalette {
            rev: 2,
            palette: Box::new(changed.clone()),
        })
        .unwrap();
        assert_eq!(
            app.project().unwrap().load_shared_palette(layout).unwrap(),
            changed
        );
        assert!(
            lm_rom::detect_identity(&app.project().unwrap().rom)
                .unwrap()
                .checksum_matches()
        );

        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().load_shared_palette(layout).unwrap(),
            installed
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), converted);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
    }
}
