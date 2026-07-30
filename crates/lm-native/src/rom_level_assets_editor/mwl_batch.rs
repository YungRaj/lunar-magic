use eframe::egui;
use lm_app::{MwlBatchExportMode, ProfiledControllerSnapshot};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

struct RunningBatchExport {
    template: PathBuf,
    result: Receiver<Result<usize, String>>,
}

#[derive(Default)]
pub(super) struct MwlBatchExportWorker {
    running: Option<RunningBatchExport>,
}

impl MwlBatchExportWorker {
    pub(super) const fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(super) fn start(
        &mut self,
        profiled: ProfiledControllerSnapshot,
        template: PathBuf,
        mode: MwlBatchExportMode,
    ) -> Result<(), String> {
        if self.running.is_some() {
            return Err("a batch MWL export is already running".into());
        }
        let (sender, result) = mpsc::channel();
        let worker_template = template.clone();
        std::thread::Builder::new()
            .name("lm-batch-mwl-export".into())
            .spawn(move || {
                let result = lm_app::export_smw_us_v1_installed_mwl_batch(&profiled, mode)
                    .and_then(|documents| {
                        lm_app::publish_mwl_batch_new(&worker_template, &documents)
                    });
                let _send_result = sender.send(result);
            })
            .map_err(|error| format!("could not create batch-MWL worker: {error}"))?;
        self.running = Some(RunningBatchExport { template, result });
        Ok(())
    }

    pub(super) fn show(&mut self, context: &egui::Context) -> Option<Result<usize, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            egui::Window::new("Exporting levels")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!(
                        "Creating numbered MWLs from {}",
                        running.template.display()
                    ));
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        completion
    }

    fn poll(&mut self) -> Option<Result<usize, String>> {
        let running = self.running.as_ref()?;
        match running.result.try_recv() {
            Ok(result) => {
                self.running = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.running = None;
                Some(Err(
                    "batch-MWL worker stopped without reporting a result".into()
                ))
            }
        }
    }
}
