mod common;

use lm_overworld::{EventReveal, EventRevealTable};
use lm_profile::smw_us_v1_overworld_event_reveal_locator;
use lm_project::Project;
use lm_rom::{RomImage, compute_snes_checksum};
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn table(count: usize, bias: u16) -> EventRevealTable {
    EventRevealTable {
        entries: (0..count)
            .map(|index| EventReveal {
                source_tile: u16::try_from(index).unwrap(),
                destination_tile: u16::try_from(index).unwrap() | bias,
            })
            .collect(),
    }
}

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
fn built_cli_exports_pristine_and_imports_then_grows_native_events() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-event-process-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let original = fs::read(&input).unwrap();
    let first_file = directory.join("events 200.lmowevt");
    let first_rom = directory.join("events 200.sfc");
    let grown_file = directory.join("events 255.lmowevt");
    let grown_rom = directory.join("events 255.sfc");
    let exported = directory.join("exported.lmowevt");
    let first = table(200, 0x200);
    let grown = table(255, 0x300);
    fs::write(&first_file, first.encode_native_event_file().unwrap()).unwrap();
    fs::write(&grown_file, grown.encode_native_event_file().unwrap()).unwrap();
    run(&[
        "smw-overworld-event-import",
        input.to_str().unwrap(),
        first_file.to_str().unwrap(),
        first_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-overworld-event-import",
        first_rom.to_str().unwrap(),
        grown_file.to_str().unwrap(),
        grown_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-overworld-event-export",
        grown_rom.to_str().unwrap(),
        exported.to_str().unwrap(),
    ]);
    let bytes = fs::read(&grown_rom).unwrap();
    let image = RomImage::from_bytes(bytes).unwrap();
    let logical = image.logical_bytes();
    assert_eq!(
        &logical[0x7fdc..0x7fe0],
        compute_snes_checksum(logical, 0x7fdc).unwrap().encoded()
    );
    let project = Project::open_supported(image).unwrap();
    assert_eq!(
        project
            .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())
            .unwrap()
            .table,
        grown
    );
    assert_eq!(
        EventRevealTable::decode_native_event_file(&fs::read(exported).unwrap()).unwrap(),
        grown
    );
    assert_eq!(fs::read(input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
