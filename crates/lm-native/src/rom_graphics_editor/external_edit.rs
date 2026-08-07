use eframe::egui;
use lm_app::{ExternalTool, ToolContext, ToolInvocation};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
const MAX_TEMP_ATTEMPTS: usize = 64;

struct PendingEdit {
    invocation: ToolInvocation,
    directory: PathBuf,
    path: PathBuf,
    expected_revision: u64,
    expected_len: usize,
}

struct RunningEdit {
    directory: PathBuf,
    path: PathBuf,
    expected_revision: u64,
    result: Receiver<Result<Vec<u8>, String>>,
}

pub(super) struct ExternalEditCompletion {
    pub(super) expected_revision: u64,
    pub(super) bytes: Vec<u8>,
}

#[derive(Default)]
pub(super) struct ExternalGraphicsEditor {
    pending: Option<PendingEdit>,
    running: Option<RunningEdit>,
}

impl ExternalGraphicsEditor {
    pub(super) const fn is_running(&self) -> bool {
        self.pending.is_some() || self.running.is_some()
    }

    pub(super) fn stage(
        &mut self,
        executable: PathBuf,
        file_name: &str,
        bytes: &[u8],
        expected_revision: u64,
    ) -> Result<(), String> {
        if self.is_running() {
            return Err("an external graphics edit is already pending".into());
        }
        if executable.as_os_str().is_empty() {
            return Err("external graphics editor executable is empty".into());
        }
        self.stage_with(
            file_name,
            bytes,
            expected_revision,
            move |directory, path| {
                Ok(ToolInvocation {
                    tool_id: "graphics-editor".into(),
                    executable,
                    arguments: vec![path.to_string_lossy().into_owned()],
                    working_directory: Some(directory.to_path_buf()),
                })
            },
        )
    }

    pub(super) fn stage_configured(
        &mut self,
        tool: &ExternalTool,
        context: ToolContext<'_>,
        file_name: &str,
        bytes: &[u8],
        expected_revision: u64,
    ) -> Result<(), String> {
        if !tool.uses_graphics_editor_argument() {
            return Err(format!(
                "configured external tool {:?} does not reference {{graphics}} or %1",
                tool.id
            ));
        }
        if tool.uses_graphics_editor_working_directory() {
            return Err(format!(
                "configured external tool {:?} cannot use {{graphics}} or %1 as its working directory",
                tool.id
            ));
        }
        self.stage_with(file_name, bytes, expected_revision, |_, path| {
            tool.expand_graphics_editor(ToolContext {
                graphics: Some(path),
                ..context
            })
            .map_err(|error| error.to_string())
        })
    }

    fn stage_with(
        &mut self,
        file_name: &str,
        bytes: &[u8],
        expected_revision: u64,
        invocation: impl FnOnce(&Path, &Path) -> Result<ToolInvocation, String>,
    ) -> Result<(), String> {
        if self.is_running() {
            return Err("an external graphics edit is already pending".into());
        }
        let (directory, path) = create_staged_file(file_name, bytes)?;
        let invocation = match invocation(&directory, &path) {
            Ok(invocation) => invocation,
            Err(error) => {
                return Err(match remove_private_directory(&directory) {
                    Ok(()) => error,
                    Err(cleanup) => format!("{error}; {cleanup}"),
                });
            }
        };
        self.pending = Some(PendingEdit {
            invocation,
            directory,
            path,
            expected_revision,
            expected_len: bytes.len(),
        });
        Ok(())
    }

    pub(super) fn show(
        &mut self,
        context: &egui::Context,
    ) -> Option<Result<ExternalEditCompletion, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            egui::Window::new("External graphics editor running")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!("Waiting for {}", running.path.display()));
                    ui.label("The staged file will reload after the editor exits successfully.");
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
            return completion;
        }
        let Some(pending) = &self.pending else {
            return completion;
        };
        let mut run = false;
        let mut cancel = false;
        egui::Window::new("Open staged graphics externally?")
            .collapsible(false)
            .resizable(true)
            .show(context, |ui| {
                ui.label(format!(
                    "Executable: {}",
                    pending.invocation.executable.display()
                ));
                ui.label(format!("Staged file: {}", pending.path.display()));
                ui.label("Arguments are passed directly without a command shell:");
                for (index, argument) in pending.invocation.arguments.iter().enumerate() {
                    ui.monospace(format!("argument[{index}] = {argument:?}"));
                }
                ui.horizontal(|ui| {
                    cancel = ui.button("Cancel").clicked();
                    run = ui.button("Run editor").clicked();
                });
            });
        if cancel {
            if let Err(error) = self.cancel_pending() {
                return Some(Err(error));
            }
        } else if run && let Err(error) = self.launch_pending() {
            return Some(Err(error));
        }
        completion
    }

    fn cancel_pending(&mut self) -> Result<(), String> {
        let Some(pending) = self.pending.take() else {
            return Ok(());
        };
        remove_private_directory(&pending.directory)
    }

    fn launch_pending(&mut self) -> Result<(), String> {
        let pending = self
            .pending
            .take()
            .ok_or("external graphics edit is not awaiting approval")?;
        let PendingEdit {
            invocation,
            directory,
            path,
            expected_revision,
            expected_len,
        } = pending;
        let worker_directory = directory.clone();
        let worker_path = path.clone();
        let (sender, result) = mpsc::channel();
        let spawn = std::thread::Builder::new()
            .name("lm-external-graphics-editor".into())
            .spawn(move || {
                let result =
                    run_and_reload(&invocation, &worker_path, &worker_directory, expected_len);
                let _send_result = sender.send(result);
            });
        if let Err(error) = spawn {
            let cleanup = remove_private_directory(&directory);
            return Err(cleanup.err().unwrap_or_else(|| {
                format!("could not create external graphics editor worker: {error}")
            }));
        }
        self.running = Some(RunningEdit {
            directory,
            path,
            expected_revision,
            result,
        });
        Ok(())
    }

    fn poll(&mut self) -> Option<Result<ExternalEditCompletion, String>> {
        let running = self.running.as_ref()?;
        match running.result.try_recv() {
            Ok(result) => {
                let expected_revision = running.expected_revision;
                self.running = None;
                Some(result.map(|bytes| ExternalEditCompletion {
                    expected_revision,
                    bytes,
                }))
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                let directory = running.directory.clone();
                self.running = None;
                let cleanup = remove_private_directory(&directory);
                Some(Err(cleanup.err().unwrap_or_else(|| {
                    "external graphics editor worker stopped without reporting a result".into()
                })))
            }
        }
    }
}

impl Drop for ExternalGraphicsEditor {
    fn drop(&mut self) {
        if let Some(pending) = self.pending.take() {
            let _cleanup = remove_private_directory(&pending.directory);
        }
    }
}

fn create_staged_file(file_name: &str, bytes: &[u8]) -> Result<(PathBuf, PathBuf), String> {
    if Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        != Some(file_name)
    {
        return Err("external graphics staged filename is invalid".into());
    }
    for _ in 0..MAX_TEMP_ATTEMPTS {
        let nonce = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("lm-graphics-edit-{}-{nonce}", std::process::id()));
        match fs::create_dir(&directory) {
            Ok(()) => {
                let path = directory.join(file_name);
                let result = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()));
                if let Err(error) = result {
                    let _cleanup = remove_private_directory(&directory);
                    return Err(format!("could not stage external graphics file: {error}"));
                }
                return Ok((directory, path));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(format!(
                    "could not create external graphics workspace: {error}"
                ));
            }
        }
    }
    Err("could not reserve a unique external graphics workspace".into())
}

fn run_and_reload(
    invocation: &ToolInvocation,
    path: &Path,
    directory: &Path,
    expected_len: usize,
) -> Result<Vec<u8>, String> {
    let result = crate::external_tools::execute(invocation).and_then(|()| {
        crate::dialogs::read_regular_bounded(
            path,
            u64::try_from(expected_len).unwrap_or(u64::MAX),
            "externally edited graphics file",
        )
        .map_err(|error| format!("could not reload externally edited graphics: {error}"))
        .and_then(|bytes| {
            if bytes.len() == expected_len {
                Ok(bytes)
            } else {
                Err(format!(
                    "externally edited graphics changed size from {expected_len} to {} bytes",
                    bytes.len()
                ))
            }
        })
    });
    let cleanup = remove_private_directory(directory);
    match (result, cleanup) {
        (Ok(bytes), Ok(())) => Ok(bytes),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(format!("{error}; {cleanup}")),
    }
}

fn remove_private_directory(directory: &Path) -> Result<(), String> {
    fs::remove_dir_all(directory).map_err(|error| {
        format!(
            "could not remove external graphics workspace {}: {error}",
            directory.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_tool(arguments: &[&str]) -> ExternalTool {
        ExternalTool {
            id: "configured-graphics".into(),
            name: "Configured Graphics".into(),
            executable: PathBuf::from("editor"),
            arguments: arguments
                .iter()
                .map(|argument| (*argument).into())
                .collect(),
            working_directory: Some("{project_dir}".into()),
            subscriptions: Vec::new(),
        }
    }

    #[test]
    fn cancellation_removes_the_private_staged_file() {
        let mut editor = ExternalGraphicsEditor::default();
        editor
            .stage(PathBuf::from("editor"), "ExGFX123.bin", &[7; 32], 4)
            .unwrap();
        let pending = editor.pending.as_ref().unwrap();
        let directory = pending.directory.clone();
        assert_eq!(fs::read(&pending.path).unwrap(), [7; 32]);
        assert_eq!(
            pending.invocation.arguments,
            [pending.path.to_string_lossy().into_owned()]
        );
        assert_eq!(
            pending.invocation.working_directory.as_deref(),
            Some(directory.as_path())
        );
        editor.cancel_pending().unwrap();
        assert!(!directory.exists());
    }

    #[test]
    #[cfg(unix)]
    fn successful_process_reloads_exact_bytes_and_cleans_up() {
        let mut editor = ExternalGraphicsEditor::default();
        editor
            .stage(PathBuf::from("/usr/bin/true"), "GFX00.bin", &[9; 64], 17)
            .unwrap();
        let directory = editor.pending.as_ref().unwrap().directory.clone();
        fs::write(&editor.pending.as_ref().unwrap().path, [8; 64]).unwrap();
        editor.launch_pending().unwrap();
        let running = editor.running.take().unwrap();
        let bytes = running
            .result
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(bytes, [8; 64]);
        assert!(!directory.exists());
    }

    #[test]
    #[cfg(unix)]
    fn oversized_replacement_is_rejected_and_cleaned_up() {
        let mut editor = ExternalGraphicsEditor::default();
        editor
            .stage(PathBuf::from("/usr/bin/true"), "GFX00.bin", &[1; 32], 0)
            .unwrap();
        let pending = editor.pending.as_ref().unwrap();
        let directory = pending.directory.clone();
        fs::write(&pending.path, [2; 33]).unwrap();
        editor.launch_pending().unwrap();
        let running = editor.running.take().unwrap();
        assert!(
            running
                .result
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap_err()
                .contains("application bound")
        );
        assert!(!directory.exists());
    }

    #[test]
    #[cfg(unix)]
    fn truncated_replacement_and_failed_process_both_clean_up() {
        let mut truncated = ExternalGraphicsEditor::default();
        truncated
            .stage(PathBuf::from("/usr/bin/true"), "GFX00.bin", &[1; 32], 0)
            .unwrap();
        let pending = truncated.pending.as_ref().unwrap();
        let directory = pending.directory.clone();
        fs::write(&pending.path, [2; 31]).unwrap();
        truncated.launch_pending().unwrap();
        let running = truncated.running.take().unwrap();
        assert!(
            running
                .result
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap_err()
                .contains("changed size")
        );
        assert!(!directory.exists());

        let mut failed = ExternalGraphicsEditor::default();
        failed
            .stage(PathBuf::from("/usr/bin/false"), "GFX00.bin", &[3; 32], 0)
            .unwrap();
        let directory = failed.pending.as_ref().unwrap().directory.clone();
        failed.launch_pending().unwrap();
        let running = failed.running.take().unwrap();
        assert!(
            running
                .result
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap_err()
                .contains("exited unsuccessfully")
        );
        assert!(!directory.exists());
    }

    #[test]
    fn dropping_an_unapproved_edit_cleans_up() {
        let directory = {
            let mut editor = ExternalGraphicsEditor::default();
            editor
                .stage(PathBuf::from("editor"), "GFX00.bin", &[4; 32], 0)
                .unwrap();
            editor.pending.as_ref().unwrap().directory.clone()
        };
        assert!(!directory.exists());
    }

    #[test]
    fn configured_tool_expands_the_private_path_and_ordinary_project_context() {
        let mut editor = ExternalGraphicsEditor::default();
        let tool = configured_tool(&["--gfx", "{graphics}", "--rom={rom}"]);
        editor
            .stage_configured(
                &tool,
                ToolContext {
                    rom: Some(Path::new("/tmp/Project/game.smc")),
                    level: Some(0x105),
                    graphics: None,
                },
                "ExGFX123.bin",
                &[6; 32],
                19,
            )
            .unwrap();
        let pending = editor.pending.as_ref().unwrap();
        assert_eq!(pending.invocation.tool_id, "configured-graphics");
        assert_eq!(pending.invocation.arguments[0], "--gfx");
        assert_eq!(
            pending.invocation.arguments[1],
            pending.path.to_string_lossy()
        );
        assert_eq!(
            pending.invocation.arguments[2],
            "--rom=/tmp/Project/game.smc"
        );
        assert_eq!(
            pending.invocation.working_directory,
            Some(PathBuf::from("/tmp/Project"))
        );
    }

    #[test]
    fn configured_tool_accepts_lunar_magics_percent_one_graphics_template() {
        let mut editor = ExternalGraphicsEditor::default();
        let tool = configured_tool(&["%1", "--unchanged=%2", "--gfx=%1"]);
        editor
            .stage_configured(
                &tool,
                ToolContext {
                    rom: Some(Path::new("/tmp/project/game.smc")),
                    ..ToolContext::default()
                },
                "GFX00.bin",
                &[9; 32],
                23,
            )
            .unwrap();
        let pending = editor.pending.as_ref().unwrap();
        let path = pending.path.to_string_lossy();
        assert_eq!(pending.invocation.arguments[0], path);
        assert_eq!(pending.invocation.arguments[1], "--unchanged=%2");
        assert_eq!(pending.invocation.arguments[2], format!("--gfx={path}"));
    }

    #[test]
    fn configured_template_failure_removes_the_private_workspace() {
        let mut editor = ExternalGraphicsEditor::default();
        let observed = std::cell::RefCell::new(None);
        let error = editor
            .stage_with("GFX00.bin", &[1; 32], 0, |directory, _| {
                *observed.borrow_mut() = Some(directory.to_path_buf());
                Err("template expansion failed".into())
            })
            .unwrap_err();
        assert_eq!(error, "template expansion failed");
        assert!(!observed.into_inner().unwrap().exists());
        assert!(!editor.is_running());
    }

    #[test]
    fn configured_tool_without_graphics_placeholder_is_rejected_before_staging() {
        let mut editor = ExternalGraphicsEditor::default();
        let error = editor
            .stage_configured(
                &configured_tool(&["--rom={rom}"]),
                ToolContext {
                    rom: Some(Path::new("/tmp/game.smc")),
                    ..ToolContext::default()
                },
                "GFX00.bin",
                &[1; 32],
                0,
            )
            .unwrap_err();
        assert!(
            error.contains("does not reference {graphics} or %1"),
            "{error}"
        );
        assert!(!editor.is_running());
    }

    #[test]
    fn configured_tool_cannot_use_the_staged_file_as_its_working_directory() {
        let mut editor = ExternalGraphicsEditor::default();
        let mut tool = configured_tool(&["{graphics}"]);
        tool.working_directory = Some("{graphics}".into());
        let error = editor
            .stage_configured(&tool, ToolContext::default(), "GFX00.bin", &[1; 32], 0)
            .unwrap_err();
        assert!(error.contains("cannot use {graphics} or %1"), "{error}");
        assert!(!editor.is_running());
    }
}
