use super::*;
use std::{
    fs::{self, File},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

fn temporary_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "lm-native-{name}-{}-{}",
        std::process::id(),
        NEXT_PATH.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn bounded_reader_accepts_regular_rom_files() {
    let path = temporary_path("rom.smc");
    let bytes = vec![0x5a; 0x8000];
    fs::write(&path, &bytes).unwrap();
    assert_eq!(read_rom(&path).unwrap(), bytes);
    fs::remove_file(path).unwrap();
}

#[test]
fn bounded_reader_rejects_directories_and_oversized_sparse_files() {
    let directory = temporary_path("directory");
    fs::create_dir(&directory).unwrap();
    assert_eq!(
        read_rom(&directory).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    fs::remove_dir(directory).unwrap();

    let path = temporary_path("oversized.smc");
    let file = File::create(&path).unwrap();
    file.set_len(MAX_ROM_FILE_LEN + 1).unwrap();
    assert_eq!(
        read_rom(&path).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn bounded_reader_accepts_exact_limit_and_rejects_one_extra_byte() {
    let path = temporary_path("exact-bound.bin");
    fs::write(&path, [1, 2, 3, 4]).unwrap();
    assert_eq!(
        read_regular_bounded(&path, 4, "bounded fixture").unwrap(),
        [1, 2, 3, 4]
    );
    assert_eq!(
        read_regular_bounded(&path, 3, "bounded fixture")
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::remove_file(path).unwrap();
}

#[test]
#[cfg(unix)]
fn bounded_reader_rejects_symbolic_links() {
    use std::os::unix::fs::symlink;

    let target = temporary_path("symlink-target.bin");
    let link = temporary_path("symlink.bin");
    fs::write(&target, [1, 2, 3]).unwrap();
    symlink(&target, &link).unwrap();
    assert_eq!(
        read_regular_bounded(&link, 3, "bounded fixture")
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    fs::remove_file(link).unwrap();
    fs::remove_file(target).unwrap();
}
