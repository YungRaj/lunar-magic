mod common;

use lm_profile::{SmwUsV1Sprite19FixState, detect_smw_us_v1_sprite19_fix};
use lm_rom::{CopierHeader, RomImage, detect_identity};
use std::{fs, process::Command};

#[test]
fn built_cli_installs_authenticates_and_preserves_the_input() {
    let input = common::pristine_smw_us_rom_path();
    let original = fs::read(&input).unwrap();
    let directory = std::env::temp_dir().join(format!(
        "lm-sprite19-fix-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("process")
    ));
    fs::create_dir(&directory).unwrap();
    let output = directory.join("sprite 19 fixed.smc");
    let first = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("smw-sprite19-fix-install")
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    let image = RomImage::from_bytes(fs::read(&output).unwrap()).unwrap();
    assert_eq!(image.copier_header(), CopierHeader::Present);
    assert!(detect_identity(&image).unwrap().checksum_matches());
    assert_eq!(
        detect_smw_us_v1_sprite19_fix(image.logical_bytes()).unwrap(),
        SmwUsV1Sprite19FixState::Installed
    );

    let duplicate = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("smw-sprite19-fix-install")
        .arg(&output)
        .arg(directory.join("duplicate.smc"))
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    fs::remove_dir_all(directory).unwrap();
}
