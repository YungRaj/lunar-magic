use lm_app::{AppState, Command, LevelController, NativeLevelEdit, RomExpansionCommand};
use lm_level::{
    LegacyHeaderEdit, LevelObjectData, MwlFile, MwlSectionKind, NativeSpriteStream,
    SpriteLengthTable,
};
use lm_project::{LevelSaveOptions, Project};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
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

#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and retained installed-ROM fixture"]
fn lunar_magic_exports_every_rust_legacy_level_header_field() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let pristine = pristine_smw_us_rom_path(&root);
    assert!(lunar_magic.is_file(), "missing {}", lunar_magic.display());
    assert!(pristine.is_file(), "missing {}", pristine.display());

    let mut app = AppState::default();
    app.load_rom(fs::read(&pristine).unwrap()).unwrap();
    let allocation_start = app.project().unwrap().rom.logical_len();
    let logical_len = allocation_start + 0x8000;
    app.dispatch(Command::ExpandRom(RomExpansionCommand {
        expected_revision: 0,
        mapper: Mapper::LoRom,
        target_logical_len: logical_len,
        fill: 0xff,
        checksum_field: 0x7fdc,
    }))
    .unwrap();
    app.dispatch(Command::SelectLevel(0x105)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let sprite_lengths = SpriteLengthTable::standard();
    let mut controller = LevelController::decode(&snapshot, layout, &sprite_lengths).unwrap();
    controller
        .apply_edits(&[
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundPalette(5)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(2)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundColor(6)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpriteTileset(9)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpritePalette(4)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ForegroundPalette(3)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ObjectTileset(7)),
            NativeLevelEdit::SetSpriteHeader(0x0a),
        ])
        .unwrap();
    let expected_layer1 = controller.level().layer1.clone();
    let expected_sprites = controller.level().sprites.clone();
    let allocation = AllocationPolicy {
        search: allocation_start..logical_len,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x2e000..0x2ec00),
            ProtectedRange(0x7fc0..0x8000),
        ],
    };
    let prepared = controller
        .prepare_commit(
            "Edit every legacy level-header field for Lunar Magic oracle",
            &LevelSaveOptions {
                layer1_allocation: allocation.clone(),
                sprite_allocation: allocation,
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    let reopened = Project::new(app.project().unwrap().rom.clone())
        .load_level_slot(0x105, layout, &sprite_lengths)
        .unwrap();
    assert_eq!(reopened.layer1, expected_layer1);
    assert_eq!(reopened.sprites, expected_sprites);

    let directory = std::env::temp_dir().join(format!(
        "lm-level-header-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let edited_rom = directory.join("Rust legacy header edit.smc");
    let exported_mwl = directory.join("Level 105 header reexported.mwl");
    fs::write(&edited_rom, app.project().unwrap().save_snapshot()).unwrap();
    let output = ProcessCommand::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ExportLevel")
        .arg(wine_path(&edited_rom))
        .arg(wine_path(&exported_mwl))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Lunar Magic export stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let exported = MwlFile::decode(&fs::read(&exported_mwl).unwrap()).unwrap();
    let exported_layer1 = exported
        .payload_section(MwlSectionKind::Layer1)
        .unwrap()
        .payload;
    let exported_sprites = exported
        .payload_section(MwlSectionKind::Sprites)
        .unwrap()
        .payload;
    assert_eq!(
        LevelObjectData::parse(&exported_layer1).unwrap().header,
        expected_layer1.header
    );
    assert_eq!(
        NativeSpriteStream::parse(&exported_sprites, false, &sprite_lengths).unwrap(),
        expected_sprites
    );
    fs::remove_dir_all(directory).unwrap();
}
