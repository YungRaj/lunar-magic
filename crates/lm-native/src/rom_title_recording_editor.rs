use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    persistence_worker::{PersistenceTarget, PersistenceWorker},
};
use eframe::egui;
use lm_app::{AppState, Command};
use lm_profile::smw_us_v1_title_recording_recorder_locator;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_TITLE_RECORDING_RECLAIM_FILL,
    smw_us_v1_title_recording_allocation_policy, smw_us_v1_title_recording_locator,
};
use lm_project::TitleRecordingRecorderState;
use lm_title::{
    TitleScreenRecording, decode_snes9x_title_recording, decode_zsnes_title_recording,
    encode_zsnes_title_recording,
};
use std::fmt::Write;

const ZSNES_STATE_LEN: usize = 0x20c13;
const SNES9X_STATE_MAX_LEN: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ImportKind {
    Native,
    Zsnes,
    Snes9x,
}

struct Workspace {
    revision: u64,
    original: Option<TitleScreenRecording>,
    original_text: String,
    text: String,
    recorder: TitleRecordingRecorderState,
}

#[derive(Default)]
pub(crate) struct RomTitleRecordingEditor {
    workspace: Option<Workspace>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    loader: DocumentLoader,
    persistence: PersistenceWorker,
    pending_import: Option<ImportKind>,
}

impl RomTitleRecordingEditor {
    pub(crate) fn stage_recovery_on_project(
        &self,
        app: &AppState,
        staged: &mut lm_project::Project,
    ) -> Result<bool, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("title-recording workspace is closed")?;
        if !workspace.is_dirty() {
            return Ok(false);
        }
        if workspace.revision != app.project_revision() {
            return Err("stale title-recording workspace cannot be recovered".into());
        }
        let recording = parse_recording(&workspace.text)?;
        let locator = smw_us_v1_title_recording_locator();
        let allocation = smw_us_v1_title_recording_allocation_policy(staged.rom.logical_len());
        staged
            .save_title_recording_detected(
                &recording,
                &locator,
                &allocation,
                SMW_US_V1_CHECKSUM_FIELD,
                SMW_US_V1_TITLE_RECORDING_RECLAIM_FILL,
            )
            .map_err(|error| error.to_string())?;
        Ok(true)
    }

    pub(crate) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        let workspace = self.workspace.as_ref()?;
        workspace.is_dirty().then(|| {
            let content_revision = workspace
                .text
                .as_bytes()
                .iter()
                .fold(0x5449_544c_4552_4543_u64, |revision, byte| {
                    revision.rotate_left(5) ^ u64::from(*byte)
                });
            app.project_revision().wrapping_mul(0xbf58_476d_1ce4_e5b9)
                ^ workspace.revision.rotate_left(31)
                ^ content_revision
        })
    }

    pub(crate) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        let workspace = self
            .workspace
            .as_ref()
            .ok_or("title-recording workspace is closed")?;
        if !workspace.is_dirty() {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app.project().ok_or("open a supported ROM first")?.clone();
        self.stage_recovery_on_project(app, &mut staged)?;
        app.recovery_snapshot_with_current_rom(staged.save_snapshot(), app.current_level())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    #[cfg(test)]
    pub(crate) fn set_recording_for_test(&mut self, text: &str) {
        self.workspace.as_mut().expect("workspace open").text = text.into();
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        let result = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())
            .and_then(|project| {
                let playback = project
                    .load_title_recording_detected(&smw_us_v1_title_recording_locator())
                    .map_err(|error| error.to_string())?;
                let recorder = project
                    .load_title_recording_recorder_detected(
                        &smw_us_v1_title_recording_recorder_locator(),
                    )
                    .map_err(|error| error.to_string())?;
                Ok((playback, recorder))
            });
        match result {
            Ok((loaded, recorder)) => {
                let original_text = loaded
                    .recording
                    .as_ref()
                    .map_or_else(String::new, |recording| encode_hex(recording.bytes()));
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: loaded.recording,
                    text: original_text.clone(),
                    original_text,
                    recorder,
                });
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() || self.persistence.is_running() {
            self.error = Some("wait for the title-recording file operation to finish".into());
            return false;
        }
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if !workspace.is_dirty() {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Editor
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> (bool, Option<Command>) {
        self.poll_file_io(context, project_revision);
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new("ROM Title-Screen Recording")
                .default_size([660.0, 520.0])
                .show(context, |ui| {
                    command = self.contents(ui, project_revision);
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_mut()?;
        let stale = workspace.revision != project_revision;
        let busy = self.loader.is_running() || self.persistence.is_running();
        ui.label(
            "Exact Lunar Magic movement payload. Enter two hexadecimal digits per byte; \
             whitespace separates bytes and the final byte must be FF.",
        );
        if workspace.original.is_none() {
            ui.label("No playback patch is installed in this ROM.");
        }
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this recording was opened. Reopen before committing.",
            );
        }
        ui.add(
            egui::TextEdit::multiline(&mut workspace.text)
                .code_editor()
                .desired_rows(18)
                .desired_width(f32::INFINITY),
        );
        let parsed = parse_recording(&workspace.text);
        match &parsed {
            Ok(recording) => {
                ui.label(format!(
                    "{} bytes, terminator present",
                    recording.bytes().len()
                ));
            }
            Err(error) if workspace.text.trim().is_empty() => {
                ui.label("Enter a recording payload to install playback.");
                let _ = error;
            }
            Err(error) => {
                ui.colored_label(egui::Color32::YELLOW, error);
            }
        }
        let mut command = None;
        ui.horizontal(|ui| {
            if ui.button("Minimal payload").clicked() {
                workspace.text = "00 00 00 FF".into();
            }
            if ui
                .add_enabled(parsed.is_ok(), egui::Button::new("Normalize hex"))
                .clicked()
                && let Ok(recording) = &parsed
            {
                workspace.text = encode_hex(recording.bytes());
            }
            if ui
                .add_enabled(
                    workspace.is_dirty() && !stale && parsed.is_ok(),
                    egui::Button::new("Commit recording to ROM"),
                )
                .clicked()
            {
                match Self::prepare_commit(workspace, project_revision) {
                    Ok(prepared) => command = prepared,
                    Err(error) => self.error = Some(error),
                }
            }
            ui.label(if workspace.is_dirty() {
                "Modified"
            } else {
                "Unchanged"
            });
        });
        ui.separator();
        ui.label("Temporary joypad recorder for creating title movements");
        match &workspace.recorder {
            TitleRecordingRecorderState::Absent => {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "The recorder temporarily repurposes overworld RAM. Install it only while recording a level, then uninstall it before loading or creating overworld save states.",
                );
                if ui
                    .add_enabled(
                        !stale && !busy,
                        egui::Button::new("Install temporary joypad recorder"),
                    )
                    .clicked()
                {
                    command = Some(Command::InstallNativeTitleRecordingRecorder {
                        rev: project_revision,
                    });
                }
            }
            TitleRecordingRecorderState::Installed { .. } => {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    "Recorder installed: create the emulator save state now, then uninstall immediately.",
                );
                if ui
                    .add_enabled(
                        !stale && !busy,
                        egui::Button::new("Uninstall temporary joypad recorder"),
                    )
                    .clicked()
                {
                    command = Some(Command::UninstallNativeTitleRecordingRecorder {
                        rev: project_revision,
                    });
                }
            }
        }
        ui.separator();
        ui.label("Recording files and emulator states");
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Import .lmtitle…"))
                .clicked()
                && let Some(path) = dialogs::choose_native_title_recording()
            {
                self.start_import(
                    ImportKind::Native,
                    BoundedRead::new(
                        path,
                        TitleScreenRecording::MAX_FILE_LEN as u64,
                        "native title recording",
                    ),
                );
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Import ZSNES state…"))
                .clicked()
                && let Some(path) = dialogs::choose_zsnes_title_recording_state()
            {
                self.start_import(
                    ImportKind::Zsnes,
                    BoundedRead::new(path, ZSNES_STATE_LEN as u64, "ZSNES title recording state"),
                );
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Import Snes9x state…"))
                .clicked()
                && let Some(path) = dialogs::choose_snes9x_title_recording_state()
            {
                self.start_import(
                    ImportKind::Snes9x,
                    BoundedRead::new(
                        path,
                        SNES9X_STATE_MAX_LEN as u64,
                        "Snes9x title recording state",
                    ),
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !stale && !busy && parsed.is_ok(),
                    egui::Button::new("Export .lmtitle…"),
                )
                .clicked()
                && let Ok(recording) = &parsed
                && let Some(path) = dialogs::choose_native_title_recording_save_path()
            {
                self.start_export(project_revision, path, recording.encode_native_file());
            }
            if ui
                .add_enabled(
                    !stale && !busy && parsed.is_ok(),
                    egui::Button::new("Export ZSNES state…"),
                )
                .clicked()
                && let Ok(recording) = &parsed
                && let Some(path) = dialogs::choose_zsnes_title_recording_save_path()
            {
                self.start_export(
                    project_revision,
                    path,
                    encode_zsnes_title_recording(recording),
                );
            }
        });
        ui.small(
            "Imports stage the exact movement payload for review; Commit recording to ROM applies it. Exports never modify the ROM.",
        );
        command
    }

    fn start_import(&mut self, kind: ImportKind, request: BoundedRead) {
        match self.loader.start(vec![request]) {
            Ok(()) => self.pending_import = Some(kind),
            Err(error) => self.error = Some(error),
        }
    }

    fn start_export(&mut self, revision: u64, path: std::path::PathBuf, bytes: Vec<u8>) {
        if let Err(error) = self
            .persistence
            .start(revision, PersistenceTarget::Create(path), bytes)
        {
            self.error = Some(error);
        }
    }

    fn poll_file_io(&mut self, context: &egui::Context, project_revision: u64) {
        if let Some(result) = self.loader.show(context) {
            let kind = self.pending_import.take();
            let result = result.and_then(|loaded| {
                let [(_, bytes)] = loaded.into_exact::<1>("title recording")?;
                let kind = kind.ok_or("title-recording import kind was lost")?;
                let recording = decode_import(kind, &bytes)?;
                let workspace = self
                    .workspace
                    .as_mut()
                    .ok_or("title-recording workspace is closed")?;
                if workspace.revision != project_revision {
                    return Err("the ROM changed while the title recording was loading".into());
                }
                workspace.text = encode_hex(recording.bytes());
                Ok(())
            });
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
        if let Some(completion) = self.persistence.show(context)
            && let Err(error) = completion.result
        {
            self.error = Some(error);
        }
    }

    fn prepare_commit(
        workspace: &Workspace,
        project_revision: u64,
    ) -> Result<Option<Command>, String> {
        if workspace.revision != project_revision {
            return Err("stale title-recording workspace cannot be committed".into());
        }
        let recording = parse_recording(&workspace.text)?;
        if workspace.original.as_ref() == Some(&recording) {
            return Ok(None);
        }
        Ok(Some(Command::ReplaceNativeTitleRecording {
            rev: workspace.revision,
            recording,
        }))
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard title-recording changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("The edited recording has not been committed to the ROM.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_close = None;
                    }
                    if ui.button("Discard").clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("Title-recording editor error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.pending_close = None;
        self.pending_import = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}

fn decode_import(kind: ImportKind, bytes: &[u8]) -> Result<TitleScreenRecording, String> {
    match kind {
        ImportKind::Native => {
            TitleScreenRecording::decode_native_file(bytes).map_err(|error| error.to_string())
        }
        ImportKind::Zsnes => decode_zsnes_title_recording(bytes).map_err(|error| error.to_string()),
        ImportKind::Snes9x => {
            decode_snes9x_title_recording(bytes).map_err(|error| error.to_string())
        }
    }
}

impl Workspace {
    fn is_dirty(&self) -> bool {
        self.text != self.original_text
    }
}

fn parse_recording(text: &str) -> Result<TitleScreenRecording, String> {
    let mut bytes = Vec::new();
    for (index, token) in text.split_whitespace().enumerate() {
        if token.len() != 2 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "byte {index} must contain exactly two hexadecimal digits"
            ));
        }
        bytes.push(
            u8::from_str_radix(token, 16)
                .map_err(|_| format!("byte {index} is not hexadecimal"))?,
        );
        if bytes.len() > TitleScreenRecording::MAX_LEN {
            return Err(format!(
                "recording exceeds the {}-byte limit",
                TitleScreenRecording::MAX_LEN
            ));
        }
    }
    TitleScreenRecording::from_bytes(bytes).map_err(|error| error.to_string())
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().saturating_mul(3));
    for (index, byte) in bytes.iter().enumerate() {
        if index != 0 {
            if index % 16 == 0 {
                text.push('\n');
            } else {
                text.push(' ');
            }
        }
        write!(text, "{byte:02X}").expect("writing into a String cannot fail");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pristine_app() -> AppState {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app
    }

    #[test]
    fn staged_title_recording_is_recovered_without_committing_live_project() {
        let app = pristine_app();
        let mut editor = RomTitleRecordingEditor::default();
        editor.open(&app);
        editor.workspace.as_mut().unwrap().text = "12 34 56 FF".into();

        assert!(editor.staged_recovery_generation(&app).is_some());
        let recovery = editor.staged_recovery_snapshot(&app).unwrap().unwrap();
        assert_eq!(app.capabilities().project, lm_app::ProjectStatus::OpenClean);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);
        assert!(
            app.project()
                .unwrap()
                .load_title_recording_detected(&smw_us_v1_title_recording_locator())
                .unwrap()
                .recording
                .is_none()
        );

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let recording = reopened
            .project()
            .unwrap()
            .load_title_recording_detected(&smw_us_v1_title_recording_locator())
            .unwrap()
            .recording
            .unwrap();
        assert_eq!(recording.bytes(), [0x12, 0x34, 0x56, 0xff]);
    }

    #[test]
    fn recording_title_and_credits_tilemaps_share_one_recovery_project() {
        let app = pristine_app();
        let live = app.project().unwrap().save_snapshot();
        let mut recording = RomTitleRecordingEditor::default();
        let mut title = crate::rom_tilemap_editor::RomTitleTilemapEditor::default();
        let mut credits = crate::rom_tilemap_editor::RomCreditsTilemapEditor::default();
        recording.open(&app);
        title.open(&app);
        credits.open(&app);
        recording.set_recording_for_test("12 34 56 FF");
        title.set_word_for_test((0, 0, 0), 0x4567);
        credits.set_word_for_test((0, 0xc9, 0), 0x5678);

        let mut staged = app.project().unwrap().clone();
        assert!(
            recording
                .stage_recovery_on_project(&app, &mut staged)
                .unwrap()
        );
        assert!(title.stage_recovery_on_project(&app, &mut staged).unwrap());
        assert!(
            credits
                .stage_recovery_on_project(&app, &mut staged)
                .unwrap()
        );
        let recovery = app
            .recovery_snapshot_with_current_rom(staged.save_snapshot(), app.current_level())
            .unwrap()
            .unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), live);
        assert_eq!(app.project().unwrap().history.undo_len(), 0);

        let mut reopened = AppState::default();
        reopened.load_recovery(recovery).unwrap();
        let project = reopened.project().unwrap();
        assert_eq!(
            project
                .load_title_recording_detected(&smw_us_v1_title_recording_locator())
                .unwrap()
                .recording
                .unwrap()
                .bytes(),
            [0x12, 0x34, 0x56, 0xff]
        );
        let title = project
            .load_title_tilemap_detected(lm_profile::smw_us_v1_title_tilemap_locator())
            .unwrap();
        assert_eq!(
            u16::from_le_bytes([
                title.tilemap.primary_bytes()[0],
                title.tilemap.primary_bytes()[1]
            ]),
            0x4567
        );
        let credits = project
            .load_credits_tilemap_detected(&lm_profile::smw_us_v1_credits_tilemap_locator())
            .unwrap();
        assert_eq!(
            credits.tilemap.words()[0xc9 * lm_overworld::CreditsTilemap::COLUMNS],
            0x5678
        );
        let image = lm_rom::RomImage::from_bytes(project.save_snapshot()).unwrap();
        assert_eq!(
            lm_rom::SnesChecksum::decode(
                image.logical_bytes(),
                lm_profile::SMW_US_V1_CHECKSUM_FIELD
            )
            .unwrap(),
            lm_rom::compute_snes_checksum(
                image.logical_bytes(),
                lm_profile::SMW_US_V1_CHECKSUM_FIELD
            )
            .unwrap()
        );
    }

    #[test]
    fn pristine_recording_install_dispatches_exact_payload_and_reopens() {
        let mut app = pristine_app();
        let mut editor = RomTitleRecordingEditor::default();
        editor.open(&app);
        let workspace = editor.workspace.as_mut().unwrap();
        assert!(workspace.original.is_none());
        assert!(!workspace.is_dirty());
        workspace.text = "12 34 56 ff".into();
        let command = RomTitleRecordingEditor::prepare_commit(workspace, app.project_revision())
            .unwrap()
            .unwrap();
        app.dispatch(command).unwrap();
        let loaded = app
            .project()
            .unwrap()
            .load_title_recording_detected(&smw_us_v1_title_recording_locator())
            .unwrap();
        assert_eq!(loaded.recording.unwrap().bytes(), [0x12, 0x34, 0x56, 0xff]);
        assert_eq!(app.project_revision(), 1);
    }

    #[test]
    fn installed_recording_reopens_clean_and_stale_or_invalid_edits_are_retained() {
        let mut app = pristine_app();
        app.dispatch(Command::ReplaceNativeTitleRecording {
            rev: 0,
            recording: TitleScreenRecording::from_bytes(vec![1, 2, 3, 0xff]).unwrap(),
        })
        .unwrap();
        let mut editor = RomTitleRecordingEditor::default();
        editor.open(&app);
        let workspace = editor.workspace.as_mut().unwrap();
        assert_eq!(workspace.text, "01 02 03 FF");
        assert!(!workspace.is_dirty());
        workspace.text = "01 02 GG FF".into();
        assert!(
            RomTitleRecordingEditor::prepare_commit(workspace, app.project_revision()).is_err()
        );
        assert!(!editor.request_close(false));
        assert_eq!(editor.pending_close, Some(PendingClose::Editor));
        editor.pending_close = None;
        editor.workspace.as_mut().unwrap().text = "01 02 04 FF".into();
        assert!(
            RomTitleRecordingEditor::prepare_commit(
                editor.workspace.as_ref().unwrap(),
                app.project_revision() + 1
            )
            .is_err()
        );
        assert!(editor.is_open());
    }

    #[test]
    fn parser_enforces_exact_tokens_bounds_and_terminator() {
        assert_eq!(
            parse_recording("00 01 02 ff").unwrap().bytes(),
            [0, 1, 2, 0xff]
        );
        assert!(parse_recording("0 01 02 ff").is_err());
        assert!(parse_recording("00, 01 02 ff").is_err());
        assert!(parse_recording("00 01 02 03").is_err());
        assert_eq!(
            encode_hex(&[0; 17]),
            "00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00\n00"
        );
    }

    #[test]
    fn native_zsnes_and_snes9x_file_routes_preserve_the_exact_recording() {
        let recording = TitleScreenRecording::from_bytes(vec![0x12, 0x34, 0x56, 0xff]).unwrap();
        assert_eq!(
            decode_import(ImportKind::Native, &recording.encode_native_file()).unwrap(),
            recording
        );
        let state = encode_zsnes_title_recording(&recording);
        assert_eq!(decode_import(ImportKind::Zsnes, &state).unwrap(), recording);
        let mut snes9x = b"#!s9xsnp:0007\nRAM:131072:".to_vec();
        snes9x.extend_from_slice(&state[0x0c13..]);
        assert_eq!(
            decode_import(ImportKind::Snes9x, &snes9x).unwrap(),
            recording
        );
        assert!(decode_import(ImportKind::Native, &state).is_err());
        assert!(decode_import(ImportKind::Zsnes, &recording.encode_native_file()).is_err());
    }
}
