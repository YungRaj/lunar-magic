use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lm-app-persistence-{}-{nonce}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn new_save_publishes_complete_bytes_and_never_overwrites() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("new.smc");
    write_new(&destination, b"first").unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"first");
    assert_eq!(
        write_new(&destination, b"second").unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&destination).unwrap(), b"first");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn published_save_is_success_even_if_private_link_cleanup_fails() {
    assert!(finalize_new_publication(Ok(()), Err(io::Error::other("cleanup"))).is_ok());
    assert_eq!(
        finalize_new_publication(
            Err(io::Error::new(io::ErrorKind::AlreadyExists, "publish")),
            Err(io::Error::other("cleanup")),
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::AlreadyExists
    );
}

#[test]
fn grouped_new_save_publishes_every_document_without_debris() {
    let directory = TestDirectory::new();
    let first = directory.0.join("Level 000.mwl");
    let second = directory.0.join("Level 001.mwl");
    write_new_group(&[(&first, b"zero"), (&second, b"one")]).unwrap();
    assert_eq!(fs::read(first).unwrap(), b"zero");
    assert_eq!(fs::read(second).unwrap(), b"one");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn streaming_new_group_drop_cancels_every_staged_document() {
    let directory = TestDirectory::new();
    let first = directory.0.join("Level 000.png");
    let second = directory.0.join("Level 001.png");
    {
        let mut group = NewFileGroup::new();
        group.stage(&first, b"zero").unwrap();
        group.stage(&second, b"one").unwrap();
        assert!(!first.exists());
        assert!(!second.exists());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    }
    assert!(!first.exists());
    assert!(!second.exists());
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}

#[test]
fn grouped_new_save_rolls_back_its_publications_on_collision() {
    let directory = TestDirectory::new();
    let first = directory.0.join("Level 000.mwl");
    let occupied = directory.0.join("Level 001.mwl");
    fs::write(&occupied, b"existing").unwrap();
    assert_eq!(
        write_new_group(&[(&first, b"zero"), (&occupied, b"one")])
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(!first.exists());
    assert_eq!(fs::read(occupied).unwrap(), b"existing");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn grouped_new_save_rejects_canonical_aliases_before_publication() {
    let directory = TestDirectory::new();
    let child = directory.0.join("child");
    fs::create_dir(&child).unwrap();
    let destination = directory.0.join("Level 000.mwl");
    let alias = child.join("..").join("Level 000.mwl");
    assert_eq!(
        write_new_group(&[(&destination, b"zero"), (&alias, b"alias")])
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(!destination.exists());
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn existing_regular_document_is_replaced_without_staging_debris() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("existing.smc");
    fs::write(&destination, b"before").unwrap();
    replace_existing(&destination, b"after").unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"after");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn mixed_group_replaces_existing_and_creates_absent_documents() {
    let directory = TestDirectory::new();
    let rom = directory.0.join("game.smc");
    let sidecar = directory.0.join("game.msc");
    let second_sidecar = directory.0.join("game.dsc");
    fs::write(&rom, b"old-rom").unwrap();
    fs::write(&sidecar, b"old-sidecar").unwrap();

    replace_or_create_group(&[
        (&rom, b"new-rom"),
        (&sidecar, b"new-sidecar"),
        (&second_sidecar, b"new-second"),
    ])
    .unwrap();
    assert_eq!(fs::read(rom).unwrap(), b"new-rom");
    assert_eq!(fs::read(sidecar).unwrap(), b"new-sidecar");
    assert_eq!(fs::read(second_sidecar).unwrap(), b"new-second");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 3);
}

#[test]
fn mixed_group_rejects_bad_destination_before_mutation() {
    let directory = TestDirectory::new();
    let rom = directory.0.join("game.smc");
    let bad = directory.0.join("game.msc");
    fs::write(&rom, b"old-rom").unwrap();
    fs::create_dir(&bad).unwrap();

    assert_eq!(
        replace_or_create_group(&[(&rom, b"new-rom"), (&bad, b"sidecar")])
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fs::read(rom).unwrap(), b"old-rom");
    assert!(bad.is_dir());
}

#[cfg(any(unix, windows))]
#[test]
fn identity_guard_removes_only_the_file_captured_for_cleanup() {
    let directory = TestDirectory::new();
    let captured = directory.0.join("captured");
    let candidate = directory.0.join("candidate");
    fs::write(&captured, b"ours").unwrap();
    let captured_metadata = fs::metadata(&captured).unwrap();

    fs::write(&candidate, b"replacement").unwrap();
    remove_if_same_file(&candidate, &captured_metadata).unwrap();
    assert_eq!(fs::read(&candidate).unwrap(), b"replacement");

    fs::remove_file(&candidate).unwrap();
    fs::hard_link(&captured, &candidate).unwrap();
    remove_if_same_file(&candidate, &captured_metadata).unwrap();
    assert!(!candidate.exists());
    assert_eq!(fs::read(&captured).unwrap(), b"ours");
}

#[cfg(unix)]
#[test]
fn replacement_preserves_permissions_and_rejects_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let directory = TestDirectory::new();
    let destination = directory.0.join("mode.smc");
    fs::write(&destination, b"before").unwrap();
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o640)).unwrap();
    replace_existing(&destination, b"after").unwrap();
    assert_eq!(
        fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
        0o640
    );

    let link = directory.0.join("link.smc");
    symlink(&destination, &link).unwrap();
    assert_eq!(
        replace_existing(&link, b"bad").unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fs::read(&destination).unwrap(), b"after");
}

#[cfg(unix)]
#[test]
fn broken_symlink_is_not_treated_as_an_unused_backup_path() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new();
    let destination = directory.0.join("document.smc");
    fs::write(&destination, b"original").unwrap();
    let occupied = directory.0.join(format!(
        ".document.smc.lm-app-{}-0.backup",
        std::process::id()
    ));
    symlink(directory.0.join("missing-target"), &occupied).unwrap();

    let selected = unused_backup_path(&destination).unwrap();
    assert_ne!(selected, occupied);
    assert!(
        fs::symlink_metadata(&occupied)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn invalid_destination_does_not_leave_a_staging_file() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("missing").join("file.smc");
    assert!(write_new(&destination, b"data").is_err());
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}

#[test]
fn paired_replacement_preserves_both_documents_and_permissions() {
    let directory = TestDirectory::new();
    let data = directory.0.join("objects.mw0");
    let descriptions = directory.0.join("objects.mw0t");
    fs::write(&data, b"old-data").unwrap();
    fs::write(&descriptions, b"old-text").unwrap();
    replace_existing_pair((&data, b"new-data"), (&descriptions, b"new-text")).unwrap();
    assert_eq!(fs::read(&data).unwrap(), b"new-data");
    assert_eq!(fs::read(&descriptions).unwrap(), b"new-text");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn paired_replacement_rejects_aliases_and_bad_second_target_before_mutation() {
    let directory = TestDirectory::new();
    let data = directory.0.join("objects.mw0");
    let descriptions = directory.0.join("objects.mw0t");
    fs::write(&data, b"old-data").unwrap();
    fs::create_dir(&descriptions).unwrap();
    assert_eq!(
        replace_existing_pair((&data, b"new-data"), (&data, b"new-text"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        replace_existing_pair((&data, b"new-data"), (&descriptions, b"new-text"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fs::read(&data).unwrap(), b"old-data");
}

#[test]
fn paired_replacement_rejects_canonical_path_alias_before_staging() {
    let directory = TestDirectory::new();
    let data = directory.0.join("objects.mw0");
    let child = directory.0.join("child");
    fs::write(&data, b"old-data").unwrap();
    fs::create_dir(&child).unwrap();
    let alias = child.join("..").join("objects.mw0");
    assert_ne!(data, alias);
    assert_eq!(
        replace_existing_pair((&data, b"new-data"), (&alias, b"new-text"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fs::read(&data).unwrap(), b"old-data");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[cfg(any(unix, windows))]
#[test]
fn paired_replacement_rejects_distinct_hard_links_to_same_file() {
    let directory = TestDirectory::new();
    let data = directory.0.join("objects.mw0");
    let alias = directory.0.join("objects.mw0t");
    fs::write(&data, b"old-data").unwrap();
    fs::hard_link(&data, &alias).unwrap();
    assert_eq!(
        replace_existing_pair((&data, b"new-data"), (&alias, b"new-text"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fs::read(&data).unwrap(), b"old-data");
    assert_eq!(fs::read(&alias).unwrap(), b"old-data");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}
