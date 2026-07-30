use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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

/// Publishes a group of newly staged documents with rollback of this group's publications.
///
/// Every payload is staged and synchronized before the first destination is created. Destinations
/// must be pairwise distinct and absent. If any hard-link publication fails, files already
/// published by this call are removed only after verifying that they still identify the staged
/// inode; unrelated replacements are never deleted.
///
/// # Errors
///
/// Returns an I/O error for an empty or aliased destination, an existing destination, staging or
/// synchronization failure, or publication failure.
pub fn write_new_group(documents: &[(&Path, &[u8])]) -> io::Result<()> {
    if documents.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "grouped save requires at least one document",
        ));
    }
    let mut staged = Vec::with_capacity(documents.len());
    let mut resolved = Vec::with_capacity(documents.len());
    for (index, (destination, bytes)) in documents.iter().copied().enumerate() {
        let name = destination.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "destination has no file name")
        })?;
        let parent = fs::canonicalize(destination.parent().unwrap_or_else(|| Path::new(".")))?;
        let resolved_destination = parent.join(name);
        if resolved[..index].contains(&resolved_destination) {
            cleanup_paths(&staged.iter().map(PathBuf::as_path).collect::<Vec<_>>());
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "grouped save destinations must differ",
            ));
        }
        resolved.push(resolved_destination);
        match stage(destination, bytes, None) {
            Ok(path) => staged.push(path),
            Err(error) => {
                cleanup_paths(&staged.iter().map(PathBuf::as_path).collect::<Vec<_>>());
                return Err(error);
            }
        }
    }

    let mut published: Vec<(&Path, fs::Metadata)> = Vec::with_capacity(documents.len());
    for ((destination, _), staged_path) in documents.iter().zip(&staged) {
        let destination = *destination;
        let metadata = match fs::metadata(staged_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                for (path, expected) in &published {
                    let _ = remove_if_same_file(path, expected);
                }
                cleanup_paths(&staged.iter().map(PathBuf::as_path).collect::<Vec<_>>());
                return Err(error);
            }
        };
        if let Err(error) = fs::hard_link(staged_path, destination) {
            for (path, expected) in &published {
                let _ = remove_if_same_file(path, expected);
            }
            cleanup_paths(&staged.iter().map(PathBuf::as_path).collect::<Vec<_>>());
            return Err(error);
        }
        published.push((destination, metadata));
    }
    cleanup_paths(&staged.iter().map(PathBuf::as_path).collect::<Vec<_>>());
    Ok(())
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

/// Replaces existing regular files and creates absent files as one recoverable publication group.
///
/// Every payload is staged before any destination changes. Existing destinations are retained as
/// sibling backups until all staged files publish. If publication fails, newly created files are
/// removed and existing originals are restored whenever the filesystem permits it.
///
/// # Errors
///
/// Returns an I/O error for an empty or aliased group, a symbolic-link/non-file destination,
/// staging failure, or publication/restoration failure.
pub fn replace_or_create_group(documents: &[(&Path, &[u8])]) -> io::Result<()> {
    if documents.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "grouped replacement requires at least one document",
        ));
    }

    struct Entry<'a> {
        destination: &'a Path,
        original: Option<fs::Metadata>,
        staged: PathBuf,
        staged_metadata: fs::Metadata,
        backup: Option<PathBuf>,
    }

    let mut entries: Vec<Entry<'_>> = Vec::with_capacity(documents.len());
    let mut resolved = Vec::with_capacity(documents.len());
    for (destination, bytes) in documents.iter().copied() {
        let Some(name) = destination.file_name() else {
            cleanup_paths(
                &entries
                    .iter()
                    .map(|entry| entry.staged.as_path())
                    .collect::<Vec<_>>(),
            );
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "destination has no file name",
            ));
        };
        let parent = match fs::canonicalize(destination.parent().unwrap_or_else(|| Path::new(".")))
        {
            Ok(parent) => parent,
            Err(error) => {
                cleanup_paths(
                    &entries
                        .iter()
                        .map(|entry| entry.staged.as_path())
                        .collect::<Vec<_>>(),
                );
                return Err(error);
            }
        };
        let resolved_destination = parent.join(name);
        if resolved.contains(&resolved_destination) {
            cleanup_paths(
                &entries
                    .iter()
                    .map(|entry| entry.staged.as_path())
                    .collect::<Vec<_>>(),
            );
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "grouped replacement destinations must differ",
            ));
        }
        resolved.push(resolved_destination);

        let original = match fs::symlink_metadata(destination) {
            Ok(metadata) if metadata.file_type().is_file() => Some(metadata),
            Ok(_) => {
                cleanup_paths(
                    &entries
                        .iter()
                        .map(|entry| entry.staged.as_path())
                        .collect::<Vec<_>>(),
                );
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "save destination must be absent or an existing regular file",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                cleanup_paths(
                    &entries
                        .iter()
                        .map(|entry| entry.staged.as_path())
                        .collect::<Vec<_>>(),
                );
                return Err(error);
            }
        };
        if let Some(metadata) = &original {
            for entry in &entries {
                let aliased = if let Some(other) = &entry.original {
                    match same_existing_file(destination, metadata, entry.destination, other) {
                        Ok(aliased) => aliased,
                        Err(error) => {
                            cleanup_paths(
                                &entries
                                    .iter()
                                    .map(|entry| entry.staged.as_path())
                                    .collect::<Vec<_>>(),
                            );
                            return Err(error);
                        }
                    }
                } else {
                    false
                };
                if aliased {
                    cleanup_paths(
                        &entries
                            .iter()
                            .map(|entry| entry.staged.as_path())
                            .collect::<Vec<_>>(),
                    );
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "grouped replacement destinations must not alias the same file",
                    ));
                }
            }
        }
        let staged = match stage(
            destination,
            bytes,
            original.as_ref().map(fs::Metadata::permissions),
        ) {
            Ok(staged) => staged,
            Err(error) => {
                cleanup_paths(
                    &entries
                        .iter()
                        .map(|entry| entry.staged.as_path())
                        .collect::<Vec<_>>(),
                );
                return Err(error);
            }
        };
        let staged_metadata = fs::metadata(&staged).inspect_err(|_| {
            let _ = fs::remove_file(&staged);
            cleanup_paths(
                &entries
                    .iter()
                    .map(|entry| entry.staged.as_path())
                    .collect::<Vec<_>>(),
            );
        })?;
        let backup = if original.is_some() {
            match unused_backup_path(destination) {
                Ok(path) => Some(path),
                Err(error) => {
                    let _ = fs::remove_file(staged);
                    cleanup_paths(
                        &entries
                            .iter()
                            .map(|entry| entry.staged.as_path())
                            .collect::<Vec<_>>(),
                    );
                    return Err(error);
                }
            }
        } else {
            None
        };
        entries.push(Entry {
            destination,
            original,
            staged,
            staged_metadata,
            backup,
        });
    }

    let mut backed_up: Vec<usize> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(backup) = &entry.backup else {
            continue;
        };
        if let Err(error) = fs::rename(entry.destination, backup) {
            for previous_index in backed_up.iter().rev() {
                let previous = &entries[*previous_index];
                if let Some(previous_backup) = &previous.backup {
                    let _ = fs::rename(previous_backup, previous.destination);
                }
            }
            cleanup_paths(
                &entries
                    .iter()
                    .map(|entry| entry.staged.as_path())
                    .collect::<Vec<_>>(),
            );
            return Err(error);
        }
        backed_up.push(index);
    }

    for (index, entry) in entries.iter().enumerate() {
        if let Err(error) = fs::rename(&entry.staged, entry.destination) {
            for published in entries[..index].iter().rev() {
                let _ = remove_if_same_file(published.destination, &published.staged_metadata);
            }
            for original in entries.iter().rev() {
                if let Some(backup) = &original.backup
                    && let Err(restore_error) = fs::rename(backup, original.destination)
                {
                    cleanup_paths(
                        &entries[index..]
                            .iter()
                            .map(|entry| entry.staged.as_path())
                            .collect::<Vec<_>>(),
                    );
                    return Err(io::Error::new(
                        restore_error.kind(),
                        format!(
                            "grouped publication failed ({error}) and an original could not be restored ({restore_error}); backup remains at {}",
                            backup.display()
                        ),
                    ));
                }
            }
            cleanup_paths(
                &entries[index..]
                    .iter()
                    .map(|entry| entry.staged.as_path())
                    .collect::<Vec<_>>(),
            );
            return Err(error);
        }
    }

    for entry in &entries {
        if let (Some(backup), Some(original)) = (&entry.backup, &entry.original) {
            let _ = remove_if_same_file(backup, original);
        }
    }
    Ok(())
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
