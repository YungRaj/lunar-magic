mod common;

use lm_oracle::observe_expanded_layer_tilemap;
use lm_overworld::ExpandedLayerTilemap;
use lm_profile::smw_us_v1_title_tilemap_locator;
use lm_project::{Project, TitleTilemapStorage};
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

fn tilemap(value: u8, secondary: bool) -> ExpandedLayerTilemap {
    let mut tilemap = ExpandedLayerTilemap::default();
    tilemap.primary_bytes_mut()[0] = value;
    tilemap.primary_bytes_mut()[ExpandedLayerTilemap::PLANE_LEN - 1] = value.wrapping_add(1);
    if secondary {
        tilemap.secondary_bytes_mut()[0] = value.wrapping_add(2);
    }
    tilemap
}

#[test]
fn built_cli_installs_updates_exports_and_reopens_title_tilemap() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-title-process-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let original = fs::read(&input).unwrap();
    let first_file = directory.join("title first.lmowlyr");
    let second_file = directory.join("title second.lmowlyr");
    let first_rom = directory.join("title first.sfc");
    let second_rom = directory.join("title second.sfc");
    let exported = directory.join("title exported.lmowlyr");
    fs::write(&first_file, tilemap(0x12, false).encode_native_file()).unwrap();
    fs::write(&second_file, tilemap(0x56, true).encode_native_file()).unwrap();
    run(&[
        "smw-title-tilemap-import",
        input.to_str().unwrap(),
        first_file.to_str().unwrap(),
        first_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-title-tilemap-import",
        first_rom.to_str().unwrap(),
        second_file.to_str().unwrap(),
        second_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-title-tilemap-export",
        second_rom.to_str().unwrap(),
        exported.to_str().unwrap(),
    ]);
    let expected = tilemap(0x56, true);
    assert_eq!(
        ExpandedLayerTilemap::decode_native_file(&fs::read(exported).unwrap()).unwrap(),
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
        .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())
        .unwrap();
    assert_eq!(loaded.tilemap, expected);
    assert!(matches!(loaded.storage, TitleTilemapStorage::Expanded(_)));
    assert_eq!(
        observe_expanded_layer_tilemap(&loaded.tilemap).unwrap(),
        observe_expanded_layer_tilemap(&expected).unwrap()
    );
    assert_eq!(fs::read(input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
