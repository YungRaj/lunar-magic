//! Permission-gated, non-blocking native external-tool process lifecycle.

use crate::external_tools;
use eframe::egui;
use lm_app::ToolInvocation;
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, TryRecvError};

const MAX_PENDING: usize = 64;

struct RunningTool {
    tool_id: String,
    result: Receiver<Result<(), String>>,
}

#[derive(Default)]
pub(crate) struct ExternalToolLauncher {
    pending: VecDeque<ToolInvocation>,
    running: Option<RunningTool>,
}

impl ExternalToolLauncher {
    pub(crate) fn enqueue(&mut self, invocation: ToolInvocation) -> Result<(), String> {
        if self.pending.len() >= MAX_PENDING {
            return Err(format!(
                "external-tool permission queue exceeds its {MAX_PENDING}-request limit"
            ));
        }
        self.pending.push_back(invocation);
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
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
            return completion;
        }

        let Some(invocation) = self.pending.front().cloned() else {
            return completion;
        };
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
            self.pending.pop_front();
            if let Err(error) = self.start(invocation) {
                return Some(Err(error));
            }
        }
        completion
    }

    fn start(&mut self, invocation: ToolInvocation) -> Result<(), String> {
        let tool_id = invocation.tool_id.clone();
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name(format!("lm-tool-{tool_id}"))
            .spawn(move || {
                let _send_result = sender.send(external_tools::execute(&invocation));
            })
            .map_err(|error| format!("could not create external-tool worker: {error}"))?;
        self.running = Some(RunningTool { tool_id, result });
        Ok(())
    }

    fn poll(&mut self) -> Option<Result<String, String>> {
        let running = self.running.as_ref()?;
        match running.result.try_recv() {
            Ok(result) => {
                let tool_id = running.tool_id.clone();
                self.running = None;
                Some(result.map(|()| format!("External tool {tool_id:?} completed successfully")))
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
        assert_eq!(launcher.pending.front(), Some(&invocation(0)));
        assert_eq!(launcher.pending.back(), Some(&invocation(MAX_PENDING - 1)));
        assert!(launcher.running.is_none());
    }

    #[test]
    fn disconnected_worker_is_reported_and_releases_running_state() {
        let (sender, receiver) = mpsc::channel::<Result<(), String>>();
        drop(sender);
        let mut launcher = ExternalToolLauncher {
            pending: VecDeque::new(),
            running: Some(RunningTool {
                tool_id: "emu".into(),
                result: receiver,
            }),
        };
        assert!(matches!(launcher.poll(), Some(Err(error)) if error.contains("emu")));
        assert!(launcher.running.is_none());
    }

    #[test]
    fn worker_reports_process_start_failure_without_blocking_the_caller() {
        let mut launcher = ExternalToolLauncher::default();
        launcher
            .start(ToolInvocation {
                tool_id: "missing".into(),
                executable: PathBuf::from("path-that-cannot-name-a-real-lm-native-test-program"),
                arguments: Vec::new(),
                working_directory: None,
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
