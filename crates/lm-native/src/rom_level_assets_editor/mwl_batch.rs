use eframe::egui;
use lm_app::{
    ExtendedUiTextKey as Key, LocalizationCatalog, MwlBatchExportMode, ProfiledControllerSnapshot,
};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

struct RunningBatchExport {
    template: PathBuf,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Option<usize>, String>>,
}

impl RunningBatchExport {
    fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
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
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        std::thread::Builder::new()
            .name("lm-batch-mwl-export".into())
            .spawn(move || {
                let result =
                    lm_app::export_smw_us_v1_installed_mwl_batch_until(&profiled, mode, || {
                        worker_cancelled.load(Ordering::Relaxed)
                    })
                    .and_then(|documents| match documents {
                        Some(documents) if !worker_cancelled.load(Ordering::Relaxed) => {
                            lm_app::publish_mwl_batch_new(&worker_template, &documents).map(Some)
                        }
                        Some(_) | None => Ok(None),
                    });
                let _send_result = sender.send(result);
            })
            .map_err(|error| format!("could not create batch-MWL worker: {error}"))?;
        self.running = Some(RunningBatchExport {
            template,
            cancelled,
            result,
        });
        Ok(())
    }

    pub(super) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Option<usize>, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            let cancellation_requested = running.cancelled.load(Ordering::Relaxed);
            egui::Window::new(super::text(catalog, Key::RomNativeAssetsMwlBatchTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(
                        super::text(catalog, Key::RomNativeAssetsMwlBatchPathFormat)
                            .replace("{path}", &running.template.display().to_string()),
                    );
                    ui.label(super::text(catalog, Key::RomNativeAssetsMwlBatchNotice));
                    if cancellation_requested {
                        ui.label(super::text(catalog, Key::RomNativeAssetsMwlBatchCancelling));
                    } else if ui
                        .button(super::text(catalog, Key::RomNativeAssetsCancel))
                        .clicked()
                        || context.input(|input| input.key_pressed(egui::Key::Escape))
                    {
                        running.request_cancel();
                    }
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        completion
    }

    fn poll(&mut self) -> Option<Result<Option<usize>, String>> {
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

#[cfg(test)]
mod tests {
    use super::{MwlBatchExportWorker, RunningBatchExport};
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };

    #[test]
    fn running_export_cancellation_flag_is_shared_with_worker() {
        let (_sender, result) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker = MwlBatchExportWorker {
            running: Some(RunningBatchExport {
                template: PathBuf::from("Export.mwl"),
                cancelled: Arc::clone(&cancelled),
                result,
            }),
        };
        worker.running.as_ref().unwrap().request_cancel();
        assert!(cancelled.load(Ordering::Relaxed));
    }
}
