use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn built_binary_round_trips_odd_depth_planar_graphics() {
    let directory = std::env::temp_dir().join(format!(
        "lm generic planar 日本語 {} {}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let indexed = directory.join("indexed tiles.bin");
    let planar = directory.join("encoded tiles.3bpp");
    let decoded = directory.join("decoded tiles.bin");
    let pixels = (0_u8..128).map(|pixel| pixel & 7).collect::<Vec<_>>();
    fs::write(&indexed, &pixels).unwrap();

    assert!(
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args(["planar", "encode", "3"])
            .arg(&indexed)
            .arg(&planar)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::metadata(&planar).unwrap().len(), 48);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args(["planar", "decode", "3"])
            .arg(&planar)
            .arg(&decoded)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(decoded).unwrap(), pixels);
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args(["planar", "decode", "3"])
            .arg(&planar)
            .arg(&planar)
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::remove_dir_all(directory).unwrap();
}
