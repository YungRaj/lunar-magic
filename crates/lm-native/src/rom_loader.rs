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
    pub(crate) cancelled: bool,
}

struct HeaderPrompt {
    request_id: u64,
    path: PathBuf,
    original: Vec<u8>,
    headered: Vec<u8>,
}

enum RomLoadWorkerResult {
    Prepared(PreparedRomOpen),
    HeaderRequired {
        original: Vec<u8>,
        headered: Vec<u8>,
    },
}

struct RunningRomLoad {
    result: Receiver<Result<RomLoadWorkerResult, String>>,
    request_id: u64,
    path: PathBuf,
}

pub(crate) struct RomLoader {
    running: Option<RunningRomLoad>,
    header_prompt: Option<HeaderPrompt>,
}

impl Default for RomLoader {
    fn default() -> Self {
        Self {
            running: None,
            header_prompt: None,
        }
    }
}

impl RomLoader {
    pub(crate) fn start(
        &mut self,
        request_id: u64,
        path: PathBuf,
        silently_add_header: bool,
    ) -> Result<(), String> {
        if self.running.is_some() || self.header_prompt.is_some() {
            return Err("a ROM load is already running".into());
        }
        self.start_worker(request_id, path, silently_add_header, None)
    }

    fn start_worker(
        &mut self,
        request_id: u64,
        path: PathBuf,
        silently_add_header: bool,
        approved_header: Option<(Vec<u8>, Vec<u8>)>,
    ) -> Result<(), String> {
        let worker_path = path.clone();
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-rom-load".into())
            .spawn(move || {
                let prepared = prepare_rom_open(&worker_path, silently_add_header, approved_header);
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
        let mut completion = self.poll();
        if self.header_prompt.is_some() {
            let mut accept = false;
            let mut reject = false;
            egui::Window::new("Missing Copier Header")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label("This ROM has no 0x200-byte copier header. Add the header now?");
                    ui.horizontal(|ui| {
                        accept = ui.button("Add Header").clicked();
                        reject = ui.button("Cancel").clicked();
                    });
                });
            if accept {
                let prompt = self.header_prompt.take().expect("visible prompt exists");
                if let Err(error) = self.start_worker(
                    prompt.request_id,
                    prompt.path.clone(),
                    true,
                    Some((prompt.original, prompt.headered)),
                ) {
                    completion = Some(RomLoadCompletion {
                        request_id: prompt.request_id,
                        path: prompt.path,
                        result: Err(error),
                        cancelled: false,
                    });
                }
            } else if reject {
                let prompt = self.header_prompt.take().expect("visible prompt exists");
                completion = Some(RomLoadCompletion {
                    request_id: prompt.request_id,
                    path: prompt.path,
                    result: Err("ROM open cancelled".into()),
                    cancelled: true,
                });
            }
        }
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
        match result {
            Ok(RomLoadWorkerResult::Prepared(prepared)) => Some(RomLoadCompletion {
                request_id: running.request_id,
                path: running.path,
                result: Ok(prepared),
                cancelled: false,
            }),
            Ok(RomLoadWorkerResult::HeaderRequired { original, headered }) => {
                self.header_prompt = Some(HeaderPrompt {
                    request_id: running.request_id,
                    path: running.path,
                    original,
                    headered,
                });
                None
            }
            Err(error) => Some(RomLoadCompletion {
                request_id: running.request_id,
                path: running.path,
                result: Err(error),
                cancelled: false,
            }),
        }
    }
}

fn prepare_rom_open(
    path: &std::path::Path,
    silently_add_header: bool,
    approved_header: Option<(Vec<u8>, Vec<u8>)>,
) -> Result<RomLoadWorkerResult, String> {
    let (original, headered) = if let Some(approved) = approved_header {
        approved
    } else {
        let original = crate::dialogs::read_regular_bounded(
            path,
            crate::dialogs::MAX_ROM_FILE_LEN,
            "selected ROM",
        )
        .map_err(|error| error.to_string())?;
        let image =
            lm_rom::RomImage::from_bytes(original.clone()).map_err(|error| error.to_string())?;
        if image.copier_header() == lm_rom::CopierHeader::Present {
            return AppState::prepare_open(original)
                .map(RomLoadWorkerResult::Prepared)
                .map_err(|error| error.to_string());
        }
        let identity = lm_rom::detect_identity(&image).map_err(|error| error.to_string())?;
        let header = lm_profile::lunar_magic_copier_header(image.logical_len(), identity.map_mode);
        let mut headered = Vec::with_capacity(original.len() + lm_rom::COPIER_HEADER_LEN);
        headered.extend_from_slice(&header);
        headered.extend_from_slice(&original);
        (original, headered)
    };
    if !silently_add_header {
        return Ok(RomLoadWorkerResult::HeaderRequired { original, headered });
    }
    let current = crate::dialogs::read_regular_bounded(
        path,
        crate::dialogs::MAX_ROM_FILE_LEN,
        "selected ROM",
    )
    .map_err(|error| error.to_string())?;
    if current != original {
        return Err("selected ROM changed before its copier header could be added".into());
    }
    lm_app::file_persistence::replace_existing(path, &headered)
        .map_err(|error| error.to_string())?;
    AppState::prepare_open(headered)
        .map(RomLoadWorkerResult::Prepared)
        .map_err(|error| error.to_string())
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
        loader.start(17, path.clone(), true).unwrap();
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
        loader.start(18, path.clone(), true).unwrap();
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

    #[test]
    fn headerless_open_silently_adds_or_defers_the_exact_lunar_magic_header() {
        let silent_path = path();
        let original = test_rom();
        fs::write(&silent_path, &original).unwrap();
        assert!(matches!(
            prepare_rom_open(&silent_path, true, None).unwrap(),
            RomLoadWorkerResult::Prepared(_)
        ));
        let written = fs::read(&silent_path).unwrap();
        assert_eq!(written.len(), original.len() + lm_rom::COPIER_HEADER_LEN);
        assert_eq!(
            &written[..lm_rom::COPIER_HEADER_LEN],
            &lm_profile::lunar_magic_copier_header(original.len(), 0x20)
        );
        assert_eq!(&written[lm_rom::COPIER_HEADER_LEN..], original);
        fs::remove_file(silent_path).unwrap();

        let prompted_path = path();
        fs::write(&prompted_path, &original).unwrap();
        let RomLoadWorkerResult::HeaderRequired { original, headered } =
            prepare_rom_open(&prompted_path, false, None).unwrap()
        else {
            panic!("disabled silent-add must request confirmation");
        };
        assert_eq!(fs::read(&prompted_path).unwrap(), original);
        assert!(matches!(
            prepare_rom_open(&prompted_path, true, Some((original, headered))).unwrap(),
            RomLoadWorkerResult::Prepared(_)
        ));
        assert_eq!(fs::read(&prompted_path).unwrap().len(), 0x8200);
        fs::remove_file(prompted_path).unwrap();
    }

    #[test]
    fn approved_header_addition_refuses_to_overwrite_a_changed_rom() {
        let path = path();
        let original = test_rom();
        fs::write(&path, &original).unwrap();
        let RomLoadWorkerResult::HeaderRequired { original, headered } =
            prepare_rom_open(&path, false, None).unwrap()
        else {
            panic!("headerless ROM must request confirmation");
        };
        let mut changed = original.clone();
        changed[1] = 0x7f;
        fs::write(&path, &changed).unwrap();
        assert!(
            prepare_rom_open(&path, true, Some((original, headered)))
                .err()
                .unwrap()
                .contains("changed")
        );
        assert_eq!(fs::read(&path).unwrap(), changed);
        fs::remove_file(path).unwrap();
    }
}
