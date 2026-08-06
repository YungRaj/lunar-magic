use eframe::egui;
use lm_app::{AppState, ControllerSnapshot, MwlBatchExportMode, ProfiledControllerSnapshot};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

enum BatchSource {
    Installed(ProfiledControllerSnapshot),
    Builtin(ControllerSnapshot),
}

struct RunningExport {
    template: PathBuf,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Option<usize>, String>>,
}

#[derive(Default)]
pub(crate) struct RomMwlBatchExportDialog {
    running: Option<RunningExport>,
    result: Option<Result<Option<usize>, String>>,
}

impl RomMwlBatchExportDialog {
    pub(crate) const fn is_open(&self) -> bool {
        self.running.is_some() || self.result.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState, mode: MwlBatchExportMode) {
        if self.is_open() {
            return;
        }
        let Some(template) = crate::dialogs::choose_mwl_batch_template() else {
            return;
        };
        let source = match app.profiled_controller_snapshot() {
            Ok(profiled) => BatchSource::Installed(profiled),
            Err(lm_app::AppError::NoRevisionProfile) => match app.controller_snapshot() {
                Ok(snapshot) => BatchSource::Builtin(snapshot),
                Err(error) => {
                    self.result = Some(Err(error.to_string()));
                    return;
                }
            },
            Err(error) => {
                self.result = Some(Err(error.to_string()));
                return;
            }
        };
        if let Err(error) = self.start(source, template, mode) {
            self.result = Some(Err(error));
        }
    }

    fn start(
        &mut self,
        source: BatchSource,
        template: PathBuf,
        mode: MwlBatchExportMode,
    ) -> Result<(), String> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_template = template.clone();
        let (sender, result) = mpsc::channel();
        let spawn = std::thread::Builder::new()
            .name("lm-rom-mwl-batch-export".into())
            .spawn(move || {
                let exported = match source {
                    BatchSource::Installed(profiled) => {
                        lm_app::export_smw_us_v1_installed_mwl_batch_until(&profiled, mode, || {
                            worker_cancelled.load(Ordering::Relaxed)
                        })
                    }
                    BatchSource::Builtin(snapshot) => {
                        lm_app::export_builtin_smw_us_v1_mwl_batch_until(&snapshot, mode, || {
                            worker_cancelled.load(Ordering::Relaxed)
                        })
                    }
                }
                .and_then(|documents| match documents {
                    Some(documents) if !worker_cancelled.load(Ordering::Relaxed) => {
                        lm_app::publish_mwl_batch_new(&worker_template, &documents).map(Some)
                    }
                    Some(_) | None => Ok(None),
                });
                let _ignored = sender.send(exported);
            });
        match spawn {
            Ok(_worker) => {
                self.running = Some(RunningExport {
                    template,
                    cancelled,
                    result,
                });
                Ok(())
            }
            Err(error) => Err(format!("could not start batch MWL export: {error}")),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if let Some(running) = &self.running {
            running.cancelled.store(true, Ordering::Relaxed);
            return !application;
        }
        self.result = None;
        true
    }

    pub(crate) fn show(&mut self, context: &egui::Context) {
        if let Some(running) = &self.running {
            match running.result.try_recv() {
                Ok(result) => {
                    self.result = Some(result);
                    self.running = None;
                }
                Err(TryRecvError::Empty) => context.request_repaint(),
                Err(TryRecvError::Disconnected) => {
                    self.result = Some(Err("batch MWL export worker disconnected".into()));
                    self.running = None;
                }
            }
        }
        let mut cancel = false;
        let mut close = false;
        if let Some(running) = &self.running {
            egui::Window::new("Exporting Multiple MWL Levels")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!("Template: {}", running.template.display()));
                    ui.label("Levels are prepared in the background and published as one group.");
                    if running.cancelled.load(Ordering::Relaxed) {
                        ui.label("Cancellation requested…");
                    } else if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            cancel |= context.input(|input| input.key_pressed(egui::Key::Escape));
        } else if let Some(result) = &self.result {
            egui::Window::new("MWL Batch Export")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    match result {
                        Ok(Some(count)) => {
                            ui.label(format!("Exported {count} levels."));
                        }
                        Ok(None) => {
                            ui.label("Batch MWL export cancelled.");
                        }
                        Err(error) => {
                            ui.colored_label(egui::Color32::RED, error);
                        }
                    };
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
        }
        if cancel {
            if let Some(running) = &self.running {
                running.cancelled.store(true, Ordering::Relaxed);
            }
        }
        if close {
            self.result = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BatchSource, RomMwlBatchExportDialog};
    use lm_app::{ControllerSnapshot, EditorMode, MwlBatchExportMode};
    use lm_rom::RomImage;
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn default_dialog_is_closed_and_close_is_idempotent() {
        let mut dialog = RomMwlBatchExportDialog::default();
        assert!(!dialog.is_open());
        assert!(dialog.request_close(false));
        assert!(!dialog.is_open());
    }

    #[test]
    fn builtin_worker_exports_all_512_vanilla_levels() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rom_bytes = fs::read(root.join("sysLMRestore/smwOrig.smc")).unwrap();
        let image = RomImage::from_bytes(rom_bytes.clone()).unwrap();
        let snapshot = ControllerSnapshot {
            revision: 0,
            mode: EditorMode::Level(0x105),
            identity: lm_rom::detect_identity(&image).unwrap(),
            document_path: None,
            rom_bytes,
        };
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "lm-native-builtin-batch-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let mut dialog = RomMwlBatchExportDialog::default();
        dialog
            .start(
                BatchSource::Builtin(snapshot),
                directory.join("Vanilla.mwl"),
                MwlBatchExportMode::All,
            )
            .unwrap();
        let result = dialog
            .running
            .as_ref()
            .unwrap()
            .result
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(result, Some(0x200));
        assert!(directory.join("Vanilla 000.mwl").is_file());
        assert!(directory.join("Vanilla 1FF.mwl").is_file());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0x200);
        fs::remove_dir_all(directory).unwrap();
    }
}
