use super::*;
use crate::{AppError, AppState, Command, FrontendEffect};
use lm_level::{
    CustomTimeSettings, Layer1VerticalScrollMode, ObjectRecord, SpriteLengthTable, SpriteRecord,
};
use lm_project::{
    LevelLayer2DescriptorTable, LevelLayer2RomLayout, LevelLayer2SaveOptions,
    LevelLayer2TilemapEncoding, LevelPointerTable, LevelSaveOptions, RatsOwnershipManifest,
};
use lm_rats::{AllocationPolicy, ProtectedRange};

fn layout() -> LevelRomLayout {
    LevelRomLayout {
        mapper: Mapper::LoRom,
        layer1: LevelPointerTable {
            offset: 0x200,
            entries: 0x200,
            stride: 3,
        },
        sprites: LevelPointerTable {
            offset: 0x800,
            entries: 0x200,
            stride: 3,
        }
        .into(),
        expanded_sprites: false,
    }
}

fn test_rom() -> Vec<u8> {
    let mut bytes = vec![0xff; 0x8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
    let number = 0x105;
    let layer_pointer = layout().layer1.pointer_offset(number).unwrap();
    let sprite_pointer = layout()
        .sprites
        .low_or_contiguous_table()
        .pointer_offset(number)
        .unwrap();
    let layer_snes = lm_rom::pc_to_snes(Mapper::LoRom, 0x1200)
        .unwrap()
        .to_le_bytes();
    let sprite_snes = lm_rom::pc_to_snes(Mapper::LoRom, 0x1300)
        .unwrap()
        .to_le_bytes();
    bytes[layer_pointer..layer_pointer + 3].copy_from_slice(&layer_snes[..3]);
    bytes[sprite_pointer..sprite_pointer + 3].copy_from_slice(&sprite_snes[..3]);
    bytes[0x1200..0x1209].copy_from_slice(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]);
    bytes[0x1300..0x1305].copy_from_slice(&[0x10, 0, 0, 1, 0xff]);
    let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    bytes
}

fn options() -> LevelSaveOptions {
    let protected = vec![
        ProtectedRange(0x200..0x800),
        ProtectedRange(0x800..0xe00),
        ProtectedRange(0x7fdc..0x7fe0),
    ];
    let policy = AllocationPolicy {
        search: 0x8000..0x10000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected,
    };
    LevelSaveOptions {
        layer1_allocation: policy.clone(),
        sprite_allocation: policy,
        previous_layer1: None,
        previous_sprites: None,
        reuse_identical: true,
        erase_fill: 0xff,
    }
}

fn layer2_layout() -> LevelLayer2RomLayout {
    LevelLayer2RomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: 0x1000,
            entries: 0x200,
            stride: 3,
        },
        background_bank_substitution: None,
        legacy_pointer_redirect: None,
        descriptor_table: None,
        maximum_compressed_len: 0x8000,
        tilemap_encoding: LevelLayer2TilemapEncoding::SplitPlanes,
    }
}

fn installed_layer2_layout() -> LevelLayer2RomLayout {
    LevelLayer2RomLayout {
        descriptor_table: Some(LevelLayer2DescriptorTable {
            offset: 0xe00,
            entries: 0x200,
            stride: 1,
        }),
        ..layer2_layout()
    }
}

fn layer2_test_rom() -> Vec<u8> {
    let mut bytes = test_rom();
    bytes.resize(0x1_0000, 0xff);
    bytes[0x1201] = 0;
    let pointer = layer2_layout().pointers.pointer_offset(0x105).unwrap();
    let snes = lm_rom::pc_to_snes(Mapper::LoRom, 0x1400)
        .unwrap()
        .to_le_bytes();
    bytes[pointer..pointer + 3].copy_from_slice(&snes[..3]);
    let encoded = lm_codec::encode_terminated_rle(&vec![0; NATIVE_LAYER2_TILEMAP_LEN]);
    bytes[0x1400..0x1400 + encoded.len()].copy_from_slice(&encoded);
    let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    bytes
}

fn installed_layer2_test_rom() -> Vec<u8> {
    let mut bytes = layer2_test_rom();
    bytes[0xe00 + 0x105] = 0x06;
    let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    bytes
}

fn layer2_object_test_rom() -> Vec<u8> {
    let mut bytes = test_rom();
    bytes.resize(0x1_0000, 0xff);
    let pointer = layer2_layout().pointers.pointer_offset(0x105).unwrap();
    let snes = lm_rom::pc_to_snes(Mapper::LoRom, 0x1400)
        .unwrap()
        .to_le_bytes();
    bytes[pointer..pointer + 3].copy_from_slice(&snes[..3]);
    bytes[0x1400..0x1409].copy_from_slice(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]);
    let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    bytes
}

fn tagged_test_rom() -> (Vec<u8>, RatsOwnershipManifest) {
    let mut project = Project::new(RomImage::from_bytes(test_rom()).unwrap());
    let level = project
        .load_level_slot(0x105, layout(), &SpriteLengthTable::standard())
        .unwrap();
    let protected = vec![
        ProtectedRange(0x200..0x800),
        ProtectedRange(0x800..0xe00),
        ProtectedRange(0x7fdc..0x7fe0),
    ];
    let policy = AllocationPolicy {
        search: 0x2000..0x7000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected,
    };
    let saved = project
        .save_level_slot_with_checksum(
            layout(),
            &level,
            &SpriteLengthTable::standard(),
            0x7fdc,
            &LevelSaveOptions {
                layer1_allocation: policy.clone(),
                sprite_allocation: policy,
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    (
        project.save_snapshot(),
        RatsOwnershipManifest {
            owned: vec![saved.layer1.block, saved.sprites.block],
            retained: Vec::new(),
        },
    )
}

#[test]
fn decoded_edit_allocates_through_app_and_reloads_natively() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    controller
        .apply_edits(&[
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(3)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::Layer1VerticalScroll(
                Layer1VerticalScrollMode::NoScrollAtBottomUnlessFlying,
            )),
            NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
                index: 0,
                record: ObjectRecord::new(vec![6, 5, 4]).unwrap(),
            }]),
            NativeLevelEdit::ReplaceSprite {
                index: 0,
                token: SpriteToken::Record(SpriteRecord {
                    encoded: vec![0, 0, 9],
                }),
            },
        ])
        .unwrap();
    let prepared = controller
        .prepare_commit("Edit level 105", &options())
        .unwrap();
    assert_eq!(prepared.expected_revision, 0);
    assert_eq!(prepared.mutation.appended.len(), 0x8000);
    let effects = app.dispatch(prepared.into_command()).unwrap();
    assert_eq!(
        effects,
        [FrontendEffect::ProjectChanged {
            description: "Edit level 105".into(),
            mode: EditorMode::Level(0x105),
            revision: 1,
        }]
    );
    let reloaded = app
        .project()
        .unwrap()
        .load_level_slot(0x105, layout(), &SpriteLengthTable::standard())
        .unwrap();
    assert_eq!(reloaded, *controller.level());
    assert_eq!(
        reloaded.layer1.header.layer1_vertical_scroll(),
        Layer1VerticalScrollMode::NoScrollAtBottomUnlessFlying
    );
    let logical = app.project().unwrap().rom.logical_bytes();
    let stored = lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap();
    let computed = lm_rom::compute_snes_checksum(logical, 0x7fdc).unwrap();
    assert_eq!(stored, computed);
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
    app.dispatch(Command::Redo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x10000);
}

#[test]
fn raw_pc_address_load_stages_only_layer1_into_the_current_ordinary_slot() {
    let mut rom = test_rom();
    let raw_address = 0x1400;
    rom[raw_address..raw_address + 9].copy_from_slice(&[6, 5, 4, 3, 2, 0x0a, 0x0b, 0x0c, 0xff]);
    let original_raw_stream = rom[raw_address..raw_address + 9].to_vec();
    let original_sprite_stream = rom[0x1300..0x1305].to_vec();
    let mut app = AppState::default();
    app.load_rom(rom).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let controller = LevelController::decode_layer1_from_pc_address(
        &snapshot,
        layout(),
        &SpriteLengthTable::standard(),
        raw_address,
    )
    .unwrap();

    assert_eq!(controller.level().number, 0x105);
    assert_eq!(controller.level().layer1.header.encoded(), [6, 5, 4, 3, 2]);
    assert_eq!(
        controller.level().layer1.objects.records[0].encoded(),
        [0x0a, 0x0b, 0x0c]
    );
    assert!(controller.layer1_is_modified());
    assert!(!controller.sprites_are_modified());
    assert!(controller.layer2().is_none());

    let prepared = controller
        .prepare_commit("Save raw Layer 1 to level 105", &options())
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    let reopened = LevelController::decode(
        &app.controller_snapshot().unwrap(),
        layout(),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    assert_eq!(reopened.level().layer1, controller.level().layer1);
    assert_eq!(reopened.level().sprites, controller.level().sprites);
    let saved = app.controller_snapshot().unwrap().rom_bytes;
    assert_eq!(&saved[0x1300..0x1305], original_sprite_stream.as_slice());
    assert_eq!(
        &saved[raw_address..raw_address + 9],
        original_raw_stream.as_slice()
    );
}

#[test]
fn raw_pc_address_load_rejects_out_of_rom_and_unterminated_bank_data() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    assert!(matches!(
        LevelController::decode_layer1_from_pc_address(
            &snapshot,
            layout(),
            &SpriteLengthTable::standard(),
            0x8000,
        ),
        Err(LevelControllerError::RawLayer1AddressOutOfRange { .. })
    ));
    assert!(matches!(
        LevelController::decode_layer1_from_pc_address(
            &snapshot,
            layout(),
            &SpriteLengthTable::standard(),
            0x7ffb,
        ),
        Err(LevelControllerError::InvalidObjectEncoding(
            ObjectStreamError::MissingTerminator
        ))
    ));
}

#[test]
fn reserved_level_mode_is_staged_as_lunar_magics_mode_zero_and_undoes() {
    let mut source = test_rom();
    source[0x1201] = 0xb2;
    let checksum = lm_rom::compute_snes_checksum(&source, 0x7fdc).unwrap();
    source[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());

    let mut app = AppState::default();
    app.load_rom(source.clone()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    assert!(controller.is_modified());
    assert_eq!(controller.normalized_reserved_level_mode(), Some(0x12));
    assert_eq!(controller.level().layer1.header.level_mode(), 0);
    assert_eq!(controller.level().layer1.header.background_color(), 5);

    let prepared = controller
        .prepare_commit("Normalize reserved level mode", &options())
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    let reopened = app
        .project()
        .unwrap()
        .load_level_slot(0x105, layout(), &SpriteLengthTable::standard())
        .unwrap();
    assert_eq!(reopened.layer1.header.level_mode(), 0);
    assert_eq!(reopened.layer1.header.background_color(), 5);
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().rom.as_file_bytes(), source);
}

#[test]
fn semantic_reserved_mode_edit_canonicalizes_but_out_of_range_still_rejects() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    controller
        .apply_edits(&[NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(
            0x1d,
        ))])
        .unwrap();
    assert_eq!(controller.normalized_reserved_level_mode(), Some(0x1d));
    assert_eq!(controller.level().layer1.header.level_mode(), 0);
    assert!(
        controller
            .apply_edits(&[NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(
                0x20,
            ))])
            .is_err()
    );
    assert_eq!(controller.normalized_reserved_level_mode(), Some(0x1d));
    assert_eq!(controller.level().layer1.header.level_mode(), 0);
}

#[test]
fn sprite_only_nonshared_commit_preserves_layer1_pointer_and_payload() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let baseline_image = RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap();
    let layer_pointer_range = layout().layer1.pointer_offset(0x105).unwrap();
    let baseline_layer_pointer = baseline_image
        .read(layer_pointer_range, 3)
        .unwrap()
        .to_vec();
    let baseline_layer_payload = baseline_image.read(0x1200, 9).unwrap().to_vec();
    let baseline_sprite_pointer = layout()
        .sprites
        .read_snes_pointer(&baseline_image, 0x105)
        .unwrap();
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    controller
        .apply_edits(&[NativeLevelEdit::InsertSprite {
            index: 1,
            token: SpriteToken::Record(SpriteRecord {
                encoded: vec![0x10, 0x01, 2],
            }),
        }])
        .unwrap();

    let prepared = controller
        .prepare_commit("Grow only level 105 sprites", &options())
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();

    let image = &app.project().unwrap().rom;
    assert_eq!(
        image.read(layer_pointer_range, 3).unwrap(),
        baseline_layer_pointer
    );
    assert_eq!(image.read(0x1200, 9).unwrap(), baseline_layer_payload);
    assert_ne!(
        layout().sprites.read_snes_pointer(image, 0x105).unwrap(),
        baseline_sprite_pointer
    );
    assert_eq!(
        app.project()
            .unwrap()
            .load_level_slot(0x105, layout(), &SpriteLengthTable::standard())
            .unwrap(),
        *controller.level()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(
        app.project().unwrap().rom.logical_bytes(),
        baseline_image.logical_bytes()
    );
}

#[test]
fn custom_time_edit_uses_staged_level_orientation_and_reopens_from_rom() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let mut controller = LevelController::decode(
        &app.controller_snapshot().unwrap(),
        layout(),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    let settings = CustomTimeSettings::new(0xabc, true).unwrap();
    controller
        .apply_edits(&[
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(3)),
            NativeLevelEdit::SetCustomTime(Some(settings)),
        ])
        .unwrap();
    assert_eq!(
        controller.level().layer1.objects.encode().unwrap(),
        [9, 8, 7, 0x4c, 0x8b, 0x8a, 0xff]
    );

    let prepared = controller
        .prepare_commit("Set custom level time", &options())
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    let reopened = LevelController::decode(
        &app.controller_snapshot().unwrap(),
        layout(),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    assert_eq!(
        reopened.level().layer1.objects.custom_time(true),
        Some(settings)
    );

    controller
        .apply_edits(&[NativeLevelEdit::SetCustomTime(None)])
        .unwrap();
    assert_eq!(controller.level().layer1.objects.custom_time(true), None);
    assert!(controller.undo());
    assert_eq!(
        controller.level().layer1.objects.custom_time(true),
        Some(settings)
    );
}

#[test]
fn layer2_tilemap_shares_history_and_commits_with_semantic_reopen() {
    let mut app = AppState::default();
    app.load_rom(layer2_test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = LevelController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    let original = controller.layer2().cloned();
    assert!(matches!(
        controller.apply_layer2_tilemap_words(&[(0, 0x9999), (1024, 0x8888)]),
        Err(LevelControllerError::Layer2TileIndex(1024))
    ));
    assert_eq!(controller.layer2().cloned(), original);
    controller
        .apply_layer2_tilemap_words(&[(0, 0x0123), (511, 0x0456)])
        .unwrap();
    assert!(controller.layer2_is_modified());
    assert!(controller.undo());
    assert!(!controller.layer2_is_modified());
    assert!(controller.redo());

    let layer2_options = LevelLayer2SaveOptions {
        allocation: options().layer1_allocation,
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    let prepared = controller
        .prepare_commit_with_layer2("Edit level 105 Layer 2", &options(), &layer2_options, false)
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    let reopened = LevelController::decode_with_layer2(
        &app.controller_snapshot().unwrap(),
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    let NativeLayer2Data::Tilemap(bytes) = reopened.layer2().unwrap() else {
        panic!("expected tilemap-backed Layer 2");
    };
    assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]), 0x0123);
    assert_eq!(u16::from_le_bytes([bytes[1022], bytes[1023]]), 0x0456);
}

#[test]
fn layer2_object_edits_share_history_and_commit_with_the_level() {
    let mut app = AppState::default();
    app.load_rom(layer2_object_test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = LevelController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    controller
        .apply_layer2_object_edits(&[ObjectEdit::Replace {
            index: 0,
            record: ObjectRecord::new(vec![6, 5, 4]).unwrap(),
        }])
        .unwrap();
    assert!(controller.layer2_is_modified());
    assert!(controller.undo());
    assert!(!controller.layer2_is_modified());
    assert!(controller.redo());

    let layer2_options = LevelLayer2SaveOptions {
        allocation: options().layer1_allocation,
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    let prepared = controller
        .prepare_commit_with_layer2(
            "Edit level 105 object-backed Layer 2",
            &options(),
            &layer2_options,
            false,
        )
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    let reopened = LevelController::decode_with_layer2(
        &app.controller_snapshot().unwrap(),
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    let NativeLayer2Data::Objects(objects) = reopened.layer2().unwrap() else {
        panic!("expected object-backed Layer 2");
    };
    assert_eq!(objects.objects.records[0].encoded(), &[6, 5, 4]);
}

#[test]
fn background_bank_and_remap_share_undo_and_persist_descriptor_only_changes() {
    let mut app = AppState::default();
    app.load_rom(installed_layer2_test_rom()).unwrap();
    let mut controller = LevelController::decode_with_layer2(
        &app.controller_snapshot().unwrap(),
        layout(),
        Some(installed_layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    assert_eq!(controller.layer2_descriptor().unwrap().raw(), 0x06);

    controller.set_layer2_map16_bank(3).unwrap();
    assert_eq!(controller.layer2_descriptor().unwrap().raw(), 0x36);
    assert!(controller.layer2_is_modified());
    assert!(controller.undo());
    assert_eq!(controller.layer2_descriptor().unwrap().raw(), 0x06);
    assert!(controller.redo());

    let layer2_options = LevelLayer2SaveOptions {
        allocation: options().layer1_allocation,
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    let mut descriptor_options = layer2_options;
    descriptor_options
        .allocation
        .protected
        .push(ProtectedRange(0xe00..0x1000));
    let prepared = controller
        .prepare_commit_with_layer2(
            "Change background Map16 bank",
            &options(),
            &descriptor_options,
            false,
        )
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    let reopened = LevelController::decode_with_layer2(
        &app.controller_snapshot().unwrap(),
        layout(),
        Some(installed_layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    assert_eq!(reopened.layer2_descriptor().unwrap().raw(), 0x36);

    let changed = controller
        .remap_layer2_tilemap("B000,C001", 0, Some(&[0]))
        .unwrap();
    assert_eq!(changed, 1);
    assert_eq!(controller.layer2_descriptor().unwrap().raw(), 0x46);
    let NativeLayer2Data::Tilemap(bytes) = controller.layer2().unwrap() else {
        panic!("expected tilemap-backed Layer 2");
    };
    assert_eq!(&bytes[..2], &[1, 0]);
}

#[test]
fn direct_map16_remap_updates_both_object_layers_with_one_undo_and_reopens() {
    let mut app = AppState::default();
    app.load_rom(layer2_object_test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = LevelController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    let layer1 = ObjectRecord::direct_map16_rectangle(0x100, 2, 2).unwrap();
    let mut layer2 = ObjectRecord::direct_map16_rectangle(0x110, 1, 1).unwrap();
    layer2
        .set_direct_map16_condition(Some(lm_level::DirectMap16Condition {
            flag: 9,
            always_show: true,
        }))
        .unwrap();
    controller
        .apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
            index: 0,
            record: layer1,
        }])])
        .unwrap();
    controller
        .apply_layer2_object_edits(&[ObjectEdit::Replace {
            index: 0,
            record: layer2,
        }])
        .unwrap();

    let before_remap = controller.state();
    let program = DirectMap16RemapProgram::parse("100,M200 110,300").unwrap();
    assert_eq!(controller.remap_direct_map16_objects(&program).unwrap(), 2);
    assert_eq!(
        controller.level().layer1.objects.records[0]
            .direct_map16_fields()
            .unwrap()
            .source_tile,
        0x200
    );
    let NativeLayer2Data::Objects(objects) = controller.layer2().unwrap() else {
        panic!("expected object-backed Layer 2");
    };
    assert_eq!(
        objects.objects.records[0]
            .direct_map16_fields()
            .unwrap()
            .source_tile,
        0x300
    );
    assert_eq!(
        objects.objects.records[0].direct_map16_condition(),
        Some(lm_level::DirectMap16Condition {
            flag: 9,
            always_show: true
        })
    );
    assert!(controller.undo());
    assert_eq!(controller.state(), before_remap);
    assert!(controller.redo());

    let layer2_options = LevelLayer2SaveOptions {
        allocation: options().layer1_allocation,
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    let prepared = controller
        .prepare_commit_with_layer2("Remap Direct Map16", &options(), &layer2_options, false)
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    let reopened = LevelController::decode_with_layer2(
        &app.controller_snapshot().unwrap(),
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    assert_eq!(
        reopened.level().layer1.objects.records[0]
            .direct_map16_fields()
            .unwrap()
            .source_tile,
        0x200
    );
    let NativeLayer2Data::Objects(objects) = reopened.layer2().unwrap() else {
        panic!("expected object-backed Layer 2");
    };
    assert_eq!(
        objects.objects.records[0]
            .direct_map16_fields()
            .unwrap()
            .source_tile,
        0x300
    );
}

#[test]
fn level_mode_storage_change_requires_approval_resets_and_reopens_atomically() {
    let mut app = AppState::default();
    app.load_rom(layer2_object_test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = LevelController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    let original_level = controller.level().clone();
    let original_layer2 = controller.layer2().cloned();
    let edits = [NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(
        0,
    ))];

    assert!(matches!(
        controller.apply_edits(&edits),
        Err(LevelControllerError::Layer2ModeChangeRequiresReset { from: 2, to: 0 })
    ));
    assert_eq!(controller.level(), &original_level);
    assert_eq!(controller.layer2().cloned(), original_layer2);
    assert!(!controller.can_undo());

    controller
        .apply_edits_with_layer2_reset(&edits, true)
        .unwrap();
    assert_eq!(controller.level().layer1.header.level_mode(), 0);
    assert!(matches!(
        controller.layer2(),
        Some(NativeLayer2Data::Tilemap(bytes)) if bytes == &vec![0; NATIVE_LAYER2_TILEMAP_LEN]
    ));
    controller
        .apply_edits_with_layer2_reset(
            &[NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(
                2,
            ))],
            true,
        )
        .unwrap();
    assert_eq!(controller.layer2().cloned(), original_layer2);
    assert!(controller.undo());
    assert!(matches!(
        controller.layer2(),
        Some(NativeLayer2Data::Tilemap(bytes)) if bytes == &vec![0; NATIVE_LAYER2_TILEMAP_LEN]
    ));
    assert!(controller.undo());
    assert_eq!(controller.level(), &original_level);
    assert_eq!(controller.layer2().cloned(), original_layer2);
    assert!(controller.redo());

    let layer2_options = LevelLayer2SaveOptions {
        allocation: options().layer1_allocation,
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    let prepared = controller
        .prepare_commit_with_layer2(
            "Change level mode and reset Layer 2",
            &options(),
            &layer2_options,
            false,
        )
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    let reopened = LevelController::decode_with_layer2(
        &app.controller_snapshot().unwrap(),
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    assert_eq!(reopened.level().layer1.header.level_mode(), 0);
    assert!(matches!(
        reopened.layer2(),
        Some(NativeLayer2Data::Tilemap(bytes)) if bytes == &vec![0; NATIVE_LAYER2_TILEMAP_LEN]
    ));
}

#[test]
fn tilemap_to_object_mode_change_creates_lunar_magics_empty_object_workspace() {
    let mut app = AppState::default();
    app.load_rom(layer2_test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = LevelController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    controller
        .apply_edits_with_layer2_reset(
            &[NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(
                1,
            ))],
            true,
        )
        .unwrap();
    let Some(NativeLayer2Data::Objects(objects)) = controller.layer2() else {
        panic!("mode 1 must create object-backed Layer 2");
    };
    assert_eq!(objects, &lm_level::LevelObjectData::default());
}

#[test]
fn staged_history_restores_baseline_and_invalidates_divergent_redo() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    let baseline = controller.level().clone();

    assert!(!controller.can_undo());
    assert!(!controller.can_redo());
    assert!(!controller.undo());
    assert!(!controller.redo());

    controller
        .apply_edits(&[NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(
            3,
        ))])
        .unwrap();
    controller
        .apply_edits(&[NativeLevelEdit::ReplaceSprite {
            index: 0,
            token: SpriteToken::Record(SpriteRecord {
                encoded: vec![0, 0, 9],
            }),
        }])
        .unwrap();
    assert!(controller.can_undo());
    assert!(!controller.can_redo());
    assert!(controller.is_modified());

    assert!(controller.undo());
    assert_eq!(controller.level().sprites, baseline.sprites);
    assert!(controller.undo());
    assert_eq!(*controller.level(), baseline);
    assert!(!controller.is_modified());
    assert!(controller.can_redo());

    assert!(controller.redo());
    controller
        .apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
            index: 0,
            record: ObjectRecord::new(vec![6, 5, 4]).unwrap(),
        }])])
        .unwrap();
    assert!(!controller.can_redo());
    assert!(controller.undo());
    assert_eq!(controller.level().layer1.objects, baseline.layer1.objects);
}

#[test]
fn sprite_encoded_lengths_follow_staged_history_and_exact_record_table() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    assert_eq!(controller.sprite_encoded_lengths().unwrap(), (5, 5));

    controller
        .apply_edits(&[NativeLevelEdit::InsertSprite {
            index: 1,
            token: SpriteToken::Record(SpriteRecord {
                encoded: vec![0, 0, 4],
            }),
        }])
        .unwrap();
    assert_eq!(controller.sprite_encoded_lengths().unwrap(), (5, 8));
    assert!(controller.undo());
    assert_eq!(controller.sprite_encoded_lengths().unwrap(), (5, 5));
    assert!(controller.redo());
    assert_eq!(controller.sprite_encoded_lengths().unwrap(), (5, 8));
}

#[test]
fn complete_screen_exit_table_is_one_undo_step_and_reopens_after_commit() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    let baseline = controller.level().layer1.objects.clone();
    let mut exits = [None; 32];
    exits[0] = Some(0x1000);
    exits[0x1f] = Some(0xffff);
    controller
        .apply_edits(&[NativeLevelEdit::Objects(vec![
            ObjectEdit::ReplaceScreenExitTable { exits },
        ])])
        .unwrap();
    assert!(controller.can_undo());
    assert!(controller.undo());
    assert_eq!(controller.level().layer1.objects, baseline);
    assert!(!controller.can_undo());
    assert!(controller.redo());

    let prepared = controller
        .prepare_commit("replace complete screen-exit table", &options())
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    let reopened = LevelController::decode(
        &app.controller_snapshot().unwrap(),
        layout(),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    let table = reopened
        .level()
        .layer1
        .objects
        .records
        .iter()
        .filter_map(ObjectRecord::screen_exit)
        .map(|exit| (exit.screen, exit.destination_and_flags))
        .collect::<Vec<_>>();
    assert_eq!(table, [(0, 0x1400), (0x1f, 0xffff)]);
}

#[test]
fn failed_and_noop_staged_edits_do_not_create_history() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    let original = controller.level().clone();

    controller.apply_edits(&[]).unwrap();
    assert!(!controller.can_undo());
    assert!(
        controller
            .apply_edits(&[NativeLevelEdit::RemoveSprite { index: 99 }])
            .is_err()
    );
    assert_eq!(*controller.level(), original);
    assert!(!controller.can_undo());
}

#[test]
fn sprite_group_transactions_commit_once_track_order_and_undo_atomically() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    controller
        .apply_edits(&[NativeLevelEdit::InsertSprite {
            index: 1,
            token: SpriteToken::Record(SpriteRecord {
                encoded: vec![0x10, 0x11, 2],
            }),
        }])
        .unwrap();
    let before_group = controller.level().sprites.clone();
    controller
        .apply_edits(&[NativeLevelEdit::DuplicateSpriteGroup {
            selected: vec![1, 0],
            major_delta: 16,
            minor_delta: 1,
        }])
        .unwrap();
    assert_eq!(controller.level().sprites.native_placements().len(), 4);
    assert!(controller.undo());
    assert_eq!(controller.level().sprites, before_group);
    assert!(controller.redo());
    let duplicated = controller.level().sprites.clone();
    controller
        .apply_edits(&[NativeLevelEdit::RelocateSpriteGroup {
            selected: vec![2, 3],
            major_delta: -1,
            minor_delta: 1,
        }])
        .unwrap();
    assert_ne!(controller.level().sprites, duplicated);
    assert!(controller.undo());
    assert_eq!(controller.level().sprites, duplicated);

    let before_failure = controller.level().clone();
    assert!(
        controller
            .apply_edits(&[NativeLevelEdit::RelocateSpriteGroup {
                selected: vec![0, 1],
                major_delta: -512,
                minor_delta: 0,
            }])
            .is_err()
    );
    assert_eq!(controller.level(), &before_failure);
}

#[test]
fn owned_level_commit_reclaims_both_snapshot_streams_and_undo_restores_them() {
    let (rom, manifest) = tagged_test_rom();
    let mut app = AppState::default();
    app.load_rom(rom).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    assert_eq!(
        controller.previous_layer1.as_ref(),
        Some(&manifest.owned[0])
    );
    assert_eq!(
        controller.previous_sprites.as_ref(),
        Some(&manifest.owned[1])
    );
    controller
        .apply_edits(&[
            NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
                index: 0,
                record: ObjectRecord::new(vec![6, 5, 4]).unwrap(),
            }]),
            NativeLevelEdit::ReplaceSprite {
                index: 0,
                token: SpriteToken::Record(SpriteRecord {
                    encoded: vec![0, 0, 9],
                }),
            },
        ])
        .unwrap();
    let prepared = controller
        .prepare_commit_with_reclamation("Owned level edit", &options(), &manifest)
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    for block in &manifest.owned {
        assert!(
            app.project().unwrap().rom.logical_bytes()[block.full_range()]
                .iter()
                .all(|byte| *byte == 0xff)
        );
    }
    assert_eq!(
        app.project()
            .unwrap()
            .load_level_slot(0x105, layout(), &SpriteLengthTable::standard())
            .unwrap(),
        *controller.level()
    );
    app.dispatch(Command::Undo).unwrap();
    for block in &manifest.owned {
        assert_eq!(
            lm_rats::parse_at(
                app.project().unwrap().rom.logical_bytes(),
                block.header_offset
            )
            .unwrap(),
            block.clone()
        );
    }
}

#[test]
fn late_edit_failure_and_stale_commit_preserve_app_and_controller() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    assert!(!controller.is_modified());
    assert!(
        controller
            .prepare_commit("unchanged", &options())
            .unwrap()
            .mutation
            .is_empty()
    );
    let original = controller.level().clone();
    controller
        .apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
            index: 0,
            record: ObjectRecord::new(vec![6, 5, 4]).unwrap(),
        }])])
        .unwrap();
    assert!(controller.is_modified());
    controller
        .apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
            index: 0,
            record: original.layer1.objects.records[0].clone(),
        }])])
        .unwrap();
    assert!(!controller.is_modified());
    assert!(
        controller
            .prepare_commit("reverted", &options())
            .unwrap()
            .mutation
            .is_empty()
    );
    assert!(matches!(
        controller.apply_edits(&[
            NativeLevelEdit::SetSpriteHeader(7),
            NativeLevelEdit::RemoveSprite { index: 9 },
        ]),
        Err(LevelControllerError::SpriteEdit { command: 1, .. })
    ));
    assert_eq!(controller.level(), &original);
    assert!(matches!(
        controller.apply_edits(&[NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
            index: 0,
            record: ObjectRecord::new(vec![1, 0, 1, 2]).unwrap(),
        }])]),
        Err(LevelControllerError::InvalidObjectEncoding(_))
    ));
    assert_eq!(controller.level(), &original);
    assert!(matches!(
        controller.apply_edits(&[NativeLevelEdit::ReplaceSprite {
            index: 0,
            token: SpriteToken::Record(SpriteRecord {
                encoded: vec![0, 0, 1, 2],
            }),
        }]),
        Err(LevelControllerError::InvalidSpriteSerialization(
            lm_level::NativeSpriteEncodingError::RecordLengthMismatch {
                token: 0,
                expected: 3,
                actual: 4,
            }
        ))
    ));
    assert_eq!(controller.level(), &original);
    let prepared = controller.prepare_commit("stale", &options()).unwrap();
    app.dispatch(Command::CommitRomWrites {
        expected_revision: 0,
        description: "newer".into(),
        writes: vec![lm_project::RomWrite {
            offset: 1,
            bytes: vec![7],
        }],
    })
    .unwrap();
    assert!(matches!(
        app.dispatch(prepared.into_command()),
        Err(AppError::StaleProjectRevision {
            expected: 0,
            actual: 1,
        })
    ));
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
}

#[test]
fn mode_mapper_and_legacy_token_mismatches_are_rejected() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let wrong_mode = app.controller_snapshot().unwrap();
    assert!(matches!(
        LevelController::decode(&wrong_mode, layout(), &SpriteLengthTable::standard()),
        Err(LevelControllerError::WrongMode(EditorMode::Map16))
    ));
    app.dispatch(Command::SelectLevel(0x105)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut bad_layout = layout();
    bad_layout.mapper = Mapper::Sa1;
    assert!(matches!(
        LevelController::decode(&snapshot, bad_layout, &SpriteLengthTable::standard()),
        Err(LevelControllerError::MapperMismatch { .. })
    ));
    let mut controller =
        LevelController::decode(&snapshot, layout(), &SpriteLengthTable::standard()).unwrap();
    let original = controller.level().clone();
    assert!(matches!(
        controller.apply_edits(&[NativeLevelEdit::ReplaceSprite {
            index: 0,
            token: SpriteToken::Control(0x90),
        }]),
        Err(LevelControllerError::SpriteEdit { .. })
    ));
    assert_eq!(controller.level(), &original);
}
