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
    DiscardAndReload,
    DiscardAndClose { quit_after: bool },
}

#[derive(Default)]
pub(crate) struct EffectState {
    pub confirmation: Option<Confirmation>,
    pub(crate) save_then: Option<Confirmation>,
    pub error: Option<String>,
    pub quit_requested: bool,
    pub(crate) requested_rom_path: Option<PathBuf>,
    pub(crate) external_tools: crate::external_tool_launcher::ExternalToolLauncher,
    pub(crate) persistence: crate::persistence_worker::PersistenceWorker,
    pub(crate) rom_loader: RomLoader,
    pub(crate) completed_rom_save: bool,
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
            FrontendEffect::LoadRomAt { request_id, path } => {
                self.load_rom_at(app, request_id, path);
            }
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
            FrontendEffect::ConfirmDiscardAndReload => {
                self.confirmation = Some(Confirmation::DiscardAndReload);
            }
            FrontendEffect::QuitApplication => self.quit_requested = true,
            FrontendEffect::LaunchExternalTool(invocation) => {
                if let Err(error) = self.external_tools.enqueue(invocation) {
                    self.error = Some(error);
                }
            }
            FrontendEffect::StageEmulatorTest(request) => {
                if let Err(error) = self.external_tools.enqueue_emulator_test(request) {
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
        if let Some(result) = self.external_tools.show(context, app.localization()) {
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
        let catalog = app.localization().cloned();
        let Some(completion) = self.rom_loader.show(context, catalog.as_ref()) else {
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
                crate::persistence_worker::PersistenceTarget::ReplaceAs(path) => {
                    app.confirm_saved_at(completion.request_id, path)
                }
                crate::persistence_worker::PersistenceTarget::Create(path) => {
                    app.confirm_saved_at(completion.request_id, path)
                }
                crate::persistence_worker::PersistenceTarget::CreateRemoving { .. }
                | crate::persistence_worker::PersistenceTarget::ReplacePair { .. }
                | crate::persistence_worker::PersistenceTarget::CreatePair { .. } => {
                    unreachable!("application ROM saves never use specialized persistence")
                }
            },
            Err(error) => {
                self.save_then = None;
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
        match result {
            Ok(effects) => {
                self.completed_rom_save = true;
                self.handle(app, context, effects);
                if let Some(confirmation) = self.save_then.take() {
                    let effects = match confirmation {
                        Confirmation::DiscardAndOpen => app.discard_and_request_open(),
                        Confirmation::DiscardAndReload => app.discard_and_request_reload(),
                        Confirmation::DiscardAndClose { quit_after } => {
                            Ok(app.discard_and_close(quit_after))
                        }
                    };
                    self.follow(app, context, effects);
                }
            }
            Err(error) => {
                self.save_then = None;
                self.error = Some(error.to_string());
            }
        }
    }

    fn start_persistence(
        &mut self,
        app: &mut AppState,
        request_id: u64,
        target: crate::persistence_worker::PersistenceTarget,
        bytes: Vec<u8>,
    ) {
        if let Err(error) = self.persistence.start(request_id, target, bytes) {
            self.save_then = None;
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
        match self
            .rom_loader
            .start(request_id, path, app.silently_add_copier_header())
        {
            Ok(()) => {}
            Err(error) => {
                let _ = app.cancel_open(request_id);
                self.error = Some(error);
            }
        }
    }

    fn load_rom_at(&mut self, app: &mut AppState, request_id: u64, path: PathBuf) {
        if let Err(error) =
            self.rom_loader
                .start(request_id, path, app.silently_add_copier_header())
        {
            let _ = app.cancel_open(request_id);
            self.error = Some(error);
        }
    }

    fn complete_rom_load(
        &mut self,
        app: &mut AppState,
        context: &egui::Context,
        completion: RomLoadCompletion,
    ) {
        if completion.cancelled {
            if let Err(error) = app.cancel_open(completion.request_id) {
                self.error = Some(error.to_string());
            }
            return;
        }
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
            self.save_then = None;
            if let Err(error) = app.cancel_save(request_id) {
                self.error = Some(error.to_string());
            }
            return;
        };
        self.complete_save_destination(app, request_id, bytes, path);
    }

    fn complete_save_destination(
        &mut self,
        app: &mut AppState,
        request_id: u64,
        bytes: &[u8],
        path: PathBuf,
    ) {
        let target = match crate::persistence_worker::PersistenceTarget::save_as(path) {
            Ok(target) => target,
            Err(error) => {
                self.save_then = None;
                self.error = Some(match app.save_failed(request_id, error.clone()) {
                    Ok(()) => error,
                    Err(acknowledgement) => {
                        format!("{error}; could not release failed save: {acknowledgement}")
                    }
                });
                return;
            }
        };
        self.start_persistence(app, request_id, target, bytes.to_vec());
    }

    pub(crate) fn save_before_confirmation_action(
        &mut self,
        app: &mut AppState,
        context: &egui::Context,
        confirmation: Confirmation,
    ) {
        self.save_then = Some(confirmation);
        match app.dispatch(lm_app::Command::Save) {
            Ok(effects) => self.handle(app, context, effects),
            Err(error) => {
                self.save_then = None;
                self.error = Some(error.to_string());
            }
        }
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

    fn lifecycle_variant_rom(
        title: &[u8; 21],
        region: u8,
        map_mode: u8,
        headered: bool,
        corrupt_checksum: bool,
    ) -> Vec<u8> {
        let mut logical = vec![0; 0x8000];
        logical[0x7fc0..0x7fd5].copy_from_slice(title);
        logical[0x7fd5] = map_mode;
        logical[0x7fd9] = region;
        let checksum = lm_rom::compute_snes_checksum(&logical, 0x7fdc).unwrap();
        logical[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        if corrupt_checksum {
            logical[0] ^= 1;
        }
        if !headered {
            return logical;
        }
        let mut physical = vec![0xa5; lm_rom::COPIER_HEADER_LEN];
        physical.extend_from_slice(&logical);
        physical
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
    fn retained_lm363_dirty_close_prompt_has_save_discard_cancel_contract() {
        let fixture = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/oracle-work/lm363/pristine-us/lifecycle-dirty-close/observation.tsv"
        ));
        let field = |name: &str| {
            fixture
                .lines()
                .find_map(|line| line.split_once('\t').filter(|(key, _)| *key == name))
                .map(|(_, value)| value)
                .unwrap_or_else(|| panic!("missing retained lifecycle field {name}"))
        };

        assert_eq!(field("dialog_title"), "Lunar Magic");
        assert_eq!(field("question"), "Save level to ROM?");
        assert_eq!(field("save_button_title"), "&Yes");
        assert_eq!(field("save_button_id"), "6");
        assert_eq!(field("discard_button_title"), "&No");
        assert_eq!(field("discard_button_id"), "7");
        assert_eq!(field("cancel_button_title"), "Cancel");
        assert_eq!(field("cancel_button_id"), "2");
        assert_eq!(field("cancel_frame_present"), "true");
        assert_eq!(field("cancel_modified_byte"), "01");
        assert_eq!(field("discard_process_closed"), "true");
        assert_eq!(field("save_command_id"), "0x2392");
    }

    #[test]
    fn retained_lm363_rom_save_completes_exact_expansion_and_checksum_boundary() {
        let fixture = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/oracle-work/lm363/pristine-us/rom-save/observation.tsv"
        ));
        let field = |name: &str| {
            fixture
                .lines()
                .find_map(|line| line.split_once('\t').filter(|(key, _)| *key == name))
                .map(|(_, value)| value)
                .unwrap_or_else(|| panic!("missing retained ROM-save field {name}"))
        };

        assert_eq!(field("dirty_prompt_question"), "Save level to ROM?");
        assert_eq!(field("dirty_prompt_yes_id"), "6");
        assert_eq!(field("save_dialog_title"), "Save Level to ROM as (in hex)");
        assert_eq!(field("save_dialog_ok_id"), "1");
        assert_eq!(field("before_physical_bytes"), "524800");
        assert_eq!(field("after_physical_bytes"), "1049088");
        assert_eq!(field("after_logical_bytes"), "1048576");
        assert_eq!(field("after_mapper"), "LoRom");
        assert_eq!(field("after_copier_header"), "Present");
        assert_eq!(field("after_rats_blocks"), "13");
        assert_eq!(field("after_checksum_matches"), "true");
        assert_eq!(field("process_closed"), "true");
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
                cancelled: false,
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
                cancelled: false,
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
    fn declining_missing_header_cancels_without_reporting_an_error() {
        let mut app = AppState::default();
        let request_id = open_request(&mut app);
        let mut state = EffectState::default();
        state.complete_rom_load(
            &mut app,
            &egui::Context::default(),
            RomLoadCompletion {
                request_id,
                path: "headerless.smc".into(),
                result: Err("ROM open cancelled".into()),
                cancelled: true,
            },
        );

        assert!(app.project().is_none());
        assert!(state.error.is_none());
        let replacement_request = open_request(&mut app);
        app.cancel_open(replacement_request).unwrap();
    }

    #[test]
    fn reload_confirmation_and_completion_preserve_level_and_replace_atomically() {
        let path = path();
        let original = test_rom();
        fs::write(&path, &original).unwrap();
        let mut app = AppState::default();
        app.load_rom_at(original, Some(path.clone())).unwrap();
        app.dispatch(Command::SelectLevel(0x12c)).unwrap();
        app.dispatch(Command::CommitRomWrites {
            expected_revision: app.project_revision(),
            description: "dirty before reload".into(),
            writes: vec![RomWrite {
                offset: 3,
                bytes: vec![0x77],
            }],
        })
        .unwrap();

        let context = egui::Context::default();
        let mut state = EffectState::default();
        let effects = app.dispatch(Command::Reload).unwrap();
        state.handle(&mut app, &context, effects);
        assert_eq!(state.confirmation, Some(Confirmation::DiscardAndReload));

        let effects = app.discard_and_request_reload().unwrap();
        let request_id = match effects.as_slice() {
            [
                FrontendEffect::LoadRomAt {
                    request_id,
                    path: requested,
                },
            ] => {
                assert_eq!(requested, &path);
                *request_id
            }
            effects => panic!("unexpected reload effects: {effects:?}"),
        };
        assert_eq!(app.project().unwrap().rom.read(3, 1).unwrap(), [0x77]);
        state.complete_rom_load(
            &mut app,
            &context,
            RomLoadCompletion {
                request_id,
                path: path.clone(),
                result: AppState::prepare_open(test_rom()).map_err(|error| error.to_string()),
                cancelled: false,
            },
        );

        assert_eq!(app.mode, lm_app::EditorMode::Level(0x12c));
        assert_eq!(app.project().unwrap().rom.read(3, 1).unwrap(), [0]);
        assert!(state.error.is_none());
        fs::remove_file(path).unwrap();
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
        assert!(state.completed_rom_save);
        assert_eq!(fs::read(&path).unwrap()[2], 9);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn save_as_overwrite_replaces_approved_file_and_adopts_destination() {
        let destination = path();
        fs::write(&destination, b"previous unrelated file").unwrap();
        let mut app = AppState::default();
        app.load_rom(test_rom()).unwrap();
        app.dispatch(Command::CommitRomWrites {
            expected_revision: app.project_revision(),
            description: "Save As overwrite".into(),
            writes: vec![RomWrite {
                offset: 6,
                bytes: vec![0x66],
            }],
        })
        .unwrap();
        let (request_id, bytes) = app
            .dispatch(Command::SaveAs)
            .unwrap()
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
        state.complete_save_destination(&mut app, request_id, &bytes, destination.clone());
        let completion = state.persistence.wait_for_test();
        assert_eq!(
            completion.target,
            crate::persistence_worker::PersistenceTarget::ReplaceAs(destination.clone())
        );
        state.complete_persistence(&context, &mut app, completion);

        assert_eq!(fs::read(&destination).unwrap(), bytes);
        assert_eq!(app.document_path.as_deref(), Some(destination.as_path()));
        assert!(!app.project().unwrap().is_modified());
        fs::remove_file(destination).unwrap();
    }

    #[test]
    fn confirmed_save_closes_only_after_successful_persistence() {
        let path = path();
        let bytes = test_rom();
        fs::write(&path, &bytes).unwrap();
        let mut app = AppState::default();
        app.load_rom_at(bytes, Some(path.clone())).unwrap();
        app.dispatch(Command::CommitRomWrites {
            expected_revision: app.project_revision(),
            description: "dirty before confirmed close".into(),
            writes: vec![RomWrite {
                offset: 4,
                bytes: vec![0x44],
            }],
        })
        .unwrap();

        let context = egui::Context::default();
        let mut state = EffectState::default();
        state.save_before_confirmation_action(
            &mut app,
            &context,
            Confirmation::DiscardAndClose { quit_after: true },
        );
        assert!(app.project().is_some());
        assert!(!state.quit_requested);

        let completion = state.persistence.wait_for_test();
        state.complete_persistence(&context, &mut app, completion);

        assert!(app.project().is_none());
        assert!(state.quit_requested);
        assert_eq!(fs::read(&path).unwrap()[4], 0x44);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_confirmed_save_keeps_dirty_project_open() {
        let path = path().join("missing-parent").join("rom.smc");
        let mut app = AppState::default();
        app.load_rom_at(test_rom(), Some(path)).unwrap();
        app.dispatch(Command::CommitRomWrites {
            expected_revision: app.project_revision(),
            description: "dirty before failed confirmed close".into(),
            writes: vec![RomWrite {
                offset: 5,
                bytes: vec![0x55],
            }],
        })
        .unwrap();

        let context = egui::Context::default();
        let mut state = EffectState::default();
        state.save_before_confirmation_action(
            &mut app,
            &context,
            Confirmation::DiscardAndClose { quit_after: true },
        );
        let completion = state.persistence.wait_for_test();
        state.complete_persistence(&context, &mut app, completion);

        assert!(app.project().unwrap().is_modified());
        assert!(!state.quit_requested);
        assert!(state.save_then.is_none());
        assert!(state.error.is_some());
        assert!(!state.completed_rom_save);
    }

    #[test]
    fn confirmed_save_close_covers_all_forty_eight_supported_identity_variants() {
        let games = [
            (b"SUPER MARIOWORLD     ", 0),
            (b"SUPER MARIOWORLD     ", 1),
            (b"ALL_STARS + WORLD    ", 1),
        ];
        let map_modes = [0x20, 0x30, 0x23, 0x32];
        let context = egui::Context::default();
        let mut completed = 0;

        for (title, region) in games {
            for map_mode in map_modes {
                for headered in [false, true] {
                    for corrupt_checksum in [false, true] {
                        let path = path();
                        let original = lifecycle_variant_rom(
                            title,
                            region,
                            map_mode,
                            headered,
                            corrupt_checksum,
                        );
                        fs::write(&path, &original).unwrap();
                        let mut app = AppState::default();
                        app.load_rom_at(original, Some(path.clone())).unwrap();
                        app.dispatch(Command::CommitRomWrites {
                            expected_revision: app.project_revision(),
                            description: "cross-variant dirty close".into(),
                            writes: vec![RomWrite {
                                offset: 0x10,
                                bytes: vec![0x66],
                            }],
                        })
                        .unwrap();
                        let expected = app.project().unwrap().save_snapshot();
                        let expected_prefix = app
                            .project()
                            .unwrap()
                            .rom
                            .copier_header_bytes()
                            .map(<[u8]>::to_vec);

                        let mut state = EffectState::default();
                        state.save_before_confirmation_action(
                            &mut app,
                            &context,
                            Confirmation::DiscardAndClose { quit_after: false },
                        );
                        assert!(app.project().is_some());
                        let completion = state.persistence.wait_for_test();
                        state.complete_persistence(&context, &mut app, completion);

                        assert!(app.project().is_none());
                        assert!(!state.quit_requested);
                        assert_eq!(fs::read(&path).unwrap(), expected);
                        assert_eq!(
                            lm_rom::RomImage::from_bytes(expected)
                                .unwrap()
                                .copier_header_bytes()
                                .map(<[u8]>::to_vec),
                            expected_prefix
                        );
                        fs::remove_file(path).unwrap();
                        completed += 1;
                    }
                }
            }
        }
        assert_eq!(completed, 48);
    }

    #[test]
    fn save_as_create_and_overwrite_cover_all_supported_identity_variants() {
        let games = [
            (b"SUPER MARIOWORLD     ", 0),
            (b"SUPER MARIOWORLD     ", 1),
            (b"ALL_STARS + WORLD    ", 1),
        ];
        let map_modes = [0x20, 0x30, 0x23, 0x32];
        let context = egui::Context::default();
        let mut completed = 0;

        for (title, region) in games {
            for map_mode in map_modes {
                for headered in [false, true] {
                    for corrupt_checksum in [false, true] {
                        for overwrite in [false, true] {
                            let destination = path();
                            if overwrite {
                                fs::write(&destination, b"approved overwrite target").unwrap();
                            }
                            let mut app = AppState::default();
                            app.load_rom(lifecycle_variant_rom(
                                title,
                                region,
                                map_mode,
                                headered,
                                corrupt_checksum,
                            ))
                            .unwrap();
                            app.dispatch(Command::CommitRomWrites {
                                expected_revision: app.project_revision(),
                                description: "cross-variant Save As".into(),
                                writes: vec![RomWrite {
                                    offset: 0x11,
                                    bytes: vec![0x77],
                                }],
                            })
                            .unwrap();
                            let expected = app.project().unwrap().save_snapshot();
                            let (request_id, bytes) = app
                                .dispatch(Command::SaveAs)
                                .unwrap()
                                .into_iter()
                                .find_map(|effect| match effect {
                                    FrontendEffect::ChooseSaveDestination { request_id, bytes } => {
                                        Some((request_id, bytes))
                                    }
                                    _ => None,
                                })
                                .unwrap();
                            assert_eq!(bytes, expected);

                            let mut state = EffectState::default();
                            state.complete_save_destination(
                                &mut app,
                                request_id,
                                &bytes,
                                destination.clone(),
                            );
                            let completion = state.persistence.wait_for_test();
                            assert_eq!(
                                completion.target,
                                if overwrite {
                                    crate::persistence_worker::PersistenceTarget::ReplaceAs(
                                        destination.clone(),
                                    )
                                } else {
                                    crate::persistence_worker::PersistenceTarget::Create(
                                        destination.clone(),
                                    )
                                }
                            );
                            state.complete_persistence(&context, &mut app, completion);

                            assert_eq!(fs::read(&destination).unwrap(), expected);
                            assert_eq!(app.document_path.as_deref(), Some(destination.as_path()));
                            assert!(!app.project().unwrap().is_modified());
                            let subsequent = app.dispatch(Command::Save).unwrap();
                            assert!(matches!(
                                subsequent.as_slice(),
                                [FrontendEffect::PersistRomAt { path, .. }]
                                    if path == &destination
                            ));
                            app.cancel_save(app.pending_save_request_id().unwrap())
                                .unwrap();
                            fs::remove_file(destination).unwrap();
                            completed += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(completed, 96);
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
