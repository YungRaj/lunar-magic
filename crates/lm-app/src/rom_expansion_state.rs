use crate::{AppError, AppState, FrontendEffect, RomExpansionCommand};

impl AppState {
    pub(crate) fn expand_sa1_rom(
        &mut self,
        expected_revision: u64,
        target_logical_len: usize,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.ensure_project_revision_capacity()?;
        self.project
            .as_mut()
            .ok_or(AppError::NoProject)?
            .expand_sa1_rom(target_logical_len)?;
        self.advance_project_revision()?;
        let mib = target_logical_len / 0x10_0000;
        let description = format!("Expand SA-1 ROM to {mib} MiB");
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn convert_rom_to_64_mbit_exlorom(
        &mut self,
        expected_revision: u64,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.ensure_project_revision_capacity()?;
        self.project
            .as_mut()
            .ok_or(AppError::NoProject)?
            .convert_to_64_mbit_exlorom()?;
        self.advance_project_revision()?;
        let description = "Convert ROM to 64-Mbit ExLoROM".to_owned();
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    /// Expands the open ROM through the revision-checked project transaction boundary.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] for a missing project, stale revision, revision overflow, invalid
    /// mapper/extent/alignment, checksum range, or transaction failure. State is unchanged on error.
    pub(crate) fn expand_rom(
        &mut self,
        request: &RomExpansionCommand,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if request.expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: request.expected_revision,
                actual: self.project_revision,
            });
        }
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        if project.identity.as_ref().map(|identity| identity.mapper) == Some(lm_rom::Mapper::Sa1) {
            return Err(AppError::Sa1ExpansionRequiresFixedTarget);
        }
        if request.target_logical_len == project.rom.logical_len() {
            return Ok(Vec::new());
        }
        self.ensure_project_revision_capacity()?;
        self.project
            .as_mut()
            .ok_or(AppError::NoProject)?
            .expand_rom(
                request.mapper,
                request.target_logical_len,
                request.fill,
                request.checksum_field,
            )?;
        self.advance_project_revision()?;
        let description = format!("Expand ROM to {:#x} bytes", request.target_logical_len);
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
    use crate::{Command, EditorMode, RomExpansionCommand};
    use lm_rom::{Mapper, RomImage, compute_snes_checksum, detect_identity};
    use std::{fs, path::PathBuf};

    fn fixture(headered: bool) -> Vec<u8> {
        let mut logical = vec![0x31; 0x8000];
        logical[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        logical[0x7fd5] = 0x20;
        logical[0x7fd9] = 1;
        logical[0x7fdb] = 0;
        let checksum = compute_snes_checksum(&logical, 0x7fdc).unwrap();
        logical[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        if headered {
            let mut bytes = vec![0xa5; 0x200];
            bytes.extend(logical);
            bytes
        } else {
            logical
        }
    }

    fn command(revision: u64, target: usize) -> Command {
        Command::ExpandRom(RomExpansionCommand {
            expected_revision: revision,
            mapper: Mapper::LoRom,
            target_logical_len: target,
            fill: 0xff,
            checksum_field: 0x7fdc,
        })
    }

    fn pristine_smw() -> Vec<u8> {
        fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("sysLMRestore/smwOrig.smc"),
        )
        .unwrap()
    }

    fn sa1_smw() -> Vec<u8> {
        let mut image = RomImage::from_bytes(pristine_smw()).unwrap();
        image.write(0x7fd5, &[0x23, 0x34]).unwrap();
        image.update_snes_checksum(0x7fdc).unwrap();
        image.as_file_bytes().to_vec()
    }

    #[test]
    fn sa1_fixed_targets_are_revisioned_reopenable_and_individually_undoable() {
        let mut app = AppState::default();
        app.load_rom(sa1_smw()).unwrap();
        app.dispatch(Command::ExpandSa1Rom {
            expected_revision: 0,
            target_logical_len: lm_project::SA1_6_MIB_LEN,
        })
        .unwrap();
        assert_eq!(app.project_revision(), 1);
        assert_eq!(
            app.project().unwrap().rom.logical_len(),
            lm_project::SA1_6_MIB_LEN
        );
        assert_eq!(
            detect_identity(&RomImage::from_bytes(app.project().unwrap().save_snapshot()).unwrap())
                .unwrap()
                .mapper,
            Mapper::Sa1
        );
        app.dispatch(Command::ExpandSa1Rom {
            expected_revision: 1,
            target_logical_len: lm_project::SA1_8_MIB_LEN,
        })
        .unwrap();
        assert_eq!(app.project_revision(), 2);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.logical_len(),
            lm_project::SA1_6_MIB_LEN
        );
        app.dispatch(Command::Redo).unwrap();
        assert_eq!(
            app.project().unwrap().rom.logical_len(),
            lm_project::SA1_8_MIB_LEN
        );
    }

    #[test]
    fn generic_expansion_cannot_bypass_sa1_metadata_and_bank_locks() {
        let mut app = AppState::default();
        app.load_rom(sa1_smw()).unwrap();
        let before = app.project().unwrap().rom.as_file_bytes().to_vec();
        let result = app.dispatch(Command::ExpandRom(RomExpansionCommand {
            expected_revision: 0,
            mapper: Mapper::Sa1,
            target_logical_len: lm_project::SA1_6_MIB_LEN,
            fill: 0,
            checksum_field: 0x7fdc,
        }));
        assert!(matches!(
            result,
            Err(AppError::Sa1ExpansionRequiresFixedTarget)
        ));
        assert_eq!(app.project_revision(), 0);
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), before);
        assert!(!app.project().unwrap().history.can_undo());
    }

    #[test]
    fn exlorom_conversion_advances_revision_and_reopens_with_the_target_mapper() {
        let mut app = AppState::default();
        app.load_rom(pristine_smw()).unwrap();
        let effects = app
            .dispatch(Command::ConvertRomTo64MbitExLoRom {
                expected_revision: 0,
            })
            .unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(app.project_revision(), 1);
        assert_eq!(app.project().unwrap().rom.logical_len(), 0x80_0000);
        assert_eq!(
            app.project().unwrap().identity.as_ref().unwrap().mapper,
            Mapper::ExLoRom
        );
        let reopened = RomImage::from_bytes(app.project().unwrap().save_snapshot()).unwrap();
        let identity = detect_identity(&reopened).unwrap();
        assert_eq!(identity.mapper, Mapper::ExLoRom);
        assert!(identity.checksum_matches());
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(
            app.project().unwrap().identity.as_ref().unwrap().mapper,
            Mapper::LoRom
        );
        app.dispatch(Command::Redo).unwrap();
        assert_eq!(
            app.project().unwrap().identity.as_ref().unwrap().mapper,
            Mapper::ExLoRom
        );
    }

    #[test]
    fn expansion_advances_once_reopens_and_is_one_undo_step() {
        let mut app = AppState::default();
        let original = fixture(true);
        app.load_rom(original.clone()).unwrap();
        assert_eq!(
            app.dispatch(command(0, 0x1_0000)).unwrap(),
            [FrontendEffect::ProjectChanged {
                description: "Expand ROM to 0x10000 bytes".into(),
                mode: EditorMode::Level(0x105),
                revision: 1,
            }]
        );
        assert_eq!(app.project().unwrap().rom.logical_len(), 0x1_0000);
        assert_eq!(
            &app.project().unwrap().rom.as_file_bytes()[..0x200],
            &original[..0x200]
        );
        assert!(
            detect_identity(&RomImage::from_bytes(app.project().unwrap().save_snapshot()).unwrap())
                .unwrap()
                .checksum_matches()
        );
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), original);
        assert!(!app.project().unwrap().history.can_undo());
        app.dispatch(Command::Redo).unwrap();
        assert_eq!(app.project().unwrap().rom.logical_len(), 0x1_0000);
    }

    #[test]
    fn stale_noop_and_late_failure_preserve_revision_rom_and_history() {
        let mut app = AppState::default();
        app.load_rom(fixture(false)).unwrap();
        let before = app.project().unwrap().rom.as_file_bytes().to_vec();
        assert!(matches!(
            app.dispatch(command(1, 0x1_0000)),
            Err(AppError::StaleProjectRevision { .. })
        ));
        assert!(app.dispatch(command(0, 0x8000)).unwrap().is_empty());
        let mut invalid = command(0, 0x1_0000);
        let Command::ExpandRom(RomExpansionCommand { checksum_field, .. }) = &mut invalid else {
            unreachable!();
        };
        *checksum_field = usize::MAX;
        assert!(matches!(
            app.dispatch(invalid),
            Err(AppError::Transaction(_))
        ));
        assert_eq!(app.project_revision(), 0);
        assert_eq!(app.project().unwrap().rom.as_file_bytes(), before);
        assert!(!app.project().unwrap().history.can_undo());
    }
}
