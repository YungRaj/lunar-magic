use eframe::egui;
use lm_app::PreparedRomCommit;
use lm_project::{GraphicsRomLayout, GraphicsSaveOptions};
use lm_rom::RomImage;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

#[derive(Clone)]
pub(super) struct GraphicsImportSource {
    pub(super) expected_revision: u64,
    pub(super) image: RomImage,
    pub(super) layout: GraphicsRomLayout,
    pub(super) checksum_field: usize,
    pub(super) options: GraphicsSaveOptions,
}

struct RunningImport {
    directory: PathBuf,
    total: usize,
    completed: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Option<PreparedRomCommit>, String>>,
}

impl RunningImport {
    fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct GraphicsImportWorker {
    running: Option<RunningImport>,
}

impl GraphicsImportWorker {
    pub(super) const fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(super) fn start(
        &mut self,
        source: GraphicsImportSource,
        directory: PathBuf,
    ) -> Result<(), String> {
        if self.running.is_some() {
            return Err("a standard GFX insertion is already running".into());
        }
        let total = source.layout.pointers.entries;
        if total == 0 || total > 0x80 {
            return Err(format!(
                "profile declares unsupported standard GFX count {total}"
            ));
        }
        let completed = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_completed = Arc::clone(&completed);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_directory = directory.clone();
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-standard-gfx-import".into())
            .spawn(move || {
                let result = prepare_import(
                    source,
                    &worker_directory,
                    &worker_completed,
                    &worker_cancelled,
                );
                let _send_result = sender.send(result);
            })
            .map_err(|error| format!("could not create standard-GFX import worker: {error}"))?;
        self.running = Some(RunningImport {
            directory,
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
    ) -> Option<Result<Option<PreparedRomCommit>, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            let completed = running.completed.load(Ordering::Relaxed);
            let cancellation_requested = running.cancelled.load(Ordering::Relaxed);
            egui::Window::new("Inserting standard GFX")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!(
                        "Reading files from {}",
                        running.directory.display()
                    ));
                    ui.add(
                        egui::ProgressBar::new(completed as f32 / running.total as f32)
                            .text(format!("{completed} / {}", running.total)),
                    );
                    ui.label("The ROM changes only after the complete set validates.");
                    if cancellation_requested {
                        ui.label("Cancelling after the current file…");
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

    fn poll(&mut self) -> Option<Result<Option<PreparedRomCommit>, String>> {
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
                    "standard-GFX import worker stopped without reporting a result".into(),
                ))
            }
        }
    }
}

fn prepare_import(
    source: GraphicsImportSource,
    directory: &Path,
    completed: &AtomicUsize,
    cancelled: &AtomicBool,
) -> Result<Option<PreparedRomCommit>, String> {
    let total = source.layout.pointers.entries;
    let mut files = Vec::with_capacity(total);
    for slot in 0..total {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let description = format!("GFX{slot:02X} raw graphics");
        let bytes = crate::dialogs::read_regular_bounded(
            &directory.join(format!("GFX{slot:02X}.bin")),
            u64::try_from(source.layout.maximum_decompressed_len).unwrap_or(u64::MAX),
            &description,
        )
        .map_err(|error| format!("{description}: {error}"))?;
        files.push(bytes);
        completed.store(slot + 1, Ordering::Relaxed);
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(None);
    }
    lm_app::prepare_standard_graphics_import(
        source.expected_revision,
        source.image,
        source.layout,
        source.checksum_field,
        &files,
        &source.options,
    )
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::RunningImport;
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    };

    #[test]
    fn cancellation_request_is_shared_with_the_import_worker() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (_sender, result) = mpsc::channel();
        let running = RunningImport {
            directory: PathBuf::from("Graphics"),
            total: 0x32,
            completed: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::clone(&cancelled),
            result,
        };
        running.request_cancel();
        assert!(cancelled.load(Ordering::Relaxed));
    }
}
