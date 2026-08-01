use crate::{
    dialogs, native_clipboard,
    rom_loader::{RomLoadCompletion, RomLoader},
};
use eframe::egui;
use lm_app::{AppState, FrontendEffect};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Confirmation {
    DiscardAndOpen,
    DiscardAndClose { quit_after: bool },
}

#[derive(Default)]
pub(crate) struct EffectState {
    pub confirmation: Option<Confirmation>,
    pub error: Option<String>,
    pub quit_requested: bool,
    pub(crate) requested_rom_path: Option<PathBuf>,
    pub(crate) external_tools: crate::external_tool_launcher::ExternalToolLauncher,
    pub(crate) persistence: crate::persistence_worker::PersistenceWorker,
    pub(crate) rom_loader: RomLoader,
}

impl EffectState {
    pub(crate) fn request_rom_path(&mut self, path: PathBuf) {
        self.requested_rom_path = Some(path);
    }

    pub(crate) fn cancel_requested_rom_path(&mut self) {
        self.requested_rom_path = None;
    }

    pub(crate) fn handle(
        &mut self,
        app: &mut AppState,
        context: &egui::Context,
        effects: Vec<FrontendEffect>,
    ) {
        for effect in effects {
            self.handle_one(app, context, effect);
        }
    }

    fn handle_one(&mut self, app: &mut AppState, context: &egui::Context, effect: FrontendEffect) {
        match effect {
            FrontendEffect::ChooseRom { request_id } => self.choose_rom(app, request_id),
            FrontendEffect::ChooseSaveDestination { request_id, bytes } => {
                self.choose_save_destination(app, request_id, &bytes);
            }
            FrontendEffect::PersistRomAt {
                request_id,
                path,
                bytes,
            } => {
                self.start_persistence(
                    app,
                    request_id,
                    crate::persistence_worker::PersistenceTarget::Replace(path),
                    bytes,
                );
            }
            FrontendEffect::ConfirmDiscardChanges { quit_after } => {
                self.confirmation = Some(Confirmation::DiscardAndClose { quit_after });
            }
            FrontendEffect::ConfirmDiscardAndOpen => {
                self.confirmation = Some(Confirmation::DiscardAndOpen);
            }
            FrontendEffect::QuitApplication => self.quit_requested = true,
            FrontendEffect::LaunchExternalTool(invocation) => {
                if let Err(error) = self.external_tools.enqueue(invocation) {
                    self.error = Some(error);
                }
            }
            FrontendEffect::ExternalToolFailed { error, .. } => {
                self.error = Some(error.to_string());
            }
            FrontendEffect::WriteClipboard(bytes) => match native_clipboard::encode_bytes(&bytes) {
                Ok(text) => context.copy_text(text),
                Err(error) => self.error = Some(error),
            },
            FrontendEffect::ApplyClipboard(_) | FrontendEffect::CutSelection { .. } => {
                self.error = Some(
                    "This editor surface has not supplied a typed selection payload yet".into(),
                );
            }
            FrontendEffect::ViewChanged(_)
            | FrontendEffect::LevelViewportChanged(_)
            | FrontendEffect::ProjectClosed
            | FrontendEffect::ProjectChanged { .. }
            | FrontendEffect::RevisionProfileChanged { .. } => {}
        }
    }

    pub(crate) fn show_external_tools(&mut self, context: &egui::Context, app: &mut AppState) {
        if let Some(result) = self.external_tools.show(context) {
            match result {
                Ok(status) => app.status = status,
                Err(error) => self.error = Some(error),
            }
        }
    }

    pub(crate) fn show_persistence(&mut self, context: &egui::Context, app: &mut AppState) {
        let Some(completion) = self.persistence.show(context) else {
            return;
        };
        self.complete_persistence(context, app, completion);
    }

    pub(crate) fn show_rom_loader(&mut self, context: &egui::Context, app: &mut AppState) {
        let Some(completion) = self.rom_loader.show(context) else {
            return;
        };
        self.complete_rom_load(app, context, completion);
    }

    fn complete_persistence(
        &mut self,
        context: &egui::Context,
        app: &mut AppState,
        completion: crate::persistence_worker::PersistenceCompletion,
    ) {
        let result = match completion.result {
            Ok(()) => match completion.target {
                crate::persistence_worker::PersistenceTarget::Replace(_) => {
                    app.confirm_saved(completion.request_id)
                }
                crate::persistence_worker::PersistenceTarget::Create(path) => {
                    app.confirm_saved_at(completion.request_id, path)
                }
                crate::persistence_worker::PersistenceTarget::ReplacePair { .. }
                | crate::persistence_worker::PersistenceTarget::CreatePair { .. } => {
                    unreachable!("application ROM saves never use paired persistence")
                }
            },
            Err(error) => {
                self.error = Some(
                    match app.save_failed(completion.request_id, error.clone()) {
                        Ok(()) => error,
                        Err(acknowledgement) => {
                            format!("{error}; could not release failed save: {acknowledgement}")
                        }
                    },
                );
                return;
            }
        };
        self.follow(app, context, result);
    }

    fn start_persistence(
        &mut self,
        app: &mut AppState,
        request_id: u64,
        target: crate::persistence_worker::PersistenceTarget,
        bytes: Vec<u8>,
    ) {
        if let Err(error) = self.persistence.start(request_id, target, bytes) {
            self.error = Some(match app.save_failed(request_id, error.clone()) {
                Ok(()) => error,
                Err(acknowledgement) => {
                    format!("{error}; could not release failed save: {acknowledgement}")
                }
            });
        }
    }

    fn choose_rom(&mut self, app: &mut AppState, request_id: u64) {
        let path = self.requested_rom_path.take().or_else(dialogs::choose_rom);
        let Some(path) = path else {
            if let Err(error) = app.cancel_open(request_id) {
                self.error = Some(error.to_string());
            }
            return;
        };
        match self.rom_loader.start(request_id, path) {
            Ok(()) => {}
            Err(error) => {
                let _ = app.cancel_open(request_id);
                self.error = Some(error);
            }
        }
    }

    fn complete_rom_load(
        &mut self,
        app: &mut AppState,
        context: &egui::Context,
        completion: RomLoadCompletion,
    ) {
        match completion.result {
            Ok(prepared) => {
                let result = app.complete_prepared_open(
                    completion.request_id,
                    prepared,
                    Some(completion.path),
                );
                self.follow(app, context, result);
            }
            Err(error) => {
                let cancellation = app.cancel_open(completion.request_id);
                self.error = Some(match cancellation {
                    Ok(()) => error,
                    Err(cancel_error) => {
                        format!("{error}; could not release failed open: {cancel_error}")
                    }
                });
            }
        }
    }

    fn choose_save_destination(&mut self, app: &mut AppState, request_id: u64, bytes: &[u8]) {
        let Some(path) = dialogs::choose_save_path() else {
            if let Err(error) = app.cancel_save(request_id) {
                self.error = Some(error.to_string());
            }
            return;
        };
        self.start_persistence(
            app,
            request_id,
            crate::persistence_worker::PersistenceTarget::Create(path),
            bytes.to_vec(),
        );
    }

    fn follow<E: std::fmt::Display>(
        &mut self,
        app: &mut AppState,
        context: &egui::Context,
        result: Result<Vec<FrontendEffect>, E>,
    ) {
        match result {
            Ok(effects) => self.handle(app, context, effects),
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::Command;
    use lm_project::RomWrite;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-native-effects-save-{}-{}.smc",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn test_rom() -> Vec<u8> {
        let mut bytes = vec![0; 0x8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes
    }

    fn open_request(app: &mut AppState) -> u64 {
        app.dispatch(Command::Open)
            .unwrap()
            .into_iter()
            .find_map(|effect| match effect {
                FrontendEffect::ChooseRom { request_id } => Some(request_id),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn rom_loader_completion_opens_the_exact_pending_request() {
        let mut app = AppState::default();
        let request_id = open_request(&mut app);
        let path = PathBuf::from("loaded.smc");
        let mut state = EffectState::default();
        state.complete_rom_load(
            &mut app,
            &egui::Context::default(),
            RomLoadCompletion {
                request_id,
                path: path.clone(),
                result: AppState::prepare_open(test_rom()).map_err(|error| error.to_string()),
            },
        );

        assert_eq!(app.document_path.as_deref(), Some(path.as_path()));
        assert!(state.error.is_none());
    }

    #[test]
    fn failed_rom_preparation_releases_pending_open() {
        let mut app = AppState::default();
        let request_id = open_request(&mut app);
        let mut state = EffectState::default();
        state.complete_rom_load(
            &mut app,
            &egui::Context::default(),
            RomLoadCompletion {
                request_id,
                path: "invalid.smc".into(),
                result: Err("invalid prepared ROM".into()),
            },
        );

        assert!(app.project().is_none());
        let replacement_request = open_request(&mut app);
        app.cancel_open(replacement_request).unwrap();
        assert!(
            state
                .error
                .as_deref()
                .unwrap()
                .contains("invalid prepared ROM")
        );
    }

    #[test]
    fn worker_completion_acknowledges_exact_application_snapshot() {
        let path = path();
        let bytes = test_rom();
        fs::write(&path, &bytes).unwrap();
        let mut app = AppState::default();
        app.load_rom_at(bytes, Some(path.clone())).unwrap();
        let mode = app.mode;
        app.dispatch(Command::CommitRomWrites {
            expected_revision: app.project_revision(),
            description: "native save worker test".into(),
            writes: vec![RomWrite {
                offset: 2,
                bytes: vec![9],
            }],
        })
        .unwrap();
        assert!(app.project().unwrap().is_modified());

        let effects = app.dispatch(Command::Save).unwrap();
        let context = egui::Context::default();
        let mut state = EffectState::default();
        state.handle(&mut app, &context, effects);
        assert!(app.pending_save_request_id().is_some());
        let completion = state.persistence.wait_for_test();
        state.complete_persistence(&context, &mut app, completion);

        assert!(app.pending_save_request_id().is_none());
        assert!(!app.project().unwrap().is_modified());
        assert_eq!(app.mode, mode);
        assert_eq!(fs::read(&path).unwrap()[2], 9);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_create_new_worker_releases_pending_save_without_marking_clean() {
        let path = path();
        fs::write(&path, b"existing destination").unwrap();
        let mut app = AppState::default();
        app.load_rom(test_rom()).unwrap();
        app.dispatch(Command::CommitRomWrites {
            expected_revision: app.project_revision(),
            description: "dirty before failed save".into(),
            writes: vec![RomWrite {
                offset: 3,
                bytes: vec![8],
            }],
        })
        .unwrap();
        let effects = app.dispatch(Command::SaveAs).unwrap();
        let (request_id, bytes) = effects
            .into_iter()
            .find_map(|effect| match effect {
                FrontendEffect::ChooseSaveDestination { request_id, bytes } => {
                    Some((request_id, bytes))
                }
                _ => None,
            })
            .unwrap();

        let context = egui::Context::default();
        let mut state = EffectState::default();
        state.start_persistence(
            &mut app,
            request_id,
            crate::persistence_worker::PersistenceTarget::Create(path.clone()),
            bytes,
        );
        let completion = state.persistence.wait_for_test();
        state.complete_persistence(&context, &mut app, completion);

        assert!(app.pending_save_request_id().is_none());
        assert!(app.project().unwrap().is_modified());
        assert_eq!(fs::read(&path).unwrap(), b"existing destination");
        assert!(state.error.is_some());
        fs::remove_file(path).unwrap();
    }
}
