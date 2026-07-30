use eframe::egui;
use lm_project::LunarRestoreArchive;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
};

const MAX_ARCHIVE_LEN: u64 = 256 * 1024 * 1024;

#[derive(Debug)]
struct LoadedRestore {
    archive_path: PathBuf,
    original_path: PathBuf,
    target_path: PathBuf,
    archive: LunarRestoreArchive,
    original: Vec<u8>,
    selected: usize,
    restore_associated_files: bool,
}

#[derive(Debug)]
struct RestoreCompletion {
    result: Result<PublishedRestore, String>,
}

#[derive(Debug)]
struct PublishedRestore {
    rom_len: usize,
    associated_file_count: usize,
}

#[derive(Debug)]
struct RunningRestore {
    target_path: PathBuf,
    record_id: u32,
    completion: Receiver<RestoreCompletion>,
}

#[derive(Default)]
pub(crate) struct RestorePointDialog {
    loaded: Option<LoadedRestore>,
    running: Option<RunningRestore>,
    completed: Option<String>,
    error: Option<String>,
}

impl RestorePointDialog {
    pub(crate) const fn is_busy(&self) -> bool {
        self.loaded.is_some() || self.running.is_some()
    }

    pub(crate) fn choose_and_open(&mut self) -> Result<bool, String> {
        if self.is_busy() {
            return Err("a restore-point workflow is already active".into());
        }
        let Some(archive_path) = crate::dialogs::choose_restore_archive() else {
            return Ok(false);
        };
        let Some(original_path) = crate::dialogs::choose_restore_original_rom() else {
            return Ok(false);
        };
        let Some(target_path) = crate::dialogs::choose_restore_target_rom() else {
            return Ok(false);
        };
        validate_paths(&archive_path, &original_path, &target_path)?;
        let archive_bytes =
            crate::dialogs::read_regular_bounded(&archive_path, MAX_ARCHIVE_LEN, "restore archive")
                .map_err(|error| error.to_string())?;
        let original = crate::dialogs::read_regular_bounded(
            &original_path,
            crate::dialogs::MAX_ROM_FILE_LEN,
            "original restore ROM",
        )
        .map_err(|error| error.to_string())?;
        let archive =
            LunarRestoreArchive::decode(&archive_bytes).map_err(|error| error.to_string())?;
        if archive.records.is_empty() {
            return Err("restore archive contains no restore points".into());
        }
        let selected = archive.records.len() - 1;
        self.loaded = Some(LoadedRestore {
            archive_path,
            original_path,
            target_path,
            archive,
            original,
            selected,
            restore_associated_files: true,
        });
        self.completed = None;
        self.error = None;
        Ok(true)
    }

    pub(crate) fn show(&mut self, context: &egui::Context) {
        self.poll();
        self.show_loaded(context);
        self.show_running(context);
        self.show_result(context);
    }

    fn show_loaded(&mut self, context: &egui::Context) {
        let Some(loaded) = self.loaded.as_mut() else {
            return;
        };
        let mut restore = false;
        let mut cancel = false;
        egui::Window::new("Restore ROM from Restore Point")
            .collapsible(false)
            .resizable(true)
            .default_width(680.0)
            .show(context, |ui| {
                ui.label(format!("Archive: {}", loaded.archive_path.display()));
                ui.label(format!("Original: {}", loaded.original_path.display()));
                ui.label(format!("Restore target: {}", loaded.target_path.display()));
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        egui::Grid::new("restore_point_list")
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("ID");
                                ui.strong("Date and time");
                                ui.strong("Type");
                                ui.strong("Description");
                                ui.end_row();
                                for (index, record) in loaded.archive.records.iter().enumerate() {
                                    let date = record.created;
                                    let time = record.created_time;
                                    let period = if time.hour < 12 { "AM" } else { "PM" };
                                    let hour = match time.hour % 12 {
                                        0 => 12,
                                        value => value,
                                    };
                                    let kind = if record.directory_version & 4 != 0 {
                                        "Reversion"
                                    } else if record.directory_version.trailing_zeros() < 2 {
                                        "Full"
                                    } else {
                                        "Delta"
                                    };
                                    ui.selectable_value(
                                        &mut loaded.selected,
                                        index,
                                        record.record_id.to_string(),
                                    );
                                    ui.label(format!(
                                        "{:02}/{:02}/{:04}  {:02}:{:02}:{:02} {period}",
                                        date.month,
                                        date.day,
                                        date.year,
                                        hour,
                                        time.minute,
                                        time.second
                                    ));
                                    ui.label(kind);
                                    ui.label(record.description_text());
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                ui.checkbox(
                    &mut loaded.restore_associated_files,
                    "Restore associated files (.msc, .dsc, .ssc, Map16, and related files)",
                );
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "The selected existing ROM will be replaced atomically. Close it in the editor first.",
                );
                ui.horizontal(|ui| {
                    restore = ui.button("Restore").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if cancel {
            self.loaded = None;
        } else if restore && let Err(error) = self.start_restore() {
            self.error = Some(error);
        }
    }

    fn start_restore(&mut self) -> Result<(), String> {
        let loaded = self
            .loaded
            .take()
            .ok_or_else(|| "no restore archive is loaded".to_owned())?;
        let record = loaded
            .archive
            .records
            .get(loaded.selected)
            .ok_or_else(|| "selected restore point no longer exists".to_owned())?;
        let record_id = record.record_id;
        let restore_associated_files = loaded.restore_associated_files;
        let target_path = loaded.target_path.clone();
        let worker_target = target_path.clone();
        let (sender, completion) = mpsc::channel();
        std::thread::Builder::new()
            .name("lm-restore-rom".into())
            .spawn(move || {
                let result = restore_and_publish(
                    &loaded.archive,
                    record_id,
                    &loaded.original,
                    &worker_target,
                    restore_associated_files,
                );
                let _send_result = sender.send(RestoreCompletion { result });
            })
            .map_err(|error| format!("could not create restore worker: {error}"))?;
        self.running = Some(RunningRestore {
            target_path,
            record_id,
            completion,
        });
        Ok(())
    }

    fn show_running(&self, context: &egui::Context) {
        let Some(running) = &self.running else {
            return;
        };
        egui::Window::new("Restoring ROM")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format!("Restore point: {}", running.record_id));
                ui.label(format!("Target: {}", running.target_path.display()));
                ui.label("Validating and publishing the reconstructed ROM…");
            });
        context.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn show_result(&mut self, context: &egui::Context) {
        if let Some(message) = self.completed.clone() {
            egui::Window::new("ROM restored")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(message);
                    if ui.button("OK").clicked() {
                        self.completed = None;
                    }
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("Restore-point error")
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

    fn poll(&mut self) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        let result = match running.completion.try_recv() {
            Ok(completion) => Some(completion.result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(
                "restore worker stopped without reporting a result".into(),
            )),
        };
        let Some(result) = result else {
            return;
        };
        let target = running.target_path.clone();
        let record_id = running.record_id;
        self.running = None;
        match result {
            Ok(publication) => {
                let associated = match publication.associated_file_count {
                    0 => String::new(),
                    1 => " and 1 associated file".to_owned(),
                    count => format!(" and {count} associated files"),
                };
                self.completed = Some(format!(
                    "Restored point {record_id} to {} ({} bytes{associated}).",
                    target.display(),
                    publication.rom_len,
                ));
            }
            Err(error) => self.error = Some(error),
        }
    }
}

fn validate_paths(archive: &Path, original: &Path, target: &Path) -> Result<(), String> {
    let archive = fs::canonicalize(archive)
        .map_err(|error| format!("cannot resolve restore archive: {error}"))?;
    let original = fs::canonicalize(original)
        .map_err(|error| format!("cannot resolve original ROM: {error}"))?;
    let target =
        fs::canonicalize(target).map_err(|error| format!("cannot resolve target ROM: {error}"))?;
    if archive == original || archive == target || original == target {
        return Err("archive, original ROM, and restore target must be distinct files".into());
    }
    Ok(())
}

fn restore_and_publish(
    archive: &LunarRestoreArchive,
    record_id: u32,
    original: &[u8],
    target: &Path,
    restore_associated_files: bool,
) -> Result<PublishedRestore, String> {
    let restored = archive
        .restore_through(record_id, original)
        .map_err(|error| error.to_string())?;
    let image =
        lm_rom::RomImage::from_bytes(restored.clone()).map_err(|error| error.to_string())?;
    lm_project::Project::open_supported(image).map_err(|error| error.to_string())?;
    let associated = if restore_associated_files {
        archive
            .restore_associated_files_through(record_id)
            .map_err(|error| error.to_string())?
    } else {
        Vec::new()
    };
    if associated.is_empty() {
        lm_app::file_persistence::replace_existing(target, &restored)
            .map_err(|error| error.to_string())?;
    } else {
        let sidecar_paths: Vec<PathBuf> = associated
            .iter()
            .map(|file| target.with_extension(file.extension))
            .collect();
        let mut documents: Vec<(&Path, &[u8])> = Vec::with_capacity(associated.len() + 1);
        documents.push((target, &restored));
        documents.extend(
            sidecar_paths
                .iter()
                .zip(&associated)
                .map(|(path, file)| (path.as_path(), file.bytes.as_slice())),
        );
        lm_app::file_persistence::replace_or_create_group(&documents)
            .map_err(|error| error.to_string())?;
    }
    Ok(PublishedRestore {
        rom_len: restored.len(),
        associated_file_count: associated.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }

    fn archive(original: &[u8]) -> LunarRestoreArchive {
        let mut bytes = vec![0; 0x250];
        bytes[0..4].copy_from_slice(b"LR\0\x02");
        put_u32(&mut bytes, 8, 2);
        put_u64(&mut bytes, 0x10, 0x130);
        put_u64(&mut bytes, 0x18, 0x130);
        put_u32(&mut bytes, 0x130 + 0x18, 0x120);
        put_u32(&mut bytes, 0x130 + 0x28, 1);
        put_u32(&mut bytes, 0x130 + 0x30, 0x108);
        put_u32(&mut bytes, 0x130 + 0x38, 5);
        bytes[0x130 + 0x3c..0x130 + 0x40].copy_from_slice(b"DIRL");
        put_u32(&mut bytes, 0x130 + 0x40, 0x0363_8000);
        put_u32(&mut bytes, 0x130 + 0x48, 1);
        put_u32(&mut bytes, 0x130 + 0x50, 0x8000);
        put_u32(&mut bytes, 0x130 + 0x60, crc32(original));
        put_u32(&mut bytes, 0x130 + 0x80, 0x109);
        put_u32(&mut bytes, 0x130 + 0x84, 3);
        put_u32(&mut bytes, 0x130 + 0x24, 0xff ^ 0xfade_c0de);
        bytes[0x230..0x235].copy_from_slice(b"Test\0");
        bytes[0x238] = 0xff;
        bytes[0x239..0x23c].copy_from_slice(b"msc");
        let stored_checksum = bytes[0x130 + 0x30..0x230]
            .iter()
            .chain(&bytes[0x230..0x235])
            .chain(&bytes[0x238..0x23c])
            .fold(0_u32, |sum, byte| sum.wrapping_add(u32::from(*byte)))
            ^ 0xc001_c0de;
        put_u32(&mut bytes, 0x130 + 0x20, stored_checksum);
        LunarRestoreArchive::decode(&bytes).unwrap()
    }

    fn rom() -> Vec<u8> {
        let mut bytes = vec![0; 0x8000];
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd9] = 1;
        let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        bytes
    }

    fn test_directory() -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("lm-restore-dialog-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn publication_replaces_only_the_existing_target_after_validation() {
        let directory = test_directory();
        fs::create_dir(&directory).unwrap();
        let target = directory.join("target.smc");
        let original = rom();
        fs::write(&target, vec![0xa5; 0x8000]).unwrap();

        let archive = archive(&original);
        assert_eq!(
            restore_and_publish(&archive, 1, &original, &target, true)
                .unwrap()
                .rom_len,
            0x8000,
        );
        assert_eq!(fs::read(&target).unwrap(), original);
        assert_eq!(fs::read(target.with_extension("msc")).unwrap(), b"msc");
        let restored = fs::read(&target).unwrap();
        assert!(restore_and_publish(&archive, 99, &original, &target, true).is_err());
        assert_eq!(fs::read(&target).unwrap(), restored);
        fs::remove_dir_all(directory).unwrap();
    }
}
