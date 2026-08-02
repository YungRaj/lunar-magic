//! Permission-gated, non-blocking native external-tool process lifecycle.

use crate::external_tools;
use eframe::egui;
use lm_app::{EmulatorTestRequest, ToolContext, ToolInvocation};
use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use tempfile::TempDir;

const MAX_PENDING: usize = 64;

struct RunningTool {
    tool_id: String,
    result: Receiver<Result<external_tools::ProcessCompletion, String>>,
    cancel: mpsc::Sender<()>,
}

struct PendingTool {
    invocation: ToolInvocation,
    workspace: Option<TempDir>,
}

#[derive(Default)]
pub(crate) struct ExternalToolLauncher {
    pending: VecDeque<PendingTool>,
    running: Option<RunningTool>,
}

impl ExternalToolLauncher {
    pub(crate) fn enqueue(&mut self, invocation: ToolInvocation) -> Result<(), String> {
        self.enqueue_pending(PendingTool {
            invocation,
            workspace: None,
        })
    }

    pub(crate) fn enqueue_emulator_test(
        &mut self,
        request: EmulatorTestRequest,
    ) -> Result<(), String> {
        if self.pending.len() >= MAX_PENDING {
            return Err(format!(
                "external-tool permission queue exceeds its {MAX_PENDING}-request limit"
            ));
        }
        let workspace = tempfile::Builder::new()
            .prefix("lunar-magic-rust-level-test-")
            .tempdir()
            .map_err(|error| format!("could not create private emulator workspace: {error}"))?;
        let suffix = if request.rom_bytes.len() % 0x8000 == 512 {
            "smc"
        } else {
            "sfc"
        };
        let rom_path = workspace.path().join(format!(
            "level-{:03X}-revision-{}.{}",
            request.level, request.revision, suffix
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&rom_path)
            .map_err(|error| format!("could not create staged emulator ROM: {error}"))?;
        file.write_all(&request.rom_bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("could not write staged emulator ROM: {error}"))?;
        let invocation = request
            .tool
            .expand(ToolContext {
                rom: Some(&rom_path),
                level: Some(request.level),
                graphics: None,
            })
            .map_err(|error| error.to_string())?;
        self.enqueue_pending(PendingTool {
            invocation,
            workspace: Some(workspace),
        })
    }

    fn enqueue_pending(&mut self, pending: PendingTool) -> Result<(), String> {
        if self.pending.len() >= MAX_PENDING {
            return Err(format!(
                "external-tool permission queue exceeds its {MAX_PENDING}-request limit"
            ));
        }
        self.pending.push_back(pending);
        Ok(())
    }

    /// Draws permission/running state and returns one completed launch result, if available.
    pub(crate) fn show(&mut self, context: &egui::Context) -> Option<Result<String, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            egui::Window::new("External tool running")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!("Waiting for external tool {:?}", running.tool_id));
                    if ui.button("Stop").clicked() {
                        let _cancelled = running.cancel.send(());
                    }
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
            return completion;
        }

        let Some(pending) = self.pending.front() else {
            return completion;
        };
        let invocation = &pending.invocation;
        let mut approve = false;
        let mut deny = false;
        egui::Window::new("Allow external tool?")
            .collapsible(false)
            .resizable(true)
            .show(context, |ui| {
                ui.label(format!("Tool ID: {:?}", invocation.tool_id));
                ui.label(format!("Executable: {}", invocation.executable.display()));
                ui.label(format!(
                    "Working directory: {}",
                    invocation
                        .working_directory
                        .as_deref()
                        .map_or_else(|| "<inherited>".into(), |path| path.display().to_string())
                ));
                ui.separator();
                ui.label("Arguments are passed directly without a command shell:");
                for (index, argument) in invocation.arguments.iter().enumerate() {
                    ui.monospace(format!("argument[{index}] = {argument:?}"));
                }
                ui.horizontal(|ui| {
                    deny = ui.button("Deny").clicked();
                    approve = ui.button("Run").clicked();
                });
            });
        if deny {
            self.pending.pop_front();
        } else if approve {
            let pending = self.pending.pop_front().expect("front entry still exists");
            if let Err(error) = self.start(pending) {
                return Some(Err(error));
            }
        }
        completion
    }

    fn start(&mut self, pending: PendingTool) -> Result<(), String> {
        let invocation = pending.invocation;
        let workspace = pending.workspace;
        let tool_id = invocation.tool_id.clone();
        let (sender, result) = mpsc::channel();
        let (cancel, cancellation) = mpsc::channel();
        std::thread::Builder::new()
            .name(format!("lm-tool-{tool_id}"))
            .spawn(move || {
                let execution = external_tools::execute_cancellable(&invocation, &cancellation);
                drop(workspace);
                let _send_result = sender.send(execution);
            })
            .map_err(|error| format!("could not create external-tool worker: {error}"))?;
        self.running = Some(RunningTool {
            tool_id,
            result,
            cancel,
        });
        Ok(())
    }

    fn poll(&mut self) -> Option<Result<String, String>> {
        let running = self.running.as_ref()?;
        match running.result.try_recv() {
            Ok(result) => {
                let tool_id = running.tool_id.clone();
                self.running = None;
                Some(result.map(|completion| match completion {
                    external_tools::ProcessCompletion::Exited => {
                        format!("External tool {tool_id:?} completed successfully")
                    }
                    external_tools::ProcessCompletion::Stopped => {
                        format!("External tool {tool_id:?} stopped")
                    }
                }))
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                let tool_id = running.tool_id.clone();
                self.running = None;
                Some(Err(format!(
                    "external-tool worker for {tool_id:?} stopped without reporting a result"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::ExternalTool;
    use std::path::PathBuf;

    fn invocation(index: usize) -> ToolInvocation {
        ToolInvocation {
            tool_id: format!("tool-{index}"),
            executable: PathBuf::from(format!("program-{index}")),
            arguments: vec![format!("argument {index}")],
            working_directory: Some(PathBuf::from(format!("directory-{index}"))),
        }
    }

    #[test]
    fn permission_queue_is_bounded_and_preserves_exact_invocations() {
        let mut launcher = ExternalToolLauncher::default();
        for index in 0..MAX_PENDING {
            launcher.enqueue(invocation(index)).unwrap();
        }
        assert!(launcher.enqueue(invocation(MAX_PENDING)).is_err());
        assert_eq!(
            launcher.pending.front().map(|pending| &pending.invocation),
            Some(&invocation(0))
        );
        assert_eq!(
            launcher.pending.back().map(|pending| &pending.invocation),
            Some(&invocation(MAX_PENDING - 1))
        );
        assert!(launcher.running.is_none());
    }

    #[test]
    fn disconnected_worker_is_reported_and_releases_running_state() {
        let (sender, receiver) =
            mpsc::channel::<Result<external_tools::ProcessCompletion, String>>();
        drop(sender);
        let (cancel, _cancellation) = mpsc::channel();
        let mut launcher = ExternalToolLauncher {
            pending: VecDeque::new(),
            running: Some(RunningTool {
                tool_id: "emu".into(),
                result: receiver,
                cancel,
            }),
        };
        assert!(matches!(launcher.poll(), Some(Err(error)) if error.contains("emu")));
        assert!(launcher.running.is_none());
    }

    #[test]
    fn emulator_request_stages_exact_rom_and_denial_removes_workspace() {
        let request = EmulatorTestRequest {
            tool: ExternalTool {
                id: "emu".into(),
                name: "Emulator".into(),
                executable: "emulator".into(),
                arguments: vec!["{rom}".into(), "--level={level_hex}".into()],
                working_directory: Some("{project_dir}".into()),
                subscriptions: Vec::new(),
            },
            revision: 17,
            level: 0x1ab,
            rom_bytes: vec![0xa5; 0x8000],
        };
        let mut launcher = ExternalToolLauncher::default();
        launcher.enqueue_emulator_test(request).unwrap();
        let pending = launcher.pending.front().unwrap();
        let workspace_path = pending.workspace.as_ref().unwrap().path().to_owned();
        let rom_path = PathBuf::from(&pending.invocation.arguments[0]);
        assert_eq!(rom_path.file_name().unwrap(), "level-1AB-revision-17.sfc");
        assert_eq!(pending.invocation.arguments[1], "--level=1AB");
        assert_eq!(
            pending.invocation.working_directory.as_deref(),
            Some(workspace_path.as_path())
        );
        assert_eq!(fs::read(rom_path).unwrap(), vec![0xa5; 0x8000]);

        launcher.pending.pop_front();
        assert!(!workspace_path.exists());
    }

    #[test]
    #[cfg(unix)]
    fn stopping_emulator_reaps_process_and_removes_workspace() {
        let workspace = tempfile::tempdir().unwrap();
        let workspace_path = workspace.path().to_owned();
        let mut launcher = ExternalToolLauncher::default();
        launcher
            .start(PendingTool {
                invocation: ToolInvocation {
                    tool_id: "sleeping-emulator".into(),
                    executable: "/bin/sleep".into(),
                    arguments: vec!["30".into()],
                    working_directory: None,
                },
                workspace: Some(workspace),
            })
            .unwrap();
        let running = launcher.running.take().unwrap();
        running.cancel.send(()).unwrap();
        assert_eq!(
            running
                .result
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap(),
            external_tools::ProcessCompletion::Stopped
        );
        assert!(!workspace_path.exists());
    }

    #[test]
    #[cfg(unix)]
    fn dropping_launcher_stops_owned_process_and_removes_workspace() {
        let workspace = tempfile::tempdir().unwrap();
        let workspace_path = workspace.path().to_owned();
        let mut launcher = ExternalToolLauncher::default();
        launcher
            .start(PendingTool {
                invocation: ToolInvocation {
                    tool_id: "drop-emulator".into(),
                    executable: "/bin/sleep".into(),
                    arguments: vec!["30".into()],
                    working_directory: None,
                },
                workspace: Some(workspace),
            })
            .unwrap();
        drop(launcher);
        for _ in 0..100 {
            if !workspace_path.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("dropping the launcher did not clean the emulator workspace");
    }

    #[test]
    fn worker_reports_process_start_failure_without_blocking_the_caller() {
        let mut launcher = ExternalToolLauncher::default();
        launcher
            .start(PendingTool {
                invocation: ToolInvocation {
                    tool_id: "missing".into(),
                    executable: PathBuf::from(
                        "path-that-cannot-name-a-real-lm-native-test-program",
                    ),
                    arguments: Vec::new(),
                    working_directory: None,
                },
                workspace: None,
            })
            .unwrap();
        let running = launcher.running.take().unwrap();
        let result = running
            .result
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("worker must report a process result");
        assert!(matches!(result, Err(error) if error.contains("missing")));
    }
}
