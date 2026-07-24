use lm_graphics::SmwPaletteFile;
use lm_profile::{SMW_US_V1_CUSTOM_PALETTE_POINTER_TABLE_OFFSET, smw_us_v1_shared_palette_layout};
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
fn built_cli_exports_edits_reopens_and_checksums_shared_palette() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = std::env::temp_dir().join(format!("lm-shared-palette-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let input = root.join("Super Mario World (USA).sfc");
    let exported = directory.join("exported.smwpal");
    let changed = directory.join("changed.smwpal");
    let output = directory.join("output.sfc");
    let reopened = directory.join("reopened.smwpal");

    let export = run("smw-shared-palette-export", &[&input, &exported]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert_eq!(
        fs::read(&exported).unwrap(),
        fs::read(root.join("oracle-work/lm363/pristine-us/palette/shared.pal")).unwrap()
    );
    let mut bytes = fs::read(&exported).unwrap();
    bytes[0x123] ^= 0x1f;
    let palette = SmwPaletteFile::decode(&bytes).unwrap();
    fs::write(&changed, palette.encode()).unwrap();
    let import = run("smw-shared-palette-import", &[&input, &changed, &output]);
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let image = RomImage::from_bytes(fs::read(&output).unwrap()).unwrap();
    assert!(detect_identity(&image).unwrap().checksum_matches());
    assert_eq!(
        Project::open_supported(image)
            .unwrap()
            .load_shared_palette(smw_us_v1_shared_palette_layout())
            .unwrap(),
        palette
    );
    let reopen = run("smw-shared-palette-export", &[&output, &reopened]);
    assert!(reopen.status.success());
    assert_eq!(fs::read(reopened).unwrap(), palette.encode());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn built_cli_installs_expanded_backend_into_pristine_rom() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory =
        std::env::temp_dir().join(format!("lm-expanded-shared-palette-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let input = root.join("Super Mario World (USA).sfc");
    let oracle_rom = root.join("oracle-work/lm363/pristine-us/palette-install-positive/after.smc");
    let oracle_project =
        Project::new(RomImage::from_bytes(fs::read(&oracle_rom).unwrap()).unwrap());
    let palette = oracle_project
        .load_shared_palette(smw_us_v1_shared_palette_layout())
        .unwrap();
    let palette_path = directory.join("expanded.smwpal");
    let output = directory.join("output.sfc");
    fs::write(&palette_path, palette.encode()).unwrap();

    let import = run(
        "smw-shared-palette-import",
        &[&input, &palette_path, &output],
    );
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let image = RomImage::from_bytes(fs::read(&output).unwrap()).unwrap();
    assert!(detect_identity(&image).unwrap().checksum_matches());
    let project = Project::open_supported(image).unwrap();
    assert_eq!(
        project
            .load_shared_palette(smw_us_v1_shared_palette_layout())
            .unwrap(),
        palette
    );
    assert!(
        project
            .rom
            .read(SMW_US_V1_CUSTOM_PALETTE_POINTER_TABLE_OFFSET, 0x600)
            .unwrap()
            .iter()
            .all(|byte| *byte == 0)
    );
    for (offset, len) in [
        (0x2d8e2, 4),
        (0x26b8, 4),
        (0x25bf, 4),
        (0x77550, 0x20),
        (0x77570, 0x60),
    ] {
        assert_eq!(
            project.rom.read(offset, len).unwrap(),
            oracle_project.rom.read(offset, len).unwrap()
        );
    }
    fs::remove_dir_all(directory).unwrap();
}
