use chrono::{Datelike as _, Timelike as _};
use eframe::egui;
use lm_project::{
    LUNAR_RESTORE_ASSOCIATED_EXTENSIONS, LUNAR_RESTORE_ASSOCIATED_FILE_COUNT, LunarRestoreArchive,
    LunarRestoreArchiveCreateRequest, LunarRestoreAutomaticDecision,
    LunarRestoreAutomaticFullReason, LunarRestoreAutomaticPolicy, LunarRestoreReversionRequest,
    PackedRestoreDate, PackedRestoreTime,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
};

const MAX_ARCHIVE_LEN: u64 = 256 * 1024 * 1024;

struct CapturedAssociatedFiles {
    files: [Option<Vec<u8>>; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
    timestamps: [u64; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT],
}

#[derive(Clone, Debug)]
struct AutomaticPolicyDraft {
    interval_enabled: bool,
    full_interval: u32,
    daily_full: bool,
}

impl Default for AutomaticPolicyDraft {
    fn default() -> Self {
        Self {
            interval_enabled: false,
            full_interval: 10,
            daily_full: false,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RestoreAppendMode {
    Delta,
    Full,
    Automatic,
}

fn automatic_description(reason: LunarRestoreAutomaticFullReason) -> &'static str {
    match reason {
        LunarRestoreAutomaticFullReason::ContinuityBreak => {
            "Automatic Full Restore Point (continuity break)."
        }
        LunarRestoreAutomaticFullReason::Interval => "Automatic Full Restore Point (interval).",
        LunarRestoreAutomaticFullReason::Daily => "Automatic Full Restore Point (daily).",
    }
}

fn windows_file_timestamp(path: &Path) -> Option<u64> {
    let duration = fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    duration
        .as_secs()
        .checked_add(11_644_473_600)?
        .checked_mul(10_000_000)?
        .checked_add(u64::from(duration.subsec_nanos() / 100))
}

fn capture_associated_files(rom_path: Option<&Path>) -> Result<CapturedAssociatedFiles, String> {
    let mut files = std::array::from_fn(|_| None);
    let mut timestamps = [0; LUNAR_RESTORE_ASSOCIATED_FILE_COUNT];
    let Some(rom_path) = rom_path else {
        return Ok(CapturedAssociatedFiles { files, timestamps });
    };
    for (slot, extension) in LUNAR_RESTORE_ASSOCIATED_EXTENSIONS.iter().enumerate() {
        let path = rom_path.with_extension(extension);
        match crate::dialogs::read_regular_bounded(
            &path,
            MAX_ARCHIVE_LEN,
            "associated restore file",
        ) {
            Ok(bytes) => {
                timestamps[slot] = windows_file_timestamp(&path).unwrap_or(0);
                files[slot] = Some(bytes);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot capture {}: {error}", path.display())),
        }
    }
    Ok(CapturedAssociatedFiles { files, timestamps })
}

fn build_full_archive(
    original: &[u8],
    current: &[u8],
    document_path: Option<&Path>,
    created: PackedRestoreDate,
    created_time: PackedRestoreTime,
) -> Result<Vec<u8>, String> {
    let associated = capture_associated_files(document_path)?;
    let mut request = LunarRestoreArchiveCreateRequest::new(
        original,
        current,
        "Manual Full Restore Point.",
        created,
        created_time,
    );
    request.last_rom_timestamp = document_path.and_then(windows_file_timestamp).unwrap_or(0);
    request.associated_files = std::array::from_fn(|slot| associated.files[slot].as_deref());
    request.associated_file_timestamps = associated.timestamps;
    let bytes = LunarRestoreArchive::create_full(&request).map_err(|error| error.to_string())?;
    LunarRestoreArchive::decode(&bytes).map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn local_restore_date_time() -> Result<(PackedRestoreDate, PackedRestoreTime), String> {
    let now = chrono::Local::now();
    Ok((
        PackedRestoreDate {
            year: u16::try_from(now.year()).map_err(|_| "local year is out of range")?,
            month: u8::try_from(now.month()).unwrap(),
            day: u8::try_from(now.day()).unwrap(),
        },
        PackedRestoreTime {
            day_of_week: u8::try_from(now.weekday().num_days_from_sunday()).unwrap(),
            hour: u8::try_from(now.hour()).unwrap(),
            minute: u8::try_from(now.minute()).unwrap(),
            second: u8::try_from(now.second()).unwrap(),
        },
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_appended_archive(
    archive: &LunarRestoreArchive,
    original: &[u8],
    current: &[u8],
    observed: &[u8],
    document_path: &Path,
    mode: RestoreAppendMode,
    automatic_policy: LunarRestoreAutomaticPolicy,
    created: PackedRestoreDate,
    created_time: PackedRestoreTime,
) -> Result<Vec<u8>, String> {
    let record_id = archive.header.latest_record_id;
    let base = archive
        .restore_through(record_id, original)
        .map_err(|error| error.to_string())?;
    let observed_timestamp = windows_file_timestamp(document_path).unwrap_or(0);
    let automatic =
        archive.automatic_decision(automatic_policy, observed_timestamp, observed, created);
    let selected_mode = match mode {
        RestoreAppendMode::Automatic => match automatic {
            LunarRestoreAutomaticDecision::Delta => RestoreAppendMode::Delta,
            LunarRestoreAutomaticDecision::Full(_) => RestoreAppendMode::Full,
        },
        explicit => explicit,
    };
    let description = match (mode, automatic) {
        (RestoreAppendMode::Automatic, LunarRestoreAutomaticDecision::Delta) => {
            "Automatic Delta Restore Point."
        }
        (RestoreAppendMode::Automatic, LunarRestoreAutomaticDecision::Full(reason)) => {
            automatic_description(reason)
        }
        (_, _) if matches!(selected_mode, RestoreAppendMode::Delta) => {
            "Manual Delta Restore Point."
        }
        _ => "Manual Full Restore Point.",
    };
    let mut associated = capture_associated_files(Some(document_path))?;
    if matches!(selected_mode, RestoreAppendMode::Delta) {
        for (slot, timestamp) in associated.timestamps.iter().enumerate() {
            if *timestamp == 0 || *timestamp == archive.header.associated_file_timestamps[slot] {
                associated.files[slot] = None;
            }
        }
    }
    let request_base = if matches!(selected_mode, RestoreAppendMode::Full) {
        original
    } else {
        &base
    };
    let mut request = LunarRestoreArchiveCreateRequest::new(
        request_base,
        current,
        description,
        created,
        created_time,
    );
    request.last_rom_timestamp = observed_timestamp;
    request.associated_files = std::array::from_fn(|slot| associated.files[slot].as_deref());
    request.associated_file_timestamps = associated.timestamps;
    let bytes = if matches!(selected_mode, RestoreAppendMode::Full) {
        archive.append_full(&request)
    } else {
        archive.append_delta(&request)
    }
    .map_err(|error| error.to_string())?;
    LunarRestoreArchive::decode(&bytes).map_err(|error| error.to_string())?;
    Ok(bytes)
}

pub(crate) fn create_full_for_open_project(app: &lm_app::AppState) -> Result<bool, String> {
    let project = app
        .project()
        .ok_or_else(|| "open a ROM before creating a restore point".to_owned())?;
    let Some(original_path) = crate::dialogs::choose_restore_original_rom() else {
        return Ok(false);
    };
    let Some(archive_path) = crate::dialogs::choose_new_restore_archive() else {
        return Ok(false);
    };
    let original = crate::dialogs::read_regular_bounded(
        &original_path,
        crate::dialogs::MAX_ROM_FILE_LEN,
        "original restore ROM",
    )
    .map_err(|error| error.to_string())?;
    let current = project.save_snapshot();
    let (created, created_time) = local_restore_date_time()?;
    let bytes = build_full_archive(
        &original,
        &current,
        app.document_path.as_deref(),
        created,
        created_time,
    )?;
    lm_app::file_persistence::write_new(&archive_path, &bytes)
        .map_err(|error| error.to_string())?;
    Ok(true)
}

pub(crate) fn append_for_open_project(
    app: &lm_app::AppState,
    mode: RestoreAppendMode,
) -> Result<bool, String> {
    append_for_open_project_with_policy(app, mode, LunarRestoreAutomaticPolicy::default())
}

fn append_for_open_project_with_policy(
    app: &lm_app::AppState,
    mode: RestoreAppendMode,
    automatic_policy: LunarRestoreAutomaticPolicy,
) -> Result<bool, String> {
    let project = app
        .project()
        .ok_or_else(|| "open a ROM before appending a restore point".to_owned())?;
    let document_path = app
        .document_path
        .as_deref()
        .ok_or_else(|| "save the open ROM before appending a restore point".to_owned())?;
    let Some(original_path) = crate::dialogs::choose_restore_original_rom() else {
        return Ok(false);
    };
    let Some(archive_path) = crate::dialogs::choose_restore_archive() else {
        return Ok(false);
    };
    let original = crate::dialogs::read_regular_bounded(
        &original_path,
        crate::dialogs::MAX_ROM_FILE_LEN,
        "original restore ROM",
    )
    .map_err(|error| error.to_string())?;
    let archive_bytes =
        crate::dialogs::read_regular_bounded(&archive_path, MAX_ARCHIVE_LEN, "restore archive")
            .map_err(|error| error.to_string())?;
    let archive = LunarRestoreArchive::decode(&archive_bytes).map_err(|error| error.to_string())?;
    let current = project.save_snapshot();
    let observed = crate::dialogs::read_regular_bounded(
        document_path,
        crate::dialogs::MAX_ROM_FILE_LEN,
        "open ROM",
    )
    .map_err(|error| error.to_string())?;
    let (created, created_time) = local_restore_date_time()?;
    let bytes = build_appended_archive(
        &archive,
        &original,
        &current,
        &observed,
        document_path,
        mode,
        automatic_policy,
        created,
        created_time,
    )?;
    lm_app::file_persistence::replace_existing(&archive_path, &bytes)
        .map_err(|error| error.to_string())?;
    Ok(true)
}

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
    automatic_policy: Option<AutomaticPolicyDraft>,
    automatic_defaults: AutomaticPolicyDraft,
}

impl RestorePointDialog {
    pub(crate) fn automatic_preferences(&self) -> String {
        format!(
            "{}:{}:{}",
            u8::from(self.automatic_defaults.interval_enabled),
            self.automatic_defaults.full_interval,
            u8::from(self.automatic_defaults.daily_full),
        )
    }

    pub(crate) fn load_automatic_preferences(&mut self, encoded: &str) -> Result<(), String> {
        let mut fields = encoded.split(':');
        let interval_enabled = parse_preference_bool(fields.next(), "interval enabled")?;
        let full_interval = fields
            .next()
            .ok_or_else(|| "missing full interval".to_owned())?
            .parse::<u32>()
            .map_err(|_| "invalid full interval".to_owned())?;
        let daily_full = parse_preference_bool(fields.next(), "daily full")?;
        if fields.next().is_some() || full_interval == 0 {
            return Err("invalid automatic restore preference fields".to_owned());
        }
        self.automatic_defaults = AutomaticPolicyDraft {
            interval_enabled,
            full_interval,
            daily_full,
        };
        Ok(())
    }

    pub(crate) const fn is_busy(&self) -> bool {
        self.loaded.is_some() || self.running.is_some() || self.automatic_policy.is_some()
    }

    pub(crate) fn open_automatic_policy(&mut self) {
        self.automatic_policy = Some(self.automatic_defaults.clone());
        self.completed = None;
        self.error = None;
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

    pub(crate) fn show(&mut self, context: &egui::Context, app: &lm_app::AppState) {
        self.poll();
        self.show_automatic_policy(context, app);
        self.show_loaded(context);
        self.show_running(context);
        self.show_result(context);
    }

    fn show_automatic_policy(&mut self, context: &egui::Context, app: &lm_app::AppState) {
        let Some(policy) = self.automatic_policy.as_mut() else {
            return;
        };
        let mut create = false;
        let mut cancel = false;
        egui::Window::new("Automatic Restore Point")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.checkbox(
                    &mut policy.interval_enabled,
                    "Create a full point after this many deltas",
                );
                ui.add_enabled(
                    policy.interval_enabled,
                    egui::DragValue::new(&mut policy.full_interval).range(1..=u32::MAX),
                );
                ui.checkbox(&mut policy.daily_full, "Create one full point per day");
                ui.label(
                    "A ROM timestamp or checksum continuity break always forces a full point.",
                );
                ui.horizontal(|ui| {
                    create = ui.button("Append").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if cancel {
            self.automatic_policy = None;
        } else if create {
            let selected = policy.clone();
            let policy = LunarRestoreAutomaticPolicy {
                full_interval: selected.interval_enabled.then_some(selected.full_interval),
                daily_full: selected.daily_full,
            };
            match append_for_open_project_with_policy(app, RestoreAppendMode::Automatic, policy) {
                Ok(true) => {
                    self.automatic_defaults = selected;
                    self.automatic_policy = None;
                    self.completed = Some("Automatic restore point appended.".to_owned());
                }
                Ok(false) => {}
                Err(error) => self.error = Some(error),
            }
        }
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
                    &loaded.archive_path,
                    record_id,
                    &loaded.original,
                    &worker_target,
                    restore_associated_files,
                );
                let result = result.map_err(|error| {
                    match append_failed_reversion(&loaded.archive, &loaded.archive_path, record_id)
                    {
                        Ok(()) => format!("{error} A failed reversion marker was recorded."),
                        Err(marker_error) => {
                            format!(
                                "{error} The failed reversion marker also failed: {marker_error}"
                            )
                        }
                    }
                });
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

fn parse_preference_bool(value: Option<&str>, field: &str) -> Result<bool, String> {
    match value {
        Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(_) => Err(format!("invalid {field} flag")),
        None => Err(format!("missing {field} flag")),
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
    archive_path: &Path,
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
    let (created, created_time) = local_restore_date_time()?;
    let reversion = LunarRestoreReversionRequest {
        target_record_id: record_id,
        restored_rom: &restored,
        created,
        created_time,
        last_rom_timestamp: windows_system_timestamp(),
        associated_file_timestamps: archive.header.associated_file_timestamps,
        failed: false,
    };
    let updated_archive = archive
        .append_reversion(&reversion)
        .map_err(|error| error.to_string())?;
    let sidecar_paths: Vec<PathBuf> = associated
        .iter()
        .map(|file| target.with_extension(file.extension))
        .collect();
    let mut documents: Vec<(&Path, &[u8])> = Vec::with_capacity(associated.len() + 2);
    documents.push((archive_path, &updated_archive));
    documents.push((target, &restored));
    documents.extend(
        sidecar_paths
            .iter()
            .zip(&associated)
            .map(|(path, file)| (path.as_path(), file.bytes.as_slice())),
    );
    lm_app::file_persistence::replace_or_create_group(&documents)
        .map_err(|error| error.to_string())?;
    Ok(PublishedRestore {
        rom_len: restored.len(),
        associated_file_count: associated.len(),
    })
}

fn windows_system_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| {
            duration
                .as_secs()
                .checked_add(11_644_473_600)?
                .checked_mul(10_000_000)?
                .checked_add(u64::from(duration.subsec_nanos() / 100))
        })
        .unwrap_or(0)
}

fn append_failed_reversion(
    archive: &LunarRestoreArchive,
    archive_path: &Path,
    record_id: u32,
) -> Result<(), String> {
    let (created, created_time) = local_restore_date_time()?;
    let request = LunarRestoreReversionRequest {
        target_record_id: record_id,
        restored_rom: &[],
        created,
        created_time,
        last_rom_timestamp: 0,
        associated_file_timestamps: archive.header.associated_file_timestamps,
        failed: true,
    };
    let bytes = archive
        .append_reversion(&request)
        .map_err(|error| error.to_string())?;
    lm_app::file_persistence::replace_existing(archive_path, &bytes)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

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
        put_u32(&mut bytes, 0x130 + 0x40, 0x0363_8001);
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
        std::env::temp_dir().join(format!(
            "lm-restore-dialog-{}-{nonce}-{}",
            std::process::id(),
            NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn publication_replaces_only_the_existing_target_after_validation() {
        let directory = test_directory();
        fs::create_dir(&directory).unwrap();
        let target = directory.join("target.smc");
        let archive_path = directory.join("points.lrp");
        let original = rom();
        fs::write(&target, vec![0xa5; 0x8000]).unwrap();

        let archive = archive(&original);
        fs::write(&archive_path, archive.bytes()).unwrap();
        assert_eq!(
            restore_and_publish(&archive, &archive_path, 1, &original, &target, true)
                .unwrap()
                .rom_len,
            0x8000,
        );
        assert_eq!(fs::read(&target).unwrap(), original);
        assert_eq!(fs::read(target.with_extension("msc")).unwrap(), b"msc");
        let revised_archive =
            LunarRestoreArchive::decode(&fs::read(&archive_path).unwrap()).unwrap();
        assert_eq!(revised_archive.records.len(), 2);
        assert_eq!(revised_archive.records[1].reversion_target_offset, 0x130);
        let restored = fs::read(&target).unwrap();
        assert!(
            restore_and_publish(&archive, &archive_path, 99, &original, &target, true).is_err()
        );
        assert_eq!(fs::read(&target).unwrap(), restored);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_restore_marker_matches_native_failed_reversion_form() {
        let directory = test_directory();
        fs::create_dir(&directory).unwrap();
        let archive_path = directory.join("points.lrp");
        let original = rom();
        let archive = archive(&original);
        fs::write(&archive_path, archive.bytes()).unwrap();

        append_failed_reversion(&archive, &archive_path, 1).unwrap();
        let marked = LunarRestoreArchive::decode(&fs::read(&archive_path).unwrap()).unwrap();
        assert_eq!(marked.records.len(), 2);
        assert_eq!(
            marked.records[1].description_text(),
            "Reverted to save point #1. (failed?)"
        );
        assert_eq!(marked.records[1].rom_hash, 0);
        assert_eq!(marked.header.last_rom_timestamp, 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn automatic_preferences_round_trip_and_reject_malformed_values() {
        let mut dialog = RestorePointDialog::default();
        dialog.load_automatic_preferences("1:27:1").unwrap();
        assert_eq!(dialog.automatic_preferences(), "1:27:1");
        assert!(dialog.load_automatic_preferences("1:0:1").is_err());
        assert!(dialog.load_automatic_preferences("yes:27:1").is_err());
        assert!(dialog.load_automatic_preferences("1:27:1:extra").is_err());
    }

    #[test]
    fn full_creation_capture_uses_exact_associated_slot_order() {
        let directory = test_directory();
        fs::create_dir(&directory).unwrap();
        let rom = directory.join("game.smc");
        fs::write(&rom, b"rom").unwrap();
        fs::write(rom.with_extension("msc"), b"msc").unwrap();
        fs::write(rom.with_extension("s16ov"), b"s16ov").unwrap();

        let associated = capture_associated_files(Some(&rom)).unwrap();
        assert_eq!(associated.files[0].as_deref(), Some(b"msc".as_slice()));
        assert_eq!(associated.files[8].as_deref(), Some(b"s16ov".as_slice()));
        assert!(associated.files[1].is_none());
        assert_ne!(associated.timestamps[0], 0);
        assert_ne!(associated.timestamps[8], 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn full_creation_round_trips_current_rom_and_nonempty_sidecars() {
        let directory = test_directory();
        fs::create_dir(&directory).unwrap();
        let rom_path = directory.join("game.smc");
        let original = rom();
        let mut current = original.clone();
        current[0x1234] = 0x5a;
        fs::write(&rom_path, &current).unwrap();
        fs::write(rom_path.with_extension("msc"), b"level names").unwrap();
        fs::write(rom_path.with_extension("s16ov"), b"sprite override").unwrap();

        let bytes = build_full_archive(
            &original,
            &current,
            Some(&rom_path),
            PackedRestoreDate {
                year: 2026,
                month: 7,
                day: 30,
            },
            PackedRestoreTime {
                day_of_week: 4,
                hour: 12,
                minute: 34,
                second: 56,
            },
        )
        .unwrap();
        let archive = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(archive.restore_through(1, &original).unwrap(), current);
        let restored = archive.restore_associated_files_through(1).unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].extension, "msc");
        assert_eq!(restored[0].bytes, b"level names");
        assert_eq!(restored[1].extension, "s16ov");
        assert_eq!(restored[1].bytes, b"sprite override");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn delta_append_stores_only_changed_sidecar_slots_and_round_trips() {
        let directory = test_directory();
        fs::create_dir(&directory).unwrap();
        let rom_path = directory.join("game.smc");
        let original = rom();
        let mut first = original.clone();
        first[0x1234] = 0x5a;
        fs::write(&rom_path, &first).unwrap();
        fs::write(rom_path.with_extension("msc"), b"unchanged").unwrap();
        fs::write(rom_path.with_extension("s16ov"), b"first override").unwrap();
        let date = PackedRestoreDate {
            year: 2026,
            month: 7,
            day: 30,
        };
        let time = PackedRestoreTime {
            day_of_week: 4,
            hour: 12,
            minute: 34,
            second: 56,
        };
        let initial = LunarRestoreArchive::decode(
            &build_full_archive(&original, &first, Some(&rom_path), date, time).unwrap(),
        )
        .unwrap();

        let mut second = first.clone();
        second[0x2345] = 0xa5;
        fs::write(&rom_path, &second).unwrap();
        fs::write(rom_path.with_extension("s16ov"), b"second override").unwrap();
        let bytes = build_appended_archive(
            &initial,
            &original,
            &second,
            &first,
            &rom_path,
            RestoreAppendMode::Delta,
            LunarRestoreAutomaticPolicy::default(),
            date,
            time,
        )
        .unwrap();
        let appended = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_eq!(appended.records.len(), 2);
        assert_eq!(appended.records[1].associated_files[0].relative_offset, 0);
        assert_ne!(appended.records[1].associated_files[8].relative_offset, 0);
        assert_eq!(appended.restore_through(2, &original).unwrap(), second);
        let restored = appended.restore_associated_files_through(2).unwrap();
        assert_eq!(restored[0].bytes, b"unchanged");
        assert_eq!(restored[1].bytes, b"second override");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn automatic_daily_policy_appends_a_full_checkpoint() {
        let directory = test_directory();
        fs::create_dir(&directory).unwrap();
        let rom_path = directory.join("game.smc");
        let original = rom();
        let mut first = original.clone();
        first[0x1111] = 0x11;
        fs::write(&rom_path, &first).unwrap();
        let first_date = PackedRestoreDate {
            year: 2026,
            month: 7,
            day: 30,
        };
        let time = PackedRestoreTime {
            day_of_week: 4,
            hour: 12,
            minute: 34,
            second: 56,
        };
        let initial = LunarRestoreArchive::decode(
            &build_full_archive(&original, &first, Some(&rom_path), first_date, time).unwrap(),
        )
        .unwrap();
        let mut second = first.clone();
        second[0x2222] = 0x22;
        let bytes = build_appended_archive(
            &initial,
            &original,
            &second,
            &first,
            &rom_path,
            RestoreAppendMode::Automatic,
            LunarRestoreAutomaticPolicy {
                full_interval: None,
                daily_full: true,
            },
            PackedRestoreDate {
                year: 2026,
                month: 7,
                day: 31,
            },
            time,
        )
        .unwrap();
        let appended = LunarRestoreArchive::decode(&bytes).unwrap();
        assert_ne!(appended.records[1].directory_version & 3, 0);
        assert_eq!(
            appended.records[1].description_text(),
            "Automatic Full Restore Point (daily)."
        );
        assert_eq!(appended.restore_through(2, &original).unwrap(), second);
        fs::remove_dir_all(directory).unwrap();
    }
}
