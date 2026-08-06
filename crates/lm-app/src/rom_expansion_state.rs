use crate::{AppError, AppState, FrontendEffect, RomExpansionCommand};

impl AppState {
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
