mod common;

use lm_overworld::{
    OverworldEndpoint, OverworldPathLink, OverworldPathLinkTable, OverworldPathTarget,
};
use lm_profile::smw_us_v1_overworld_path_patch_locator;
use lm_project::{OverworldPathLinkStorage, Project};
use lm_rom::{RomImage, detect_identity};
use std::{fs, process::Command};

fn table(count: u16) -> OverworldPathLinkTable {
    OverworldPathLinkTable {
        links: (0..count)
            .map(|value| OverworldPathLink {
                source: OverworldEndpoint {
                    x: value,
                    y: value + 1,
                    submap: u8::try_from(value % 7).unwrap(),
                },
                destination: OverworldEndpoint {
                    x: value + 2,
                    y: value + 3,
                    submap: u8::try_from((value + 1) % 7).unwrap(),
                },
                target: OverworldPathTarget {
                    y_tile: u8::try_from(value).unwrap(),
                    x_tile: u8::try_from(value + 1).unwrap(),
                },
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
fn built_cli_installs_exports_grows_and_reopens_expanded_special_paths() {
    let directory = std::env::temp_dir().join(format!("lm path process {}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let input = common::pristine_smw_us_rom_path();
    let links20 = directory.join("twenty.lmow");
    let expanded20 = directory.join("expanded twenty.sfc");
    let exported = directory.join("exported.lmow");
    let links30 = directory.join("thirty.lmow");
    let expanded30 = directory.join("expanded thirty.sfc");
    fs::write(&links20, table(20).encode_native_file().unwrap()).unwrap();

    let install = run(
        "smw-overworld-path-import",
        &[&input, &links20, &expanded20],
    );
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let image20 = RomImage::from_bytes(fs::read(&expanded20).unwrap()).unwrap();
    assert!(detect_identity(&image20).unwrap().checksum_matches());
    let loaded20 = Project::open_supported(image20)
        .unwrap()
        .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
        .unwrap();
    assert_eq!(loaded20.table, table(20));
    assert!(matches!(
        loaded20.storage,
        OverworldPathLinkStorage::CurrentPatch { .. }
    ));

    let export = run("smw-overworld-path-export", &[&expanded20, &exported]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert_eq!(
        OverworldPathLinkTable::decode_native_file(&fs::read(&exported).unwrap()).unwrap(),
        table(20)
    );

    fs::write(&links30, table(30).encode_native_file().unwrap()).unwrap();
    let grow = run(
        "smw-overworld-path-import",
        &[&expanded20, &links30, &expanded30],
    );
    assert!(
        grow.status.success(),
        "{}",
        String::from_utf8_lossy(&grow.stderr)
    );
    let image30 = RomImage::from_bytes(fs::read(&expanded30).unwrap()).unwrap();
    assert!(detect_identity(&image30).unwrap().checksum_matches());
    assert_eq!(
        Project::open_supported(image30)
            .unwrap()
            .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())
            .unwrap()
            .table,
        table(30)
    );
    assert_eq!(fs::read(&input).unwrap().len(), 0x80_000);
    fs::remove_dir_all(directory).unwrap();
}
