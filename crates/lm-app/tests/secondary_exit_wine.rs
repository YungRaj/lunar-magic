use lm_level::{MwlFile, MwlSecondaryExit, SecondaryExit, SpriteLengthTable};
use lm_project::{MwlNativeLevel, Project};
use lm_rom::{RomImage, detect_identity};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const PRISTINE_SMW_US_SHA256: &str =
    "0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b";

fn pristine_smw_us_rom_path(root: &Path) -> PathBuf {
    for path in [
        root.join("Super Mario World (USA).sfc"),
        root.join("SMW-working.sfc"),
        root.join("sysLMRestore/smwOrig.smc"),
    ] {
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(image) = RomImage::from_bytes(bytes) else {
            continue;
        };
        if lm_oracle::sha256_hex(image.logical_bytes()) == PRISTINE_SMW_US_SHA256 {
            return path;
        }
    }
    panic!("verified pristine SMW-US fixture not found");
}

fn wine_path(path: &Path) -> String {
    let rendered = path.display().to_string().replace('/', r"\");
    format!(r"Z:\{}", rendered.trim_start_matches('\\'))
}

fn run_lunar_magic(executable: &Path, operation: &str, rom: &Path, mwl: &Path) {
    let output = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(executable)
        .arg(operation)
        .arg(wine_path(rom))
        .arg(wine_path(mwl))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{operation} stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn decode_level(path: &Path) -> MwlNativeLevel {
    let lengths = SpriteLengthTable::standard();
    MwlNativeLevel::decode(
        &MwlFile::decode(&fs::read(path).unwrap()).unwrap(),
        &lengths,
        32,
        &[false; 256],
    )
    .unwrap()
}

fn write_level(path: &Path, level: &MwlNativeLevel) {
    fs::write(
        path,
        level
            .encode(&SpriteLengthTable::standard(), &[false; 256])
            .unwrap()
            .encode()
            .unwrap(),
    )
    .unwrap();
}

/// Proves both secondary-exit index boundaries, every packed field boundary, and the semantic
/// clear operation through Lunar Magic's actual binary-MWL importer and exporter. The ROM table is
/// independently reopened after each phase so MWL equality alone cannot hide a persistence bug.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and a pristine SMW-US ROM fixture"]
fn lunar_magic_imports_reexports_and_clears_secondary_exit_boundaries() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = root.join("lm363/Lunar Magic.exe");
    let original_rom = pristine_smw_us_rom_path(&root);
    let directory = std::env::temp_dir().join(format!(
        "lm-secondary-exit-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let rom = directory.join("secondary exits.sfc");
    let source_mwl = directory.join("source.mwl");
    let edited_mwl = directory.join("edited boundaries.mwl");
    let boundary_reexport = directory.join("reexported boundaries.mwl");
    let cleared_mwl = directory.join("cleared.mwl");
    let clear_reexport = directory.join("reexported clear.mwl");
    fs::copy(&original_rom, &rom).unwrap();

    run_lunar_magic(&executable, "-ExportLevel", &rom, &source_mwl);
    let mut edited = decode_level(&source_mwl);
    let minimum = MwlSecondaryExit {
        index: 0,
        exit: SecondaryExit {
            destination_level: 0x105,
            position_and_method: 0x00,
            screen: 0x00,
            x: 0x00,
            y: 0x00,
            destination_flags: 0x00,
            x_and_overworld_flags: 0x00,
            additional_flags: 0x00,
        },
        reserved: 0,
    };
    let maximum = MwlSecondaryExit {
        index: 0x1fff,
        exit: SecondaryExit {
            destination_level: 0x105,
            position_and_method: 0xff,
            screen: 0x1f,
            x: 0x0f,
            y: 0x07,
            destination_flags: 0xf7,
            x_and_overworld_flags: 0xf0,
            additional_flags: 0xff,
        },
        reserved: 0,
    };
    let expected = vec![minimum, maximum];
    edited.secondary_exits = vec![
        minimum,
        MwlSecondaryExit {
            index: 0x1fff,
            exit: SecondaryExit {
                destination_level: 0x105,
                position_and_method: 0x55,
                ..SecondaryExit::default()
            },
            reserved: 0xaa,
        },
        MwlSecondaryExit {
            index: 0x2000,
            exit: SecondaryExit {
                destination_level: 0x105,
                position_and_method: 0x77,
                ..SecondaryExit::default()
            },
            reserved: 0xbb,
        },
        MwlSecondaryExit {
            reserved: 0xcc,
            ..maximum
        },
    ];
    write_level(&edited_mwl, &edited);

    run_lunar_magic(&executable, "-ImportLevel", &rom, &edited_mwl);
    run_lunar_magic(&executable, "-ExportLevel", &rom, &boundary_reexport);
    assert_eq!(decode_level(&boundary_reexport).secondary_exits, expected);
    let boundary_project = Project::new(RomImage::from_bytes(fs::read(&rom).unwrap()).unwrap());
    let boundary_table = boundary_project
        .load_secondary_exit_table_detected(lm_profile::smw_us_v1_secondary_exit_locator())
        .unwrap()
        .table;
    assert_eq!(boundary_table.entries[0], expected[0].exit);
    assert_eq!(boundary_table.entries[0x1fff], expected[1].exit);

    let mut cleared = decode_level(&boundary_reexport);
    cleared.secondary_exits.clear();
    write_level(&cleared_mwl, &cleared);
    run_lunar_magic(&executable, "-ImportLevel", &rom, &cleared_mwl);
    run_lunar_magic(&executable, "-ExportLevel", &rom, &clear_reexport);
    assert!(decode_level(&clear_reexport).secondary_exits.is_empty());
    let cleared_image = RomImage::from_bytes(fs::read(&rom).unwrap()).unwrap();
    assert!(detect_identity(&cleared_image).unwrap().checksum_matches());
    let cleared_table = Project::new(cleared_image)
        .load_secondary_exit_table_detected(lm_profile::smw_us_v1_secondary_exit_locator())
        .unwrap()
        .table;
    assert_eq!(cleared_table.entries[0], SecondaryExit::default());
    assert_eq!(cleared_table.entries[0x1fff], SecondaryExit::default());

    fs::remove_dir_all(directory).unwrap();
}
