use lm_app::{ControllerSnapshot, EditorMode, NativeLevelAssetsController};
use lm_graphics::PaletteOwnership;
use lm_level::{MwlFile, SpriteLengthTable};
use lm_project::{
    ExAnimationRomLayout, LevelPointerTable, MwlNativeLevel, NativeLevelAssetsLayout,
};
use lm_rom::{Mapper, RomImage, detect_identity};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn wine_path(path: &Path) -> String {
    let rendered = path.display().to_string().replace('/', r"\");
    format!(r"Z:\{}", rendered.trim_start_matches('\\'))
}

fn run_lunar_magic(executable: &Path, operation: &str, rom: &Path, mwl: &Path, level: &str) {
    let output = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(executable)
        .arg(operation)
        .arg(wine_path(rom))
        .arg(wine_path(mwl))
        .arg(level)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{operation} stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Proves the complete installed-ROM exporter reciprocally through Lunar Magic rather than only
/// comparing it with a retained file: Rust exports, Lunar Magic imports, and Lunar Magic exports
/// the resulting level again with every semantic domain intact.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and retained installed-ROM fixture"]
fn rust_installed_rom_export_round_trips_semantically_through_lunar_magic() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = root.join("lm363/Lunar Magic.exe");
    let installed =
        root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc");
    assert!(executable.is_file(), "missing {}", executable.display());
    assert!(installed.is_file(), "missing {}", installed.display());

    let source_bytes = fs::read(&installed).unwrap();
    let image = RomImage::from_bytes(source_bytes.clone()).unwrap();
    let snapshot = ControllerSnapshot {
        revision: 0,
        mode: EditorMode::Level(0),
        identity: detect_identity(&image).unwrap(),
        document_path: Some(installed.clone()),
        rom_bytes: source_bytes,
    };
    let mut level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
    level_layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
    let layout = NativeLevelAssetsLayout {
        level: level_layout,
        palette: lm_profile::smw_us_v1_custom_palette_layout(),
        exanimation: ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x8138b,
                entries: 0x200,
                stride: 3,
            },
            maximum_records: 32,
            maximum_encoded_len: 0x8000,
        },
        expanded_settings: Some(lm_profile::smw_us_v1_expanded_settings_layout()),
    };
    let lengths = SpriteLengthTable::standard();
    let modes = [false; 256];
    let controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout,
        Some(lm_profile::smw_us_v1_layer2_layout(&image).unwrap()),
        &lengths,
        &modes,
        PaletteOwnership::editable(257),
    )
    .unwrap();
    let expected = controller.export_smw_us_v1_installed_mwl().unwrap();

    let directory = std::env::temp_dir().join(format!(
        "lm-native-mwl-export-wine-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let rom = directory.join("installed export target.smc");
    let rust_mwl = directory.join("rust exported level 000.mwl");
    let reexported_mwl = directory.join("lunar magic reexported level 000.mwl");
    fs::copy(&installed, &rom).unwrap();
    let restore = directory.join("sysLMRestore");
    fs::create_dir(&restore).unwrap();
    fs::copy(
        root.join("sysLMRestore/smwOrig.smc"),
        restore.join("smwOrig.smc"),
    )
    .unwrap();
    fs::copy(
        root.join("sysLMRestore/Super Mario World (USA).lrp"),
        restore.join("Super Mario World (USA).lrp"),
    )
    .unwrap();
    fs::write(
        &rust_mwl,
        expected.encode(&lengths, &modes).unwrap().encode().unwrap(),
    )
    .unwrap();

    run_lunar_magic(&executable, "-ImportLevel", &rom, &rust_mwl, "000");
    run_lunar_magic(&executable, "-ExportLevel", &rom, &reexported_mwl, "000");
    let actual = MwlNativeLevel::decode(
        &MwlFile::decode(&fs::read(&reexported_mwl).unwrap()).unwrap(),
        &lengths,
        32,
        &modes,
    )
    .unwrap();

    assert_eq!(actual.header, expected.header);
    assert_eq!(actual.layer1, expected.layer1);
    assert_eq!(actual.layer2_descriptor, expected.layer2_descriptor);
    assert_eq!(actual.layer2, expected.layer2);
    assert_eq!(actual.sprites, expected.sprites);
    assert_eq!(actual.palette, expected.palette);
    assert_eq!(actual.secondary_exits, expected.secondary_exits);
    assert_eq!(actual.exanimation, expected.exanimation);
    assert_eq!(actual.expanded_settings, expected.expanded_settings);
    assert!(
        detect_identity(&RomImage::from_bytes(fs::read(&rom).unwrap()).unwrap())
            .unwrap()
            .checksum_matches()
    );

    fs::remove_dir_all(directory).unwrap();
}
