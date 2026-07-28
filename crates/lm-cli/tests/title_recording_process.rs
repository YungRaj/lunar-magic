mod common;

use lm_oracle::observe_title_recording;
use lm_profile::smw_us_v1_title_recording_locator;
use lm_project::{Project, TitleRecordingStorage};
use lm_rom::{RomImage, compute_snes_checksum};
use lm_title::{TitleScreenRecording, decode_zsnes_title_recording, encode_zsnes_title_recording};
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

fn recording(value: u8, length: usize) -> TitleScreenRecording {
    let mut bytes = vec![value; length];
    *bytes.last_mut().unwrap() = 0xff;
    TitleScreenRecording::from_bytes(bytes).unwrap()
}

#[test]
fn built_cli_imports_zst_updates_exports_and_reopens_recording() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-title-recording-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let original = fs::read(&input).unwrap();
    let first = recording(0x12, 7);
    let second = recording(0x56, 0x101);
    let first_state = directory.join("first.zst");
    let second_file = directory.join("second.lmtitle");
    let first_rom = directory.join("first.sfc");
    let snes9x_state = directory.join("first.000");
    let snes9x_rom = directory.join("first-s9x.sfc");
    let second_rom = directory.join("second.sfc");
    let exported_file = directory.join("exported.lmtitle");
    let exported_state = directory.join("exported.zst");
    let encoded_first = encode_zsnes_title_recording(&first);
    fs::write(&first_state, &encoded_first).unwrap();
    let mut snes9x = b"#!s9xsnp:0007\nRAM:131072:".to_vec();
    snes9x.extend_from_slice(&encoded_first[0x0c13..]);
    fs::write(&snes9x_state, snes9x).unwrap();
    fs::write(&second_file, second.encode_native_file()).unwrap();
    run(&[
        "smw-title-recording-zst-import",
        input.to_str().unwrap(),
        first_state.to_str().unwrap(),
        first_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-title-recording-s9x-import",
        input.to_str().unwrap(),
        snes9x_state.to_str().unwrap(),
        snes9x_rom.to_str().unwrap(),
    ]);
    let snes9x_project =
        Project::open_supported(RomImage::from_bytes(fs::read(&snes9x_rom).unwrap()).unwrap())
            .unwrap();
    assert_eq!(
        snes9x_project
            .load_title_recording_detected(&smw_us_v1_title_recording_locator())
            .unwrap()
            .recording,
        Some(first.clone())
    );
    run(&[
        "smw-title-recording-import",
        first_rom.to_str().unwrap(),
        second_file.to_str().unwrap(),
        second_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-title-recording-export",
        second_rom.to_str().unwrap(),
        exported_file.to_str().unwrap(),
    ]);
    run(&[
        "smw-title-recording-zst-export",
        second_rom.to_str().unwrap(),
        exported_state.to_str().unwrap(),
    ]);
    let native =
        TitleScreenRecording::decode_native_file(&fs::read(exported_file).unwrap()).unwrap();
    let zsnes = decode_zsnes_title_recording(&fs::read(exported_state).unwrap()).unwrap();
    assert_eq!(native, second);
    assert_eq!(zsnes, second);
    assert_eq!(
        observe_title_recording(&native).unwrap(),
        observe_title_recording(&zsnes).unwrap()
    );
    let bytes = fs::read(&second_rom).unwrap();
    assert_eq!(
        &bytes[0x7fdc..0x7fe0],
        compute_snes_checksum(&bytes, 0x7fdc).unwrap().encoded()
    );
    let project = Project::open_supported(RomImage::from_bytes(bytes).unwrap()).unwrap();
    let loaded = project
        .load_title_recording_detected(&smw_us_v1_title_recording_locator())
        .unwrap();
    assert_eq!(loaded.recording, Some(second));
    assert!(matches!(
        loaded.storage,
        TitleRecordingStorage::Installed { .. }
    ));
    assert_eq!(fs::read(input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
