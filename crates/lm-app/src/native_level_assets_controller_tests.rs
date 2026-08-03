use super::*;
use crate::PaletteControllerEdit;
use lm_codec::encode_terminated_rle;
use lm_graphics::{
    Bgr555, ExAnimationFeature, Palette, PaletteChange, PaletteEntryOwner, PaletteInterchangeFile,
};
use lm_level::{
    CustomTimeSettings, Layer1VerticalScrollMode, LegacyHeaderEdit, MwlFile, MwlLayer2Descriptor,
    MwlLevelHeaderSection, NATIVE_LAYER2_TILEMAP_LEN, NativeSpriteStream, ObjectRecord,
    ObjectStream, SpriteRecord, SpriteToken, split_layer2_tilemap_planes,
};
use lm_project::{
    ExAnimationRomLayout, ExAnimationSaveOptions, ExpandedLevelSettingsLayout,
    LevelLayer2DescriptorTable, LevelLayer2RomLayout, LevelLayer2SaveOptions,
    LevelLayer2TilemapEncoding, LevelPointerTable, LevelRomLayout, LevelSaveOptions,
    NativeLevelAssetsLayer2Layout, NativeLevelAssetsLayer2SaveOptions,
    NativeLevelAssetsSaveOptions, PaletteRomLayout, PaletteSaveOptions, RatsOwnershipManifest,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{SnesChecksum, compute_snes_checksum, detect_identity, pc_to_snes};

fn table(offset: usize) -> LevelPointerTable {
    LevelPointerTable {
        offset,
        entries: 1,
        stride: 3,
    }
}

fn layout() -> NativeLevelAssetsLayout {
    NativeLevelAssetsLayout {
        level: LevelRomLayout {
            mapper: Mapper::LoRom,
            layer1: table(0x20),
            sprites: table(0x30).into(),
            expanded_sprites: false,
        },
        palette: PaletteRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(0x40),
            colors_per_palette: 2,
        },
        exanimation: ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(0x50),
            maximum_records: 8,
            maximum_encoded_len: 0x100,
        },
        expanded_settings: Some(ExpandedLevelSettingsLayout {
            mapper: Mapper::LoRom,
            table_offset: 0x60,
            entries: 1,
            stride: 32,
        }),
    }
}

fn layer2_layout() -> LevelLayer2RomLayout {
    LevelLayer2RomLayout {
        mapper: Mapper::LoRom,
        pointers: table(0x90),
        background_bank_substitution: None,
        legacy_pointer_redirect: None,
        descriptor_table: None,
        maximum_compressed_len: 0x100,
        tilemap_encoding: LevelLayer2TilemapEncoding::SplitPlanes,
    }
}

fn pointer(bytes: &mut [u8], offset: usize, target: usize) {
    let value = pc_to_snes(Mapper::LoRom, target).unwrap().to_le_bytes();
    bytes[offset..offset + 3].copy_from_slice(&value[..3]);
}

fn snapshot() -> ControllerSnapshot {
    let mut bytes = vec![0xff; 0x8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
    pointer(&mut bytes, 0x20, 0x100);
    pointer(&mut bytes, 0x30, 0x120);
    pointer(&mut bytes, 0x40, 0x140);
    pointer(&mut bytes, 0x50, 0x160);
    pointer(&mut bytes, 0x90, 0x190);
    bytes[0x100..0x109].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 0xff]);
    bytes[0x120..0x125].copy_from_slice(&[0x10, 0, 1, 2, 0xff]);
    bytes[0x140..0x144].copy_from_slice(&[1, 0, 2, 0]);
    bytes[0x160..0x187].fill(0);
    bytes[0x190..0x199].copy_from_slice(&[1, 2, 3, 4, 5, 0x01, 0x10, 0x20, 0xff]);
    bytes[0x60..0x80].fill(0x5a);
    let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    let image = RomImage::from_bytes(bytes.clone()).unwrap();
    ControllerSnapshot {
        revision: 7,
        mode: EditorMode::Level(0),
        identity: detect_identity(&image).unwrap(),
        document_path: None,
        rom_bytes: bytes,
    }
}

fn tilemap_snapshot() -> ControllerSnapshot {
    let mut snapshot = snapshot();
    snapshot.rom_bytes[0x101] &= 0xe0;
    let tilemap = vec![0_u8; NATIVE_LAYER2_TILEMAP_LEN];
    let encoded =
        encode_terminated_rle(&split_layer2_tilemap_planes(&tilemap).expect("valid tilemap"));
    snapshot.rom_bytes[0x190..0x190 + encoded.len()].copy_from_slice(&encoded);
    let checksum = compute_snes_checksum(&snapshot.rom_bytes, 0x7fdc).unwrap();
    snapshot.rom_bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    snapshot.identity =
        detect_identity(&RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap()).unwrap();
    snapshot
}

fn options() -> NativeLevelAssetsSaveOptions {
    let allocation = AllocationPolicy {
        search: 0x200..0x7000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x20..0x53),
            ProtectedRange(0x60..0x80),
            ProtectedRange(0x90..0x93),
            ProtectedRange(0xb0..0xb1),
            ProtectedRange(0x7fc0..0x8000),
        ],
    };
    NativeLevelAssetsSaveOptions {
        level: LevelSaveOptions {
            layer1_allocation: allocation.clone(),
            sprite_allocation: allocation.clone(),
            previous_layer1: None,
            previous_sprites: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        palette: PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        exanimation: ExAnimationSaveOptions {
            allocation,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    }
}

fn feature_installation() -> InstalledLayout<InstalledExAnimationFeatureRomLayout> {
    InstalledLayout::Unconditional(InstalledExAnimationFeatureRomLayout {
        table_locator: lm_project::ChainedSnesPointerLocator {
            mapper: Mapper::LoRom,
            first_operand_offset: 0xb1,
            final_operand_displacement: 0x46,
        },
    })
}

fn feature_snapshot() -> ControllerSnapshot {
    let mut snapshot = snapshot();
    pointer(&mut snapshot.rom_bytes, 0xb1, 0x300);
    pointer(&mut snapshot.rom_bytes, 0x346, 0x501);
    snapshot.rom_bytes[0x500] = 0;
    snapshot.rom_bytes[0x501] = 0xa5;
    let checksum = compute_snes_checksum(&snapshot.rom_bytes, 0x7fdc).unwrap();
    snapshot.rom_bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    snapshot.identity =
        detect_identity(&RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap()).unwrap();
    snapshot
}

fn layer2_options() -> LevelLayer2SaveOptions {
    LevelLayer2SaveOptions {
        allocation: options().level.layer1_allocation,
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    }
}

fn tagged_snapshot() -> (ControllerSnapshot, RatsOwnershipManifest) {
    let original = snapshot();
    let mut project = Project::new(RomImage::from_bytes(original.rom_bytes).unwrap());
    let assets = project
        .load_native_level_assets(0, layout(), &SpriteLengthTable::standard(), &[false; 256])
        .unwrap();
    let saved = project
        .save_native_level_assets(
            assets.as_save_assets(),
            layout(),
            &SpriteLengthTable::standard(),
            &[false; 256],
            0x7fdc,
            &options(),
        )
        .unwrap();
    let bytes = project.save_snapshot();
    let image = RomImage::from_bytes(bytes.clone()).unwrap();
    (
        ControllerSnapshot {
            revision: 7,
            mode: EditorMode::Level(0),
            identity: detect_identity(&image).unwrap(),
            document_path: None,
            rom_bytes: bytes,
        },
        RatsOwnershipManifest {
            owned: vec![
                saved.layer1.block,
                saved.sprites.block,
                saved.palette.block,
                saved.exanimation.block,
            ],
            retained: Vec::new(),
        },
    )
}

fn tagged_layer2_snapshot() -> (ControllerSnapshot, RatsOwnershipManifest) {
    let original = snapshot();
    let mut project = Project::new(RomImage::from_bytes(original.rom_bytes).unwrap());
    let layout_with_layer2 = NativeLevelAssetsLayer2Layout {
        core: layout(),
        layer2: layer2_layout(),
    };
    let assets = project
        .load_native_level_assets_with_layer2(
            0,
            layout_with_layer2,
            &SpriteLengthTable::standard(),
            &[false; 256],
        )
        .unwrap();
    let saved = project
        .save_native_level_assets_with_layer2(
            assets.as_save_assets(),
            layout_with_layer2,
            &SpriteLengthTable::standard(),
            &[false; 256],
            0x7fdc,
            &NativeLevelAssetsLayer2SaveOptions {
                core: options(),
                layer2: layer2_options(),
            },
        )
        .unwrap();
    let bytes = project.save_snapshot();
    let image = RomImage::from_bytes(bytes.clone()).unwrap();
    (
        ControllerSnapshot {
            revision: 7,
            mode: EditorMode::Level(0),
            identity: detect_identity(&image).unwrap(),
            document_path: None,
            rom_bytes: bytes,
        },
        RatsOwnershipManifest {
            owned: vec![
                saved.core.layer1.block,
                saved.core.sprites.block,
                saved.core.palette.block,
                saved.core.exanimation.block,
                saved.layer2.block,
            ],
            retained: Vec::new(),
        },
    )
}

fn mwl_source(controller: &NativeLevelAssetsController) -> MwlNativeLevel {
    let mut header = MwlLevelHeaderSection([0; MwlLevelHeaderSection::ENCODED_LEN]);
    header.set_level_number(u16::try_from(controller.assets().level.number).unwrap());
    MwlNativeLevel {
        version: MwlFile::CURRENT_VERSION,
        flags: 0,
        attribution: [0; MwlFile::ATTRIBUTION_LEN],
        header,
        layer1_metadata: [0; 2],
        layer1: controller.assets().level.layer1.clone(),
        layer2_descriptor: controller
            .layer2_descriptor()
            .unwrap_or_else(|| MwlLayer2Descriptor::from_raw(0)),
        layer2_source_address: 0,
        layer2: controller.layer2().unwrap().clone(),
        sprite_metadata: [0; 2],
        sprites: controller.assets().level.sprites.clone(),
        palette_metadata: [0; 2],
        palette: controller.assets().palette.clone(),
        secondary_exit_metadata: [0; 2],
        secondary_exits: Vec::new(),
        exanimation_metadata: [0; 2],
        exanimation: Some(controller.assets().exanimation.clone()),
        expanded_settings: controller.assets().expanded_settings.clone(),
    }
}

#[test]
fn complete_mwl_modeled_assets_stage_commit_and_reopen_together() {
    let snapshot = snapshot();
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let mut source = mwl_source(&controller);
    source.layer1.header.set_level_mode(3).unwrap();
    source.palette.colors[1] = Bgr555(0x1234);
    source.exanimation.as_mut().unwrap().setting = 7;
    source
        .expanded_settings
        .as_mut()
        .unwrap()
        .set_word(4, 0xabcd)
        .unwrap();
    let NativeLayer2Data::Objects(layer2) = &mut source.layer2 else {
        panic!("fixture requires object Layer 2");
    };
    layer2
        .objects
        .records
        .push(layer2.objects.records[0].clone());

    controller.replace_modeled_assets_from_mwl(&source).unwrap();
    let expected_core = controller.assets().clone();
    let expected_layer2 = controller.layer2().unwrap().clone();
    let prepared = controller
        .prepare_commit_with_layer2("import modeled MWL assets", &options(), &layer2_options())
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("import modeled MWL assets", &prepared.mutation)
        .unwrap();
    let reopened = project
        .load_native_level_assets_with_layer2(
            0,
            NativeLevelAssetsLayer2Layout {
                core: layout(),
                layer2: layer2_layout(),
            },
            &SpriteLengthTable::standard(),
            &[false; 256],
        )
        .unwrap();
    assert_eq!(reopened.core, expected_core);
    assert_eq!(reopened.layer2, expected_layer2);
}

#[test]
fn reserved_mode_mwl_import_canonicalizes_before_tilemap_storage_commit() {
    let snapshot = tilemap_snapshot();
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let mut source = mwl_source(&controller);
    assert!(matches!(source.layer2, NativeLayer2Data::Tilemap(_)));
    source.layer1.header.set_background_color(6).unwrap();
    source.layer1.header.set_level_mode(0x12).unwrap();

    controller.replace_modeled_assets_from_mwl(&source).unwrap();
    assert_eq!(controller.normalized_reserved_level_mode(), Some(0x12));
    assert_eq!(controller.assets().level.layer1.header.level_mode(), 0);
    assert_eq!(
        controller.assets().level.layer1.header.background_color(),
        6
    );
    let expected_core = controller.assets().clone();
    let expected_layer2 = controller.layer2().unwrap().clone();
    let prepared = controller
        .prepare_commit_with_layer2("import reserved MWL mode", &options(), &layer2_options())
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("import reserved MWL mode", &prepared.mutation)
        .unwrap();
    let reopened = project
        .load_native_level_assets_with_layer2(
            0,
            NativeLevelAssetsLayer2Layout {
                core: layout(),
                layer2: layer2_layout(),
            },
            &SpriteLengthTable::standard(),
            &[false; 256],
        )
        .unwrap();
    assert_eq!(reopened.core, expected_core);
    assert_eq!(reopened.layer2, expected_layer2);
    assert_eq!(reopened.core.level.layer1.header.level_mode(), 0);
    assert_eq!(reopened.core.level.layer1.header.background_color(), 6);
}

#[test]
fn mwl_import_ignores_screen_exits_and_preserves_raw_layer1_order() {
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot(),
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let mut source = mwl_source(&controller);
    source.sprites = NativeSpriteStream {
        header: 0,
        expanded: false,
        tokens: Vec::new(),
    };
    source.layer1.header.set_last_screen(31).unwrap();
    source.layer1.objects = ObjectStream {
        records: vec![ObjectRecord::new(vec![0x9f, 0, 2, 0, 4]).unwrap()],
    };

    controller.replace_modeled_assets_from_mwl(&source).unwrap();

    assert_eq!(controller.assets().level.layer1.header.last_screen(), 0);
    assert!(!controller.assets().level.layer1.objects.records[0].advances_screen());

    let mut backward = mwl_source(&controller);
    backward.layer1.objects = ObjectStream {
        records: vec![
            ObjectRecord::new(vec![31, 0, 1]).unwrap(),
            ObjectRecord::new(vec![1, 0x10, 0]).unwrap(),
            ObjectRecord::new(vec![0, 0, 1]).unwrap(),
            ObjectRecord::new(vec![2, 0x11, 0]).unwrap(),
        ],
    };
    let expected_order = backward.layer1.objects.clone();

    controller
        .replace_modeled_assets_from_mwl(&backward)
        .unwrap();

    assert_eq!(controller.assets().level.layer1.header.last_screen(), 31);
    assert_eq!(controller.assets().level.layer1.objects, expected_order);

    let mut out_of_range = mwl_source(&controller);
    out_of_range.layer1.objects = ObjectStream {
        records: vec![
            ObjectRecord::new(vec![0x1f, 0x0f, 1]).unwrap(),
            ObjectRecord::new(vec![1, 0x10, 0]).unwrap(),
        ],
    };
    let expected_out_of_range = out_of_range.layer1.objects.clone();

    controller
        .replace_modeled_assets_from_mwl(&out_of_range)
        .unwrap();

    assert_eq!(controller.assets().level.layer1.header.last_screen(), 0);
    assert_eq!(
        controller.assets().level.layer1.objects,
        expected_out_of_range
    );

    out_of_range.layer1.objects.records[1]
        .set_advances_screen(true)
        .unwrap();
    let expected_wrapped_advance = out_of_range.layer1.objects.clone();
    controller
        .replace_modeled_assets_from_mwl(&out_of_range)
        .unwrap();
    assert_eq!(controller.assets().level.layer1.header.last_screen(), 0x11);
    assert_eq!(
        controller.assets().level.layer1.objects,
        expected_wrapped_advance
    );

    let expected_core = controller.assets().clone();
    let prepared = controller
        .prepare_commit_with_layer2("import extent and raw order", &options(), &layer2_options())
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot().rom_bytes).unwrap());
    project
        .apply_mutation("import extent and raw order", &prepared.mutation)
        .unwrap();
    let reopened = project
        .load_native_level_assets_with_layer2(
            0,
            NativeLevelAssetsLayer2Layout {
                core: layout(),
                layer2: layer2_layout(),
            },
            &SpriteLengthTable::standard(),
            &[false; 256],
        )
        .unwrap();
    assert_eq!(reopened.core, expected_core);
}

#[test]
fn mwl_staging_preflight_failures_preserve_every_controller_domain() {
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot(),
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let before_core = controller.assets().clone();
    let before_layer2 = controller.layer2().cloned();
    let before_descriptor = controller.layer2_descriptor();
    let mut source = mwl_source(&controller);
    source.palette.colors.push(Bgr555(0x7777));
    assert!(matches!(
        controller.replace_modeled_assets_from_mwl(&source),
        Err(NativeLevelAssetsControllerError::MwlPaletteShape {
            expected: 2,
            actual: 3
        })
    ));
    assert_eq!(controller.assets(), &before_core);
    assert_eq!(controller.layer2(), before_layer2.as_ref());
    assert_eq!(controller.layer2_descriptor(), before_descriptor);
    assert!(!controller.is_modified());
}

#[test]
fn complete_mwl_import_rejects_missing_lfix3_before_producing_a_mutation() {
    let snapshot = snapshot();
    let controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let source = mwl_source(&controller);

    assert!(matches!(
        controller.prepare_smw_us_v1_installed_mwl_import(&source, &options(), &layer2_options()),
        Err(NativeLevelAssetsControllerError::MwlLfix3Unavailable)
    ));
    assert!(!controller.is_modified());
    assert_eq!(controller.revision(), snapshot.revision);
}

#[test]
fn mixed_edits_prepare_one_checksum_valid_semantically_reopenable_commit() {
    let snapshot = snapshot();
    let mut controller = NativeLevelAssetsController::decode(
        &snapshot,
        layout(),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    controller
        .apply_edits(&[
            NativeLevelAssetsControllerEdit::Palette(vec![PaletteControllerEdit::ApplyChanges(
                vec![PaletteChange {
                    index: 1,
                    color: Bgr555(0x1234),
                }],
            )]),
            NativeLevelAssetsControllerEdit::ExAnimation(vec![
                ExAnimationControllerEdit::SetSetting(3),
            ]),
            NativeLevelAssetsControllerEdit::ExpandedSettingsWords(vec![(4, 0xabcd)]),
        ])
        .unwrap();
    let expected = controller.assets().clone();
    let prepared = controller
        .prepare_commit("mixed native assets", &options())
        .unwrap();
    assert_eq!(prepared.expected_revision, 7);
    assert!(!prepared.mutation.is_empty());
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("commit", &prepared.mutation)
        .unwrap();
    let reopened = project
        .load_native_level_assets(0, layout(), &SpriteLengthTable::standard(), &[false; 256])
        .unwrap();
    assert_eq!(reopened, expected);
    assert_eq!(
        SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc).unwrap(),
        compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap()
    );
}

#[test]
fn aggregate_decode_stages_reserved_level_mode_fallback_without_losing_background_color() {
    let mut snapshot = snapshot();
    snapshot.rom_bytes[0x101] = 0xd2;
    let checksum = compute_snes_checksum(&snapshot.rom_bytes, 0x7fdc).unwrap();
    snapshot.rom_bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    snapshot.identity =
        detect_identity(&RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap()).unwrap();

    let controller = NativeLevelAssetsController::decode(
        &snapshot,
        layout(),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    assert!(controller.is_modified());
    assert_eq!(controller.normalized_reserved_level_mode(), Some(0x12));
    assert_eq!(controller.assets().level.layer1.header.level_mode(), 0);
    assert_eq!(
        controller.assets().level.layer1.header.background_color(),
        6
    );
}

#[test]
fn aggregate_level_mode_storage_change_is_explicit_failure_atomic_and_resettable() {
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot(),
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let original_assets = controller.assets().clone();
    let original_layer2 = controller.layer2().cloned();
    let edits = [NativeLevelAssetsControllerEdit::Level(vec![
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(0)),
    ])];

    assert!(matches!(
        controller.apply_edits(&edits),
        Err(
            NativeLevelAssetsControllerError::Layer2ModeChangeRequiresReset {
                command: 0,
                from: 2,
                to: 0,
            }
        )
    ));
    assert_eq!(controller.assets(), &original_assets);
    assert_eq!(controller.layer2().cloned(), original_layer2);

    controller
        .apply_edits_with_layer2_reset(&edits, true)
        .unwrap();
    assert_eq!(controller.assets().level.layer1.header.level_mode(), 0);
    assert!(matches!(
        controller.layer2(),
        Some(NativeLayer2Data::Tilemap(bytes)) if bytes == &vec![0; NATIVE_LAYER2_TILEMAP_LEN]
    ));
    controller
        .apply_edits_with_layer2_reset(
            &[NativeLevelAssetsControllerEdit::Level(vec![
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(2)),
            ])],
            true,
        )
        .unwrap();
    assert_eq!(controller.layer2().cloned(), original_layer2);
}

#[test]
fn aggregate_mode_storage_changes_apply_recovered_installed_descriptor_masks() {
    let mut snapshot = snapshot();
    snapshot.rom_bytes[0xb0] = 0xd5;
    let checksum = compute_snes_checksum(&snapshot.rom_bytes, 0x7fdc).unwrap();
    snapshot.rom_bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    let mut installed_layout = layer2_layout();
    installed_layout.descriptor_table = Some(LevelLayer2DescriptorTable {
        offset: 0xb0,
        entries: 1,
        stride: 1,
    });
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(installed_layout),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    assert_eq!(controller.layer2_descriptor().unwrap().raw(), 0xd5);

    for (mode, descriptor) in [(0, 0xda), (2, 0xc0)] {
        controller
            .apply_edits_with_layer2_reset(
                &[NativeLevelAssetsControllerEdit::Level(vec![
                    NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(mode)),
                ])],
                true,
            )
            .unwrap();
        assert_eq!(controller.layer2_descriptor().unwrap().raw(), descriptor);
    }
}

#[test]
fn complete_installed_header_batch_resets_layer2_and_reopens_losslessly() {
    let snapshot = snapshot();
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let edits = [NativeLevelAssetsControllerEdit::Level(vec![
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundPalette(1)),
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LastScreen(2)),
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(0)),
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundColor(3)),
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpriteTileset(4)),
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::DefaultMusicSelector(5)),
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::TimeLimitSelector(2)),
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpritePalette(6)),
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ForegroundPalette(7)),
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ObjectTileset(8)),
        NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::Layer1VerticalScroll(
            Layer1VerticalScrollMode::NoScrollAtBottomUnlessFlying,
        )),
        NativeLevelEdit::SetCustomTime(Some(CustomTimeSettings::new(0xabc, true).unwrap())),
    ])];
    controller
        .apply_edits_with_layer2_reset(&edits, true)
        .unwrap();
    let expected_core = controller.assets().clone();
    let expected_layer2 = controller.layer2().unwrap().clone();
    let prepared = controller
        .prepare_commit_with_layer2("complete installed header", &options(), &layer2_options())
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("complete installed header", &prepared.mutation)
        .unwrap();
    let reopened = project
        .load_native_level_assets_with_layer2(
            0,
            NativeLevelAssetsLayer2Layout {
                core: layout(),
                layer2: layer2_layout(),
            },
            &SpriteLengthTable::standard(),
            &[false; 256],
        )
        .unwrap();
    assert_eq!(reopened.core, expected_core);
    assert_eq!(reopened.layer2, expected_layer2);
}

#[test]
fn owned_aggregate_reclaims_four_payloads_keeps_direct_write_atomic_and_undoes() {
    let (snapshot, manifest) = tagged_snapshot();
    let mut controller = NativeLevelAssetsController::decode(
        &snapshot,
        layout(),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    assert_eq!(
        controller.previous_blocks,
        [
            Some(manifest.owned[0].clone()),
            Some(manifest.owned[1].clone()),
            Some(manifest.owned[2].clone()),
            Some(manifest.owned[3].clone()),
            None,
        ]
    );
    controller
        .apply_edits(&[
            NativeLevelAssetsControllerEdit::Level(vec![
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(3)),
                NativeLevelEdit::ReplaceSprite {
                    index: 0,
                    token: SpriteToken::Record(SpriteRecord {
                        encoded: vec![0, 1, 9],
                    }),
                },
            ]),
            NativeLevelAssetsControllerEdit::Palette(vec![PaletteControllerEdit::ApplyChanges(
                vec![PaletteChange {
                    index: 1,
                    color: Bgr555(0x1234),
                }],
            )]),
            NativeLevelAssetsControllerEdit::ExAnimation(vec![
                ExAnimationControllerEdit::SetSetting(3),
            ]),
            NativeLevelAssetsControllerEdit::ExpandedSettingsWords(vec![(4, 0xabcd)]),
        ])
        .unwrap();
    let expected = controller.assets().clone();
    let prepared = controller
        .prepare_commit_with_reclamation("owned native assets", &options(), &manifest)
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("commit", &prepared.mutation)
        .unwrap();
    for block in &manifest.owned {
        assert!(
            project.rom.logical_bytes()[block.full_range()]
                .iter()
                .all(|byte| *byte == 0xff)
        );
    }
    assert_eq!(
        project
            .load_native_level_assets(0, layout(), &SpriteLengthTable::standard(), &[false; 256])
            .unwrap(),
        expected
    );
    project.undo().unwrap();
    for block in &manifest.owned {
        assert_eq!(
            lm_rats::parse_at(project.rom.logical_bytes(), block.header_offset).unwrap(),
            block.clone()
        );
    }
    assert_eq!(project.rom.read(0x60, 32).unwrap(), &[0x5a; 32]);
}

#[test]
fn layer2_object_edit_commits_with_every_core_domain_and_reopens() {
    let snapshot = snapshot();
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let NativeLayer2Data::Objects(layer2) = controller.layer2().unwrap() else {
        panic!("fixture level mode must use Layer 2 objects");
    };
    let duplicate = layer2.objects.records[0].clone();
    controller
        .apply_edits(&[
            NativeLevelAssetsControllerEdit::Layer2Objects(vec![ObjectEdit::Insert {
                index: 1,
                record: duplicate,
            }]),
            NativeLevelAssetsControllerEdit::ExpandedSettingsWords(vec![(1, 0x1234)]),
        ])
        .unwrap();
    let expected_core = controller.assets().clone();
    let expected_layer2 = controller.layer2().unwrap().clone();
    let prepared = controller
        .prepare_commit_with_layer2("Layer 2 aggregate", &options(), &layer2_options())
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("Layer 2 aggregate", &prepared.mutation)
        .unwrap();
    let reopened = project
        .load_native_level_assets_with_layer2(
            0,
            NativeLevelAssetsLayer2Layout {
                core: layout(),
                layer2: layer2_layout(),
            },
            &SpriteLengthTable::standard(),
            &[false; 256],
        )
        .unwrap();
    assert_eq!(reopened.core, expected_core);
    assert_eq!(reopened.layer2, expected_layer2);
    assert_eq!(
        SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc).unwrap(),
        compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap()
    );
}

#[test]
fn layer2_storage_and_late_tile_failures_are_atomic() {
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot(),
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let before_core = controller.assets().clone();
    let before_layer2 = controller.layer2().unwrap().clone();
    assert!(matches!(
        controller.apply_edits(&[
            NativeLevelAssetsControllerEdit::ExpandedSettingsWords(vec![(1, 0x1234)]),
            NativeLevelAssetsControllerEdit::Layer2TilemapWords(vec![(0, 0xbeef)]),
        ]),
        Err(NativeLevelAssetsControllerError::Layer2StorageMismatch {
            command: 1,
            expected: "tilemap"
        })
    ));
    assert_eq!(controller.assets(), &before_core);
    assert_eq!(controller.layer2(), Some(&before_layer2));
    assert!(!controller.is_modified());
}

#[test]
fn layer2_tilemap_words_are_little_endian_atomic_and_reopenable() {
    let snapshot = tilemap_snapshot();
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    assert!(matches!(
        controller.layer2(),
        Some(NativeLayer2Data::Tilemap(_))
    ));
    controller
        .apply_edits(&[NativeLevelAssetsControllerEdit::Layer2TilemapWords(vec![
            (0, 0x1234),
            (0x3ff, 0xabcd),
        ])])
        .unwrap();
    let expected = controller.layer2().unwrap().clone();
    let prepared = controller
        .prepare_commit_with_layer2("Layer 2 tilemap", &options(), &layer2_options())
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("Layer 2 tilemap", &prepared.mutation)
        .unwrap();
    assert_eq!(
        project.load_level_layer2(0, 0, layer2_layout()).unwrap(),
        expected
    );

    let before = controller.layer2().unwrap().clone();
    assert!(matches!(
        controller.apply_edits(&[NativeLevelAssetsControllerEdit::Layer2TilemapWords(vec![
            (4, 1),
            (4, 2)
        ])]),
        Err(NativeLevelAssetsControllerError::Layer2TileDuplicate {
            command: 0,
            index: 4
        })
    ));
    assert_eq!(controller.layer2(), Some(&before));
}

#[test]
fn layer2_remap_is_selection_scoped_reopenable_and_rejects_unmodeled_bank_changes() {
    let snapshot = tilemap_snapshot();
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    controller
        .apply_edits(&[NativeLevelAssetsControllerEdit::Layer2TilemapRemap {
            script: "8000,8001".into(),
            global_offset: 0,
            selection: Some(vec![0, 16]),
        }])
        .unwrap();
    let NativeLayer2Data::Tilemap(bytes) = controller.layer2().unwrap() else {
        panic!("mode zero must load a Layer 2 tilemap");
    };
    assert_eq!(&bytes[0..2], &[1, 0]);
    assert_eq!(&bytes[32..34], &[1, 0]);
    assert_eq!(&bytes[2..4], &[0, 0]);

    let expected = controller.layer2().unwrap().clone();
    let prepared = controller
        .prepare_commit_with_layer2("Layer 2 remap", &options(), &layer2_options())
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("Layer 2 remap", &prepared.mutation)
        .unwrap();
    assert_eq!(
        project.load_level_layer2(0, 0, layer2_layout()).unwrap(),
        expected
    );

    let before = controller.layer2().unwrap().clone();
    assert!(matches!(
        controller.apply_edits(&[NativeLevelAssetsControllerEdit::Layer2TilemapRemap {
            script: "8001,9000".into(),
            global_offset: 0,
            selection: Some(vec![0]),
        }]),
        Err(
            NativeLevelAssetsControllerError::Layer2RemapRequiresInstalledBank {
                command: 0,
                bank: 1
            }
        )
    ));
    assert_eq!(controller.layer2(), Some(&before));
}

#[test]
fn installed_layer2_descriptor_persists_cross_bank_remap_atomically() {
    let mut snapshot = tilemap_snapshot();
    snapshot.rom_bytes[0xb0] = 0x06;
    let checksum = compute_snes_checksum(&snapshot.rom_bytes, 0x7fdc).unwrap();
    snapshot.rom_bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    let mut installed_layout = layer2_layout();
    installed_layout.descriptor_table = Some(LevelLayer2DescriptorTable {
        offset: 0xb0,
        entries: 1,
        stride: 1,
    });
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(installed_layout),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    assert_eq!(controller.layer2_descriptor().unwrap().raw(), 0x06);
    controller
        .apply_edits(&[NativeLevelAssetsControllerEdit::Layer2TilemapRemap {
            script: "8000,9000".into(),
            global_offset: 0,
            selection: Some(vec![0]),
        }])
        .unwrap();
    assert_eq!(controller.layer2_descriptor().unwrap().raw(), 0x16);

    let expected = controller.layer2().unwrap().clone();
    let prepared = controller
        .prepare_commit_with_layer2("cross-bank Layer 2 remap", &options(), &layer2_options())
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes.clone()).unwrap());
    project
        .apply_mutation("cross-bank Layer 2 remap", &prepared.mutation)
        .unwrap();
    let reopened = project
        .load_level_layer2_with_descriptor(0, 0, installed_layout)
        .unwrap();
    assert_eq!(reopened.data, expected);
    assert_eq!(reopened.descriptor.unwrap().raw(), 0x16);
    assert_eq!(project.rom.logical_bytes()[0xb0], 0x16);
    assert_eq!(
        SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc).unwrap(),
        compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap()
    );
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.save_snapshot(), snapshot.rom_bytes);
}

#[test]
fn owned_layer2_aggregate_reclaims_all_five_payloads_atomically() {
    let (snapshot, manifest) = tagged_layer2_snapshot();
    let mut controller = NativeLevelAssetsController::decode_with_layer2(
        &snapshot,
        layout(),
        Some(layer2_layout()),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    assert_eq!(
        controller.previous_blocks,
        std::array::from_fn(|index| Some(manifest.owned[index].clone()))
    );
    let NativeLayer2Data::Objects(layer2) = controller.layer2().unwrap() else {
        panic!("fixture level mode must use Layer 2 objects");
    };
    let duplicate = layer2.objects.records[0].clone();
    controller
        .apply_edits(&[
            NativeLevelAssetsControllerEdit::Level(vec![
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundPalette(3)),
                NativeLevelEdit::ReplaceSprite {
                    index: 0,
                    token: SpriteToken::Record(SpriteRecord {
                        encoded: vec![0, 1, 9],
                    }),
                },
            ]),
            NativeLevelAssetsControllerEdit::Layer2Objects(vec![ObjectEdit::Insert {
                index: 1,
                record: duplicate,
            }]),
            NativeLevelAssetsControllerEdit::Palette(vec![PaletteControllerEdit::ApplyChanges(
                vec![PaletteChange {
                    index: 1,
                    color: Bgr555(0x1234),
                }],
            )]),
            NativeLevelAssetsControllerEdit::ExAnimation(vec![
                ExAnimationControllerEdit::SetSetting(3),
            ]),
        ])
        .unwrap();
    let expected = controller.layer2().unwrap().clone();
    let prepared = controller
        .prepare_commit_with_layer2_and_reclamation(
            "owned Layer 2 aggregate",
            &options(),
            &layer2_options(),
            &manifest,
        )
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("owned Layer 2 aggregate", &prepared.mutation)
        .unwrap();
    for block in &manifest.owned {
        assert!(
            project.rom.logical_bytes()[block.full_range()]
                .iter()
                .all(|byte| *byte == 0xff)
        );
    }
    assert_eq!(
        project.load_level_layer2(0, 2, layer2_layout()).unwrap(),
        expected
    );
}

#[test]
fn late_cross_domain_failure_rolls_back_the_complete_aggregate() {
    let mut controller = NativeLevelAssetsController::decode(
        &snapshot(),
        layout(),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let before = controller.assets().clone();
    assert!(
        controller
            .apply_edits(&[
                NativeLevelAssetsControllerEdit::ExAnimation(vec![
                    ExAnimationControllerEdit::SetSetting(9),
                ]),
                NativeLevelAssetsControllerEdit::ExpandedSettingsWords(vec![(16, 1)]),
            ])
            .is_err()
    );
    assert_eq!(controller.assets(), &before);
    assert!(!controller.is_modified());
}

#[test]
fn complete_palette_replacement_is_slot_agnostic_shape_checked_and_atomic() {
    let ownership =
        PaletteOwnership::from_owners(vec![PaletteEntryOwner::Editable, PaletteEntryOwner::Fixed]);
    let mut controller = NativeLevelAssetsController::decode(
        &snapshot(),
        layout(),
        &SpriteLengthTable::standard(),
        &[false; 256],
        ownership,
    )
    .unwrap();
    let baseline = controller.assets().clone();
    let imported = PaletteInterchangeFile {
        source_palette: 0x1ff,
        palette: Palette {
            colors: vec![Bgr555(0x1234), baseline.palette.colors[1]],
        },
    };
    controller.replace_palette_file(&imported).unwrap();
    assert_eq!(controller.assets().palette, imported.palette);
    let accepted = controller.assets().clone();

    let wrong_shape = PaletteInterchangeFile {
        source_palette: 0,
        palette: Palette { colors: vec![] },
    };
    assert!(controller.replace_palette_file(&wrong_shape).is_err());
    assert_eq!(controller.assets(), &accepted);

    let mut protected = imported;
    protected.palette.colors[1].0 ^= 1;
    assert!(matches!(
        controller.replace_palette_file(&protected),
        Err(NativeLevelAssetsControllerError::PaletteEdit { .. })
    ));
    assert_eq!(controller.assets(), &accepted);
}

#[test]
fn installed_animation_features_load_edit_commit_and_reopen_with_the_aggregate() {
    let snapshot = feature_snapshot();
    let mut controller = NativeLevelAssetsController::decode_with_layer2_and_features(
        &snapshot,
        layout(),
        None,
        feature_installation(),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let loaded = controller.exanimation_features().unwrap();
    assert_eq!(loaded.options.encode(), 0xa5);

    let mut edited = loaded.options;
    edited.set_enabled(ExAnimationFeature::PaletteAnimation, true);
    edited.set_enabled(ExAnimationFeature::VanillaAnimation, false);
    edited.set_enabled(ExAnimationFeature::GlobalExAnimation, true);
    edited.set_enabled(ExAnimationFeature::LevelExAnimation, false);
    controller
        .apply_edits(&[NativeLevelAssetsControllerEdit::ExAnimationFeatures(edited)])
        .unwrap();
    assert!(controller.is_modified());

    let prepared = controller
        .prepare_commit("animation feature aggregate", &options())
        .unwrap();
    let original = snapshot.rom_bytes;
    let mut project = Project::new(RomImage::from_bytes(original.clone()).unwrap());
    project
        .apply_mutation("animation feature aggregate", &prepared.mutation)
        .unwrap();
    let reopened = project
        .load_installed_exanimation_features(0, feature_installation())
        .unwrap();
    assert_eq!(reopened.options.encode(), 0x55);
    assert_eq!(
        SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc).unwrap(),
        compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap()
    );
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.logical_bytes(), original);
}
