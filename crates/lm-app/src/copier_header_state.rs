use crate::{AppError, AppState, FrontendEffect};
use lm_rom::CopierHeader;

impl AppState {
    pub(crate) fn set_lunar_magic_copier_header(
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
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        let identity = project
            .identity
            .as_ref()
            .ok_or(AppError::CopierHeaderIdentityMismatch)?;
        let header =
            lm_profile::lunar_magic_copier_header(project.rom.logical_len(), identity.map_mode);
        if project.rom.copier_header_bytes() == Some(header.as_slice()) {
            return Ok(Vec::new());
        }
        self.ensure_project_revision_capacity()?;
        let description = "Add Lunar Magic synthesized copier header".to_owned();
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let changed = project.set_copier_header_exact(description.clone(), &header)?;
        debug_assert!(changed);
        self.advance_project_revision()?;
        self.status.clone_from(&description);
        Ok(vec![FrontendEffect::ProjectChanged {
            description,
            mode: self.mode,
            revision: self.project_revision,
        }])
    }

    pub(crate) fn set_copier_header(
        &mut self,
        expected_revision: u64,
        target: CopierHeader,
        fill: u8,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        if expected_revision != self.project_revision {
            return Err(AppError::StaleProjectRevision {
                expected: expected_revision,
                actual: self.project_revision,
            });
        }
        self.require_no_pending_save()?;
        let project = self.project.as_ref().ok_or(AppError::NoProject)?;
        if project.rom.copier_header() == target {
            return Ok(Vec::new());
        }
        self.ensure_project_revision_capacity()?;
        let description = match target {
            CopierHeader::Present => format!("Add copier header (fill {fill:02X})"),
            CopierHeader::Absent => "Remove copier header".into(),
        };
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        let changed = project.set_copier_header(description.clone(), target, fill)?;
        debug_assert!(changed);
        self.advance_project_revision()?;
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
    use lm_rom::COPIER_HEADER_LEN;
    use std::path::PathBuf;

    fn original() -> Vec<u8> {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        crate::test_support::pristine_smw_us_rom_bytes()
    }

    #[test]
    fn add_remove_and_history_preserve_every_logical_and_header_byte() {
        let source = original();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        app.dispatch(Command::SetCopierHeader {
            rev: 0,
            target: CopierHeader::Present,
            fill: 0xa5,
        })
        .unwrap();
        let headered = app.project().unwrap().save_snapshot();
        assert_eq!(&headered[..COPIER_HEADER_LEN], &[0xa5; COPIER_HEADER_LEN]);
        assert_eq!(&headered[COPIER_HEADER_LEN..], source);
        assert_eq!(app.project_revision(), 1);

        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), source);
        app.dispatch(Command::Redo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), headered);

        app.dispatch(Command::SetCopierHeader {
            rev: 3,
            target: CopierHeader::Absent,
            fill: 0,
        })
        .unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), source);
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), headered);
    }

    #[test]
    fn lunar_magic_canonical_header_adds_replaces_and_round_trips_history_exactly() {
        let source = original();
        let logical = lm_rom::RomImage::from_bytes(source.clone())
            .unwrap()
            .logical_bytes()
            .to_vec();
        let mut noncanonical = vec![0x7e; COPIER_HEADER_LEN];
        noncanonical.extend_from_slice(&logical);
        let mut app = AppState::default();
        app.load_rom(noncanonical.clone()).unwrap();
        app.dispatch(Command::SetLunarMagicCopierHeader { rev: 0 })
            .unwrap();
        let canonical = lm_profile::smw_us_v1_lunar_magic_copier_header();
        let installed = app.project().unwrap().save_snapshot();
        assert_eq!(&installed[..COPIER_HEADER_LEN], &canonical);
        assert_eq!(&installed[COPIER_HEADER_LEN..], logical);
        assert_eq!(app.project_revision(), 1);

        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), noncanonical);
        app.dispatch(Command::Redo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), installed);
        assert!(
            app.dispatch(Command::SetLunarMagicCopierHeader { rev: 3 })
                .unwrap()
                .is_empty()
        );
        assert_eq!(app.project_revision(), 3);
        assert_eq!(app.project().unwrap().history.undo_len(), 1);
    }

    #[test]
    fn lunar_magic_header_action_uses_the_current_size_and_map_mode() {
        let source = original();
        let mut app = AppState::default();
        app.load_rom(source).unwrap();
        app.dispatch(crate::Command::ExpandRom(crate::RomExpansionCommand {
            expected_revision: 0,
            mapper: lm_rom::Mapper::LoRom,
            target_logical_len: 0x20_0000,
            fill: 0,
            checksum_field: 0x7fdc,
        }))
        .unwrap();
        app.dispatch(Command::SetLunarMagicCopierHeader { rev: 1 })
            .unwrap();
        let expected = lm_profile::lunar_magic_copier_header(0x20_0000, 0x20);
        assert_eq!(
            app.project().unwrap().rom.copier_header_bytes(),
            Some(expected.as_slice())
        );
    }

    #[test]
    fn lunar_magic_header_action_uses_sa1_fast_mapping_fields_after_expansion() {
        let mut image = lm_rom::RomImage::from_bytes(original()).unwrap();
        image.write(0x7fd5, &[0x23, 0x34]).unwrap();
        image.update_snes_checksum(0x7fdc).unwrap();
        let mut app = AppState::default();
        app.load_rom(image.as_file_bytes().to_vec()).unwrap();
        app.dispatch(Command::ExpandSa1Rom {
            expected_revision: 0,
            target_logical_len: lm_project::SA1_6_MIB_LEN,
        })
        .unwrap();
        app.dispatch(Command::SetLunarMagicCopierHeader { rev: 1 })
            .unwrap();
        let expected = lm_profile::lunar_magic_copier_header(lm_project::SA1_6_MIB_LEN, 0x23);
        assert_eq!(&expected[..4], &[0x00, 0x03, 0x30, 0x80]);
        assert_eq!(
            app.project().unwrap().rom.copier_header_bytes(),
            Some(expected.as_slice())
        );
    }

    #[test]
    fn lunar_magic_canonical_header_rejections_leave_bytes_revision_and_history_unchanged() {
        let source = original();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        app.project.as_mut().unwrap().identity = None;
        assert!(matches!(
            app.dispatch(Command::SetLunarMagicCopierHeader { rev: 0 }),
            Err(AppError::CopierHeaderIdentityMismatch)
        ));
        assert_eq!(app.project().unwrap().save_snapshot(), source);
        assert_eq!(app.project_revision(), 0);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        app.project.as_mut().unwrap().identity = Some(
            lm_rom::detect_identity(&lm_rom::RomImage::from_bytes(source.clone()).unwrap())
                .unwrap(),
        );
        assert!(matches!(
            app.dispatch(Command::SetLunarMagicCopierHeader { rev: 1 }),
            Err(AppError::StaleProjectRevision { .. })
        ));
        app.dispatch(Command::Save).unwrap();
        assert!(matches!(
            app.dispatch(Command::SetLunarMagicCopierHeader { rev: 0 }),
            Err(AppError::SaveInProgress)
        ));
        assert_eq!(app.project().unwrap().save_snapshot(), source);
        assert_eq!(app.project_revision(), 0);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
    }

    #[test]
    fn stale_no_op_and_pending_save_requests_are_atomic() {
        let source = original();
        let mut app = AppState::default();
        app.load_rom(source.clone()).unwrap();
        assert!(
            app.dispatch(Command::SetCopierHeader {
                rev: 0,
                target: CopierHeader::Absent,
                fill: 0xff,
            })
            .unwrap()
            .is_empty()
        );
        assert!(matches!(
            app.dispatch(Command::SetCopierHeader {
                rev: 1,
                target: CopierHeader::Present,
                fill: 0xff,
            }),
            Err(AppError::StaleProjectRevision { .. })
        ));
        app.dispatch(Command::Save).unwrap();
        assert!(matches!(
            app.dispatch(Command::SetCopierHeader {
                rev: 0,
                target: CopierHeader::Present,
                fill: 0xff,
            }),
            Err(AppError::SaveInProgress)
        ));
        assert_eq!(app.project().unwrap().save_snapshot(), source);
        assert_eq!(app.project_revision(), 0);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
    }
}
