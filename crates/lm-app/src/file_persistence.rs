use std::fs;
use std::io;
use std::path::Path;

mod staging;

use staging::{cleanup_paths, stage, unused_backup_path};

/// Publishes a newly staged document without replacing any existing directory entry.
///
/// # Errors
///
/// Returns an I/O error if staging, synchronization, or publication fails, or if the destination
/// already exists. Once publication succeeds, failure to remove the private staging link is
/// treated as non-authoritative cleanup rather than falsely reporting that no file was saved.
pub fn write_new(destination: &Path, bytes: &[u8]) -> io::Result<()> {
    let staged = stage(destination, bytes, None)?;
    let publication = fs::hard_link(&staged, destination);
    let cleanup = fs::remove_file(&staged);
    finalize_new_publication(publication, cleanup)
}

fn finalize_new_publication(
    publication: io::Result<()>,
    cleanup: io::Result<()>,
) -> io::Result<()> {
    drop(cleanup);
    publication
}

/// Replaces an existing regular document from a same-directory staged snapshot.
///
/// On platforms where rename cannot replace a destination, the old file is moved to a unique
/// sibling backup first and restored if publication fails. Existing permissions are copied to the
/// staged snapshot. Symbolic links and non-files are rejected instead of replacing their directory
/// entries unexpectedly.
///
/// # Errors
///
/// Returns an I/O error for a missing/non-regular/symlink destination or any staging,
/// synchronization, replacement, backup, or restoration failure.
pub fn replace_existing(destination: &Path, bytes: &[u8]) -> io::Result<()> {
    let metadata = fs::symlink_metadata(destination)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "save destination must be an existing regular file",
        ));
    }
    let staged = stage(destination, bytes, Some(metadata.permissions()))?;
    match fs::rename(&staged, destination) {
        Ok(()) => Ok(()),
        Err(error)
            if destination.exists()
                && matches!(
                    error.kind(),
                    io::ErrorKind::AlreadyExists | io::ErrorKind::PermissionDenied
                ) =>
        {
            replace_with_backup(destination, &staged, &metadata).inspect_err(|_| {
                let _ = fs::remove_file(&staged);
            })
        }
        Err(error) => {
            let _ = fs::remove_file(staged);
            Err(error)
        }
    }
}

/// Replaces two existing regular documents as one recoverable publication group.
///
/// Both payloads are staged and synchronized before either destination moves. The two originals
/// are then retained as sibling backups until both snapshots publish. A failure restores both
/// original files whenever the filesystem permits it; an unrecoverable restoration error reports
/// the retained backup path instead of silently deleting the last known-good data.
///
/// # Errors
///
/// Returns an I/O error for aliased, missing, non-regular, or symlink destinations; staging and
/// synchronization failures; or publication/restoration failures.
pub fn replace_existing_pair(first: (&Path, &[u8]), second: (&Path, &[u8])) -> io::Result<()> {
    if first.0 == second.0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "paired save destinations must differ",
        ));
    }
    let first_metadata = regular_file_metadata(first.0)?;
    let second_metadata = regular_file_metadata(second.0)?;
    if same_existing_file(first.0, &first_metadata, second.0, &second_metadata)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "paired save destinations must not alias the same file",
        ));
    }
    let first_staged = stage(first.0, first.1, Some(first_metadata.permissions()))?;
    let first_staged_metadata = fs::metadata(&first_staged).inspect_err(|_| {
        let _ = fs::remove_file(&first_staged);
    })?;
    let second_staged = match stage(second.0, second.1, Some(second_metadata.permissions())) {
        Ok(staged) => staged,
        Err(error) => {
            let _ = fs::remove_file(first_staged);
            return Err(error);
        }
    };
    let first_backup = unused_backup_path(first.0)?;
    let second_backup = match unused_backup_path(second.0) {
        Ok(path) => path,
        Err(error) => {
            cleanup_paths(&[&first_staged, &second_staged]);
            return Err(error);
        }
    };

    if let Err(error) = fs::rename(first.0, &first_backup) {
        cleanup_paths(&[&first_staged, &second_staged]);
        return Err(error);
    }
    if let Err(error) = fs::rename(second.0, &second_backup) {
        let restore = fs::rename(&first_backup, first.0);
        cleanup_paths(&[&first_staged, &second_staged]);
        return match restore {
            Ok(()) => Err(error),
            Err(restore_error) => Err(pair_restore_error(&error, &restore_error, &first_backup)),
        };
    }
    if let Err(error) = fs::rename(&first_staged, first.0) {
        return rollback_pair(
            error,
            first.0,
            second.0,
            &first_backup,
            &second_backup,
            Some(&second_staged),
        );
    }
    if let Err(error) = fs::rename(&second_staged, second.0) {
        let _ = remove_if_same_file(first.0, &first_staged_metadata);
        return rollback_pair(
            error,
            first.0,
            second.0,
            &first_backup,
            &second_backup,
            None,
        );
    }
    let _ = remove_if_same_file(&first_backup, &first_metadata);
    let _ = remove_if_same_file(&second_backup, &second_metadata);
    Ok(())
}

fn same_existing_file(
    first: &Path,
    first_metadata: &fs::Metadata,
    second: &Path,
    second_metadata: &fs::Metadata,
) -> io::Result<bool> {
    if fs::canonicalize(first)? == fs::canonicalize(second)? {
        return Ok(true);
    }
    Ok(same_file_metadata(first_metadata, second_metadata))
}

fn remove_if_same_file(path: &Path, expected: &fs::Metadata) -> io::Result<()> {
    let actual = fs::metadata(path)?;
    if same_file_metadata(&actual, expected) {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn same_file_metadata(first: &fs::Metadata, second: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    first.dev() == second.dev() && first.ino() == second.ino()
}

#[cfg(windows)]
fn same_file_metadata(first: &fs::Metadata, second: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    first.volume_serial_number().is_some()
        && first.volume_serial_number() == second.volume_serial_number()
        && first.file_index().is_some()
        && first.file_index() == second.file_index()
}

#[cfg(not(any(unix, windows)))]
fn same_file_metadata(_first: &fs::Metadata, _second: &fs::Metadata) -> bool {
    false
}

fn regular_file_metadata(path: &Path) -> io::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "save destination must be an existing regular file",
        ))
    } else {
        Ok(metadata)
    }
}

fn rollback_pair(
    publish_error: io::Error,
    first: &Path,
    second: &Path,
    first_backup: &Path,
    second_backup: &Path,
    unconsumed_stage: Option<&Path>,
) -> io::Result<()> {
    if let Some(stage) = unconsumed_stage {
        let _ = fs::remove_file(stage);
    }
    let first_restore = fs::rename(first_backup, first);
    let second_restore = fs::rename(second_backup, second);
    match (first_restore, second_restore) {
        (Ok(()), Ok(())) => Err(publish_error),
        (Err(error), _) => Err(pair_restore_error(&publish_error, &error, first_backup)),
        (_, Err(error)) => Err(pair_restore_error(&publish_error, &error, second_backup)),
    }
}

fn pair_restore_error(publish: &io::Error, restore: &io::Error, backup: &Path) -> io::Error {
    io::Error::new(
        restore.kind(),
        format!(
            "paired publication failed ({publish}) and an original could not be restored ({restore}); backup remains at {}",
            backup.display()
        ),
    )
}

fn replace_with_backup(
    destination: &Path,
    staged: &Path,
    original_metadata: &fs::Metadata,
) -> io::Result<()> {
    let backup = unused_backup_path(destination)?;
    fs::rename(destination, &backup)?;
    match fs::rename(staged, destination) {
        Ok(()) => {
            let _ = remove_if_same_file(&backup, original_metadata);
            Ok(())
        }
        Err(publish_error) => match fs::rename(&backup, destination) {
            Ok(()) => Err(publish_error),
            Err(restore_error) => Err(io::Error::new(
                restore_error.kind(),
                format!(
                    "failed to publish staged save ({publish_error}) and restore original ({restore_error}); backup remains at {}",
                    backup.display()
                ),
            )),
        },
    }
}

#[cfg(test)]
#[path = "file_persistence_tests.rs"]
mod tests;
