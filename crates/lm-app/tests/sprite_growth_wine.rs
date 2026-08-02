use lm_app::{AppState, Command, LevelController, MwlDocumentController, NativeLevelEdit};
use lm_level::{
    CustomTimeSettings, Layer1VerticalScrollMode, Layer2ScrollSettings, LevelObjectData, MwlFile,
    MwlLevelHeaderSection, MwlSectionKind, NativeSpriteStream, ObjectCoordinateNibbles, ObjectEdit,
    SpriteLengthTable, SpriteToken,
};
use lm_project::{LevelSaveOptions, MwlNativeLevel, Project, SpritePointerTable};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SnesPointer24, detect_identity};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const PRISTINE_SMW_US_SHA256: &str =
    "0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b";

/// Proves the complete semantic MWL aggregate can round-trip through Lunar Magic itself.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn rust_semantic_mwl_round_trip_is_accepted_by_lunar_magic() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = pristine_smw_us_rom_path(&root);
    let directory = std::env::temp_dir().join(format!(
        "lm-semantic-mwl-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let imported_rom = directory.join("semantic import.sfc");
    let source_mwl = directory.join("source.mwl");
    let rust_mwl = directory.join("rust canonical.mwl");
    let reexported_mwl = directory.join("reexported.mwl");
    fs::copy(&original_rom, &imported_rom).unwrap();

    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &imported_rom,
        &source_mwl,
        "105",
    );
    let lengths = SpriteLengthTable::standard();
    let modes = [false; 256];
    let mut source = MwlNativeLevel::decode(
        &MwlFile::decode(&fs::read(&source_mwl).unwrap()).unwrap(),
        &lengths,
        32,
        &modes,
    )
    .unwrap();
    let custom_time = CustomTimeSettings::new(0xabc, true).unwrap();
    let vertical = lm_profile::smw_us_v1_level_mode(source.layer1.header.level_mode()).vertical;
    source
        .layer1
        .objects
        .set_custom_time(vertical, Some(custom_time))
        .unwrap();
    fs::write(
        &rust_mwl,
        source.encode(&lengths, &modes).unwrap().encode().unwrap(),
    )
    .unwrap();
    run_lunar_magic_level_command(
        &lunar_magic,
        "-ImportLevel",
        &imported_rom,
        &rust_mwl,
        "105",
    );
    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &imported_rom,
        &reexported_mwl,
        "105",
    );
    let actual = MwlNativeLevel::decode(
        &MwlFile::decode(&fs::read(&reexported_mwl).unwrap()).unwrap(),
        &lengths,
        32,
        &modes,
    )
    .unwrap();
    assert_eq!(actual.header, source.header);
    assert_eq!(actual.layer1, source.layer1);
    assert_eq!(actual.layer2, source.layer2);
    assert_eq!(actual.sprites, source.sprites);
    assert_eq!(actual.palette, source.palette);
    assert_eq!(actual.secondary_exits, source.secondary_exits);
    assert_eq!(actual.exanimation, source.exanimation);
    assert_eq!(actual.expanded_settings, source.expanded_settings);
    assert_eq!(
        actual.layer1.objects.custom_time(vertical),
        Some(custom_time)
    );
    fs::remove_dir_all(directory).unwrap();
}

/// Proves Lunar Magic selects and preserves expanded framing written directly by Rust.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn lunar_magic_exports_rust_installed_expanded_sprite_framing() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc");
    assert!(original_rom.is_file(), "missing {}", original_rom.display());
    let directory = std::env::temp_dir().join(format!(
        "lm-expanded-sprite-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let edited_rom = directory.join("Rust expanded sprites.sfc");
    let reexported_mwl = directory.join("Lunar Magic reexport.mwl");
    let lengths = SpriteLengthTable::standard();
    let image = RomImage::from_bytes(fs::read(&original_rom).unwrap()).unwrap();
    let mut project = Project::new(image.clone());
    let mut layout = lm_profile::smw_us_v1_vanilla_level_layout();
    layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
    let mut expected = project.load_level_slot(0x105, layout, &lengths).unwrap();
    expected.sprites.expanded = true;
    expected.sprites.header |= NativeSpriteStream::EXPANDED_HEADER_FLAG;
    let (x, mut fields) = match &expected.sprites.tokens[0] {
        SpriteToken::Record(first) => {
            let fields = first.native_fields().unwrap();
            (fields.x, fields)
        }
        SpriteToken::Screen(_) | SpriteToken::Control(_) => {
            panic!("level 105 must begin with an ordinary sprite record");
        }
    };
    fields.extra_bits = 3;
    let SpriteToken::Record(first) = &mut expected.sprites.tokens[0] else {
        unreachable!();
    };
    first.set_native_fields(fields, &lengths).unwrap();
    expected
        .sprites
        .relocate_expanded_record(0, 16, x, 2 * 32 + 31, false, &lengths)
        .unwrap();
    layout.expanded_sprites = true;
    let protected = vec![ProtectedRange(0x7fc0..0x8000)];
    project
        .relocate_level_sprites_with_checksum(
            layout,
            &expected,
            &lengths,
            0x7fdc,
            &LevelSaveOptions {
                layer1_allocation: AllocationPolicy {
                    search: 0..0,
                    bank_size: None,
                    fill_bytes: vec![0xff],
                    protected: protected.clone(),
                },
                sprite_allocation: AllocationPolicy {
                    search: 0x80_000..image.logical_len(),
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0x00, 0xff],
                    protected,
                },
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    fs::write(&edited_rom, project.save_snapshot()).unwrap();

    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &edited_rom,
        &reexported_mwl,
        "105",
    );
    let modes = [false; 256];
    let reexported = MwlFile::decode(&fs::read(&reexported_mwl).unwrap()).unwrap();
    let raw_sprites = reexported.payload_section(MwlSectionKind::Sprites).unwrap();
    assert_eq!(reexported.flags, 0);
    assert!(NativeSpriteStream::header_uses_expanded_framing(
        raw_sprites.payload[0]
    ));
    assert!(
        raw_sprites
            .payload
            .windows(2)
            .any(|bytes| bytes == [0xff, 2])
    );
    assert!(
        raw_sprites
            .payload
            .windows(2)
            .any(|bytes| bytes == [0xff, 0xff])
    );
    let actual = MwlNativeLevel::decode(&reexported, &lengths, 32, &modes).unwrap();
    assert!(actual.sprites.expanded);
    assert!(NativeSpriteStream::header_uses_expanded_framing(
        actual.sprites.header
    ));
    assert_eq!(actual.sprites, expected.sprites);
    let reopened_rom = RomImage::from_bytes(fs::read(&edited_rom).unwrap()).unwrap();
    assert!(detect_identity(&reopened_rom).unwrap().checksum_matches());
    let mut reopened_layout = lm_profile::smw_us_v1_vanilla_level_layout();
    reopened_layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&reopened_rom).unwrap();
    reopened_layout.expanded_sprites = true;
    let reopened = Project::new(reopened_rom)
        .load_level_slot(0x105, reopened_layout, &lengths)
        .unwrap();
    assert_eq!(reopened.layer1, expected.layer1);
    assert_eq!(reopened.sprites, expected.sprites);
    fs::remove_dir_all(directory).unwrap();
}

/// Proves Lunar Magic removes expanded framing when no sprite token needs its grammar.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn lunar_magic_downgrades_unneeded_expanded_sprite_framing() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc");
    assert!(original_rom.is_file(), "missing {}", original_rom.display());
    let directory = std::env::temp_dir().join(format!(
        "lm-sprite-framing-downgrade-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let edited_rom = directory.join("Rust unnecessary expanded sprites.sfc");
    let reexported_mwl = directory.join("Lunar Magic downgraded reexport.mwl");
    let lengths = SpriteLengthTable::standard();
    let image = RomImage::from_bytes(fs::read(&original_rom).unwrap()).unwrap();
    let mut project = Project::new(image.clone());
    let mut layout = lm_profile::smw_us_v1_vanilla_level_layout();
    layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
    let mut written = project.load_level_slot(0x105, layout, &lengths).unwrap();
    assert!(
        written
            .sprites
            .tokens
            .iter()
            .all(|token| matches!(token, SpriteToken::Record(record) if record.encoded.first() != Some(&0xff)))
    );
    written.sprites.expanded = true;
    written.sprites.header |= NativeSpriteStream::EXPANDED_HEADER_FLAG;
    assert!(!written.sprites.requires_expanded_framing());

    // Write the intentionally noncanonical stream directly. Semantic Rust save/export paths
    // canonicalize it, but this oracle needs Lunar Magic itself to make the decision.
    layout.expanded_sprites = true;
    let protected = vec![ProtectedRange(0x7fc0..0x8000)];
    project
        .relocate_level_sprites_with_checksum(
            layout,
            &written,
            &lengths,
            0x7fdc,
            &LevelSaveOptions {
                layer1_allocation: AllocationPolicy {
                    search: 0..0,
                    bank_size: None,
                    fill_bytes: vec![0xff],
                    protected: protected.clone(),
                },
                sprite_allocation: AllocationPolicy {
                    search: 0x80_000..image.logical_len(),
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0x00, 0xff],
                    protected,
                },
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    fs::write(&edited_rom, project.save_snapshot()).unwrap();

    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &edited_rom,
        &reexported_mwl,
        "105",
    );
    let modes = [false; 256];
    let reexported = MwlFile::decode(&fs::read(&reexported_mwl).unwrap()).unwrap();
    let raw_sprites = reexported.payload_section(MwlSectionKind::Sprites).unwrap();
    assert_eq!(reexported.flags, 0);
    assert!(!NativeSpriteStream::header_uses_expanded_framing(
        raw_sprites.payload[0]
    ));
    assert_eq!(raw_sprites.payload.last(), Some(&0xff));
    assert_ne!(
        raw_sprites
            .payload
            .get(raw_sprites.payload.len().saturating_sub(2)),
        Some(&0xff)
    );
    let actual = MwlNativeLevel::decode(&reexported, &lengths, 32, &modes).unwrap();
    let mut canonical = written.sprites.clone();
    canonical.canonicalize_framing();
    assert!(!actual.sprites.expanded);
    assert_eq!(actual.sprites, canonical);
    let reopened_rom = RomImage::from_bytes(fs::read(&edited_rom).unwrap()).unwrap();
    assert!(detect_identity(&reopened_rom).unwrap().checksum_matches());
    fs::remove_dir_all(directory).unwrap();
}

/// Proves vertical expanded streams use Lunar Magic's orientation-specific coordinate key.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn lunar_magic_matches_vertical_expanded_sprite_ordering() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc");
    assert!(original_rom.is_file(), "missing {}", original_rom.display());
    let directory = std::env::temp_dir().join(format!(
        "lm-vertical-expanded-sprite-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let edited_rom = directory.join("Rust vertical expanded sprites.sfc");
    let reexported_mwl = directory.join("Lunar Magic vertical reexport.mwl");
    let lengths = SpriteLengthTable::standard();
    let image = RomImage::from_bytes(fs::read(&original_rom).unwrap()).unwrap();
    let mut layout = lm_profile::smw_us_v1_vanilla_level_layout();
    layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
    let mut project = Project::new(image.clone());

    let (level, selected, partner, screen, x, mut expected) = (0_usize..=0x01ff)
        .find_map(|level| {
            let loaded = project.load_level_slot(level, layout, &lengths).ok()?;
            if !lm_profile::smw_us_v1_level_mode(loaded.layer1.header.level_mode()).vertical {
                return None;
            }
            let placements = loaded.sprites.native_placements();
            let (candidate, partner) = placements.iter().find_map(|candidate| {
                let partner = placements.iter().find(|later| {
                    later.token_index > candidate.token_index
                        && later.screen == candidate.screen
                        && later.minor & 0x0f < 0x0f
                })?;
                Some((candidate, partner))
            })?;
            Some((
                level,
                candidate.token_index,
                partner.token_index,
                u8::try_from(candidate.screen).ok()?,
                u8::try_from(candidate.major % 16).ok()?,
                loaded,
            ))
        })
        .expect("installed fixture must contain a sortable vertical sprite pair");
    expected.sprites.expanded = true;
    expected.sprites.header |= NativeSpriteStream::EXPANDED_HEADER_FLAG;
    let mut selected_with_controls = None;
    let mut expanded_tokens = Vec::with_capacity(expected.sprites.tokens.len() + 4);
    for (index, token) in std::mem::take(&mut expected.sprites.tokens)
        .into_iter()
        .enumerate()
    {
        if index == selected || index == partner {
            expanded_tokens.push(SpriteToken::Screen(2));
            if index == selected {
                selected_with_controls = Some(expanded_tokens.len());
            }
            expanded_tokens.push(token);
            expanded_tokens.push(SpriteToken::Screen(0));
        } else {
            expanded_tokens.push(token);
        }
    }
    expected.sprites.tokens = expanded_tokens;
    let selected_with_controls = selected_with_controls.unwrap();
    let reordered = expected
        .sprites
        .relocate_expanded_record(
            selected_with_controls,
            screen,
            x,
            2 * 32 + 0x0f,
            true,
            &lengths,
        )
        .unwrap();
    assert_ne!(reordered, selected_with_controls);
    layout.expanded_sprites = true;
    let protected = vec![ProtectedRange(0x7fc0..0x8000)];
    project
        .relocate_level_sprites_with_checksum(
            layout,
            &expected,
            &lengths,
            0x7fdc,
            &LevelSaveOptions {
                layer1_allocation: AllocationPolicy {
                    search: 0..0,
                    bank_size: None,
                    fill_bytes: vec![0xff],
                    protected: protected.clone(),
                },
                sprite_allocation: AllocationPolicy {
                    search: 0x80_000..image.logical_len(),
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0x00, 0xff],
                    protected,
                },
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    fs::write(&edited_rom, project.save_snapshot()).unwrap();

    let level_text = format!("{level:03X}");
    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &edited_rom,
        &reexported_mwl,
        &level_text,
    );
    let reexported = MwlFile::decode(&fs::read(&reexported_mwl).unwrap()).unwrap();
    let actual = MwlNativeLevel::decode(&reexported, &lengths, 32, &[false; 256]).unwrap();
    assert_eq!(reexported.flags, 0);
    assert_eq!(actual.sprites, expected.sprites);
    let reopened_rom = RomImage::from_bytes(fs::read(&edited_rom).unwrap()).unwrap();
    assert!(detect_identity(&reopened_rom).unwrap().checksum_matches());
    layout.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&reopened_rom).unwrap();
    let reopened = Project::new(reopened_rom)
        .load_level_slot(level, layout, &lengths)
        .unwrap();
    assert_eq!(reopened.sprites, expected.sprites);
    fs::remove_dir_all(directory).unwrap();
}

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

/// Proves the recovered packed main/midway entrance fields at the MWL boundary.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn lunar_magic_imports_and_reexports_rust_packed_entrance_edits() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = pristine_smw_us_rom_path(&root);
    let directory = std::env::temp_dir().join(format!(
        "lm-mwl-entrance-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let imported_rom = directory.join("entrance edit.sfc");
    let source_mwl = directory.join("source.mwl");
    let edited_mwl = directory.join("edited.mwl");
    let reexported_mwl = directory.join("reexported.mwl");
    fs::copy(&original_rom, &imported_rom).unwrap();

    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &imported_rom,
        &source_mwl,
        "105",
    );
    let mut document =
        MwlDocumentController::decode(edited_mwl.clone(), &fs::read(&source_mwl).unwrap()).unwrap();
    let source_header =
        MwlLevelHeaderSection::decode(document.value().section(MwlSectionKind::LevelHeader))
            .unwrap();
    let mut main = source_header.main_entrance();
    main.position = main.position & 0x0f | 0x20;
    let layer2_scroll = Layer2ScrollSettings::Separate {
        horizontal: 0x1b,
        vertical: 0x12,
    };
    document
        .apply_edits(
            0,
            &[
                lm_app::MwlDocumentEdit::SetMainEntrance(main),
                lm_app::MwlDocumentEdit::SetLayer2Scroll(layer2_scroll),
            ],
        )
        .unwrap();
    let mut layer1 = document.layer1().unwrap();
    layer1
        .header
        .set_layer1_vertical_scroll(Layer1VerticalScrollMode::NoScrollAtBottomUnlessFlying);
    document.replace_layer1(1, &layer1).unwrap();
    fs::write(&edited_mwl, document.begin_save().unwrap().bytes).unwrap();

    run_lunar_magic_level_command(
        &lunar_magic,
        "-ImportLevel",
        &imported_rom,
        &edited_mwl,
        "105",
    );
    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &imported_rom,
        &reexported_mwl,
        "105",
    );
    let reexported = MwlFile::decode(&fs::read(&reexported_mwl).unwrap()).unwrap();
    let reopened =
        MwlLevelHeaderSection::decode(reexported.section(MwlSectionKind::LevelHeader)).unwrap();
    assert_eq!(reopened.main_entrance(), main);
    assert_eq!(reopened.layer2_scroll_settings(), layer2_scroll);
    let reopened_layer1 = reexported.payload_section(MwlSectionKind::Layer1).unwrap();
    assert_eq!(
        LevelObjectData::parse(&reopened_layer1.payload)
            .unwrap()
            .header
            .layer1_vertical_scroll(),
        Layer1VerticalScrollMode::NoScrollAtBottomUnlessFlying
    );
    assert_eq!(reopened.midway_entrance(), source_header.midway_entrance());
    for (index, byte) in source_header.0.into_iter().enumerate() {
        if ![2, 3, 4, 5, 6, 9, 10, 11, 12, 14, 15, 17].contains(&index) {
            assert_eq!(reopened.0[index], byte);
        }
    }
    fs::remove_dir_all(directory).unwrap();
}

/// Proves direct writes to the four pristine SMW entrance planes are interpreted by Lunar Magic
/// for both physical copier-header shapes.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn rust_direct_rom_main_entrance_edit_is_exported_by_lunar_magic() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = pristine_smw_us_rom_path(&root);
    let logical_rom = RomImage::from_bytes(fs::read(&original_rom).unwrap())
        .unwrap()
        .logical_bytes()
        .to_vec();
    let directory = std::env::temp_dir().join(format!(
        "lm-rom-entrance-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();

    let canonical_lunar_magic_header =
        || lm_profile::smw_us_v1_lunar_magic_copier_header().to_vec();
    for (name, copier_header) in [
        ("headerless", None),
        ("headered", Some(canonical_lunar_magic_header())),
    ] {
        let edited_rom = directory.join(format!("Rust entrance edit {name}.sfc"));
        let exported_mwl = directory.join(format!("exported {name}.mwl"));
        let mut app = AppState::default();
        app.load_rom(logical_rom.clone()).unwrap();
        if copier_header.is_some() {
            app.dispatch(Command::SetLunarMagicSmwUsCopierHeader { rev: 0 })
                .unwrap();
        }
        app.dispatch(Command::SelectLevel(0x105)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut controller = lm_app::VanillaEntranceController::decode(
            &snapshot,
            lm_profile::smw_us_v1_vanilla_entrance_layout(),
        )
        .unwrap();
        let mut expected = controller.entrance();
        expected.position ^= 0x10;
        controller.set_entrance(expected);
        app.dispatch(
            controller
                .prepare_commit("Wine main entrance oracle")
                .unwrap()
                .into_command(),
        )
        .unwrap();
        fs::write(&edited_rom, app.project().unwrap().save_snapshot()).unwrap();

        let rust_image = RomImage::from_bytes(fs::read(&edited_rom).unwrap()).unwrap();
        assert_eq!(rust_image.copier_header_bytes(), copier_header.as_deref());
        run_lunar_magic_level_command(
            &lunar_magic,
            "-ExportLevel",
            &edited_rom,
            &exported_mwl,
            "105",
        );
        let after_lunar_magic = RomImage::from_bytes(fs::read(&edited_rom).unwrap()).unwrap();
        let expected_lunar_magic_header = copier_header
            .clone()
            .unwrap_or_else(canonical_lunar_magic_header);
        assert_eq!(
            after_lunar_magic.copier_header_bytes(),
            Some(expected_lunar_magic_header.as_slice())
        );
        assert!(
            detect_identity(&after_lunar_magic)
                .unwrap()
                .checksum_matches()
        );

        let exported = MwlFile::decode(&fs::read(&exported_mwl).unwrap()).unwrap();
        let actual =
            MwlLevelHeaderSection::decode(exported.section(MwlSectionKind::LevelHeader)).unwrap();
        assert_eq!(
            actual.layer2_scroll_settings(),
            Layer2ScrollSettings::Original {
                table_index: expected.position >> 4,
            }
        );
        let actual_entrance = actual.main_entrance();
        assert_eq!(actual_entrance.position, expected.position);
        assert_eq!(
            actual_entrance.vertical_settings,
            expected.vertical_settings
        );
        assert_eq!(
            actual_entrance.screen_and_method,
            expected.screen_and_method
        );
        assert_eq!(
            actual_entrance.level_mode_and_screen,
            expected.level_mode_and_screen
        );
    }
    fs::remove_dir_all(directory).unwrap();
}

/// Proves owned direct updates to Lunar Magic's installed four-plane separate-midway table.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn rust_updates_installed_separate_midway_table_and_lunar_magic_reexports_it() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = pristine_smw_us_rom_path(&root);
    let directory = std::env::temp_dir().join(format!(
        "lm-rom-midway-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let edited_rom = directory.join("installed midway.sfc");
    let source_mwl = directory.join("source.mwl");
    let install_mwl = directory.join("install.mwl");
    let exported_mwl = directory.join("exported.mwl");
    fs::copy(&original_rom, &edited_rom).unwrap();

    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &edited_rom,
        &source_mwl,
        "105",
    );
    let mut mwl =
        MwlDocumentController::decode(install_mwl.clone(), &fs::read(&source_mwl).unwrap())
            .unwrap();
    let header =
        MwlLevelHeaderSection::decode(mwl.value().section(MwlSectionKind::LevelHeader)).unwrap();
    let mut main = header.main_entrance();
    main.flags |= 0x20;
    let initial = lm_level::MwlMidwayEntranceSettings {
        flags: 0xa5,
        position: 0x6c,
        high_position: 0x21,
        additional_flags: 0x87,
    };
    mwl.apply_edits(
        0,
        &[
            lm_app::MwlDocumentEdit::SetMainEntrance(main),
            lm_app::MwlDocumentEdit::SetMidwayEntrance(initial),
        ],
    )
    .unwrap();
    fs::write(&install_mwl, mwl.begin_save().unwrap().bytes).unwrap();
    run_lunar_magic_level_command(
        &lunar_magic,
        "-ImportLevel",
        &edited_rom,
        &install_mwl,
        "105",
    );

    let mut app = AppState::default();
    app.load_rom(fs::read(&edited_rom).unwrap()).unwrap();
    app.dispatch(Command::SelectLevel(0x105)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = lm_app::VanillaEntranceController::decode_with_midway(
        &snapshot,
        lm_profile::smw_us_v1_vanilla_entrance_layout(),
        lm_profile::smw_us_v1_separate_midway_locator(),
    )
    .unwrap();
    let mut expected = controller.midway_entrance().unwrap();
    assert_eq!(expected.flags, initial.flags);
    assert_eq!(expected.position, initial.position);
    assert_eq!(expected.additional_flags, initial.additional_flags);
    assert_eq!(expected.high_position, initial.high_position);
    expected.position ^= 1;
    expected.high_position ^= 2;
    controller.set_midway_entrance(expected);
    app.dispatch(
        controller
            .prepare_commit("Wine installed midway oracle")
            .unwrap()
            .into_command(),
    )
    .unwrap();
    fs::write(&edited_rom, app.project().unwrap().save_snapshot()).unwrap();

    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &edited_rom,
        &exported_mwl,
        "105",
    );
    let exported = MwlFile::decode(&fs::read(&exported_mwl).unwrap()).unwrap();
    let actual =
        MwlLevelHeaderSection::decode(exported.section(MwlSectionKind::LevelHeader)).unwrap();
    assert_eq!(
        actual.midway_entrance(),
        lm_level::MwlMidwayEntranceSettings {
            flags: expected.flags,
            position: expected.position,
            high_position: expected.high_position,
            additional_flags: expected.additional_flags,
        }
    );
    fs::remove_dir_all(directory).unwrap();
}

/// Proves the clean-room first-time separate-midway installer is accepted by Lunar Magic.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn rust_installs_separate_midway_runtime_that_lunar_magic_reexports() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = pristine_smw_us_rom_path(&root);
    let directory = std::env::temp_dir().join(format!(
        "lm-midway-install-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let installed_rom = directory.join("Rust installed midway.sfc");
    let exported_mwl = directory.join("exported.mwl");

    let mut app = AppState::default();
    app.load_rom(fs::read(&original_rom).unwrap()).unwrap();
    app.dispatch(Command::SelectLevel(0x105)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let controller = lm_app::VanillaEntranceController::decode_with_midway(
        &snapshot,
        lm_profile::smw_us_v1_vanilla_entrance_layout(),
        lm_profile::smw_us_v1_separate_midway_locator(),
    )
    .unwrap();
    assert!(controller.midway_entrance().is_none());
    let expected = lm_level::SeparateMidwayEntrance {
        flags: 0xa5,
        position: 0x6c,
        additional_flags: 0x87,
        high_position: 0x21,
    };
    app.dispatch(
        controller
            .prepare_midway_install(expected)
            .unwrap()
            .into_command(),
    )
    .unwrap();
    fs::write(&installed_rom, app.project().unwrap().save_snapshot()).unwrap();

    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &installed_rom,
        &exported_mwl,
        "105",
    );
    let exported = MwlFile::decode(&fs::read(&exported_mwl).unwrap()).unwrap();
    let header =
        MwlLevelHeaderSection::decode(exported.section(MwlSectionKind::LevelHeader)).unwrap();
    assert_ne!(header.main_entrance().flags & 0x20, 0);
    assert_eq!(
        header.midway_entrance(),
        lm_level::MwlMidwayEntranceSettings {
            flags: expected.flags,
            position: expected.position,
            high_position: expected.high_position,
            additional_flags: expected.additional_flags,
        }
    );
    fs::remove_dir_all(directory).unwrap();
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
    let original_rom = pristine_smw_us_rom_path(&root);
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
    let original_rom = pristine_smw_us_rom_path(&root);
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
    let original_rom = pristine_smw_us_rom_path(&root);
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

/// Proves every boundary of Lunar Magic's recovered command-zero screen-exit forms reciprocally.
#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and SMW ROM fixtures"]
fn lunar_magic_imports_and_reexports_all_rust_screen_exit_boundaries() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let original_rom = pristine_smw_us_rom_path(&root);
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
    fs::copy(&original_rom, &imported_rom).unwrap();

    let level = format!("{level_number:X}");
    run_lunar_magic_level_command(
        &lunar_magic,
        "-ExportLevel",
        &imported_rom,
        &source_mwl,
        &level,
    );

    let mut current_mwl = source_mwl;
    for (case, (screen, destination_and_flags)) in
        [(0, 0), (0x1f, 0x0fff), (0, 0x1000), (0x1f, 0xffff)]
            .into_iter()
            .enumerate()
    {
        let edited_mwl = directory.join(format!("Rust screen exit case {case}.mwl"));
        let reexported_mwl = directory.join(format!("reexported screen exit case {case}.mwl"));
        let mut document =
            MwlDocumentController::decode(edited_mwl.clone(), &fs::read(&current_mwl).unwrap())
                .unwrap();
        let mut expected = document.layer1().unwrap();
        let previous = expected.objects.records[record_index].clone();
        let advance = previous.encoded()[0] & 0x80;
        let mut replacement = previous;
        replacement
            .set_screen_exit(screen, destination_and_flags)
            .unwrap();
        assert_eq!(replacement.encoded()[0] & 0x80, advance);
        let decoded = replacement.screen_exit().unwrap();
        assert_eq!(decoded.screen, screen);
        let canonical_destination = destination_and_flags | lm_level::SCREEN_EXIT_REQUIRED_FLAG;
        assert_eq!(decoded.destination_and_flags, canonical_destination);
        assert_eq!(
            decoded.encoding,
            if canonical_destination & 0xf000 == 0 {
                lm_level::ScreenExitObjectEncoding::Compact
            } else {
                lm_level::ScreenExitObjectEncoding::Extended
            }
        );
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
        current_mwl = reexported_mwl;
    }
    assert!(
        detect_identity(&RomImage::from_bytes(fs::read(&imported_rom).unwrap()).unwrap())
            .unwrap()
            .checksum_matches()
    );
    fs::remove_dir_all(directory).unwrap();
}
