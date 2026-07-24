use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(super) fn cleanup_paths(paths: &[&Path]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

pub(super) fn stage(
    destination: &Path,
    bytes: &[u8],
    permissions: Option<fs::Permissions>,
) -> io::Result<PathBuf> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "destination has no file name"))?
        .to_string_lossy();
    for attempt in 0_u16..128 {
        let staged = parent.join(format!(
            ".{name}.lm-app-{}-{attempt}.tmp",
            std::process::id()
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = (|| {
            file.write_all(bytes)?;
            file.flush()?;
            if let Some(permissions) = permissions {
                file.set_permissions(permissions)?;
            }
            file.sync_all()
        })();
        drop(file);
        if let Err(error) = result {
            let _ = fs::remove_file(staged);
            return Err(error);
        }
        return Ok(staged);
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a staging file",
    ))
}

pub(super) fn unused_backup_path(destination: &Path) -> io::Result<PathBuf> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "destination has no file name"))?
        .to_string_lossy();
    for attempt in 0_u16..128 {
        let backup = parent.join(format!(
            ".{name}.lm-app-{}-{attempt}.backup",
            std::process::id()
        ));
        match fs::symlink_metadata(&backup) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(backup),
            Ok(_) => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a backup path",
    ))
}
