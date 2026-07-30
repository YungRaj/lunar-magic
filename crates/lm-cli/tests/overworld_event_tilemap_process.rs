mod common;

use lm_oracle::observe_event_tilemap_buffers;
use lm_overworld::EventTilemapBuffers;
use lm_profile::smw_us_v1_event_tilemap_locator;
use lm_project::{EventTilemapCompression, Project};
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

fn buffers(value: u8) -> EventTilemapBuffers {
    let mut buffers = EventTilemapBuffers::default();
    buffers.primary_bytes_mut()[7] = value;
    buffers.primary_bytes_mut()[0x807] = value.wrapping_add(1);
    buffers.secondary_high_bytes_mut()[9] = value.wrapping_add(2);
    buffers
}

#[test]
fn built_cli_installs_updates_exports_and_reopens_event_tilemaps() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-event-tilemap-process-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let original = fs::read(&input).unwrap();
    let first_file = directory.join("first.lmowtil");
    let second_file = directory.join("second.lmowtil");
    let first_rom = directory.join("first.sfc");
    let second_rom = directory.join("second.sfc");
    let exported = directory.join("exported.lmowtil");
    fs::write(&first_file, buffers(0x12).encode_native_file()).unwrap();
    fs::write(&second_file, buffers(0x34).encode_native_file()).unwrap();
    run(&[
        "smw-overworld-event-tilemap-import",
        input.to_str().unwrap(),
        first_file.to_str().unwrap(),
        first_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-overworld-event-tilemap-import",
        first_rom.to_str().unwrap(),
        second_file.to_str().unwrap(),
        second_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-overworld-event-tilemap-export",
        second_rom.to_str().unwrap(),
        exported.to_str().unwrap(),
    ]);
    assert_eq!(
        EventTilemapBuffers::decode_native_file(&fs::read(exported).unwrap()).unwrap(),
        buffers(0x34)
    );
    let bytes = fs::read(&second_rom).unwrap();
    let image = RomImage::from_bytes(bytes).unwrap();
    let logical = image.logical_bytes();
    assert_eq!(
        &logical[0x7fdc..0x7fe0],
        compute_snes_checksum(logical, 0x7fdc).unwrap().encoded()
    );
    let project = Project::open_supported(image).unwrap();
    assert_eq!(
        project
            .load_event_tilemap_buffers_detected(
                smw_us_v1_event_tilemap_locator(),
                EventTilemapCompression::Lz2,
            )
            .unwrap()
            .buffers,
        buffers(0x34)
    );
    assert_eq!(
        observe_event_tilemap_buffers(
            &project
                .load_event_tilemap_buffers_detected(
                    smw_us_v1_event_tilemap_locator(),
                    EventTilemapCompression::Lz2,
                )
                .unwrap()
                .buffers,
        )
        .unwrap(),
        observe_event_tilemap_buffers(&buffers(0x34)).unwrap(),
    );
    assert_eq!(fs::read(input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
