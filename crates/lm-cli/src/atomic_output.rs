use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Publishes a new output without exposing partially written bytes or replacing any existing path.
///
/// Data is written and synchronized through a unique file in the destination directory. A hard
/// link then publishes that inode under the requested name atomically and with create-new
/// semantics. Existing regular files, symlinks, and hard-link aliases are therefore preserved.
///
/// # Errors
///
/// Returns an I/O error if the destination has no parent/name, already exists, cannot support
/// same-directory hard-link publication, or any staging operation fails.
pub fn write_new(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    write_new_batch(&[(path, bytes.as_ref())])
}

/// Publishes several new outputs as one all-or-nothing create-new group.
///
/// Every payload is fully staged before any destination appears. If publication of a later output
/// fails, earlier links created by this call are removed and all staging files are cleaned up.
///
/// # Errors
///
/// Returns an I/O error for duplicate/invalid destinations, an existing destination, unsupported
/// hard-link publication, or any staging failure.
pub fn write_new_batch(outputs: &[(&Path, &[u8])]) -> io::Result<()> {
    let mut resolved = Vec::with_capacity(outputs.len());
    for (index, (path, _)) in outputs.iter().enumerate() {
        let identity = destination_identity(path)?;
        if resolved[..index].contains(&identity) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "output paths must not alias the same destination",
            ));
        }
        resolved.push(identity);
    }

    let mut staged = Vec::with_capacity(outputs.len());
    for (path, bytes) in outputs {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let (temporary, mut file) = match create_temporary(parent) {
            Ok(value) => value,
            Err(error) => {
                cleanup_staging(&staged);
                return Err(error);
            }
        };
        if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(&temporary);
            cleanup_staging(&staged);
            return Err(error);
        }
        drop(file);
        staged.push((temporary, (*path).to_path_buf()));
    }

    for (published, (temporary, destination)) in staged.iter().enumerate() {
        if let Err(error) = fs::hard_link(temporary, destination) {
            for (published_temporary, published_destination) in staged[..published].iter().rev() {
                let _ = remove_if_same_file(published_destination, published_temporary);
            }
            cleanup_staging(&staged);
            return Err(error);
        }
    }
    cleanup_staging(&staged);
    Ok(())
}

fn destination_identity(path: &Path) -> io::Result<PathBuf> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "output path has no file name")
    })?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    Ok(fs::canonicalize(parent)?.join(name))
}

fn remove_if_same_file(destination: &Path, staged: &Path) -> io::Result<()> {
    if same_file(destination, staged)? {
        fs::remove_file(destination)
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn same_file(first: &Path, second: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let first = fs::metadata(first)?;
    let second = fs::metadata(second)?;
    Ok(first.dev() == second.dev() && first.ino() == second.ino())
}

#[cfg(windows)]
fn same_file(first: &Path, second: &Path) -> io::Result<bool> {
    let first = fs::File::open(first)?;
    let second = fs::File::open(second)?;
    Ok(lm_windows::file_identity(&first)? == lm_windows::file_identity(&second)?)
}

#[cfg(not(any(unix, windows)))]
fn same_file(_first: &Path, _second: &Path) -> io::Result<bool> {
    Ok(false)
}

fn cleanup_staging(staged: &[(PathBuf, PathBuf)]) {
    for (temporary, _) in staged {
        let _ = fs::remove_file(temporary);
    }
}

fn create_temporary(parent: &Path) -> io::Result<(PathBuf, fs::File)> {
    for _ in 0..128 {
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".lm-output-{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary output name",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-cli-atomic-output-{}-{}",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn publishes_complete_new_file_and_removes_staging_name() {
        let directory = directory();
        let output = directory.join("result.bin");
        write_new(&output, [1, 2, 3]).unwrap();
        assert_eq!(fs::read(&output).unwrap(), [1, 2, 3]);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn existing_destination_is_never_replaced() {
        let directory = directory();
        let output = directory.join("result.bin");
        fs::write(&output, [7, 8]).unwrap();
        assert_eq!(
            write_new(&output, [1, 2, 3]).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(&output).unwrap(), [7, 8]);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn symlink_alias_is_preserved_on_unix() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let directory = directory();
            let source = directory.join("source.bin");
            let output = directory.join("output.bin");
            fs::write(&source, [9]).unwrap();
            symlink(&source, &output).unwrap();
            assert_eq!(
                write_new(&output, [1]).unwrap_err().kind(),
                io::ErrorKind::AlreadyExists
            );
            assert_eq!(fs::read(&source).unwrap(), [9]);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn batch_collision_rolls_back_every_new_destination() {
        let directory = directory();
        let first = directory.join("first.bin");
        let second = directory.join("second.bin");
        fs::write(&second, [7]).unwrap();
        assert_eq!(
            write_new_batch(&[(&first, &[1]), (&second, &[2])])
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(!first.exists());
        assert_eq!(fs::read(&second).unwrap(), [7]);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn batch_publishes_all_payloads_and_rejects_duplicate_names() {
        let directory = directory();
        let first = directory.join("first.bin");
        let second = directory.join("second.bin");
        write_new_batch(&[(&first, &[1]), (&second, &[2, 3])]).unwrap();
        assert_eq!(fs::read(&first).unwrap(), [1]);
        assert_eq!(fs::read(&second).unwrap(), [2, 3]);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
        assert_eq!(
            write_new_batch(&[(&directory.join("x"), &[]), (&directory.join("x"), &[])])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn batch_rejects_canonical_parent_aliases_before_staging() {
        let directory = directory();
        let child = directory.join("child");
        fs::create_dir(&child).unwrap();
        let first = child.join("result.bin");
        let second = child.join("..").join("child").join("result.bin");
        assert_eq!(
            write_new_batch(&[(&first, &[1]), (&second, &[2])])
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(fs::read_dir(&child).unwrap().count(), 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rollback_identity_check_never_removes_a_replacement_inode() {
        let directory = directory();
        let staged = directory.join("staged");
        let destination = directory.join("destination");
        fs::write(&staged, [1]).unwrap();
        fs::write(&destination, [2]).unwrap();
        remove_if_same_file(&destination, &staged).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), [2]);

        fs::remove_file(&destination).unwrap();
        fs::hard_link(&staged, &destination).unwrap();
        remove_if_same_file(&destination, &staged).unwrap();
        assert!(!destination.exists());
        assert_eq!(fs::read(&staged).unwrap(), [1]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn batch_rejects_symlinked_parent_aliases_on_unix() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let directory = directory();
            let child = directory.join("child");
            let alias = directory.join("alias");
            fs::create_dir(&child).unwrap();
            symlink(&child, &alias).unwrap();
            assert_eq!(
                write_new_batch(&[
                    (&child.join("result.bin"), &[1]),
                    (&alias.join("result.bin"), &[2]),
                ])
                .unwrap_err()
                .kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(fs::read_dir(&child).unwrap().count(), 0);
            fs::remove_dir_all(directory).unwrap();
        }
    }
}
