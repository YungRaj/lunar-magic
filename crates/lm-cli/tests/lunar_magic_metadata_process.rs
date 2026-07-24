use lm_oracle::observe_lunar_magic_rom_metadata;
use lm_profile::smw_us_v1_lunar_magic_metadata_layout;
use lm_project::Project;
use lm_rom::{LunarMagicRomMetadata, RomImage, compute_snes_checksum};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn run(arguments: &[&str]) {
    assert!(
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args(arguments)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn built_cli_transfers_exact_metadata_between_real_lm363_saves() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let first_rom = root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc");
    let second_rom = root.join("oracle-work/lm363/pristine-us/level-save-105/after.smc");
    let first_before = fs::read(&first_rom).unwrap();
    let second_before = fs::read(&second_rom).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-rom-metadata-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let first_file = directory.join("first.lmrommd");
    let second_file = directory.join("second.lmrommd");
    let output_rom = directory.join("updated.smc");
    let roundtrip_file = directory.join("roundtrip.lmrommd");
    run(&[
        "smw-lm-metadata-export",
        first_rom.to_str().unwrap(),
        first_file.to_str().unwrap(),
    ]);
    run(&[
        "smw-lm-metadata-export",
        second_rom.to_str().unwrap(),
        second_file.to_str().unwrap(),
    ]);
    let first = LunarMagicRomMetadata::decode_file(&fs::read(&first_file).unwrap()).unwrap();
    let second = LunarMagicRomMetadata::decode_file(&fs::read(&second_file).unwrap()).unwrap();
    assert_ne!(
        observe_lunar_magic_rom_metadata(&first).unwrap(),
        observe_lunar_magic_rom_metadata(&second).unwrap()
    );
    run(&[
        "smw-lm-metadata-import",
        first_rom.to_str().unwrap(),
        second_file.to_str().unwrap(),
        output_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-lm-metadata-export",
        output_rom.to_str().unwrap(),
        roundtrip_file.to_str().unwrap(),
    ]);
    assert_eq!(
        fs::read(&roundtrip_file).unwrap(),
        fs::read(&second_file).unwrap()
    );
    let bytes = fs::read(&output_rom).unwrap();
    assert_eq!(
        &bytes[0x81dc..0x81e0],
        compute_snes_checksum(&bytes[0x200..], 0x7fdc)
            .unwrap()
            .encoded()
    );
    let project = Project::open_supported(RomImage::from_bytes(bytes).unwrap()).unwrap();
    assert_eq!(
        project
            .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())
            .unwrap(),
        Some(second)
    );
    assert_eq!(fs::read(&first_rom).unwrap(), first_before);
    assert_eq!(fs::read(&second_rom).unwrap(), second_before);
    fs::remove_dir_all(directory).unwrap();
}
