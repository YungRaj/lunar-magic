mod common;

use lm_oracle::observe_credits_tilemap;
use lm_overworld::CreditsTilemap;
use lm_profile::{SMW_US_V1_CREDITS_BLANK_WORD, smw_us_v1_credits_tilemap_locator};
use lm_project::{CreditsTilemapStorage, Project};
use lm_rom::{RomImage, compute_snes_checksum};
use std::{
    fs,
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

fn tilemap(value: u16) -> CreditsTilemap {
    let mut tilemap = CreditsTilemap::blank(SMW_US_V1_CREDITS_BLANK_WORD);
    tilemap.words_mut()[255 * CreditsTilemap::COLUMNS + 31] = value;
    tilemap.words_mut()[254 * CreditsTilemap::COLUMNS + 30] = value.wrapping_add(1);
    tilemap
}

#[test]
fn built_cli_installs_updates_exports_and_reopens_expanded_credits() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-credits-process-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let original = fs::read(&input).unwrap();
    let first_file = directory.join("credits first.lmcred");
    let second_file = directory.join("credits second.lmcred");
    let first_rom = directory.join("credits first.sfc");
    let second_rom = directory.join("credits second.sfc");
    let exported = directory.join("credits exported.lmcred");
    fs::write(&first_file, tilemap(0x1234).encode_native_file()).unwrap();
    fs::write(&second_file, tilemap(0x5678).encode_native_file()).unwrap();
    run(&[
        "smw-credits-tilemap-import",
        input.to_str().unwrap(),
        first_file.to_str().unwrap(),
        first_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-credits-tilemap-import",
        first_rom.to_str().unwrap(),
        second_file.to_str().unwrap(),
        second_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-credits-tilemap-export",
        second_rom.to_str().unwrap(),
        exported.to_str().unwrap(),
    ]);
    let expected = tilemap(0x5678);
    assert_eq!(
        CreditsTilemap::decode_native_file(&fs::read(exported).unwrap()).unwrap(),
        expected
    );
    let bytes = fs::read(&second_rom).unwrap();
    let image = RomImage::from_bytes(bytes).unwrap();
    let logical = image.logical_bytes();
    assert_eq!(
        &logical[0x7fdc..0x7fe0],
        compute_snes_checksum(logical, 0x7fdc).unwrap().encoded()
    );
    let project = Project::open_supported(image).unwrap();
    let loaded = project
        .load_credits_tilemap_detected(&smw_us_v1_credits_tilemap_locator())
        .unwrap();
    assert_eq!(loaded.tilemap, expected);
    assert!(matches!(loaded.storage, CreditsTilemapStorage::Expanded(_)));
    assert_eq!(
        observe_credits_tilemap(&loaded.tilemap).unwrap(),
        observe_credits_tilemap(&expected).unwrap()
    );
    assert_eq!(fs::read(input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
