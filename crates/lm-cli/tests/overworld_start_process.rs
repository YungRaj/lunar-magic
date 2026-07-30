mod common;

use lm_overworld::{NativeOverworldPlayerStarts, Submap};
use lm_profile::{
    SMW_US_V1_OVERWORLD_CUSTOM_START_ENABLED, SMW_US_V1_OVERWORLD_CUSTOM_START_PATCH_OFFSET,
    smw_us_v1_overworld_player_start_layout,
};
use lm_project::Project;
use lm_rom::{RomImage, detect_identity};
use std::{fs, process::Command};

fn run(operation: &str, arguments: &[&std::path::Path]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg(operation)
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn built_cli_exports_changes_and_reopens_native_player_starts() {
    let directory = std::env::temp_dir().join(format!("lm starts process {}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let vanilla_file = directory.join("vanilla.lmowst");
    let changed_file = directory.join("changed.lmowst");
    let changed_rom = directory.join("changed.sfc");
    let reopened_file = directory.join("reopened.lmowst");

    let export = run("smw-overworld-start-export", &[&input, &vanilla_file]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let mut changed =
        NativeOverworldPlayerStarts::decode_file(&fs::read(&vanilla_file).unwrap()).unwrap();
    changed.starts[0].submap = Submap::ForestOfIllusion;
    changed.starts[0].x = 0x88;
    changed.starts[0].y = 0xa8;
    fs::write(&changed_file, changed.encode_file().unwrap()).unwrap();

    let import = run(
        "smw-overworld-start-import",
        &[&input, &changed_file, &changed_rom],
    );
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let image = RomImage::from_bytes(fs::read(&changed_rom).unwrap()).unwrap();
    assert!(detect_identity(&image).unwrap().checksum_matches());
    let project = Project::open_supported(image).unwrap();
    assert_eq!(
        project
            .load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())
            .unwrap(),
        changed
    );
    assert_eq!(
        project
            .rom
            .read(SMW_US_V1_OVERWORLD_CUSTOM_START_PATCH_OFFSET, 3)
            .unwrap(),
        SMW_US_V1_OVERWORLD_CUSTOM_START_ENABLED
    );
    let reopen = run(
        "smw-overworld-start-export",
        &[&changed_rom, &reopened_file],
    );
    assert!(reopen.status.success());
    assert_eq!(
        fs::read(&reopened_file).unwrap(),
        changed.encode_file().unwrap()
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
