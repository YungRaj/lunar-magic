use chrono::{Datelike as _, Timelike as _};
use eframe::egui;
use lm_app::{
    AppState, ControllerSnapshot, LevelUsageScanOptions, LevelUsageScanProgress,
    LevelUsageTimestamp, ProfiledControllerSnapshot,
};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

enum ScanSource {
    BuiltIn(ControllerSnapshot),
    Profiled(Box<ProfiledControllerSnapshot>),
}

struct PendingScan {
    source: ScanSource,
    output: PathBuf,
    options: LevelUsageScanOptions,
}

enum WorkerEvent {
    Progress(LevelUsageScanProgress),
    Complete(Result<CompletedScan, String>),
}

struct CompletedScan {
    output: PathBuf,
    bytes: usize,
    diagnostics: usize,
}

struct RunningScan {
    output: PathBuf,
    progress: LevelUsageScanProgress,
    cancel: Arc<AtomicBool>,
    events: mpsc::Receiver<WorkerEvent>,
}

#[derive(Default)]
pub(crate) struct LevelUsageDialog {
    pending: Option<PendingScan>,
    running: Option<RunningScan>,
    completed: Option<String>,
    error: Option<String>,
}

impl LevelUsageDialog {
    pub(crate) const fn is_busy(&self) -> bool {
        self.pending.is_some() || self.running.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) -> Result<(), String> {
        if self.is_busy() {
            return Err("a level-usage analysis is already active".into());
        }
        let snapshot = app
            .controller_snapshot()
            .map_err(|error| error.to_string())?;
        let document = snapshot
            .document_path
            .as_ref()
            .ok_or_else(|| "save the ROM before creating LevelAnalysis.txt".to_string())?;
        let output = document
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("LevelAnalysis.txt");
        let source = app.profiled_controller_snapshot().map_or_else(
            |_| ScanSource::BuiltIn(snapshot),
            |profiled| ScanSource::Profiled(Box::new(profiled)),
        );
        self.pending = Some(PendingScan {
            source,
            output,
            options: LevelUsageScanOptions::default(),
        });
        self.completed = None;
        self.error = None;
        Ok(())
    }

    pub(crate) fn show(&mut self, context: &egui::Context) {
        self.poll();
        self.show_options(context);
        self.show_progress(context);
        self.show_completion(context);
        self.show_error(context);
    }

    fn show_options(&mut self, context: &egui::Context) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        let mut open = true;
        let mut analyze = false;
        let mut cancel = false;
        egui::Window::new("Analyze Level Usage")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.checkbox(&mut pending.options.map16, "Analyze Map16 tiles");
                ui.add_enabled_ui(pending.options.map16, |ui| {
                    ui.checkbox(
                        &mut pending.options.only_unused_defined_map16,
                        "Only report tiles that are defined but not used",
                    );
                });
                ui.checkbox(&mut pending.options.graphics, "Analyze graphics files");
                ui.add_enabled_ui(pending.options.graphics, |ui| {
                    ui.checkbox(
                        &mut pending.options.only_unused_inserted_graphics,
                        "Only report files that are inserted but not loaded",
                    );
                });
                ui.checkbox(&mut pending.options.sprites, "Analyze sprites");
                ui.checkbox(&mut pending.options.music, "Analyze music");
                ui.separator();
                ui.label(format!("Output: {}", pending.output.display()));
                ui.horizontal(|ui| {
                    analyze = ui.button("Analyze").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if !open || cancel {
            self.pending = None;
        } else if analyze && let Err(error) = self.start() {
            self.error = Some(error);
        }
    }

    fn start(&mut self) -> Result<(), String> {
        let pending = self
            .pending
            .take()
            .ok_or_else(|| "no level-usage analysis is configured".to_string())?;
        let output = pending.output.clone();
        let worker_output = output.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, events) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-level-usage".into())
            .spawn(move || {
                let progress_sender = sender.clone();
                let mut progress = |value| {
                    let _send_result = progress_sender.send(WorkerEvent::Progress(value));
                    if worker_cancel.load(Ordering::Relaxed) {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                };
                let result = match pending.source {
                    ScanSource::BuiltIn(snapshot) => lm_app::scan_builtin_smw_us_v1_level_usage(
                        &snapshot,
                        pending.options,
                        &mut progress,
                    ),
                    ScanSource::Profiled(snapshot) => lm_app::scan_smw_us_v1_level_usage(
                        snapshot.as_ref(),
                        pending.options,
                        &mut progress,
                    ),
                }
                .map_err(|error| error.to_string())
                .and_then(|result| {
                    let bytes = result
                        .report
                        .encode_lunar_magic_363(local_timestamp())
                        .map_err(|error| error.to_string())?;
                    if worker_output.exists() {
                        lm_app::file_persistence::replace_existing(&worker_output, &bytes)
                    } else {
                        lm_app::file_persistence::write_new(&worker_output, &bytes)
                    }
                    .map_err(|error| error.to_string())?;
                    Ok(CompletedScan {
                        output: worker_output,
                        bytes: bytes.len(),
                        diagnostics: result.diagnostics.len(),
                    })
                });
                let _send_result = sender.send(WorkerEvent::Complete(result));
            })
            .map_err(|error| format!("could not start level-usage worker: {error}"))?;
        self.running = Some(RunningScan {
            output,
            progress: LevelUsageScanProgress {
                completed: 0,
                total: 0x200,
                current_level: Some(0),
                loaded: 0,
                skipped: 0,
            },
            cancel,
            events,
        });
        Ok(())
    }

    fn show_progress(&mut self, context: &egui::Context) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        let mut cancel = context.input(|input| input.key_pressed(egui::Key::Escape));
        egui::Window::new("Analyzing Level Usage")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format!("Output: {}", running.output.display()));
                let fraction = if running.progress.total == 0 {
                    0.0
                } else {
                    let completed = u16::try_from(running.progress.completed).unwrap_or(u16::MAX);
                    let total = u16::try_from(running.progress.total).unwrap_or(u16::MAX);
                    f32::from(completed) / f32::from(total)
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .show_percentage()
                        .text(format!(
                            "{} / {} levels",
                            running.progress.completed, running.progress.total
                        )),
                );
                if let Some(level) = running.progress.current_level {
                    ui.label(format!("Scanning level {level:03X}…"));
                }
                cancel |= ui.button("Cancel").clicked();
            });
        if cancel {
            running.cancel.store(true, Ordering::Relaxed);
        }
        context.request_repaint_after(std::time::Duration::from_millis(50));
    }

    fn poll(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let mut completion = None;
        while let Ok(event) = running.events.try_recv() {
            match event {
                WorkerEvent::Progress(progress) => running.progress = progress,
                WorkerEvent::Complete(result) => completion = Some(result),
            }
        }
        let Some(result) = completion else {
            return;
        };
        self.running = None;
        match result {
            Ok(completed) => {
                self.completed = Some(format!(
                    "Created {} ({} bytes, {} diagnostics).",
                    completed.output.display(),
                    completed.bytes,
                    completed.diagnostics
                ));
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn show_completion(&mut self, context: &egui::Context) {
        let Some(message) = self.completed.clone() else {
            return;
        };
        egui::Window::new("Level usage analysis complete")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(message);
                if ui.button("OK").clicked() {
                    self.completed = None;
                }
            });
    }

    fn show_error(&mut self, context: &egui::Context) {
        let Some(error) = self.error.clone() else {
            return;
        };
        egui::Window::new("Level usage analysis error")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.colored_label(egui::Color32::RED, error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
    }
}

fn local_timestamp() -> LevelUsageTimestamp {
    let now = chrono::Local::now();
    LevelUsageTimestamp {
        month: u8::try_from(now.month()).unwrap_or_default(),
        day: u8::try_from(now.day()).unwrap_or_default(),
        year: u16::try_from(now.year()).unwrap_or(1970),
        hour: u8::try_from(now.hour()).unwrap_or_default(),
        minute: u8::try_from(now.minute()).unwrap_or_default(),
        second: u8::try_from(now.second()).unwrap_or_default(),
    }
}
