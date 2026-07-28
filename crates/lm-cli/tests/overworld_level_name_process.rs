mod common;

use lm_overworld::{NativeOverworldLevelNameTable, OverworldLevelName, OverworldMetadata};
use lm_profile::{smw_us_v1_overworld_level_name_locator, smw_us_v1_overworld_level_name_runtime};
use lm_project::{OverworldLevelNameStorage, Project};
use lm_rom::{RomImage, detect_identity};
use std::{fs, process::Command};

fn table(count: usize) -> NativeOverworldLevelNameTable {
    NativeOverworldLevelNameTable {
        names: (0..count)
            .map(|slot| OverworldLevelName {
                level: NativeOverworldLevelNameTable::level_for_slot(slot).unwrap(),
                tiles: [u8::try_from(slot).unwrap(); OverworldLevelName::TILE_COUNT],
                raw_flags: 0,
            })
            .collect(),
    }
}

fn run(operation: &str, arguments: &[&std::path::Path]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg(operation)
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn built_cli_exports_installs_and_grows_native_level_name_table() {
    let directory = std::env::temp_dir().join(format!("lm names process {}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let vanilla_file = directory.join("vanilla.lmowmeta");
    let names100 = directory.join("names100.lmowmeta");
    let expanded100 = directory.join("expanded100.sfc");
    let names140 = directory.join("names140.lmowmeta");
    let expanded140 = directory.join("expanded140.sfc");

    let export = run("smw-overworld-name-export", &[&input, &vanilla_file]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert_eq!(
        OverworldMetadata::decode_file(&fs::read(&vanilla_file).unwrap())
            .unwrap()
            .level_names
            .len(),
        93
    );
    for (path, value) in [(&names100, table(100)), (&names140, table(140))] {
        fs::write(
            path,
            OverworldMetadata {
                level_names: value.names,
                ..OverworldMetadata::default()
            }
            .encode_file()
            .unwrap(),
        )
        .unwrap();
    }
    let install = run(
        "smw-overworld-name-import",
        &[&input, &names100, &expanded100],
    );
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let grow = run(
        "smw-overworld-name-import",
        &[&expanded100, &names140, &expanded140],
    );
    assert!(
        grow.status.success(),
        "{}",
        String::from_utf8_lossy(&grow.stderr)
    );
    let image = RomImage::from_bytes(fs::read(&expanded140).unwrap()).unwrap();
    assert!(detect_identity(&image).unwrap().checksum_matches());
    let loaded = Project::open_supported(image)
        .unwrap()
        .load_overworld_level_names_detected(
            smw_us_v1_overworld_level_name_locator(),
            smw_us_v1_overworld_level_name_runtime(),
        )
        .unwrap();
    assert_eq!(loaded.table, table(140));
    assert!(matches!(
        loaded.storage,
        OverworldLevelNameStorage::Expanded { .. }
    ));
    assert_eq!(fs::read(&input).unwrap().len(), 0x80_000);
    fs::remove_dir_all(directory).unwrap();
}
