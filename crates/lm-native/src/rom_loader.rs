//! Bounded ROM reading and application-neutral project preparation on one worker.

use eframe::egui;
use lm_app::{AppState, PreparedRomOpen};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
};

pub(crate) struct RomLoadCompletion {
    pub(crate) request_id: u64,
    pub(crate) path: PathBuf,
    pub(crate) result: Result<PreparedRomOpen, String>,
}

struct RunningRomLoad {
    result: Receiver<Result<PreparedRomOpen, String>>,
    request_id: u64,
    path: PathBuf,
}

#[derive(Default)]
pub(crate) struct RomLoader {
    running: Option<RunningRomLoad>,
}

impl RomLoader {
    pub(crate) fn start(&mut self, request_id: u64, path: PathBuf) -> Result<(), String> {
        if self.running.is_some() {
            return Err("a ROM load is already running".into());
        }
        let worker_path = path.clone();
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-rom-load".into())
            .spawn(move || {
                let prepared = crate::dialogs::read_regular_bounded(
                    &worker_path,
                    crate::dialogs::MAX_ROM_FILE_LEN,
                    "selected ROM",
                )
                .map_err(|error| error.to_string())
                .and_then(|bytes| AppState::prepare_open(bytes).map_err(|error| error.to_string()));
                let _send_result = sender.send(prepared);
            })
            .map_err(|error| format!("could not create ROM-loader worker: {error}"))?;
        self.running = Some(RunningRomLoad {
            result,
            request_id,
            path,
        });
        Ok(())
    }

    pub(crate) fn show(&mut self, context: &egui::Context) -> Option<RomLoadCompletion> {
        let completion = self.poll();
        if self.running.is_some() {
            egui::Window::new("Opening ROM")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label("Reading and validating the selected ROM…");
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        completion
    }

    fn poll(&mut self) -> Option<RomLoadCompletion> {
        let running = self.running.as_ref()?;
        let result = match running.result.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                Err("ROM-loader worker stopped without reporting a result".into())
            }
        };
        let running = self.running.take()?;
        Some(RomLoadCompletion {
            request_id: running.request_id,
            path: running.path,
            result,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-rom-loader-{}-{}.smc",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn test_rom() -> Vec<u8> {
        let mut bytes = vec![0; 0x8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes
    }

    #[test]
    fn worker_reads_and_prepares_supported_rom() {
        let path = path();
        fs::write(&path, test_rom()).unwrap();
        let mut loader = RomLoader::default();
        loader.start(17, path.clone()).unwrap();
        let running = loader.running.take().unwrap();
        let result = running
            .result
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(result.is_ok());
        assert_eq!(running.request_id, 17);
        assert_eq!(running.path, path);
        fs::remove_file(running.path).unwrap();
    }

    #[test]
    fn worker_rejects_malformed_rom_without_prepared_state() {
        let path = path();
        fs::write(&path, [0; 4]).unwrap();
        let mut loader = RomLoader::default();
        loader.start(18, path.clone()).unwrap();
        let running = loader.running.take().unwrap();
        assert!(
            running
                .result
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .is_err()
        );
        fs::remove_file(path).unwrap();
    }
}
