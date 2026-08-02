use fs2::FileExt;
use lm_app::RecoverySnapshot;
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};

const MAGIC: &[u8; 8] = b"LMRECOV1";
const MAX_ROM_LEN: usize = 0x20_00000 + 512;
const MAX_RECORD_LEN: usize = 8 + 8 + 2 + 4 + 4 + MAX_ROM_LEN * 2 + 4;
const MAX_PENDING_RECORDS: usize = 16;
static SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

enum RecoveryCommand {
    Write(RecoverySnapshot),
    ClearCurrent(PathBuf),
    RemovePending(PathBuf),
}

type RecoveryResult = (Option<u64>, Result<(), String>);

#[derive(Debug, Eq, PartialEq)]
struct PendingRecovery {
    path: PathBuf,
    snapshot: RecoverySnapshot,
}

#[derive(Default)]
pub(crate) struct RecoveryStore {
    path: Option<PathBuf>,
    sender: Option<SyncSender<RecoveryCommand>>,
    results: Option<Receiver<RecoveryResult>>,
    worker: Option<JoinHandle<()>>,
    session_lock: Option<(PathBuf, File)>,
    pending: VecDeque<PendingRecovery>,
    queued_revision: Option<u64>,
    clear_queued: bool,
    pub error: Option<String>,
}

impl RecoveryStore {
    pub fn enable(&mut self) {
        let Some(project_dirs) =
            directories::ProjectDirs::from("org", "LunarMagicRust", "Lunar Magic Rust")
        else {
            self.error = Some("cannot determine the crash-recovery directory".into());
            return;
        };
        let data_directory = project_dirs.data_local_dir();
        let directory = data_directory.join("recovery");
        let (path, lock_path, lock) = match reserve_session(&directory) {
            Ok(reservation) => reservation,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let discovered = discover_records(&directory).map(|mut records| {
            let legacy = data_directory.join("session.recovery");
            if legacy.exists() {
                records.insert(0, legacy);
            }
            records
        });
        self.session_lock = Some((lock_path, lock));
        self.enable_paths(path, discovered);
    }

    #[cfg(test)]
    fn enable_at(&mut self, path: PathBuf) {
        let records = if path.exists() {
            vec![path.clone()]
        } else {
            Vec::new()
        };
        self.enable_paths(path, Ok(records));
    }

    fn enable_paths(&mut self, path: PathBuf, discovered: Result<Vec<PathBuf>, String>) {
        match discovered {
            Ok(paths) => {
                let (records, error) = read_pending_records(paths);
                self.pending = records.into();
                self.error = error;
            }
            Err(error) => self.error = Some(error),
        }
        let worker_path = path.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let (result_sender, results) = mpsc::channel();
        self.worker = Some(thread::spawn(move || {
            while let Ok(command) = receiver.recv() {
                let (revision, result) = match command {
                    RecoveryCommand::Write(snapshot) => {
                        let revision = snapshot.revision;
                        (Some(revision), write_record(&worker_path, &snapshot))
                    }
                    RecoveryCommand::ClearCurrent(path) => (None, remove_record(&path)),
                    RecoveryCommand::RemovePending(path) => (None, remove_pending_record(&path)),
                };
                let _ignored = result_sender.send((revision, result));
            }
        }));
        self.path = Some(path);
        self.sender = Some(sender);
        self.results = Some(results);
    }

    pub fn synchronize_project(
        &mut self,
        revision: Option<u64>,
        snapshot: impl FnOnce() -> Option<RecoverySnapshot>,
    ) {
        self.poll_results();
        if !self.pending.is_empty() || self.path.is_none() {
            return;
        }
        let Some(sender) = &self.sender else { return };
        let command = if let Some(revision) = revision {
            if self.queued_revision == Some(revision) {
                return;
            }
            let Some(snapshot) = snapshot() else { return };
            RecoveryCommand::Write(snapshot)
        } else {
            if self.clear_queued {
                return;
            }
            RecoveryCommand::ClearCurrent(self.path.clone().expect("enabled store has a path"))
        };
        match sender.try_send(command) {
            Ok(()) => {
                if let Some(revision) = revision {
                    self.queued_revision = Some(revision);
                    self.clear_queued = false;
                } else {
                    self.queued_revision = None;
                    self.clear_queued = true;
                }
            }
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {
                self.error = Some("crash-recovery worker stopped unexpectedly".into());
                self.sender = None;
            }
        }
    }

    fn poll_results(&mut self) {
        let Some(results) = &self.results else { return };
        for (revision, result) in results.try_iter() {
            if let Err(error) = result {
                if revision.is_some() {
                    self.queued_revision = None;
                } else {
                    self.clear_queued = false;
                }
                self.error = Some(error);
            }
        }
    }

    pub fn discard_pending(&mut self) {
        if let Some(pending) = self.pending.pop_front() {
            self.queue_clear(pending.path);
        }
    }

    pub fn pending_snapshot(&self) -> Option<&RecoverySnapshot> {
        self.pending.front().map(|pending| &pending.snapshot)
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn complete_pending_recovery(&mut self) {
        if let Some(pending) = self.pending.pop_front() {
            self.queue_clear(pending.path);
        }
    }

    fn queue_clear(&mut self, path: PathBuf) {
        self.poll_results();
        let Some(sender) = &self.sender else { return };
        match sender.try_send(RecoveryCommand::RemovePending(path)) {
            Ok(()) => {}
            Err(TrySendError::Full(command)) => {
                if let RecoveryCommand::RemovePending(path) = command
                    && let Err(error) = remove_pending_record(&path)
                {
                    self.error = Some(error);
                }
            }
            Err(TrySendError::Disconnected(_)) => {
                self.error = Some("crash-recovery worker stopped unexpectedly".into());
                self.sender = None;
            }
        }
    }
}

impl Drop for RecoveryStore {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _result = worker.join();
        }
        if let Some((path, lock)) = self.session_lock.take() {
            let _result = FileExt::unlock(&lock);
            let _result = fs::remove_file(path);
        }
    }
}

fn write_record(path: &Path, snapshot: &RecoverySnapshot) -> Result<(), String> {
    let encoded = encode(snapshot)?;
    let parent = path
        .parent()
        .ok_or_else(|| "recovery path has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create recovery directory: {error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("cannot create recovery staging file: {error}"))?;
    temporary
        .write_all(&encoded)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("cannot write recovery record: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("cannot publish recovery record: {}", error.error))?;
    Ok(())
}

fn remove_record(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot remove recovery record: {error}")),
    }
}

fn remove_pending_record(record: &Path) -> Result<(), String> {
    remove_record(record)?;
    let lock = record.with_extension("lock");
    match fs::remove_file(lock) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot remove recovery session lock: {error}")),
    }
}

fn unique_session_name() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "session-{}-{timestamp:032x}-{sequence:016x}.recovery",
        std::process::id()
    )
}

fn reserve_session(directory: &Path) -> Result<(PathBuf, PathBuf, File), String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create recovery directory: {error}"))?;
    for _attempt in 0..16 {
        let record_path = directory.join(unique_session_name());
        let lock_path = record_path.with_extension("lock");
        match File::options()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(lock) => {
                lock.try_lock_exclusive()
                    .map_err(|error| format!("cannot lock recovery session: {error}"))?;
                return Ok((record_path, lock_path, lock));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("cannot reserve recovery session: {error}")),
        }
    }
    Err("cannot allocate a unique recovery session".into())
}

fn discover_records(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot inspect recovery directory: {error}")),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot inspect recovery directory: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("recovery") {
            let lock_path = path.with_extension("lock");
            match File::options().read(true).write(true).open(&lock_path) {
                Ok(lock) => match lock.try_lock_exclusive() {
                    Ok(()) => {
                        FileExt::unlock(&lock).map_err(|error| {
                            format!("cannot release stale recovery session lock: {error}")
                        })?;
                        paths.push(path);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => {
                        return Err(format!("cannot inspect recovery session lock: {error}"));
                    }
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => paths.push(path),
                Err(error) => {
                    return Err(format!("cannot inspect recovery session lock: {error}"));
                }
            }
        }
    }
    paths.sort();
    if paths.len() > MAX_PENDING_RECORDS {
        return Err(format!(
            "recovery directory contains more than {MAX_PENDING_RECORDS} records"
        ));
    }
    Ok(paths)
}

fn read_pending_records(paths: Vec<PathBuf>) -> (Vec<PendingRecovery>, Option<String>) {
    let mut records = Vec::new();
    let mut first_error = None;
    for path in paths {
        match read_record(&path) {
            Ok(Some(snapshot)) => records.push(PendingRecovery { path, snapshot }),
            Ok(None) => {
                first_error.get_or_insert_with(|| "discovered recovery record disappeared".into());
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    (records, first_error)
}

fn read_record(path: &Path) -> Result<Option<RecoverySnapshot>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot inspect recovery record: {error}")),
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_RECORD_LEN as u64 {
        return Err("recovery record is not a bounded regular file".into());
    }
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| "recovery record length is not representable".to_owned())?;
    let mut bytes = Vec::with_capacity(capacity);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|error| format!("cannot read recovery record: {error}"))?;
    decode(&bytes).map(Some)
}

fn encode(snapshot: &RecoverySnapshot) -> Result<Vec<u8>, String> {
    if snapshot.saved_baseline == snapshot.current_rom {
        return Err("recovery snapshot is not dirty".into());
    }
    if snapshot.saved_baseline.len() > MAX_ROM_LEN || snapshot.current_rom.len() > MAX_ROM_LEN {
        return Err("recovery ROM exceeds the bounded physical size".into());
    }
    if snapshot.level.is_some_and(|level| level > 0x1ff) {
        return Err("recovery snapshot contains an invalid level".into());
    }
    let mut bytes =
        Vec::with_capacity(30 + snapshot.saved_baseline.len() + snapshot.current_rom.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&snapshot.revision.to_le_bytes());
    bytes.extend_from_slice(&snapshot.level.unwrap_or(u16::MAX).to_le_bytes());
    let baseline_len = u32::try_from(snapshot.saved_baseline.len())
        .map_err(|_| "recovery baseline length is not representable".to_owned())?;
    let current_len = u32::try_from(snapshot.current_rom.len())
        .map_err(|_| "recovery current length is not representable".to_owned())?;
    bytes.extend_from_slice(&baseline_len.to_le_bytes());
    bytes.extend_from_slice(&current_len.to_le_bytes());
    bytes.extend_from_slice(&snapshot.saved_baseline);
    bytes.extend_from_slice(&snapshot.current_rom);
    bytes.extend_from_slice(&crc32(&bytes).to_le_bytes());
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<RecoverySnapshot, String> {
    if bytes.len() < 30 || bytes.get(..8) != Some(MAGIC) {
        return Err("recovery record has an invalid header".into());
    }
    let stored_crc = u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap());
    if crc32(&bytes[..bytes.len() - 4]) != stored_crc {
        return Err("recovery record checksum does not match".into());
    }
    let revision = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let raw_level = u16::from_le_bytes(bytes[16..18].try_into().unwrap());
    if raw_level != u16::MAX && raw_level > 0x1ff {
        return Err("recovery record contains an invalid level".into());
    }
    let baseline_len = u32::from_le_bytes(bytes[18..22].try_into().unwrap()) as usize;
    let current_len = u32::from_le_bytes(bytes[22..26].try_into().unwrap()) as usize;
    if baseline_len > MAX_ROM_LEN || current_len > MAX_ROM_LEN {
        return Err("recovery ROM exceeds the bounded physical size".into());
    }
    let payload_end = 26usize
        .checked_add(baseline_len)
        .and_then(|end| end.checked_add(current_len))
        .ok_or_else(|| "recovery record length overflow".to_owned())?;
    if payload_end.checked_add(4) != Some(bytes.len()) {
        return Err("recovery record length does not match its payload".into());
    }
    let saved_baseline = bytes[26..26 + baseline_len].to_vec();
    let current_rom = bytes[26 + baseline_len..payload_end].to_vec();
    if saved_baseline == current_rom {
        return Err("recovery record does not contain unsaved changes".into());
    }
    Ok(RecoverySnapshot {
        revision,
        level: (raw_level != u16::MAX).then_some(raw_level),
        saved_baseline,
        current_rom,
    })
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> RecoverySnapshot {
        RecoverySnapshot {
            revision: 42,
            level: Some(0x1ab),
            saved_baseline: vec![0; 0x8000],
            current_rom: {
                let mut bytes = vec![0; 0x8000];
                bytes[7] = 9;
                bytes
            },
        }
    }

    #[test]
    fn record_round_trips_and_rejects_corruption_trailing_bytes_and_clean_state() {
        let expected = snapshot();
        let encoded = encode(&expected).unwrap();
        assert_eq!(decode(&encoded).unwrap(), expected);

        let mut corrupt = encoded.clone();
        corrupt[30] ^= 1;
        assert!(decode(&corrupt).unwrap_err().contains("checksum"));
        let mut trailing = encoded;
        trailing.push(0);
        assert!(decode(&trailing).is_err());

        let mut clean = snapshot();
        clean.current_rom.clone_from(&clean.saved_baseline);
        assert!(encode(&clean).is_err());
    }

    #[test]
    fn file_publication_replaces_atomically_and_clear_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.recovery");
        write_record(&path, &snapshot()).unwrap();
        assert_eq!(read_record(&path).unwrap(), Some(snapshot()));
        remove_record(&path).unwrap();
        remove_record(&path).unwrap();
        assert_eq!(read_record(&path).unwrap(), None);
    }

    #[test]
    fn worker_flushes_on_drop_discovers_pending_and_discards_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.recovery");
        {
            let mut store = RecoveryStore::default();
            store.enable_at(path.clone());
            store.synchronize_project(Some(42), || Some(snapshot()));
            store.synchronize_project(Some(42), || {
                panic!("an already queued revision must not be cloned again")
            });
        }
        assert_eq!(read_record(&path).unwrap(), Some(snapshot()));

        {
            let mut store = RecoveryStore::default();
            store.enable_at(path.clone());
            assert_eq!(store.pending_snapshot(), Some(&snapshot()));
            assert_eq!(store.pending_count(), 1);
            store.discard_pending();
        }
        assert_eq!(read_record(&path).unwrap(), None);
    }

    #[test]
    fn pending_record_is_retained_until_success_or_explicit_discard() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.recovery");
        write_record(&path, &snapshot()).unwrap();

        {
            let mut store = RecoveryStore::default();
            store.enable_at(path.clone());
            assert_eq!(store.pending_snapshot(), Some(&snapshot()));
            // Merely reading/cloning the candidate for a recovery attempt cannot remove it.
            let attempted = store.pending_snapshot().cloned().unwrap();
            assert_eq!(attempted, snapshot());
        }

        assert_eq!(read_record(&path).unwrap(), Some(snapshot()));
    }

    #[test]
    fn multiple_session_records_are_queued_and_removed_independently() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("session-1.recovery");
        let second_path = directory.path().join("session-2.recovery");
        let corrupt_path = directory.path().join("session-3.recovery");
        let current_path = directory.path().join("session-current.recovery");
        let first = snapshot();
        let mut second = snapshot();
        second.revision = 77;
        second.level = Some(0x105);
        second.current_rom[8] = 3;
        write_record(&first_path, &first).unwrap();
        write_record(&second_path, &second).unwrap();
        fs::write(&corrupt_path, b"corrupt").unwrap();

        {
            let mut store = RecoveryStore::default();
            store.enable_paths(current_path, discover_records(directory.path()));
            assert_eq!(store.pending_count(), 2);
            assert!(
                store
                    .error
                    .as_ref()
                    .is_some_and(|error| error.contains("header"))
            );
            assert_eq!(store.pending_snapshot(), Some(&first));
            store.complete_pending_recovery();
            assert_eq!(store.pending_snapshot(), Some(&second));
            store.discard_pending();
            assert_eq!(store.pending_count(), 0);
        }

        assert_eq!(read_record(&first_path).unwrap(), None);
        assert_eq!(read_record(&second_path).unwrap(), None);
        assert!(
            corrupt_path.exists(),
            "invalid records are not deleted implicitly"
        );
    }

    #[test]
    fn session_names_are_unique_and_discovery_is_bounded() {
        assert_ne!(unique_session_name(), unique_session_name());
        let directory = tempfile::tempdir().unwrap();
        for index in 0..=MAX_PENDING_RECORDS {
            File::create(directory.path().join(format!("session-{index}.recovery"))).unwrap();
        }
        assert!(
            discover_records(directory.path())
                .unwrap_err()
                .contains(&MAX_PENDING_RECORDS.to_string())
        );
    }

    #[test]
    fn discovery_skips_records_owned_by_a_live_session() {
        let directory = tempfile::tempdir().unwrap();
        let record_path = directory.path().join("session-live.recovery");
        let lock_path = record_path.with_extension("lock");
        write_record(&record_path, &snapshot()).unwrap();
        let lock = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(lock_path)
            .unwrap();
        lock.try_lock_exclusive().unwrap();

        assert!(discover_records(directory.path()).unwrap().is_empty());
        FileExt::unlock(&lock).unwrap();
        assert_eq!(
            discover_records(directory.path()).unwrap(),
            vec![record_path]
        );
    }
}
