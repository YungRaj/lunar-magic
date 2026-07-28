use eframe::egui;
use lm_app::{AppState, Command};
use lm_profile::smw_us_v1_title_recording_locator;
use lm_title::TitleScreenRecording;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    revision: u64,
    original: Option<TitleScreenRecording>,
    original_text: String,
    text: String,
}

#[derive(Default)]
pub(crate) struct RomTitleRecordingEditor {
    workspace: Option<Workspace>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomTitleRecordingEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        let result = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())
            .and_then(|project| {
                project
                    .load_title_recording_detected(&smw_us_v1_title_recording_locator())
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(loaded) => {
                let original_text = loaded
                    .recording
                    .as_ref()
                    .map_or_else(String::new, |recording| encode_hex(recording.bytes()));
                self.workspace = Some(Workspace {
                    revision: app.project_revision(),
                    original: loaded.recording,
                    text: original_text.clone(),
                    original_text,
                });
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
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
        command
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
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
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
}
