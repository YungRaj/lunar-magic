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
    instance_id: u64,
    tool_id: String,
    result: Receiver<Result<external_tools::ProcessCompletion, String>>,
    cancel: mpsc::Sender<()>,
}

struct PendingTool {
    invocation: ToolInvocation,
    workspace: Option<TempDir>,
    options: LaunchOptions,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct LaunchOptions {
    pub(crate) allow_multiple_instances: bool,
    pub(crate) hide_console_window: bool,
    pub(crate) open_other: bool,
}

#[derive(Default)]
pub(crate) struct ExternalToolLauncher {
    pending: VecDeque<PendingTool>,
    running: Vec<RunningTool>,
    next_instance_id: u64,
}

impl ExternalToolLauncher {
    #[cfg(test)]
    pub(crate) fn pending_tool_ids(&self) -> Vec<&str> {
        self.pending
            .iter()
            .map(|pending| pending.invocation.tool_id.as_str())
            .collect()
    }

    pub(crate) fn stop_tool(&mut self, tool_id: &str) -> bool {
        let pending_before = self.pending.len();
        self.pending
            .retain(|pending| pending.invocation.tool_id != tool_id);
        let mut stopped = self.pending.len() != pending_before;
        for running in self
            .running
            .iter()
            .filter(|running| running.tool_id == tool_id)
        {
            let _ = running.cancel.send(());
            stopped = true;
        }
        stopped
    }

    pub(crate) fn enqueue(&mut self, invocation: ToolInvocation) -> Result<(), String> {
        self.enqueue_with_options(invocation, LaunchOptions::default())
    }

    pub(crate) fn enqueue_with_options(
        &mut self,
        invocation: ToolInvocation,
        options: LaunchOptions,
    ) -> Result<(), String> {
        if !options.allow_multiple_instances
            && (self
                .pending
                .iter()
                .any(|pending| pending.invocation.tool_id == invocation.tool_id)
                || self
                    .running
                    .iter()
                    .any(|running| running.tool_id == invocation.tool_id))
        {
            return Ok(());
        }
        self.enqueue_pending(PendingTool {
            invocation,
            workspace: None,
            options,
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
            options: LaunchOptions::default(),
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
        for running in &self.running {
            egui::Window::new(format!("External tool {:?} running", running.tool_id))
                .id(egui::Id::new((
                    "external-tool-running",
                    running.instance_id,
                )))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!("Waiting for external tool {:?}", running.tool_id));
                    if ui.button("Stop").clicked() {
                        let _cancelled = running.cancel.send(());
                    }
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
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
        let options = pending.options;
        let tool_id = invocation.tool_id.clone();
        let (sender, result) = mpsc::channel();
        let (cancel, cancellation) = mpsc::channel();
        std::thread::Builder::new()
            .name(format!("lm-tool-{tool_id}"))
            .spawn(move || {
                let execution = if options.open_other {
                    external_tools::execute_associated(&invocation)
                        .map(|()| external_tools::ProcessCompletion::Exited)
                } else {
                    external_tools::execute_cancellable(
                        &invocation,
                        &cancellation,
                        external_tools::ProcessOptions {
                            hide_console_window: options.hide_console_window,
                        },
                    )
                };
                drop(workspace);
                let _send_result = sender.send(execution);
            })
            .map_err(|error| format!("could not create external-tool worker: {error}"))?;
        let instance_id = self.next_instance_id;
        self.next_instance_id = self.next_instance_id.wrapping_add(1);
        self.running.push(RunningTool {
            instance_id,
            tool_id,
            result,
            cancel,
        });
        Ok(())
    }

    fn poll(&mut self) -> Option<Result<String, String>> {
        for index in 0..self.running.len() {
            match self.running[index].result.try_recv() {
                Ok(result) => {
                    let tool_id = self.running.swap_remove(index).tool_id;
                    return Some(result.map(|completion| match completion {
                        external_tools::ProcessCompletion::Exited => {
                            format!("External tool {tool_id:?} completed successfully")
                        }
                        external_tools::ProcessCompletion::Stopped => {
                            format!("External tool {tool_id:?} stopped")
                        }
                    }));
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    let tool_id = self.running.swap_remove(index).tool_id;
                    return Some(Err(format!(
                        "external-tool worker for {tool_id:?} stopped without reporting a result"
                    )));
                }
            }
        }
        None
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
        assert!(launcher.running.is_empty());
    }

    #[test]
    fn stop_tool_removes_only_matching_pending_requests() {
        let mut launcher = ExternalToolLauncher::default();
        launcher.enqueue(invocation(1)).unwrap();
        launcher.enqueue(invocation(2)).unwrap();
        launcher.enqueue(invocation(1)).unwrap();
        assert!(launcher.stop_tool("tool-1"));
        assert_eq!(launcher.pending.len(), 1);
        assert_eq!(launcher.pending[0].invocation.tool_id, "tool-2");
        assert!(!launcher.stop_tool("missing"));
    }

    #[test]
    fn disconnected_worker_is_reported_and_releases_running_state() {
        let (sender, receiver) =
            mpsc::channel::<Result<external_tools::ProcessCompletion, String>>();
        drop(sender);
        let (cancel, _cancellation) = mpsc::channel();
        let mut launcher = ExternalToolLauncher {
            pending: VecDeque::new(),
            running: vec![RunningTool {
                instance_id: 0,
                tool_id: "emu".into(),
                result: receiver,
                cancel,
            }],
            next_instance_id: 1,
        };
        assert!(matches!(launcher.poll(), Some(Err(error)) if error.contains("emu")));
        assert!(launcher.running.is_empty());
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
                options: LaunchOptions::default(),
            })
            .unwrap();
        let running = launcher.running.pop().unwrap();
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
                options: LaunchOptions::default(),
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
                options: LaunchOptions::default(),
            })
            .unwrap();
        let running = launcher.running.pop().unwrap();
        let result = running
            .result
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("worker must report a process result");
        assert!(matches!(result, Err(error) if error.contains("missing")));
    }

    #[test]
    fn default_policy_deduplicates_a_tool_but_allow_multiple_retains_each_request() {
        let mut launcher = ExternalToolLauncher::default();
        launcher.enqueue(invocation(1)).unwrap();
        launcher.enqueue(invocation(1)).unwrap();
        assert_eq!(launcher.pending.len(), 1);
        launcher
            .enqueue_with_options(
                invocation(1),
                LaunchOptions {
                    allow_multiple_instances: true,
                    hide_console_window: true,
                    open_other: false,
                },
            )
            .unwrap();
        assert_eq!(launcher.pending.len(), 2);
        assert_eq!(
            launcher.pending[1].options,
            LaunchOptions {
                allow_multiple_instances: true,
                hide_console_window: true,
                open_other: false,
            }
        );
    }

    #[test]
    #[cfg(unix)]
    fn multiple_instances_are_tracked_and_cancelled_independently() {
        let mut launcher = ExternalToolLauncher::default();
        for _ in 0..2 {
            launcher
                .start(PendingTool {
                    invocation: ToolInvocation {
                        tool_id: "parallel".into(),
                        executable: "/bin/sleep".into(),
                        arguments: vec!["30".into()],
                        working_directory: None,
                    },
                    workspace: None,
                    options: LaunchOptions {
                        allow_multiple_instances: true,
                        hide_console_window: false,
                        open_other: false,
                    },
                })
                .unwrap();
        }
        assert_eq!(launcher.running.len(), 2);
        assert_ne!(
            launcher.running[0].instance_id,
            launcher.running[1].instance_id
        );
        assert!(launcher.stop_tool("parallel"));
        for running in launcher.running.drain(..) {
            assert_eq!(
                running
                    .result
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap()
                    .unwrap(),
                external_tools::ProcessCompletion::Stopped
            );
        }
    }
}
