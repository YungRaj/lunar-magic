use crate::{AppState, RecentDocuments, file_persistence};
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

pub struct RecentStateFile {
    path: PathBuf,
    persisted: Vec<u8>,
}

impl RecentStateFile {
    /// Opens a bounded regular recent-document store and installs its value into `app`.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe file types, excessive or malformed data, or file-system I/O.
    pub fn load(path: PathBuf, app: &mut AppState) -> Result<Self, Box<dyn std::error::Error>> {
        let persisted = match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                    return Err("recent-state path must be a regular file or not yet exist".into());
                }
                let length = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
                if length > RecentDocuments::MAX_FILE_BYTES {
                    return Err("recent-state file exceeds its bounded format limit".into());
                }
                let bytes = read_bounded(&path)?;
                app.set_recent_documents(RecentDocuments::decode(&bytes)?);
                bytes
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                RecentDocuments::default().encode()?
            }
            Err(error) => return Err(error.into()),
        };
        Ok(Self { path, persisted })
    }

    /// Atomically persists the recent-document list only when its canonical bytes changed.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding or safe create/replace publication fails.
    pub fn persist_if_changed(
        &mut self,
        app: &AppState,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let bytes = app.recent_documents().encode()?;
        if bytes == self.persisted {
            return Ok(false);
        }
        match fs::symlink_metadata(&self.path) {
            Ok(_) => file_persistence::replace_existing(&self.path, &bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                file_persistence::write_new(&self.path, &bytes)?;
            }
            Err(error) => return Err(error.into()),
        }
        self.persisted = bytes;
        Ok(true)
    }
}

fn read_bounded(path: &std::path::Path) -> io::Result<Vec<u8>> {
    let limit = u64::try_from(RecentDocuments::MAX_FILE_BYTES).unwrap_or(u64::MAX);
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "recent-state path must be a regular file",
        ));
    }
    if metadata.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "recent-state file exceeds its bounded format limit",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() > RecentDocuments::MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "recent-state file exceeds its bounded format limit",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn temporary_directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-recent-state-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn missing_store_is_created_only_after_the_value_changes_and_reloads() {
        let directory = temporary_directory();
        let path = directory.join("Recent 日本語.lmrecent");
        let mut app = AppState::default();
        let mut state = RecentStateFile::load(path.clone(), &mut app).unwrap();
        assert!(!state.persist_if_changed(&app).unwrap());
        assert!(!path.exists());

        let mut recent = app.recent_documents().clone();
        recent.note("My Hack 日本語.smc");
        app.set_recent_documents(recent);
        assert!(state.persist_if_changed(&app).unwrap());
        assert!(!state.persist_if_changed(&app).unwrap());

        let mut reopened = AppState::default();
        RecentStateFile::load(path.clone(), &mut reopened).unwrap();
        assert_eq!(reopened.recent_documents(), app.recent_documents());
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn malformed_and_oversized_stores_do_not_replace_application_state() {
        let directory = temporary_directory();
        let path = directory.join("recent");
        fs::write(&path, b"bad").unwrap();
        let mut app = AppState::default();
        let mut existing = RecentDocuments::default();
        existing.note("keep.smc");
        app.set_recent_documents(existing.clone());
        assert!(RecentStateFile::load(path.clone(), &mut app).is_err());
        assert_eq!(app.recent_documents(), &existing);

        fs::write(&path, vec![0; RecentDocuments::MAX_FILE_BYTES + 1]).unwrap();
        assert!(RecentStateFile::load(path.clone(), &mut app).is_err());
        assert_eq!(app.recent_documents(), &existing);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
