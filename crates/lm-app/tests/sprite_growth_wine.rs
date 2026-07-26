use lm_app::{AppState, Command, LevelController, MwlDocumentController, NativeLevelEdit};
use lm_level::{
    LevelObjectData, MwlFile, MwlSectionKind, NativeSpriteStream, ObjectCoordinateNibbles,
    ObjectEdit, SpriteLengthTable, SpriteToken,
};
use lm_project::{LevelSaveOptions, Project, SpritePointerTable};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SnesPointer24, detect_identity};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn wine_path(path: &Path) -> String {
    let rendered = path.display().to_string().replace('/', r"\");
    format!(r"Z:\{}", rendered.trim_start_matches('\\'))
}

fn run_lunar_magic_level_command(
    lunar_magic: &Path,
    operation: &str,
    rom: &Path,
    artifact: &Path,
    level: &str,
) {
    let output = ProcessCommand::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(lunar_magic)
        .arg(operation)
        .arg(wine_path(rom))
        .arg(wine_path(artifact))
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

fn shared_sprite_bank(image: &RomImage, bank_offset: usize) -> std::ops::Range<usize> {
    let bank = image.logical_bytes()[bank_offset];
    let first = SnesPointer24::new((u32::from(bank) << 16) | 0x8000)
        .unwrap()
        .to_pc(Mapper::LoRom)
        .unwrap();
    first..first + 0x8000
}

fn insert_and_move_first_sprite(controller: &mut LevelController, lengths: &SpriteLengthTable) {
    let inserted = controller.level().sprites.tokens[0].clone();
    controller
        .apply_edits(&[NativeLevelEdit::InsertSprite {
            index: 1,
            token: inserted,
        }])
        .unwrap();
    let mut moved = controller.level().sprites.tokens[0].clone();
    let SpriteToken::Record(record) = &mut moved else {
        panic!("level 105 must begin with an ordinary sprite record");
    };
    let mut fields = record.native_fields().unwrap();
    fields.screen = (fields.screen + 1) & 0x1f;
    fields.x = (fields.x + 2) & 0x0f;
    fields.y_low = (fields.y_low + 3) & 0x1f;
    record.set_native_fields(fields, lengths).unwrap();
    controller
        .apply_edits(&[
            NativeLevelEdit::ReplaceSprite {
                index: 0,
                token: moved,
            },
            NativeLevelEdit::SortLegacySpritesByScreen { selected: 0 },
        ])
        .unwrap();
}

/// Proves that Lunar Magic 3.63 accepts and semantically preserves a sprite stream grown and
/// relocated by the Rust editor.
///
/// This remains ignored for ordinary cross-platform test runs because it requires Wine, the
/// legally supplied Lunar Magic 3.63 executable, and the supplied clean SMW ROM.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn rust_sprite_growth_reopens_with_the_inserted_sprite_in_lunar_magic() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = root.join("Super Mario World (USA).sfc");
    assert!(lunar_magic.is_file(), "missing {}", lunar_magic.display());
    assert!(original_rom.is_file(), "missing {}", original_rom.display());

    let directory = std::env::temp_dir().join(format!(
        "lm-sprite-growth-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let edited = directory.join("rust sprite growth.sfc");
    let exported = directory.join("Lunar Magic level 105.mwl");

    let mut app = AppState::default();
    app.load_rom(fs::read(&original_rom).unwrap()).unwrap();
    app.dispatch(Command::SelectLevel(0x105)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let lengths = SpriteLengthTable::standard();
    let mut controller = LevelController::decode(&snapshot, layout, &lengths).unwrap();
    insert_and_move_first_sprite(&mut controller, &lengths);
    let expected = controller.level().sprites.clone();

    let image = RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap();
    let SpritePointerTable::SplitSharedBank { bank_offset, .. } = layout.sprites else {
        panic!("pristine SMW sprite layout must use a shared bank");
    };
    let protected = vec![ProtectedRange(
        snapshot.identity.internal_header_offset..snapshot.identity.internal_header_offset + 0x40,
    )];
    let allocation = AllocationPolicy {
        search: image.logical_len().min(0x80_000)..image.logical_len(),
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: protected.clone(),
    };
    let prepared = controller
        .prepare_commit_with_shared_bank_sprite_relocation(
            "Insert sprite for Lunar Magic compatibility oracle",
            &LevelSaveOptions {
                layer1_allocation: allocation,
                sprite_allocation: AllocationPolicy {
                    search: shared_sprite_bank(&image, bank_offset),
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected,
                },
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    fs::write(&edited, app.project().unwrap().save_snapshot()).unwrap();

    let reopen = ProcessCommand::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ExportLevel")
        .arg(wine_path(&edited))
        .arg(wine_path(&exported))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        reopen.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&reopen.stdout),
        String::from_utf8_lossy(&reopen.stderr)
    );
    assert!(
        String::from_utf8_lossy(&reopen.stdout).contains("Level 105 exported"),
        "{}",
        String::from_utf8_lossy(&reopen.stdout)
    );

    let mwl_bytes = fs::read(&exported).unwrap();
    let mwl = MwlFile::decode(&mwl_bytes).unwrap();
    let exported_sprites = mwl.payload_section(MwlSectionKind::Sprites).unwrap();
    assert_eq!(
        NativeSpriteStream::parse(&exported_sprites.payload, false, &lengths).unwrap(),
        expected
    );
    assert_eq!(mwl.version, 0x0363);
    assert_eq!(mwl.encode().unwrap(), mwl_bytes);
    let reopened_rom = RomImage::from_bytes(fs::read(&edited).unwrap()).unwrap();
    assert!(detect_identity(&reopened_rom).unwrap().checksum_matches());
    fs::remove_dir_all(directory).unwrap();
}

/// Proves the reciprocal boundary: Lunar Magic imports a typed Rust MWL sprite edit and emits the
/// same semantics when the resulting ROM is exported again.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn lunar_magic_imports_and_reexports_a_rust_mwl_sprite_edit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = root.join("Super Mario World (USA).sfc");
    assert!(lunar_magic.is_file(), "missing {}", lunar_magic.display());
    assert!(original_rom.is_file(), "missing {}", original_rom.display());

    let directory = std::env::temp_dir().join(format!(
        "lm-mwl-sprite-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let imported_rom = directory.join("Lunar Magic imported.sfc");
    let source_mwl = directory.join("source level 105.mwl");
    let edited_mwl = directory.join("Rust edited level 105.mwl");
    let reexported_mwl = directory.join("reexported level 105.mwl");
    fs::copy(&original_rom, &imported_rom).unwrap();

    let export = ProcessCommand::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ExportLevel")
        .arg(wine_path(&imported_rom))
        .arg(wine_path(&source_mwl))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&export.stdout),
        String::from_utf8_lossy(&export.stderr)
    );

    let lengths = SpriteLengthTable::standard();
    let mut document =
        MwlDocumentController::decode(edited_mwl.clone(), &fs::read(&source_mwl).unwrap()).unwrap();
    let mut expected = document.sprites(false, &lengths).unwrap();
    let duplicate = expected.tokens[0].clone();
    expected.insert(1, duplicate).unwrap();
    let SpriteToken::Record(first) = &mut expected.tokens[0] else {
        panic!("level 105 must begin with an ordinary sprite record");
    };
    let mut fields = first.native_fields().unwrap();
    fields.x = (fields.x + 1) & 0x0f;
    fields.y_low = (fields.y_low + 1) & 0x1f;
    first.set_native_fields(fields, &lengths).unwrap();
    document.replace_sprites(0, &expected, &lengths).unwrap();
    fs::write(&edited_mwl, document.begin_save().unwrap().bytes).unwrap();

    let import = ProcessCommand::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ImportLevel")
        .arg(wine_path(&imported_rom))
        .arg(wine_path(&edited_mwl))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        import.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&import.stdout),
        String::from_utf8_lossy(&import.stderr)
    );

    let reexport = ProcessCommand::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ExportLevel")
        .arg(wine_path(&imported_rom))
        .arg(wine_path(&reexported_mwl))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        reexport.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&reexport.stdout),
        String::from_utf8_lossy(&reexport.stderr)
    );

    let reexported = MwlFile::decode(&fs::read(&reexported_mwl).unwrap()).unwrap();
    let payload = reexported.payload_section(MwlSectionKind::Sprites).unwrap();
    assert_eq!(
        NativeSpriteStream::parse(&payload.payload, false, &lengths).unwrap(),
        expected
    );
    let reopened_rom = RomImage::from_bytes(fs::read(&imported_rom).unwrap()).unwrap();
    assert!(detect_identity(&reopened_rom).unwrap().checksum_matches());
    fs::remove_dir_all(directory).unwrap();
}

/// Proves that Lunar Magic imports a typed Rust MWL Layer 1 object edit and preserves its complete
/// header/object semantics on re-export.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn lunar_magic_imports_and_reexports_a_rust_mwl_object_edit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = root.join("Super Mario World (USA).sfc");
    assert!(lunar_magic.is_file(), "missing {}", lunar_magic.display());
    assert!(original_rom.is_file(), "missing {}", original_rom.display());

    let directory = std::env::temp_dir().join(format!(
        "lm-mwl-object-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let imported_rom = directory.join("Lunar Magic object import.sfc");
    let source_mwl = directory.join("source object level 105.mwl");
    let edited_mwl = directory.join("Rust object edit level 105.mwl");
    let reexported_mwl = directory.join("reexported object level 105.mwl");
    fs::copy(&original_rom, &imported_rom).unwrap();

    let export = ProcessCommand::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ExportLevel")
        .arg(wine_path(&imported_rom))
        .arg(wine_path(&source_mwl))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&export.stdout),
        String::from_utf8_lossy(&export.stderr)
    );

    let mut document =
        MwlDocumentController::decode(edited_mwl.clone(), &fs::read(&source_mwl).unwrap()).unwrap();
    let mut expected = document.layer1().unwrap();
    let duplicate = expected.objects.records[0].clone();
    expected
        .objects
        .apply_edits(&[
            ObjectEdit::Insert {
                index: 1,
                record: duplicate,
            },
            ObjectEdit::RelocateOrdinary {
                index: 0,
                screen: 2,
                coordinates: ObjectCoordinateNibbles {
                    first: 0x0e,
                    second: 0x0d,
                },
            },
        ])
        .unwrap();
    document.replace_layer1(0, &expected).unwrap();
    fs::write(&edited_mwl, document.begin_save().unwrap().bytes).unwrap();

    let import = ProcessCommand::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ImportLevel")
        .arg(wine_path(&imported_rom))
        .arg(wine_path(&edited_mwl))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        import.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&import.stdout),
        String::from_utf8_lossy(&import.stderr)
    );

    let reexport = ProcessCommand::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ExportLevel")
        .arg(wine_path(&imported_rom))
        .arg(wine_path(&reexported_mwl))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        reexport.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&reexport.stdout),
        String::from_utf8_lossy(&reexport.stderr)
    );

    let reexported = MwlFile::decode(&fs::read(&reexported_mwl).unwrap()).unwrap();
    let payload = reexported.payload_section(MwlSectionKind::Layer1).unwrap();
    assert_eq!(LevelObjectData::parse(&payload.payload).unwrap(), expected);
    let reopened_rom = RomImage::from_bytes(fs::read(&imported_rom).unwrap()).unwrap();
    assert!(detect_identity(&reopened_rom).unwrap().checksum_matches());
    fs::remove_dir_all(directory).unwrap();
}

/// Proves Lunar Magic's recovered command-zero screen-exit forms reciprocally: Rust changes a real
/// pristine exit across the compact/extended representation boundary, Lunar Magic imports it, and
/// its own exporter emits the identical decoded Layer 1 stream.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn lunar_magic_imports_and_reexports_a_rust_screen_exit_edit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = root.join("Super Mario World (USA).sfc");
    let rom_bytes = fs::read(&original_rom).unwrap();
    let lengths = SpriteLengthTable::standard();
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let project = Project::new(RomImage::from_bytes(rom_bytes).unwrap());
    let (level_number, record_index) = (0..layout.layer1.entries)
        .find_map(|level| {
            let loaded = project.load_level_slot(level, layout, &lengths).ok()?;
            loaded
                .layer1
                .objects
                .records
                .iter()
                .position(|record| record.screen_exit().is_some())
                .map(|index| (level, index))
        })
        .expect("pristine SMW must contain a native screen-exit object");

    let directory = std::env::temp_dir().join(format!(
        "lm-screen-exit-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let imported_rom = directory.join("Lunar Magic screen exit import.sfc");
    let source_mwl = directory.join("source screen exit.mwl");
    let edited_mwl = directory.join("Rust edited screen exit.mwl");
    let reexported_mwl = directory.join("reexported screen exit.mwl");
    fs::copy(&original_rom, &imported_rom).unwrap();

    let level = format!("{level_number:X}");
    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &imported_rom,
        &source_mwl,
        &level,
    );

    let mut document =
        MwlDocumentController::decode(edited_mwl.clone(), &fs::read(&source_mwl).unwrap()).unwrap();
    let mut expected = document.layer1().unwrap();
    let exit = expected.objects.records[record_index]
        .screen_exit()
        .expect("Rust and Lunar Magic object ordering must identify the same exit");
    let replacement_destination = match exit.encoding {
        lm_level::ScreenExitObjectEncoding::Compact => exit.destination_and_flags | 0x1000,
        lm_level::ScreenExitObjectEncoding::Extended => exit.destination_and_flags & 0x0fff,
    };
    let mut replacement = expected.objects.records[record_index].clone();
    replacement
        .set_screen_exit(exit.screen, replacement_destination)
        .unwrap();
    expected
        .objects
        .apply_edits(&[ObjectEdit::Replace {
            index: record_index,
            record: replacement,
        }])
        .unwrap();
    document.replace_layer1(0, &expected).unwrap();
    fs::write(&edited_mwl, document.begin_save().unwrap().bytes).unwrap();

    run_lunar_magic_level_command(
        &lunar_magic,
        "-ImportLevel",
        &imported_rom,
        &edited_mwl,
        &level,
    );
    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &imported_rom,
        &reexported_mwl,
        &level,
    );

    let reexported = MwlFile::decode(&fs::read(&reexported_mwl).unwrap()).unwrap();
    let payload = reexported.payload_section(MwlSectionKind::Layer1).unwrap();
    assert_eq!(LevelObjectData::parse(&payload.payload).unwrap(), expected);
    assert!(
        detect_identity(&RomImage::from_bytes(fs::read(&imported_rom).unwrap()).unwrap())
            .unwrap()
            .checksum_matches()
    );
    fs::remove_dir_all(directory).unwrap();
}
