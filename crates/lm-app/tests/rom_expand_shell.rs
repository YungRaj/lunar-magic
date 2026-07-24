use lm_rom::{RomImage, compute_snes_checksum, detect_identity};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn fixture() -> Vec<u8> {
    let mut logical = vec![0x42; 0x8000];
    logical[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    logical[0x7fd5] = 0x20;
    logical[0x7fd9] = 1;
    logical[0x7fdb] = 0;
    let checksum = compute_snes_checksum(&logical, 0x7fdc).unwrap();
    logical[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    let mut bytes = vec![0x7e; 0x200];
    bytes.extend(logical);
    bytes
}

#[test]
fn built_application_expands_undoes_redoes_and_saves_as() {
    let directory = std::env::temp_dir().join(format!(
        "lm application expansion 日本語 {} {}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("source game.smc");
    let output = directory.join("expanded game.smc");
    let script = directory.join("expansion commands.txt");
    let original = fixture();
    fs::write(&input, &original).unwrap();
    fs::write(
        &script,
        format!(
            "rom-expand 10000 ff\nundo\nredo\nsave-as {}\nquit\n",
            output.display()
        ),
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--rom")
        .arg(&input)
        .arg("--script")
        .arg(&script)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    let expanded = fs::read(output).unwrap();
    assert_eq!(&expanded[..0x200], &original[..0x200]);
    let image = RomImage::from_bytes(expanded).unwrap();
    assert_eq!(image.logical_len(), 0x1_0000);
    assert!(detect_identity(&image).unwrap().checksum_matches());
    fs::remove_dir_all(directory).unwrap();
}
