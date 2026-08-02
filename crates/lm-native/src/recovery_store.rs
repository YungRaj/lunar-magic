use lm_app::RecoverySnapshot;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

const MAGIC: &[u8; 8] = b"LMRECOV1";
const MAX_ROM_LEN: usize = 0x20_00000 + 512;
const MAX_RECORD_LEN: usize = 8 + 8 + 2 + 4 + 4 + MAX_ROM_LEN * 2 + 4;

enum RecoveryCommand {
    Write(RecoverySnapshot),
    Clear,
}

type RecoveryResult = (Option<u64>, Result<(), String>);

#[derive(Default)]
pub(crate) struct RecoveryStore {
    path: Option<PathBuf>,
    sender: Option<SyncSender<RecoveryCommand>>,
    results: Option<Receiver<RecoveryResult>>,
    worker: Option<JoinHandle<()>>,
    pub pending: Option<RecoverySnapshot>,
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
        self.enable_at(project_dirs.data_local_dir().join("session.recovery"));
    }

    fn enable_at(&mut self, path: PathBuf) {
        match read_record(&path) {
            Ok(record) => self.pending = record,
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
                    RecoveryCommand::Clear => (None, remove_record(&worker_path)),
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
        if self.pending.is_some() || self.path.is_none() {
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
            RecoveryCommand::Clear
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
        self.pending = None;
        self.clear_queued = false;
        self.synchronize_project(None, || None);
    }

    pub fn clear_current(&mut self) {
        self.synchronize_project(None, || None);
    }

    pub fn take_pending(&mut self) -> Option<RecoverySnapshot> {
        self.pending.take()
    }
}

impl Drop for RecoveryStore {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _result = worker.join();
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
            assert_eq!(store.pending, Some(snapshot()));
            store.discard_pending();
        }
        assert_eq!(read_record(&path).unwrap(), None);
    }
}
