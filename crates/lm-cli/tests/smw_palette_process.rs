use lm_graphics::SmwPaletteFile;
use lm_oracle::Observation;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn built_cli_normalizes_and_observes_expanded_native_palette() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-smw-palette-process-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("shared palette 日本語.smwpal");
    let normalized = directory.join("normalized palette.smwpal");
    let observation = directory.join("palette observation.txt");
    let mut bytes = vec![0; SmwPaletteFile::EXPANDED_FILE_LEN];
    bytes[0..2].copy_from_slice(&0x7fff_u16.to_le_bytes());
    bytes[0x800..].fill(0x5a);
    fs::write(&input, &bytes).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("smw-palette-file")
        .arg(&input)
        .arg(&normalized)
        .arg(&observation)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read(&normalized).unwrap(), bytes);
    let observed = Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
    assert_eq!(observed.get("smw-palette/backend"), Some("expanded"));
    assert_eq!(
        observed.get("smw-palette/colors/0000/bgr555"),
        Some("32767")
    );

    assert!(
        !Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .arg("smw-palette-file")
            .arg(&input)
            .arg(&normalized)
            .status()
            .unwrap()
            .success()
    );
    fs::remove_dir_all(directory).unwrap();
}
