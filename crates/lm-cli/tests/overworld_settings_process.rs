mod common;

use lm_level::ExpandedOverworldSettings;
use lm_oracle::Observation;
use lm_profile::{
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START, SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
    smw_us_v1_expanded_settings_layout,
};
use lm_project::Project;
use lm_rom::{Mapper, RomImage, detect_identity};
use std::{fs, process::Command};

fn run(operation: &str, arguments: &[&std::path::Path]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg(operation)
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn built_cli_exports_installs_and_reopens_native_overworld_settings() {
    let directory =
        std::env::temp_dir().join(format!("lm settings process {}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let defaults_file = directory.join("defaults.lmowset");
    let collision_rom = directory.join("unrelated first-fit block.sfc");
    let collision_defaults = directory.join("collision defaults.lmowset");
    let collision_expanded_rom = directory.join("collision expanded.sfc");
    let changed_file = directory.join("changed.lmowset");
    let expanded_rom = directory.join("expanded.sfc");
    let reopened_file = directory.join("reopened.lmowset");
    let observation_file = directory.join("layer3.lmobs");

    let export = run("smw-overworld-settings-export", &[&input, &defaults_file]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let mut changed =
        ExpandedOverworldSettings::decode_file(&fs::read(&defaults_file).unwrap()).unwrap();

    let mut collision =
        Project::open_supported(RomImage::from_bytes(fs::read(&input).unwrap()).unwrap()).unwrap();
    collision
        .expand_rom(Mapper::LoRom, 0x10_0000, 0xff, 0x7fdc)
        .unwrap();
    let mut unrelated = vec![0x30; 0x8008];
    unrelated[..4].copy_from_slice(b"STAR");
    unrelated[4..6].copy_from_slice(&0x7fff_u16.to_le_bytes());
    unrelated[6..8].copy_from_slice(&0x8000_u16.to_le_bytes());
    collision
        .rom
        .write(
            SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START,
            &unrelated,
        )
        .unwrap();
    collision.refresh_checksum(0x7fdc).unwrap();
    fs::write(&collision_rom, collision.save_snapshot()).unwrap();
    let collision_export = run(
        "smw-overworld-settings-export",
        &[&collision_rom, &collision_defaults],
    );
    assert!(
        collision_export.status.success(),
        "{}",
        String::from_utf8_lossy(&collision_export.stderr)
    );
    assert_eq!(
        fs::read(&collision_defaults).unwrap(),
        fs::read(&defaults_file).unwrap()
    );

    changed.records[6].set_word(9, 0x4567).unwrap();
    fs::write(&changed_file, changed.encode_file()).unwrap();

    let collision_install = run(
        "smw-overworld-settings-import",
        &[&collision_rom, &changed_file, &collision_expanded_rom],
    );
    assert!(
        collision_install.status.success(),
        "{}",
        String::from_utf8_lossy(&collision_install.stderr)
    );
    let collision_before = RomImage::from_bytes(fs::read(&collision_rom).unwrap()).unwrap();
    let collision_after = RomImage::from_bytes(fs::read(&collision_expanded_rom).unwrap()).unwrap();
    assert_eq!(
        &collision_after.logical_bytes()
            [SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START..0x09_0000],
        &collision_before.logical_bytes()
            [SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START..0x09_0000]
    );
    let collision_project = Project::open_supported(collision_after.clone()).unwrap();
    let collision_layout =
        lm_profile::smw_us_v1_installed_expanded_settings_layout(&collision_project)
            .unwrap()
            .unwrap();
    assert_eq!(collision_layout.table_offset, 0x09_2d08);
    assert_eq!(
        lm_profile::load_smw_us_v1_overworld_settings(&collision_project)
            .unwrap()
            .settings,
        changed
    );
    assert!(
        detect_identity(&collision_after)
            .unwrap()
            .checksum_matches()
    );

    let install = run(
        "smw-overworld-settings-import",
        &[&input, &changed_file, &expanded_rom],
    );
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let image = RomImage::from_bytes(fs::read(&expanded_rom).unwrap()).unwrap();
    assert!(detect_identity(&image).unwrap().checksum_matches());
    assert_eq!(
        Project::open_supported(image)
            .unwrap()
            .load_expanded_overworld_settings(
                SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
                smw_us_v1_expanded_settings_layout(),
            )
            .unwrap(),
        changed
    );

    let reopen = run(
        "smw-overworld-settings-export",
        &[&expanded_rom, &reopened_file],
    );
    assert!(
        reopen.status.success(),
        "{}",
        String::from_utf8_lossy(&reopen.stderr)
    );
    assert_eq!(fs::read(&reopened_file).unwrap(), changed.encode_file());
    let observe = run(
        "smw-overworld-layer3-settings-observe",
        &[&expanded_rom, &observation_file],
    );
    assert!(
        observe.status.success(),
        "{}",
        String::from_utf8_lossy(&observe.stderr)
    );
    let observation =
        Observation::from_text(&fs::read_to_string(&observation_file).unwrap()).unwrap();
    assert_eq!(
        observation.get("overworld/layer3/settings/6/address-layout/7"),
        Some("4567")
    );
    assert_eq!(
        RomImage::from_bytes(fs::read(&input).unwrap())
            .unwrap()
            .logical_bytes()
            .len(),
        0x80_000
    );
    fs::remove_dir_all(directory).unwrap();
}
