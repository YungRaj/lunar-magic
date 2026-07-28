mod common;

use lm_overworld::{EventReveal, SpecialEventRevealTable};
use lm_profile::smw_us_v1_special_event_reveal_locator;
use lm_project::{Project, SpecialEventRevealStorage};
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

fn table(bias: u16) -> SpecialEventRevealTable {
    let mut table = SpecialEventRevealTable::default();
    for index in 0_u16..24 {
        table.reveals[usize::from(index)] = EventReveal {
            source_tile: index + bias,
            destination_tile: index + bias + 0x200,
        };
        table.directions[usize::from(index)] = index.to_le_bytes()[0] ^ bias.to_le_bytes()[0];
    }
    table
}

#[test]
fn built_cli_exports_installs_updates_and_reopens_special_events() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-special-event-process-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let original = fs::read(&input).unwrap();
    let first_file = directory.join("special first.lmowspc");
    let second_file = directory.join("special second.lmowspc");
    let first_rom = directory.join("special first.sfc");
    let second_rom = directory.join("special second.sfc");
    let exported = directory.join("special exported.lmowspc");
    fs::write(&first_file, table(0x100).encode_native_file().unwrap()).unwrap();
    fs::write(&second_file, table(0x180).encode_native_file().unwrap()).unwrap();
    run(&[
        "smw-overworld-special-event-import",
        input.to_str().unwrap(),
        first_file.to_str().unwrap(),
        first_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-overworld-special-event-import",
        first_rom.to_str().unwrap(),
        second_file.to_str().unwrap(),
        second_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-overworld-special-event-export",
        second_rom.to_str().unwrap(),
        exported.to_str().unwrap(),
    ]);
    assert_eq!(
        SpecialEventRevealTable::decode_native_file(&fs::read(exported).unwrap()).unwrap(),
        table(0x180)
    );
    let bytes = fs::read(&second_rom).unwrap();
    assert_eq!(
        &bytes[0x7fdc..0x7fe0],
        compute_snes_checksum(&bytes, 0x7fdc).unwrap().encoded()
    );
    let project = Project::open_supported(RomImage::from_bytes(bytes).unwrap()).unwrap();
    let loaded = project
        .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())
        .unwrap();
    assert_eq!(loaded.table, table(0x180));
    assert!(matches!(
        loaded.storage,
        SpecialEventRevealStorage::Expanded { .. }
    ));
    assert_eq!(fs::read(input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
