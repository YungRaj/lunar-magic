mod common;

use lm_level::SecondaryExitTable;
use lm_oracle::observe_secondary_exit_table;
use lm_profile::smw_us_v1_secondary_exit_locator;
use lm_project::{Project, SecondaryExitStorage};
use lm_rom::{RomImage, compute_snes_checksum};
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
fn built_cli_exports_updates_and_reopens_real_lm363_secondary_exits() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let input = root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc");
    let original = fs::read(&input).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-secondary-exit-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let exported = directory.join("original.lmsexit");
    let edited_file = directory.join("edited.lmsexit");
    let output_rom = directory.join("edited.smc");
    let reopened_file = directory.join("reopened.lmsexit");
    run(&[
        "smw-secondary-exit-export",
        input.to_str().unwrap(),
        exported.to_str().unwrap(),
    ]);
    let baseline = SecondaryExitTable::decode_native_file(&fs::read(&exported).unwrap()).unwrap();
    let mut edited = baseline.clone();
    edited.entries[0x123].destination_level = 0x105;
    edited.entries[0x123].position_and_method = 0x21;
    edited.entries[0x123].screen = 4;
    fs::write(&edited_file, edited.encode_native_file().unwrap()).unwrap();
    run(&[
        "smw-secondary-exit-import",
        input.to_str().unwrap(),
        edited_file.to_str().unwrap(),
        output_rom.to_str().unwrap(),
    ]);
    run(&[
        "smw-secondary-exit-export",
        output_rom.to_str().unwrap(),
        reopened_file.to_str().unwrap(),
    ]);
    assert_eq!(
        fs::read(&reopened_file).unwrap(),
        fs::read(&edited_file).unwrap()
    );
    assert_ne!(
        observe_secondary_exit_table(&baseline).unwrap(),
        observe_secondary_exit_table(&edited).unwrap()
    );
    let bytes = fs::read(&output_rom).unwrap();
    assert_eq!(
        &bytes[0x81dc..0x81e0],
        compute_snes_checksum(&bytes[0x200..], 0x7fdc)
            .unwrap()
            .encoded()
    );
    let project = Project::open_supported(RomImage::from_bytes(bytes).unwrap()).unwrap();
    let loaded = project
        .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
        .unwrap();
    assert_eq!(loaded.table, edited);
    assert!(matches!(
        loaded.storage,
        SecondaryExitStorage::Installed { .. }
    ));
    assert_eq!(fs::read(&input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn built_cli_installs_expanded_secondary_exits_into_pristine_smw() {
    let input = common::pristine_smw_us_rom_path();
    let original = fs::read(&input).unwrap();
    let source = Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
    let mut table = source
        .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
        .unwrap()
        .table;
    table.entries[0x400].destination_level = 0x105;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-secondary-install-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let table_file = directory.join("expanded.lmsexit");
    let output_rom = directory.join("installed.smc");
    fs::write(&table_file, table.encode_native_file().unwrap()).unwrap();
    run(&[
        "smw-secondary-exit-import",
        input.to_str().unwrap(),
        table_file.to_str().unwrap(),
        output_rom.to_str().unwrap(),
    ]);
    let installed = fs::read(&output_rom).unwrap();
    let project = Project::open_supported(RomImage::from_bytes(installed).unwrap()).unwrap();
    let loaded = project
        .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
        .unwrap();
    assert_eq!(loaded.table, table);
    assert!(matches!(
        loaded.storage,
        SecondaryExitStorage::Installed {
            fixed_prefix_planes: 0,
            tagged_planes,
            ..
        } if tagged_planes.len() == 6
    ));
    assert_eq!(fs::read(&input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
