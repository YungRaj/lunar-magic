use super::{BatchImageSource, render_batch_level_canvas};
use eframe::egui;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LevelImageFormat {
    Png,
    Bmp,
}

impl LevelImageFormat {
    pub(super) const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Bmp => "bmp",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LevelImageBatchOptions {
    pub modified_only: bool,
    pub auto_set_screens: bool,
}

impl Default for LevelImageBatchOptions {
    fn default() -> Self {
        Self {
            modified_only: true,
            auto_set_screens: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LevelImageBatchReport {
    pub exported: usize,
    pub skipped_unrenderable: usize,
}

struct RunningBatch {
    template: PathBuf,
    format: LevelImageFormat,
    options: LevelImageBatchOptions,
    total: usize,
    completed: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Option<LevelImageBatchReport>, String>>,
}

impl RunningBatch {
    fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct LevelImageBatchWorker {
    running: Option<RunningBatch>,
}

impl LevelImageBatchWorker {
    pub(super) const fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(super) fn start(
        &mut self,
        source: BatchImageSource,
        template: PathBuf,
        format: LevelImageFormat,
        options: LevelImageBatchOptions,
    ) -> Result<(), String> {
        if self.running.is_some() {
            return Err("a level image batch is already running".into());
        }
        let total = source.profile.level.layer1.entries;
        if total == 0 || total > usize::from(u16::MAX) + 1 {
            return Err(format!(
                "profile declares unsupported level image count {total}"
            ));
        }
        let completed = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_completed = Arc::clone(&completed);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_template = template.clone();
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-level-image-batch".into())
            .spawn(move || {
                let result = export_batch(
                    &source,
                    &worker_template,
                    format,
                    options,
                    &worker_completed,
                    &worker_cancelled,
                );
                let _send_result = sender.send(result);
            })
            .map_err(|error| format!("could not create level-image worker: {error}"))?;
        self.running = Some(RunningBatch {
            template,
            format,
            options,
            total,
            completed,
            cancelled,
            result,
        });
        Ok(())
    }

    pub(super) fn show(
        &mut self,
        context: &egui::Context,
    ) -> Option<Result<Option<LevelImageBatchReport>, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            let completed = running.completed.load(Ordering::Relaxed);
            let cancellation_requested = running.cancelled.load(Ordering::Relaxed);
            egui::Window::new("Exporting level images")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!(
                        "Staging numbered {} images from {}",
                        running.format.extension().to_uppercase(),
                        running.template.display()
                    ));
                    ui.label(if running.options.modified_only {
                        "Selection: levels whose Layer 1 data is in expanded ROM space"
                    } else {
                        "Selection: all level slots"
                    });
                    ui.add(
                        egui::ProgressBar::new(completed as f32 / running.total as f32)
                            .text(format!("{completed} / {}", running.total)),
                    );
                    ui.label("Files become visible only after the complete batch is staged.");
                    if cancellation_requested {
                        ui.label("Cancelling after the current level…");
                    } else if ui.button("Cancel").clicked()
                        || context.input(|input| input.key_pressed(egui::Key::Escape))
                    {
                        running.request_cancel();
                    }
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        completion
    }

    fn poll(&mut self) -> Option<Result<Option<LevelImageBatchReport>, String>> {
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
                    "level-image worker stopped without reporting a result".into()
                ))
            }
        }
    }
}

fn export_batch(
    source: &BatchImageSource,
    template: &std::path::Path,
    format: LevelImageFormat,
    options: LevelImageBatchOptions,
    completed: &AtomicUsize,
    cancelled: &AtomicBool,
) -> Result<Option<LevelImageBatchReport>, String> {
    let total = source.profile.level.layer1.entries;
    let mut group = lm_app::file_persistence::NewFileGroup::new();
    let mut report = LevelImageBatchReport {
        exported: 0,
        skipped_unrenderable: 0,
    };
    for slot in 0..total {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let level = u16::try_from(slot).map_err(|error| error.to_string())?;
        if options.modified_only
            && !lm_app::native_level_is_in_expanded_area(
                &source.image,
                source.profile.mapper,
                source.profile.level.layer1,
                slot,
            )?
        {
            completed.store(slot + 1, Ordering::Relaxed);
            continue;
        }
        let canvas = match render_batch_level_canvas(source, level, options.auto_set_screens) {
            Ok(canvas) => canvas,
            Err(_) => {
                report.skipped_unrenderable += 1;
                completed.store(slot + 1, Ordering::Relaxed);
                continue;
            }
        };
        let bytes = match format {
            LevelImageFormat::Png => lm_render::encode_png(&canvas).map_err(|e| e.to_string())?,
            LevelImageFormat::Bmp => lm_render::encode_bmp(&canvas).map_err(|e| e.to_string())?,
        };
        let destination = batch_output_path(template, level, format)?;
        group
            .stage(&destination, &bytes)
            .map_err(|error| format!("level {level:03X}: {error}"))?;
        completed.store(slot + 1, Ordering::Relaxed);
        report.exported += 1;
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(None);
    }
    if report.exported != 0 {
        group.publish().map_err(|error| error.to_string())?;
    }
    Ok(Some(report))
}

fn batch_output_path(
    template: &std::path::Path,
    level: u16,
    format: LevelImageFormat,
) -> Result<PathBuf, String> {
    let stem = template
        .file_stem()
        .ok_or("level-image batch template requires a file name")?;
    let mut name = stem.to_os_string();
    name.push(format!(" {level:03X}.{}", format.extension()));
    Ok(template.with_file_name(name))
}

#[cfg(test)]
mod tests {
    use super::{LevelImageBatchOptions, LevelImageFormat, RunningBatch, batch_output_path};
    use std::path::{Path, PathBuf};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    };

    #[test]
    fn batch_names_use_the_original_template_and_uppercase_level_numbers() {
        assert_eq!(
            batch_output_path(
                Path::new("/tmp/My Export.png"),
                0x00a,
                LevelImageFormat::Png
            )
            .unwrap(),
            Path::new("/tmp/My Export 00A.png")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/世界.bmp"), 0x105, LevelImageFormat::Bmp).unwrap(),
            Path::new("/tmp/世界 105.bmp")
        );
        assert!(batch_output_path(Path::new("/"), 0, LevelImageFormat::Png).is_err());
    }

    #[test]
    fn original_batch_defaults_select_modified_levels_without_auto_sizing() {
        assert_eq!(
            LevelImageBatchOptions::default(),
            LevelImageBatchOptions {
                modified_only: true,
                auto_set_screens: false,
            }
        );
    }

    #[test]
    fn cancellation_request_is_shared_with_the_render_worker() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (_sender, result) = mpsc::channel();
        let running = RunningBatch {
            template: PathBuf::from("images/Levels.png"),
            format: LevelImageFormat::Png,
            options: LevelImageBatchOptions::default(),
            total: 0x200,
            completed: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::clone(&cancelled),
            result,
        };
        running.request_cancel();
        assert!(cancelled.load(Ordering::Relaxed));
    }
}
