use chrono::{Datelike as _, Timelike as _};
use eframe::egui;
use lm_app::{
    AppState, ControllerSnapshot, ExtendedUiTextKey as Key, LevelUsageScanOptions,
    LevelUsageScanProgress, LevelUsageTimestamp, LocalizationCatalog, ProfiledControllerSnapshot,
};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

const ORIGINAL_DIALOG_ID: u16 = 0x0425;

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

    pub(crate) fn show(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        self.poll(catalog);
        self.show_options(context, catalog);
        self.show_progress(context, catalog);
        self.show_completion(context, catalog);
        self.show_error(context, catalog);
    }

    fn show_options(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        let mut open = true;
        let mut analyze = false;
        let mut cancel = false;
        egui::Window::new(level_usage_dialog_title(catalog))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.checkbox(
                    &mut pending.options.map16,
                    level_usage_dialog_text(catalog, 0x72, "Analyze Map16."),
                );
                ui.add_enabled_ui(pending.options.map16, |ui| {
                    ui.checkbox(
                        &mut pending.options.only_unused_defined_map16,
                        level_usage_dialog_text(
                            catalog,
                            0x73,
                            "Only report if tile defined but not used.",
                        ),
                    );
                });
                ui.checkbox(
                    &mut pending.options.graphics,
                    level_usage_dialog_text(catalog, 0x74, "Analyze Graphics."),
                );
                ui.add_enabled_ui(pending.options.graphics, |ui| {
                    ui.checkbox(
                        &mut pending.options.only_unused_inserted_graphics,
                        level_usage_dialog_text(
                            catalog,
                            0x75,
                            "Only report if file inserted but not loaded.",
                        ),
                    );
                });
                ui.checkbox(
                    &mut pending.options.sprites,
                    level_usage_dialog_text(catalog, 0x76, "Analyze Sprites."),
                );
                ui.checkbox(
                    &mut pending.options.music,
                    level_usage_dialog_text(catalog, 0x65, "Analyze Music."),
                );
                ui.separator();
                ui.label(
                    text(catalog, Key::LevelUsageOutputFormat)
                        .replace("{path}", &pending.output.display().to_string()),
                );
                ui.horizontal(|ui| {
                    analyze = ui
                        .button(level_usage_dialog_text(catalog, 1, "OK"))
                        .clicked();
                    cancel = ui
                        .button(level_usage_dialog_text(catalog, 2, "Cancel"))
                        .clicked();
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

    fn show_progress(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        let mut cancel = context.input(|input| input.key_pressed(egui::Key::Escape));
        egui::Window::new(text(catalog, Key::LevelUsageProgressTitle))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(
                    text(catalog, Key::LevelUsageOutputFormat)
                        .replace("{path}", &running.output.display().to_string()),
                );
                let fraction = if running.progress.total == 0 {
                    0.0
                } else {
                    let completed = u16::try_from(running.progress.completed).unwrap_or(u16::MAX);
                    let total = u16::try_from(running.progress.total).unwrap_or(u16::MAX);
                    f32::from(completed) / f32::from(total)
                };
                ui.add(
                    egui::ProgressBar::new(fraction).show_percentage().text(
                        text(catalog, Key::LevelUsageLevelsFormat)
                            .replace("{completed}", &running.progress.completed.to_string())
                            .replace("{total}", &running.progress.total.to_string()),
                    ),
                );
                if let Some(level) = running.progress.current_level {
                    ui.label(
                        text(catalog, Key::LevelUsageScanningFormat)
                            .replace("{level}", &format!("{level:03X}")),
                    );
                }
                cancel |= ui.button(text(catalog, Key::LevelUsageCancel)).clicked();
            });
        if cancel {
            running.cancel.store(true, Ordering::Relaxed);
        }
        context.request_repaint_after(std::time::Duration::from_millis(50));
    }

    fn poll(&mut self, catalog: Option<&LocalizationCatalog>) {
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
                self.completed = Some(
                    text(catalog, Key::LevelUsageCompleteFormat)
                        .replace("{path}", &completed.output.display().to_string())
                        .replace("{bytes}", &completed.bytes.to_string())
                        .replace("{diagnostics}", &completed.diagnostics.to_string()),
                );
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn show_completion(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        let Some(message) = self.completed.clone() else {
            return;
        };
        egui::Window::new(text(catalog, Key::LevelUsageCompleteTitle))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(message);
                if ui.button(text(catalog, Key::LevelUsageOk)).clicked() {
                    self.completed = None;
                }
            });
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        let Some(error) = self.error.clone() else {
            return;
        };
        egui::Window::new(text(catalog, Key::LevelUsageErrorTitle))
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.colored_label(egui::Color32::RED, error);
                if ui.button(text(catalog, Key::LevelUsageOk)).clicked() {
                    self.error = None;
                }
            });
    }
}

fn level_usage_dialog_title(catalog: Option<&LocalizationCatalog>) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_DIALOG_ID))
        .unwrap_or("Analyze Resources in Levels")
        .to_owned()
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

fn level_usage_dialog_text(
    catalog: Option<&LocalizationCatalog>,
    control_id: u32,
    fallback: &str,
) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_control_text(ORIGINAL_DIALOG_ID, control_id))
        .unwrap_or(fallback)
        .to_owned()
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

#[cfg(test)]
mod localization_tests {
    use super::*;
    use lm_app::{OriginalDialogTextKey, UiTextKey};

    #[test]
    fn original_resource_analysis_template_localizes_all_matching_options_and_round_trips() {
        let catalog = LocalizationCatalog::new(
            "fr-test",
            UiTextKey::ALL.map(|key| (key, key.english().to_owned())),
        )
        .unwrap()
        .with_original_dialog_texts([
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Analyser les ressources des niveaux".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 1,
                    control_id: 0x72,
                },
                "Analyser Map16.".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 2,
                    control_id: 1,
                },
                "Valider".into(),
            ),
        ])
        .unwrap();

        assert_eq!(
            level_usage_dialog_title(Some(&catalog)),
            "Analyser les ressources des niveaux"
        );
        assert_eq!(
            level_usage_dialog_text(Some(&catalog), 0x72, "fallback"),
            "Analyser Map16."
        );
        assert_eq!(level_usage_dialog_text(Some(&catalog), 1, "OK"), "Valider");
        assert_eq!(
            level_usage_dialog_text(Some(&catalog), 0x76, "Analyze Sprites."),
            "Analyze Sprites."
        );
        assert_eq!(
            level_usage_dialog_title(None),
            "Analyze Resources in Levels"
        );

        let reopened = LocalizationCatalog::decode(&catalog.encode().unwrap()).unwrap();
        assert_eq!(
            level_usage_dialog_title(Some(&reopened)),
            "Analyser les ressources des niveaux"
        );
    }

    #[test]
    fn complete_level_usage_surface_has_no_literal_native_widget_text() {
        let source = include_str!("level_usage_dialog.rs");
        for literal in [
            "egui::Window::new(\"",
            "ui.button(\"",
            "ui.label(format!(\"Output:",
        ] {
            assert!(
                !source.contains(literal),
                "literal level-usage widget text: {literal}"
            );
        }
        assert!(source.contains("level_usage_dialog_text(catalog"));
        assert!(source.contains("original_dialog_control_text"));
    }
}
