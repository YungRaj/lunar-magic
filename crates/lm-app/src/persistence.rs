use crate::state::PendingSave;
use crate::{AppError, AppState, FrontendEffect, ToolEvent};
use std::path::PathBuf;

impl AppState {
    /// Returns the request identifier expected by the active save callback.
    #[must_use]
    pub fn pending_save_request_id(&self) -> Option<u64> {
        self.pending_save.as_ref().map(|pending| pending.request_id)
    }

    /// Acknowledges that a frontend atomically persisted the most recent snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if no save is pending, no project is open, or newer edits make the
    /// acknowledged snapshot stale.
    pub fn confirm_saved(&mut self, request_id: u64) -> Result<Vec<FrontendEffect>, AppError> {
        self.finish_save(request_id, None)
    }

    /// Acknowledges a successful save and adopts the selected document path.
    ///
    /// This is used for `Save As` and first saves of pathless documents. The path is retained even
    /// when newer edits make the acknowledged snapshot stale, because that snapshot did reach the
    /// selected destination.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if no save is pending, no project is open, or newer edits make the
    /// acknowledged snapshot stale.
    pub fn confirm_saved_at(
        &mut self,
        request_id: u64,
        path: impl Into<PathBuf>,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        self.finish_save(request_id, Some(path.into()))
    }

    /// Reports that the frontend could not persist the pending save snapshot.
    ///
    /// The pending slot is released so the user can immediately retry.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NoPendingSave`] unless a save effect is awaiting acknowledgement.
    pub fn save_failed(
        &mut self,
        request_id: u64,
        message: impl Into<String>,
    ) -> Result<(), AppError> {
        self.take_pending_save(request_id)?;
        self.status = format!("Save failed: {}", message.into());
        Ok(())
    }

    /// Cancels a pending destination chooser without treating cancellation as an I/O failure.
    ///
    /// The project, document path, dirty baseline, and edit history remain unchanged. The pending
    /// slot is released so Save or Save As can be requested again.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NoPendingSave`] unless a save effect is awaiting acknowledgement.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), AppError> {
        self.take_pending_save(request_id)?;
        self.status = "Save cancelled".into();
        Ok(())
    }

    pub(crate) fn begin_save(&mut self) -> Result<(u64, Vec<u8>), AppError> {
        if self.pending_save.is_some() {
            return Err(AppError::SaveAlreadyPending);
        }
        let bytes = self
            .project
            .as_ref()
            .ok_or(AppError::NoProject)?
            .rom
            .as_file_bytes()
            .to_vec();
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(AppError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            bytes: bytes.clone(),
        });
        Ok((request_id, bytes))
    }

    fn finish_save(
        &mut self,
        request_id: u64,
        path: Option<PathBuf>,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        let saved = self.take_pending_save(request_id)?.bytes;
        let project = self.project.as_mut().ok_or(AppError::NoProject)?;
        if let Some(path) = path {
            self.recent_documents.note(&path);
            self.document_path = Some(path);
        }
        if project.save_snapshot() != saved {
            self.status = "Save completed, but newer edits remain unsaved".into();
            return Err(AppError::StaleSaveAcknowledgement);
        }
        project.mark_saved();
        self.status = "ROM saved".into();
        Ok(self.external_tool_event_effects(ToolEvent::ProjectSaved))
    }

    fn take_pending_save(&mut self, request_id: u64) -> Result<PendingSave, AppError> {
        let pending = self.pending_save.as_ref().ok_or(AppError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(AppError::StaleSaveRequest {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save.take().ok_or(AppError::NoPendingSave)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, FrontendEffect, SaveStatus};

    fn test_rom() -> Vec<u8> {
        let mut bytes = vec![0; 0x8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        bytes
    }

    #[test]
    fn cancelled_destination_releases_pending_slot_for_retry() {
        let mut app = AppState::default();
        app.load_rom(test_rom()).unwrap();
        assert!(matches!(
            app.dispatch(Command::SaveAs).unwrap().as_slice(),
            [FrontendEffect::ChooseSaveDestination { .. }]
        ));
        assert_eq!(app.capabilities().save, SaveStatus::Pending);
        app.cancel_save(app.pending_save_request_id().unwrap())
            .unwrap();
        assert_eq!(app.capabilities().save, SaveStatus::Idle);
        assert_eq!(app.document_path, None);
        assert_eq!(app.status, "Save cancelled");
        assert!(matches!(
            app.dispatch(Command::SaveAs).unwrap().as_slice(),
            [FrontendEffect::ChooseSaveDestination { .. }]
        ));
    }

    #[test]
    fn cancellation_preserves_modified_state_history_and_path() {
        let mut app = AppState::default();
        app.load_rom_at(test_rom(), Some("source.smc".into()))
            .unwrap();
        app.dispatch(Command::CommitRomWrites {
            expected_revision: 0,
            description: "edit".into(),
            writes: vec![lm_project::RomWrite {
                offset: 1,
                bytes: vec![7],
            }],
        })
        .unwrap();
        app.dispatch(Command::SaveAs).unwrap();
        app.cancel_save(app.pending_save_request_id().unwrap())
            .unwrap();
        assert!(app.project().unwrap().is_modified());
        assert!(app.project().unwrap().history.can_undo());
        assert_eq!(app.document_path, Some("source.smc".into()));
        assert_eq!(app.project_revision(), 1);
        assert!(matches!(app.cancel_save(99), Err(AppError::NoPendingSave)));
    }

    #[test]
    fn acknowledged_save_as_moves_destination_to_recent_front() {
        let mut app = AppState::default();
        app.load_rom_at(test_rom(), Some("source.smc".into()))
            .unwrap();
        app.dispatch(Command::SaveAs).unwrap();
        app.confirm_saved_at(app.pending_save_request_id().unwrap(), "保存/copy.smc")
            .unwrap();
        assert_eq!(app.document_path, Some("保存/copy.smc".into()));
        assert_eq!(
            app.recent_documents().paths(),
            [PathBuf::from("保存/copy.smc"), PathBuf::from("source.smc")]
        );
        assert!(!app.project().unwrap().is_modified());
        assert!(!app.project().unwrap().history.can_undo());
    }

    #[test]
    fn document_replacement_cannot_race_a_pending_save() {
        let mut app = AppState::default();
        app.load_rom_at(test_rom(), Some("source.smc".into()))
            .unwrap();
        app.dispatch(Command::Save).unwrap();
        let revision = app.project_revision();
        for command in [Command::Open, Command::Close, Command::Quit] {
            assert!(matches!(
                app.dispatch(command),
                Err(AppError::SaveInProgress)
            ));
            assert_eq!(app.document_path, Some("source.smc".into()));
            assert!(app.project().is_some());
            assert_eq!(app.project_revision(), revision);
            assert_eq!(app.capabilities().save, SaveStatus::Pending);
        }
        app.confirm_saved(app.pending_save_request_id().unwrap())
            .unwrap();
        assert!(
            app.dispatch(Command::Close)
                .unwrap()
                .contains(&FrontendEffect::ProjectClosed)
        );
    }

    #[test]
    fn delayed_save_callbacks_cannot_complete_or_cancel_a_newer_request() {
        let mut app = AppState::default();
        app.load_rom_at(test_rom(), Some("source.smc".into()))
            .unwrap();
        app.dispatch(Command::Save).unwrap();
        let first = app.pending_save_request_id().unwrap();
        app.confirm_saved(first).unwrap();

        app.project
            .as_mut()
            .unwrap()
            .apply_writes(
                "new edit",
                &[lm_project::RomWrite {
                    offset: 3,
                    bytes: vec![9],
                }],
            )
            .unwrap();
        app.dispatch(Command::Save).unwrap();
        let second = app.pending_save_request_id().unwrap();
        assert_ne!(first, second);

        for result in [
            app.confirm_saved(first).map(drop),
            app.cancel_save(first),
            app.save_failed(first, "late failure"),
        ] {
            assert!(matches!(
                result,
                Err(AppError::StaleSaveRequest { expected, actual })
                    if expected == second && actual == first
            ));
            assert_eq!(app.pending_save_request_id(), Some(second));
            assert!(app.project().unwrap().is_modified());
        }

        app.confirm_saved(second).unwrap();
        assert_eq!(app.pending_save_request_id(), None);
        assert!(!app.project().unwrap().is_modified());
    }

    #[test]
    fn save_request_id_overflow_does_not_create_pending_work() {
        let mut app = AppState::default();
        app.load_rom(test_rom()).unwrap();
        app.next_save_request = u64::MAX;
        assert!(matches!(
            app.dispatch(Command::Save),
            Err(AppError::SaveRequestOverflow)
        ));
        assert_eq!(app.pending_save_request_id(), None);
    }
}
