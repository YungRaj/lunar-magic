use crate::{AppError, AppState, EditorMode, FrontendEffect};
use lm_project::Project;
use lm_rom::RomImage;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PendingOpen {
    pub id: u64,
    pub revision: u64,
}

/// Fully parsed supported ROM state awaiting request-bound installation.
///
/// The wrapped project is intentionally opaque: frontends may prepare it on a worker, but only
/// [`AppState::complete_prepared_open`] can install it after checking application state.
pub struct PreparedRomOpen {
    project: Project,
}

impl AppState {
    /// Parses and qualifies ROM bytes without reading or mutating application state.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] when framing, mapping, or supported-game identity is invalid.
    pub fn prepare_open(bytes: Vec<u8>) -> Result<PreparedRomOpen, AppError> {
        let project = Project::open_supported(RomImage::from_bytes(bytes)?)?;
        Ok(PreparedRomOpen { project })
    }

    pub(crate) fn begin_open(&mut self) -> Result<Vec<FrontendEffect>, AppError> {
        if self.pending_open.is_some() {
            return Err(AppError::OpenAlreadyPending);
        }
        let id = self.next_open_request;
        self.next_open_request = self
            .next_open_request
            .checked_add(1)
            .ok_or(AppError::OpenRequestOverflow)?;
        self.pending_open = Some(PendingOpen {
            id,
            revision: self.project_revision,
        });
        Ok(vec![FrontendEffect::ChooseRom { request_id: id }])
    }

    /// Installs the ROM selected for a specific asynchronous chooser request.
    ///
    /// The selected image is parsed before replacing current document state. A completion becomes
    /// stale if another document was installed or the current project changed after the chooser
    /// opened.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] for an unknown/stale request or an unsupported ROM. Existing document
    /// state is preserved on every failure.
    pub fn complete_open(
        &mut self,
        request_id: u64,
        bytes: Vec<u8>,
        path: Option<PathBuf>,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        self.validate_open_request(request_id)?;
        let prepared = Self::prepare_open(bytes)?;
        self.complete_prepared_open(request_id, prepared, path)
    }

    /// Installs a worker-prepared ROM for one exact still-current open request.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] for unknown, stale, or context-invalidated requests. The prepared
    /// project is never partially installed.
    pub fn complete_prepared_open(
        &mut self,
        request_id: u64,
        prepared: PreparedRomOpen,
        path: Option<PathBuf>,
    ) -> Result<Vec<FrontendEffect>, AppError> {
        self.validate_open_request(request_id)?;
        self.pending_open = None;
        self.install_project(prepared.project, path);
        Ok(self.external_tool_event_effects(crate::ToolEvent::ProjectOpened))
    }

    fn validate_open_request(&mut self, request_id: u64) -> Result<(), AppError> {
        let pending = self.pending_open.ok_or(AppError::NoPendingOpen)?;
        if pending.id != request_id {
            return Err(AppError::StaleOpenRequest {
                expected: pending.id,
                actual: request_id,
            });
        }
        if pending.revision != self.project_revision
            || self.project.as_ref().is_some_and(Project::is_modified)
        {
            self.pending_open = None;
            return Err(AppError::OpenContextChanged);
        }
        Ok(())
    }

    /// Cancels one matching ROM chooser request.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if no chooser is pending or the supplied request is stale.
    pub fn cancel_open(&mut self, request_id: u64) -> Result<(), AppError> {
        let pending = self.pending_open.ok_or(AppError::NoPendingOpen)?;
        if pending.id != request_id {
            return Err(AppError::StaleOpenRequest {
                expected: pending.id,
                actual: request_id,
            });
        }
        self.pending_open = None;
        self.status = "Open cancelled".into();
        Ok(())
    }

    pub(crate) fn install_project(&mut self, mut project: Project, path: Option<PathBuf>) {
        if let Some(path) = &path {
            self.recent_documents.note(path);
        }
        project.history.set_limit(self.undo_operation_limit());
        self.project = Some(project);
        self.revision_profile = None;
        self.document_path = path;
        self.pending_save = None;
        self.pending_open = None;
        self.selection = None;
        self.mode = EditorMode::Level(0x105);
        self.level_navigation.reset(Some(0x105));
        self.project_revision = 0;
        self.status = "ROM loaded".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;

    fn test_rom(marker: u8) -> Vec<u8> {
        let mut bytes = vec![0; 0x8000];
        bytes[1] = marker;
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        bytes
    }

    fn open_request(app: &mut AppState) -> u64 {
        match app.dispatch(Command::Open).unwrap().as_slice() {
            [FrontendEffect::ChooseRom { request_id }] => *request_id,
            effect => panic!("unexpected effect: {effect:?}"),
        }
    }

    #[test]
    fn chooser_completion_is_request_bound_and_cancel_is_retryable() {
        let mut app = AppState::default();
        let first = open_request(&mut app);
        assert!(matches!(
            app.dispatch(Command::Open),
            Err(AppError::OpenAlreadyPending)
        ));
        assert!(matches!(
            app.complete_open(first + 1, test_rom(1), None),
            Err(AppError::StaleOpenRequest { .. })
        ));
        app.cancel_open(first).unwrap();
        let second = open_request(&mut app);
        assert_ne!(first, second);
        app.complete_open(second, test_rom(2), Some("chosen.smc".into()))
            .unwrap();
        assert_eq!(app.project().unwrap().rom.read(1, 1).unwrap(), [2]);
        assert_eq!(app.document_path, Some("chosen.smc".into()));
        assert!(matches!(
            app.complete_open(second, test_rom(3), None),
            Err(AppError::NoPendingOpen)
        ));
    }

    #[test]
    fn opaque_prepared_open_is_installed_only_for_its_live_request() {
        let prepared = AppState::prepare_open(test_rom(7)).unwrap();
        let mut app = AppState::default();
        let request = open_request(&mut app);
        assert!(matches!(
            app.complete_prepared_open(request + 1, prepared, Some("prepared.smc".into())),
            Err(AppError::StaleOpenRequest { .. })
        ));
        app.cancel_open(request).unwrap();

        let prepared = AppState::prepare_open(test_rom(8)).unwrap();
        let request = open_request(&mut app);
        app.complete_prepared_open(request, prepared, Some("prepared.smc".into()))
            .unwrap();
        assert_eq!(app.project().unwrap().rom.read(1, 1).unwrap(), [8]);
        assert_eq!(app.document_path, Some("prepared.smc".into()));
    }

    #[test]
    fn edits_made_after_chooser_open_prevent_silent_replacement() {
        let mut app = AppState::default();
        app.load_rom(test_rom(1)).unwrap();
        let request = open_request(&mut app);
        app.dispatch(Command::CommitRomWrites {
            expected_revision: 0,
            description: "edit while chooser is open".into(),
            writes: vec![lm_project::RomWrite {
                offset: 2,
                bytes: vec![9],
            }],
        })
        .unwrap();
        assert!(matches!(
            app.complete_open(request, test_rom(2), Some("new.smc".into())),
            Err(AppError::OpenContextChanged)
        ));
        assert_eq!(app.project().unwrap().rom.read(1, 2).unwrap(), [1, 9]);
        assert_eq!(app.document_path, None);
    }

    #[test]
    fn malformed_selection_preserves_existing_document_and_releases_no_request() {
        let mut app = AppState::default();
        app.load_rom_at(test_rom(1), Some("old.smc".into()))
            .unwrap();
        let request = open_request(&mut app);
        assert!(app.complete_open(request, vec![0; 4], None).is_err());
        assert_eq!(app.project().unwrap().rom.read(1, 1).unwrap(), [1]);
        assert_eq!(app.document_path, Some("old.smc".into()));
        // A parse failure keeps the same request active so a frontend can report the error and
        // either retry another selection or explicitly cancel the chooser lifecycle.
        app.cancel_open(request).unwrap();
    }

    #[test]
    fn unsupported_mapper_selection_preserves_existing_document_and_request() {
        let mut app = AppState::default();
        app.load_rom_at(test_rom(1), Some("old.smc".into()))
            .unwrap();
        let request = open_request(&mut app);
        let mut unsupported = test_rom(2);
        unsupported[0x7fd5] = 0x21;
        assert!(matches!(
            app.complete_open(request, unsupported, Some("hirom.smc".into())),
            Err(AppError::Identity(
                lm_rom::IdentityError::UnsupportedMapMode(0x21)
            ))
        ));
        assert_eq!(app.project().unwrap().rom.read(1, 1).unwrap(), [1]);
        assert_eq!(app.document_path, Some("old.smc".into()));
        app.cancel_open(request).unwrap();
    }

    #[test]
    fn recent_documents_change_only_after_successful_named_open() {
        let mut app = AppState::default();
        app.load_rom_at(test_rom(1), Some("古い.smc".into()))
            .unwrap();
        assert_eq!(app.recent_documents().paths(), [PathBuf::from("古い.smc")]);
        let request = open_request(&mut app);
        assert!(
            app.complete_open(request, vec![0; 4], Some("broken.smc".into()))
                .is_err()
        );
        assert_eq!(app.recent_documents().paths(), [PathBuf::from("古い.smc")]);
        app.cancel_open(request).unwrap();
        app.dispatch(Command::Close).unwrap();
        app.load_rom_at(test_rom(2), Some("新しい.smc".into()))
            .unwrap();
        assert_eq!(
            app.recent_documents().paths(),
            [PathBuf::from("新しい.smc"), PathBuf::from("古い.smc")]
        );
    }

    #[test]
    fn confirmed_replacement_is_atomic_when_request_ids_are_exhausted() {
        let mut app = AppState::default();
        app.load_rom_at(test_rom(1), Some("old.smc".into()))
            .unwrap();
        app.next_open_request = u64::MAX;
        assert!(matches!(
            app.discard_and_request_open(),
            Err(AppError::OpenRequestOverflow)
        ));
        assert_eq!(app.project().unwrap().rom.read(1, 1).unwrap(), [1]);
        assert_eq!(app.document_path, Some("old.smc".into()));
    }

    #[test]
    fn close_and_quit_cannot_abandon_an_open_chooser() {
        let mut app = AppState::default();
        app.load_rom(test_rom(1)).unwrap();
        let request = open_request(&mut app);
        assert!(matches!(
            app.dispatch(Command::Close),
            Err(AppError::OpenInProgress)
        ));
        assert!(matches!(
            app.dispatch(Command::Quit),
            Err(AppError::OpenInProgress)
        ));
        assert!(app.project().is_some());
        app.cancel_open(request).unwrap();
        assert!(
            app.dispatch(Command::Close)
                .unwrap()
                .contains(&FrontendEffect::ProjectClosed)
        );
    }

    #[test]
    fn startup_loader_cannot_bypass_document_replacement_protocol() {
        let mut app = AppState::default();
        app.load_rom_at(test_rom(1), Some("old.smc".into()))
            .unwrap();
        app.dispatch(Command::CommitRomWrites {
            expected_revision: 0,
            description: "unsaved edit".into(),
            writes: vec![lm_project::RomWrite {
                offset: 2,
                bytes: vec![7],
            }],
        })
        .unwrap();
        assert!(matches!(
            app.load_rom_at(test_rom(2), Some("new.smc".into())),
            Err(AppError::ProjectAlreadyOpen)
        ));
        assert_eq!(app.project().unwrap().rom.read(1, 2).unwrap(), [1, 7]);
        assert_eq!(app.document_path, Some("old.smc".into()));
        assert!(app.project().unwrap().is_modified());
    }

    #[test]
    fn startup_loader_cannot_supersede_a_pending_chooser() {
        let mut app = AppState::default();
        let request = open_request(&mut app);
        assert!(matches!(
            app.load_rom(test_rom(1)),
            Err(AppError::OpenAlreadyPending)
        ));
        app.complete_open(request, test_rom(2), None).unwrap();
        assert_eq!(app.project().unwrap().rom.read(1, 1).unwrap(), [2]);
    }
}
