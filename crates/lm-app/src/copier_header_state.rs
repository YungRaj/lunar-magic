use crate::{AppError, AppState, FrontendEffect};
use lm_rom::CopierHeader;

impl AppState {
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
    use std::{fs, path::PathBuf};

    fn original() -> Vec<u8> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        fs::read(root.join("Super Mario World (USA).sfc")).unwrap()
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
