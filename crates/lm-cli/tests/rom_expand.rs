use lm_rom::{RomImage, compute_snes_checksum, detect_identity};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn fixture() -> Vec<u8> {
    let mut logical = vec![0x22; 0x8000];
    logical[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    logical[0x7fd5] = 0x20;
    logical[0x7fd9] = 1;
    logical[0x7fdb] = 0;
    let checksum = compute_snes_checksum(&logical, 0x7fdc).unwrap();
    logical[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    let mut bytes = vec![0xa5; 0x200];
    bytes.extend(logical);
    bytes
}

#[test]
fn built_binary_expands_headered_rom_copy_on_write() {
    let directory = std::env::temp_dir().join(format!(
        "lm ROM expansion 日本語 {} {}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("source game.smc");
    let output = directory.join("expanded game.smc");
    let original = fixture();
    fs::write(&input, &original).unwrap();

    assert!(
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .arg("rom-expand")
            .arg(&input)
            .arg(&output)
            .args(["lorom", "10000", "ff"])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    let expanded = fs::read(&output).unwrap();
    assert_eq!(&expanded[..0x200], &original[..0x200]);
    let image = RomImage::from_bytes(expanded).unwrap();
    assert_eq!(image.logical_len(), 0x1_0000);
    assert!(detect_identity(&image).unwrap().checksum_matches());

    assert!(
        !Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .arg("rom-expand")
            .arg(&input)
            .arg(&output)
            .args(["lorom", "10000", "ff"])
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::remove_dir_all(directory).unwrap();
}
