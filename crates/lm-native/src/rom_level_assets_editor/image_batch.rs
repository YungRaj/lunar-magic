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
    const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Bmp => "bmp",
        }
    }
}

struct RunningBatch {
    directory: PathBuf,
    format: LevelImageFormat,
    total: usize,
    completed: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Option<usize>, String>>,
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
        directory: PathBuf,
        format: LevelImageFormat,
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
        let worker_directory = directory.clone();
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-level-image-batch".into())
            .spawn(move || {
                let result = export_batch(
                    &source,
                    &worker_directory,
                    format,
                    &worker_completed,
                    &worker_cancelled,
                );
                let _send_result = sender.send(result);
            })
            .map_err(|error| format!("could not create level-image worker: {error}"))?;
        self.running = Some(RunningBatch {
            directory,
            format,
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
    ) -> Option<Result<Option<usize>, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            let completed = running.completed.load(Ordering::Relaxed);
            let cancellation_requested = running.cancelled.load(Ordering::Relaxed);
            egui::Window::new("Exporting level images")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!(
                        "Staging {} images in {}",
                        running.format.extension().to_uppercase(),
                        running.directory.display()
                    ));
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
                    "level-image worker stopped without reporting a result".into()
                ))
            }
        }
    }
}

fn export_batch(
    source: &BatchImageSource,
    directory: &std::path::Path,
    format: LevelImageFormat,
    completed: &AtomicUsize,
    cancelled: &AtomicBool,
) -> Result<Option<usize>, String> {
    let total = source.profile.level.layer1.entries;
    let mut group = lm_app::file_persistence::NewFileGroup::new();
    for slot in 0..total {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let level = u16::try_from(slot).map_err(|error| error.to_string())?;
        let canvas = render_batch_level_canvas(source, level)
            .map_err(|error| format!("level {level:03X}: {error}"))?;
        let bytes = match format {
            LevelImageFormat::Png => lm_render::encode_png(&canvas).map_err(|e| e.to_string())?,
            LevelImageFormat::Bmp => lm_render::encode_bmp(&canvas).map_err(|e| e.to_string())?,
        };
        let destination = batch_output_path(directory, level, format);
        group
            .stage(&destination, &bytes)
            .map_err(|error| format!("level {level:03X}: {error}"))?;
        completed.store(slot + 1, Ordering::Relaxed);
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(None);
    }
    group.publish().map_err(|error| error.to_string())?;
    Ok(Some(total))
}

fn batch_output_path(directory: &std::path::Path, level: u16, format: LevelImageFormat) -> PathBuf {
    directory.join(format!("Level {level:03X}.{}", format.extension()))
}

#[cfg(test)]
mod tests {
    use super::{LevelImageFormat, RunningBatch, batch_output_path};
    use std::path::{Path, PathBuf};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    };

    #[test]
    fn batch_names_use_fixed_three_digit_uppercase_level_numbers() {
        assert_eq!(
            batch_output_path(Path::new("/tmp/images"), 0x00a, LevelImageFormat::Png),
            Path::new("/tmp/images/Level 00A.png")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/images"), 0x105, LevelImageFormat::Bmp),
            Path::new("/tmp/images/Level 105.bmp")
        );
    }

    #[test]
    fn cancellation_request_is_shared_with_the_render_worker() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (_sender, result) = mpsc::channel();
        let running = RunningBatch {
            directory: PathBuf::from("images"),
            format: LevelImageFormat::Png,
            total: 0x200,
            completed: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::clone(&cancelled),
            result,
        };
        running.request_cancel();
        assert!(cancelled.load(Ordering::Relaxed));
    }
}
