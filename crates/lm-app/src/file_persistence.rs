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

/// Publishes a new document while removing an obsolete regular sibling as one recoverable change.
///
/// The destination must remain create-new. If the obsolete file exists, it is moved to a private
/// backup before publication and restored if publication loses a collision race or otherwise
/// fails. Symbolic links and non-files are rejected rather than removed.
///
/// # Errors
///
/// Returns an I/O error for aliased paths, an existing destination, a non-regular obsolete path,
/// staging or publication failure, or failure to restore the obsolete file.
pub fn write_new_removing_existing(
    destination: &Path,
    obsolete: &Path,
    bytes: &[u8],
) -> io::Result<()> {
    let destination_name = destination.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "destination has no file name")
    })?;
    let obsolete_name = obsolete.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "obsolete path has no file name",
        )
    })?;
    let destination_parent =
        fs::canonicalize(destination.parent().unwrap_or_else(|| Path::new(".")))?;
    let obsolete_parent = fs::canonicalize(obsolete.parent().unwrap_or_else(|| Path::new(".")))?;
    if destination_parent.join(destination_name) == obsolete_parent.join(obsolete_name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "new and obsolete destinations must differ",
        ));
    }
    match fs::symlink_metadata(destination) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "save destination already exists",
            ));
        }
        Err(error) => return Err(error),
    }
    let obsolete_metadata = match fs::symlink_metadata(obsolete) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return write_new(destination, bytes);
        }
        Ok(metadata) if metadata.file_type().is_file() => metadata,
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "obsolete destination must be absent or an existing regular file",
            ));
        }
        Err(error) => return Err(error),
    };
    let obsolete_identity = capture_file_identity(obsolete, &obsolete_metadata)?;

    let staged = stage(destination, bytes, None)?;
    let staged_metadata = fs::metadata(&staged).inspect_err(|_| {
        let _ = fs::remove_file(&staged);
    })?;
    let staged_identity = capture_file_identity(&staged, &staged_metadata).inspect_err(|_| {
        let _ = fs::remove_file(&staged);
    })?;
    let backup = unused_backup_path(obsolete).inspect_err(|_| {
        let _ = fs::remove_file(&staged);
    })?;
    if let Err(error) = fs::rename(obsolete, &backup) {
        let _ = fs::remove_file(staged);
        return Err(error);
    }
    if let Err(error) = fs::hard_link(&staged, destination) {
        let restore = fs::rename(&backup, obsolete);
        let _ = fs::remove_file(staged);
        return match restore {
            Ok(()) => Err(error),
            Err(restore_error) => Err(io::Error::new(
                restore_error.kind(),
                format!(
                    "new publication failed ({error}) and the obsolete file could not be restored ({restore_error}); backup remains at {}",
                    backup.display()
                ),
            )),
        };
    }
    let _ = remove_if_same_file(&staged, &staged_identity);
    let _ = remove_if_same_file(&backup, &obsolete_identity);
    Ok(())
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
    let mut group = NewFileGroup::new();
    for (destination, bytes) in documents.iter().copied() {
        group.stage(destination, bytes)?;
    }
    group.publish()
}

/// A streaming create-new publication group.
///
/// Each payload is staged and synchronized immediately, allowing callers to discard large encoded
/// buffers before preparing the next document. Dropping an unpublished group removes all private
/// staging files. [`Self::publish`] makes the complete group visible with rollback on failure.
#[derive(Default)]
pub struct NewFileGroup {
    entries: Vec<(PathBuf, PathBuf)>,
    resolved: Vec<PathBuf>,
}

impl NewFileGroup {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            resolved: Vec::new(),
        }
    }

    /// Stages and synchronizes one new document without publishing it.
    ///
    /// # Errors
    ///
    /// Rejects missing names, canonical destination aliases, and staging failures.
    pub fn stage(&mut self, destination: &Path, bytes: &[u8]) -> io::Result<()> {
        let name = destination.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "destination has no file name")
        })?;
        let parent = fs::canonicalize(destination.parent().unwrap_or_else(|| Path::new(".")))?;
        let resolved_destination = parent.join(name);
        if self.resolved.contains(&resolved_destination) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "grouped save destinations must differ",
            ));
        }
        match fs::symlink_metadata(destination) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "grouped save destination already exists",
                ));
            }
            Err(error) => return Err(error),
        }
        let staged = stage(destination, bytes, None)?;
        self.resolved.push(resolved_destination);
        self.entries.push((destination.to_owned(), staged));
        Ok(())
    }

    /// Publishes every staged document, rolling back this group's visible files on failure.
    ///
    /// # Errors
    ///
    /// Rejects an empty group and reports metadata, collision, or hard-link failures.
    pub fn publish(mut self) -> io::Result<()> {
        if self.entries.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "grouped save requires at least one document",
            ));
        }
        let mut published: Vec<(PathBuf, FileIdentity)> = Vec::with_capacity(self.entries.len());
        for (destination, staged_path) in &self.entries {
            let metadata = match fs::metadata(staged_path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    for (path, expected) in &published {
                        let _ = remove_if_same_file(path, expected);
                    }
                    return Err(error);
                }
            };
            let identity = match capture_file_identity(staged_path, &metadata) {
                Ok(identity) => identity,
                Err(error) => {
                    for (path, expected) in &published {
                        let _ = remove_if_same_file(path, expected);
                    }
                    return Err(error);
                }
            };
            if let Err(error) = fs::hard_link(staged_path, destination) {
                for (path, expected) in &published {
                    let _ = remove_if_same_file(path, expected);
                }
                return Err(error);
            }
            published.push((destination.clone(), identity));
        }
        self.cleanup();
        Ok(())
    }

    fn cleanup(&mut self) {
        cleanup_paths(
            &self
                .entries
                .iter()
                .map(|(_, staged)| staged.as_path())
                .collect::<Vec<_>>(),
        );
        self.entries.clear();
    }
}

impl Drop for NewFileGroup {
    fn drop(&mut self) {
        self.cleanup();
    }
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

/// Replaces a group of existing regular files as one recoverable publication.
///
/// Every destination must exist as a regular non-symlink file before staging begins. The shared
/// grouped replacement then retains all originals as backups until every staged file publishes.
///
/// # Errors
///
/// Returns an I/O error for an empty group, a missing/non-regular/symlink destination, aliased
/// paths, staging failure, or publication/restoration failure.
pub fn replace_existing_group(documents: &[(&Path, &[u8])]) -> io::Result<()> {
    if documents.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "grouped replacement requires at least one document",
        ));
    }
    for (destination, _) in documents {
        let metadata = fs::symlink_metadata(destination)?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "grouped replacement destination must be an existing regular file: {}",
                    destination.display()
                ),
            ));
        }
    }
    replace_or_create_group(documents)
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
        original_identity: Option<FileIdentity>,
        staged: PathBuf,
        staged_identity: FileIdentity,
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
        let original_identity = match original.as_ref() {
            Some(metadata) => match capture_file_identity(destination, metadata) {
                Ok(identity) => Some(identity),
                Err(error) => {
                    cleanup_paths(
                        &entries
                            .iter()
                            .map(|entry| entry.staged.as_path())
                            .collect::<Vec<_>>(),
                    );
                    return Err(error);
                }
            },
            None => None,
        };
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
        let staged_identity =
            capture_file_identity(&staged, &staged_metadata).inspect_err(|_| {
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
            original_identity,
            staged,
            staged_identity,
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
                let _ = remove_if_same_file(published.destination, &published.staged_identity);
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
        if let (Some(backup), Some(original_identity)) = (&entry.backup, &entry.original_identity) {
            let _ = remove_if_same_file(backup, original_identity);
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
    let first_identity = capture_file_identity(first.0, &first_metadata)?;
    let second_identity = capture_file_identity(second.0, &second_metadata)?;
    let first_staged = stage(first.0, first.1, Some(first_metadata.permissions()))?;
    let first_staged_metadata = fs::metadata(&first_staged).inspect_err(|_| {
        let _ = fs::remove_file(&first_staged);
    })?;
    let first_staged_identity = capture_file_identity(&first_staged, &first_staged_metadata)
        .inspect_err(|_| {
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
        let _ = remove_if_same_file(first.0, &first_staged_identity);
        return rollback_pair(
            error,
            first.0,
            second.0,
            &first_backup,
            &second_backup,
            None,
        );
    }
    let _ = remove_if_same_file(&first_backup, &first_identity);
    let _ = remove_if_same_file(&second_backup, &second_identity);
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
    Ok(capture_file_identity(first, first_metadata)?
        == capture_file_identity(second, second_metadata)?)
}

fn remove_if_same_file(path: &Path, expected: &FileIdentity) -> io::Result<()> {
    let actual = fs::metadata(path)?;
    if &capture_file_identity(path, &actual)? == expected {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

#[cfg(unix)]
fn capture_file_identity(_path: &Path, metadata: &fs::Metadata) -> io::Result<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    Ok(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
type FileIdentity = lm_windows::FileIdentity;

#[cfg(windows)]
fn capture_file_identity(path: &Path, _metadata: &fs::Metadata) -> io::Result<FileIdentity> {
    lm_windows::file_identity(&fs::File::open(path)?)
}

#[cfg(not(any(unix, windows)))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct FileIdentity(PathBuf);

#[cfg(not(any(unix, windows)))]
fn capture_file_identity(path: &Path, _metadata: &fs::Metadata) -> io::Result<FileIdentity> {
    fs::canonicalize(path).map(FileIdentity)
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
    let original_identity = capture_file_identity(destination, original_metadata)?;
    let backup = unused_backup_path(destination)?;
    fs::rename(destination, &backup)?;
    match fs::rename(staged, destination) {
        Ok(()) => {
            let _ = remove_if_same_file(&backup, &original_identity);
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
