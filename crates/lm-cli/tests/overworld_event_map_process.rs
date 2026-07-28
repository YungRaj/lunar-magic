mod common;

use lm_overworld::EventNumberMap;
use lm_profile::smw_us_v1_overworld_event_number_map_locator;
use lm_project::{OverworldEventNumberMapStorage, Project};
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

#[test]
fn built_cli_exports_legacy_map_installs_extended_and_reopens() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-event-map-process-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let original = fs::read(&input).unwrap();
    let pristine_file = directory.join("pristine map.lmowmap");
    let changed_file = directory.join("changed map.lmowmap");
    let output = directory.join("event map output.sfc");
    let exported = directory.join("event map exported.lmowmap");
    run(&[
        "smw-overworld-event-map-export",
        input.to_str().unwrap(),
        pristine_file.to_str().unwrap(),
    ]);
    let pristine = EventNumberMap::decode_native_file(&fs::read(&pristine_file).unwrap()).unwrap();
    assert_eq!(pristine.stored_len(), EventNumberMap::VANILLA_LEN);
    assert_eq!(pristine.get(0x28), 3);
    let mut changed = pristine;
    changed.set(0xff, 0x7e);
    fs::write(&changed_file, changed.encode_native_file().unwrap()).unwrap();
    run(&[
        "smw-overworld-event-map-import",
        input.to_str().unwrap(),
        changed_file.to_str().unwrap(),
        output.to_str().unwrap(),
    ]);
    run(&[
        "smw-overworld-event-map-export",
        output.to_str().unwrap(),
        exported.to_str().unwrap(),
    ]);
    assert_eq!(
        EventNumberMap::decode_native_file(&fs::read(&exported).unwrap()).unwrap(),
        changed
    );
    let bytes = fs::read(&output).unwrap();
    assert_eq!(
        &bytes[0x7fdc..0x7fe0],
        compute_snes_checksum(&bytes, 0x7fdc).unwrap().encoded()
    );
    let project = Project::open_supported(RomImage::from_bytes(bytes).unwrap()).unwrap();
    let loaded = project
        .load_overworld_event_number_map_detected(smw_us_v1_overworld_event_number_map_locator())
        .unwrap();
    assert_eq!(loaded.map, changed);
    assert_eq!(
        loaded.storage,
        OverworldEventNumberMapStorage::InstalledExtended
    );
    assert_eq!(fs::read(input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
