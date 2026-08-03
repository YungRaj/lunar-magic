use super::*;
use lm_graphics::{Bgr555, CompactExAnimation, ExAnimationFrame, ExAnimationRecord, Palette};
use lm_level::{
    ExpandedLevelSettingsRecord, Layer3TilemapGraphicsDescriptor, MwlPayloadSection, ObjectEdit,
    ObjectRecord, SpriteLengthTable, SpriteRecord, SpriteToken,
};
use lm_project::MwlOptionalLevelAssets;

fn file() -> MwlFile {
    let mut sections: [MwlSection; MwlFile::SECTION_COUNT] =
        std::array::from_fn(|_| MwlSection::default());
    let mut level_header = [0x5a; MwlLevelHeaderSection::ENCODED_LEN];
    level_header[..2].copy_from_slice(&0x0105_u16.to_le_bytes());
    sections[MwlSectionKind::LevelHeader as usize].bytes = level_header.to_vec();
    sections[MwlSectionKind::Layer1 as usize].bytes = vec![1, 2, 3];
    MwlFile {
        version: MwlFile::CURRENT_VERSION,
        flags: 7,
        attribution: [0x20; MwlFile::ATTRIBUTION_LEN],
        sections,
    }
}

fn controller() -> MwlDocumentController {
    let file = file();
    MwlDocumentController::decode("level.mwl".into(), &file.encode().unwrap()).unwrap()
}

fn sprite_controller() -> MwlDocumentController {
    let mut source = file();
    source
        .set_payload_section(
            MwlSectionKind::Layer1,
            &MwlPayloadSection {
                metadata: [0x1020_3040, 0x5060_7080],
                payload: vec![1, 2, 3, 4, 5, 0x11, 0x22, 0x33, 0xff],
            },
        )
        .unwrap();
    source
        .set_payload_section(
            MwlSectionKind::Sprites,
            &MwlPayloadSection {
                metadata: [0x1122_3344, 0x5566_7788],
                payload: vec![4, 0x11, 0xd0, 0xbd, 0xff],
            },
        )
        .unwrap();
    MwlDocumentController::decode("level.mwl".into(), &source.encode().unwrap()).unwrap()
}

fn layer1_controller() -> MwlDocumentController {
    let mut source = file();
    source
        .set_payload_section(
            MwlSectionKind::Layer1,
            &MwlPayloadSection {
                metadata: [0x1020_3040, 0x5060_7080],
                payload: vec![1, 2, 3, 4, 5, 0x11, 0x22, 0x33, 0xff],
            },
        )
        .unwrap();
    MwlDocumentController::decode("level.mwl".into(), &source.encode().unwrap()).unwrap()
}

#[test]
fn typed_layer1_replacement_preserves_metadata_and_is_undoable() {
    let mut controller = layer1_controller();
    let original_sprites = controller.value().section(MwlSectionKind::Sprites).to_vec();
    let mut layer1 = controller.layer1().unwrap();
    layer1
        .objects
        .apply_edits(&[ObjectEdit::Insert {
            index: 1,
            record: ObjectRecord::new(vec![0x21, 0x44, 0x55]).unwrap(),
        }])
        .unwrap();

    controller.replace_layer1(0, &layer1).unwrap();

    assert_eq!(controller.layer1().unwrap(), layer1);
    assert_eq!(
        controller
            .value()
            .payload_section(MwlSectionKind::Layer1)
            .unwrap()
            .metadata,
        [0x1020_3040, 0x5060_7080]
    );
    assert_eq!(
        controller.value().section(MwlSectionKind::Sprites),
        original_sprites
    );
    assert!(controller.undo(1).unwrap());
    assert_eq!(controller.layer1().unwrap().objects.records.len(), 1);
}

#[test]
fn typed_layer1_replacement_rejects_stale_and_invalid_records_atomically() {
    let mut controller = layer1_controller();
    let original = controller.value().clone();
    let mut layer1 = controller.layer1().unwrap();
    let record = ObjectRecord::new(vec![1, 2, 3]).unwrap();
    layer1.objects.records.push(record.clone());

    assert!(matches!(
        controller.replace_layer1(1, &layer1),
        Err(MwlDocumentControllerError::StaleRevision { .. })
    ));
    layer1.objects.records.resize(10_922, record);
    assert!(matches!(
        controller.replace_layer1(0, &layer1),
        Err(MwlDocumentControllerError::Layer1Encoding(
            ObjectStreamError::BankLimitExceeded
        ))
    ));
    assert_eq!(controller.value(), &original);
    assert_eq!(controller.revision(), 0);
}

#[test]
fn typed_sprite_replacement_preserves_metadata_and_unrelated_sections() {
    let lengths = SpriteLengthTable::standard();
    let mut controller = sprite_controller();
    let original_layer = controller.value().section(MwlSectionKind::Layer1).to_vec();
    let mut sprites = controller.sprites(false, &lengths).unwrap();
    let duplicate = sprites.tokens[0].clone();
    sprites.insert(1, duplicate).unwrap();

    controller.replace_sprites(0, &sprites, &lengths).unwrap();

    assert_eq!(controller.revision(), 1);
    assert_eq!(controller.sprites(false, &lengths).unwrap(), sprites);
    let section = controller
        .value()
        .payload_section(MwlSectionKind::Sprites)
        .unwrap();
    assert_eq!(section.metadata, [0x1122_3344, 0x5566_7788]);
    assert_eq!(
        controller.value().section(MwlSectionKind::Layer1),
        original_layer
    );
    assert!(matches!(sprites.tokens[1], SpriteToken::Record(_)));
    assert!(controller.undo(1).unwrap());
    assert_eq!(
        controller
            .value()
            .payload_section(MwlSectionKind::Sprites)
            .unwrap()
            .payload,
        [4, 0x11, 0xd0, 0xbd, 0xff]
    );
}

#[test]
fn typed_legacy_sprite_replacement_stably_canonicalizes_screen_order() {
    let lengths = SpriteLengthTable::standard();
    let mut controller = sprite_controller();
    let record = |screen: u8, id: u8| {
        SpriteToken::Record(SpriteRecord {
            encoded: vec![u8::from(screen & 0x10 != 0) << 1, screen & 0x0f, id],
        })
    };
    let sprites = NativeSpriteStream {
        header: 4,
        expanded: false,
        tokens: vec![record(31, 1), record(0, 2), record(31, 3)],
    };

    controller.replace_sprites(0, &sprites, &lengths).unwrap();

    assert_eq!(
        controller.sprites(false, &lengths).unwrap().tokens,
        [record(0, 2), record(31, 1), record(31, 3)]
    );
}

#[test]
fn typed_expanded_sprite_replacement_canonicalizes_orientation_aware_order() {
    let lengths = SpriteLengthTable::standard();
    let mut controller = sprite_controller();
    let sprites = NativeSpriteStream {
        header: NativeSpriteStream::EXPANDED_HEADER_FLAG,
        expanded: true,
        tokens: vec![
            SpriteToken::Screen(2),
            SpriteToken::Record(SpriteRecord {
                encoded: vec![2, 0x0f, 1],
            }),
            SpriteToken::Screen(1),
            SpriteToken::Record(SpriteRecord {
                encoded: vec![0, 0, 2],
            }),
        ],
    };
    let mut expected = sprites.clone();
    expected.canonicalize_for_orientation(false).unwrap();

    controller.replace_sprites(0, &sprites, &lengths).unwrap();

    assert_eq!(controller.sprites(true, &lengths).unwrap(), expected);
}

#[test]
fn typed_sprite_replacement_canonicalizes_record_effective_upper_y_state() {
    let lengths = SpriteLengthTable::standard();
    let mut controller = sprite_controller();
    let record = controller.sprites(false, &lengths).unwrap().tokens[0].clone();
    let sprites = NativeSpriteStream {
        header: 4,
        expanded: true,
        tokens: vec![
            SpriteToken::Screen(0),
            SpriteToken::Screen(3),
            SpriteToken::Control(0x80),
            SpriteToken::Screen(3),
            record.clone(),
            SpriteToken::Screen(3),
        ],
    };

    controller.replace_sprites(0, &sprites, &lengths).unwrap();

    assert_eq!(
        controller.sprites(true, &lengths).unwrap(),
        NativeSpriteStream {
            header: 4 | NativeSpriteStream::EXPANDED_HEADER_FLAG,
            expanded: true,
            tokens: vec![SpriteToken::Screen(3), record],
        }
    );
}

#[test]
fn typed_sprite_replacement_rejects_stale_and_invalid_streams_atomically() {
    let lengths = SpriteLengthTable::standard();
    let mut controller = sprite_controller();
    let original = controller.value().clone();
    let mut malformed = controller.sprites(false, &lengths).unwrap();
    malformed
        .tokens
        .push(SpriteToken::Record(lm_level::SpriteRecord {
            encoded: vec![1, 2],
        }));

    assert!(matches!(
        controller.replace_sprites(1, &malformed, &lengths),
        Err(MwlDocumentControllerError::StaleRevision { .. })
    ));
    assert!(matches!(
        controller.replace_sprites(0, &malformed, &lengths),
        Err(MwlDocumentControllerError::SpriteEncoding(_))
    ));
    assert_eq!(controller.value(), &original);
    assert_eq!(controller.revision(), 0);
    assert!(!controller.can_undo());
}

#[test]
fn layer3_settings_edit_preserves_opaque_state_and_is_undoable() {
    let mut original = [0_u8; ExpandedLevelSettingsRecord::ENCODED_LEN];
    for (index, byte) in original.iter_mut().enumerate() {
        *byte = u8::try_from(index).unwrap();
    }
    let record = ExpandedLevelSettingsRecord::decode(&original).unwrap();
    let mut source = file();
    source.set_expanded_settings_section(&record);
    let mut controller =
        MwlDocumentController::decode("level.mwl".into(), &source.encode().unwrap()).unwrap();
    let descriptor = Layer3TilemapGraphicsDescriptor::new(0xabc, 2, 3).unwrap();

    controller
        .apply_layer3_settings(0, true, descriptor)
        .unwrap();

    let edited = controller.value().expanded_settings_section().unwrap();
    assert!(edited.layer3_tilemap_enabled());
    assert_eq!(
        edited.layer3_tilemap_graphics_descriptor().unwrap(),
        descriptor
    );
    assert_eq!(
        edited.word(0).unwrap() & !0x2000,
        record.word(0).unwrap() & !0x2000
    );
    for word in 2..ExpandedLevelSettingsRecord::WORD_COUNT {
        assert_eq!(edited.word(word).unwrap(), record.word(word).unwrap());
    }
    assert!(controller.undo(1).unwrap());
    assert_eq!(
        controller.value().expanded_settings_section().unwrap(),
        record
    );
    assert!(controller.redo(2).unwrap());
}

#[test]
fn layer3_settings_edit_rejects_missing_section_and_stale_revision_atomically() {
    let mut controller = controller();
    let descriptor = Layer3TilemapGraphicsDescriptor::new(1, 0, 0).unwrap();
    assert!(
        controller
            .apply_layer3_settings(0, true, descriptor)
            .is_err()
    );
    assert_eq!(controller.revision(), 0);
    assert!(!controller.is_modified());
    assert!(!controller.can_undo());
    assert!(
        controller
            .apply_layer3_settings(1, true, descriptor)
            .is_err()
    );
    assert_eq!(controller.revision(), 0);
}

fn optional_assets() -> MwlOptionalLevelAssets {
    MwlOptionalLevelAssets {
        palette_metadata: [7, 0x10_8031],
        palette: Palette {
            colors: (0_u16..257).map(Bgr555).collect(),
        },
        exanimation_metadata: [0, 0x10_97e9],
        exanimation: Some(CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![ExAnimationRecord::new(1, 0, 0, 0x100, false, &[0, 6], false).unwrap()],
        }),
    }
}

#[test]
fn optional_assets_import_is_one_revision_and_preserves_unrelated_sections() {
    let modes = [false; 256];
    let assets = optional_assets();
    let mut source = MwlFile::default();
    assets.install_into(&mut source, &modes).unwrap();
    let mut controller = controller();
    let original = controller.value().clone();

    controller
        .import_optional_assets(0, &source, 32, &modes)
        .unwrap();

    assert_eq!(controller.revision(), 1);
    assert_eq!(
        controller.value().section(MwlSectionKind::Layer1),
        original.section(MwlSectionKind::Layer1)
    );
    assert_eq!(
        MwlOptionalLevelAssets::decode(controller.value(), 32, &modes).unwrap(),
        assets
    );
    assert!(controller.undo(1).unwrap());
    assert_eq!(controller.value(), &original);
    assert!(controller.redo(2).unwrap());
    assert_eq!(
        MwlOptionalLevelAssets::decode(controller.value(), 32, &modes).unwrap(),
        assets
    );
}

#[test]
fn bad_optional_source_and_stale_revision_leave_history_unchanged() {
    let modes = [false; 256];
    let mut source = MwlFile::default();
    source.set_section(MwlSectionKind::Palette, vec![0; 8]);
    let mut controller = controller();
    let original = controller.value().clone();

    assert!(matches!(
        controller.import_optional_assets(0, &source, 32, &modes),
        Err(MwlDocumentControllerError::OptionalAssets(_))
    ));
    assert!(matches!(
        controller.import_optional_assets(1, &source, 32, &modes),
        Err(MwlDocumentControllerError::StaleRevision { .. })
    ));
    assert_eq!(controller.value(), &original);
    assert_eq!(controller.revision(), 0);
    assert!(!controller.can_undo());
}

#[test]
fn direct_optional_replacement_enforces_record_limit_before_history() {
    let modes = [false; 256];
    let mut assets = optional_assets();
    assets
        .exanimation
        .as_mut()
        .unwrap()
        .records
        .push(ExAnimationRecord::new(1, 0, 0, 0x101, false, &[2, 6], false).unwrap());
    let mut controller = controller();
    let original = controller.value().clone();

    assert!(matches!(
        controller.replace_optional_assets(0, &assets, 1, &modes),
        Err(MwlDocumentControllerError::OptionalAssets(_))
    ));
    assert_eq!(controller.value(), &original);
    assert_eq!(controller.revision(), 0);
    assert!(!controller.can_undo());
}

#[test]
fn semantic_optional_edit_batch_is_atomic_and_one_revision() {
    let modes = [false; 256];
    let assets = optional_assets();
    let mut source = MwlFile::default();
    assets.install_into(&mut source, &modes).unwrap();
    let mut controller =
        MwlDocumentController::decode("level.mwl".into(), &source.encode().unwrap()).unwrap();
    let original = controller.value().clone();
    let edits = [
        MwlOptionalAssetsEdit::SetPaletteColor {
            index: 256,
            color: Bgr555(0x1234),
        },
        MwlOptionalAssetsEdit::SetTrigger {
            index: 3,
            value: Some(7),
        },
        MwlOptionalAssetsEdit::ReplaceFrame {
            record: 0,
            index: 0,
            frame: ExAnimationFrame {
                source_words: vec![0x1234],
            },
        },
    ];

    controller
        .apply_optional_assets_edits(0, 32, &modes, &edits)
        .unwrap();

    assert_eq!(controller.revision(), 1);
    let edited = MwlOptionalLevelAssets::decode(controller.value(), 32, &modes).unwrap();
    assert_eq!(edited.palette.colors[256], Bgr555(0x1234));
    assert_eq!(edited.exanimation.as_ref().unwrap().trigger_values[3], 7);
    assert_eq!(
        edited.exanimation.as_ref().unwrap().records[0].frame_bytes(false),
        [0x34, 0x12]
    );
    assert!(controller.undo(1).unwrap());
    assert_eq!(controller.value(), &original);
}

#[test]
fn late_semantic_optional_edit_failure_rolls_back_earlier_edits() {
    let modes = [false; 256];
    let assets = optional_assets();
    let mut source = MwlFile::default();
    assets.install_into(&mut source, &modes).unwrap();
    let mut controller =
        MwlDocumentController::decode("level.mwl".into(), &source.encode().unwrap()).unwrap();
    let original = controller.value().clone();

    assert!(matches!(
        controller.apply_optional_assets_edits(
            0,
            32,
            &modes,
            &[
                MwlOptionalAssetsEdit::SetPaletteMetadata([1, 2]),
                MwlOptionalAssetsEdit::RemoveRecord { index: 99 },
            ],
        ),
        Err(MwlDocumentControllerError::OptionalEdit { command: 1, .. })
    ));
    assert_eq!(controller.value(), &original);
    assert_eq!(controller.revision(), 0);
    assert!(!controller.can_undo());
}

#[test]
fn mixed_edits_are_atomic_canonical_and_preserve_unowned_sections() {
    let mut controller = controller();
    let original_sprites = controller.value().sections[MwlSectionKind::Sprites as usize].clone();
    controller
        .apply_edits(
            0,
            &[
                MwlDocumentEdit::SetFlags(0x1234_5678),
                MwlDocumentEdit::SetLevelNumber(0x01ab),
                MwlDocumentEdit::ReplaceSection {
                    section: MwlSectionKind::Layer1,
                    bytes: vec![9, 8, 7, 6],
                },
            ],
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert_eq!(controller.value().flags, 0x1234_5678);
    assert_eq!(
        MwlLevelHeaderSection::decode(
            &controller.value().sections[MwlSectionKind::LevelHeader as usize].bytes
        )
        .unwrap()
        .level_number(),
        0x01ab
    );
    assert_eq!(
        controller.value().sections[MwlSectionKind::Layer1 as usize].bytes,
        [9, 8, 7, 6]
    );
    assert_eq!(
        controller.value().sections[MwlSectionKind::Sprites as usize],
        original_sprites
    );
    let encoded = controller.begin_save().unwrap().bytes;
    assert_eq!(MwlFile::decode(&encoded).unwrap(), *controller.value());
}

#[test]
fn packed_entrance_edits_preserve_every_unowned_header_byte() {
    let mut controller = controller();
    let original = MwlLevelHeaderSection::decode(
        &controller.value().sections[MwlSectionKind::LevelHeader as usize].bytes,
    )
    .unwrap()
    .0;
    let main = lm_level::MwlMainEntranceSettings {
        position: 1,
        vertical_settings: 2,
        screen_and_method: 3,
        level_mode_and_screen: 4,
        flags: 5,
        high_position: 6,
        additional_flags: 7,
    };
    let midway = lm_level::MwlMidwayEntranceSettings {
        position: 8,
        flags: 9,
        high_position: 10,
        additional_flags: 11,
    };
    controller
        .apply_edits(
            0,
            &[
                MwlDocumentEdit::SetMainEntrance(main),
                MwlDocumentEdit::SetMidwayEntrance(midway),
            ],
        )
        .unwrap();
    let header = MwlLevelHeaderSection::decode(
        &controller.value().sections[MwlSectionKind::LevelHeader as usize].bytes,
    )
    .unwrap();
    assert_eq!(header.main_entrance(), main);
    assert_eq!(header.midway_entrance(), midway);
    for (index, byte) in original.into_iter().enumerate() {
        if ![2, 3, 4, 5, 6, 9, 10, 11, 12, 14, 15].contains(&index) {
            assert_eq!(header.0[index], byte);
        }
    }
}

#[test]
fn typed_layer2_scroll_edit_is_atomic_and_preserves_unowned_header_bytes() {
    let mut controller = controller();
    let original = MwlLevelHeaderSection::decode(
        &controller.value().sections[MwlSectionKind::LevelHeader as usize].bytes,
    )
    .unwrap()
    .0;
    let settings = lm_level::Layer2ScrollSettings::Separate {
        horizontal: 0x1b,
        vertical: 0x12,
    };
    controller
        .apply_edits(0, &[MwlDocumentEdit::SetLayer2Scroll(settings)])
        .unwrap();
    let header = MwlLevelHeaderSection::decode(
        &controller.value().sections[MwlSectionKind::LevelHeader as usize].bytes,
    )
    .unwrap();
    assert_eq!(header.layer2_scroll_settings(), settings);
    for (index, byte) in original.into_iter().enumerate() {
        if ![2, 17].contains(&index) {
            assert_eq!(header.0[index], byte);
        }
    }

    let before = controller.value().clone();
    assert!(
        controller
            .apply_edits(
                1,
                &[MwlDocumentEdit::SetLayer2Scroll(
                    lm_level::Layer2ScrollSettings::Separate {
                        horizontal: 32,
                        vertical: 0,
                    },
                )],
            )
            .is_err()
    );
    assert_eq!(controller.value(), &before);
    assert_eq!(controller.revision(), 1);
}

#[test]
fn typed_sprite_spawn_edit_preserves_shared_flags_and_is_undoable() {
    let mut controller = controller();
    let original = MwlLevelHeaderSection::decode(
        &controller.value().sections[MwlSectionKind::LevelHeader as usize].bytes,
    )
    .unwrap();
    let settings = original
        .sprite_spawn_settings()
        .with_properties(3, true)
        .unwrap();

    controller
        .apply_edits(0, &[MwlDocumentEdit::SetSpriteSpawnSettings(settings)])
        .unwrap();
    let changed = MwlLevelHeaderSection::decode(
        &controller.value().sections[MwlSectionKind::LevelHeader as usize].bytes,
    )
    .unwrap();
    assert_eq!(changed.0[6] & 0xf8, original.0[6] & 0xf8);
    assert_eq!(changed.sprite_spawn_settings().vertical_range(), 3);
    assert!(changed.sprite_spawn_settings().smart_spawn());
    for (index, byte) in original.0.into_iter().enumerate() {
        if index != 6 {
            assert_eq!(changed.0[index], byte);
        }
    }

    assert!(controller.undo(1).unwrap());
    let restored = MwlLevelHeaderSection::decode(
        &controller.value().sections[MwlSectionKind::LevelHeader as usize].bytes,
    )
    .unwrap();
    assert_eq!(restored, original);
    assert!(controller.redo(2).unwrap());
    let redone = MwlLevelHeaderSection::decode(
        &controller.value().sections[MwlSectionKind::LevelHeader as usize].bytes,
    )
    .unwrap();
    assert_eq!(redone, changed);
}

#[test]
fn late_bad_header_edit_rolls_back_the_whole_batch() {
    let mut controller = controller();
    let original = controller.value().clone();
    assert!(matches!(
        controller.apply_edits(
            0,
            &[
                MwlDocumentEdit::SetFlags(99),
                MwlDocumentEdit::ReplaceSection {
                    section: MwlSectionKind::LevelHeader,
                    bytes: vec![0; 3],
                },
                MwlDocumentEdit::SetLevelNumber(1),
            ]
        ),
        Err(MwlDocumentControllerError::Edit { command: 2, .. })
    ));
    assert_eq!(controller.value(), &original);
    assert_eq!(controller.revision(), 0);
}

#[test]
fn revisions_noops_and_oversized_sections_are_checked() {
    let mut controller = controller();
    controller.apply_edits(0, &[]).unwrap();
    assert_eq!(controller.revision(), 0);
    assert!(matches!(
        controller.apply_edits(1, &[]),
        Err(MwlDocumentControllerError::StaleRevision { .. })
    ));
    assert!(matches!(
        controller.apply_edits(
            0,
            &[MwlDocumentEdit::ReplaceSection {
                section: MwlSectionKind::Layer2,
                bytes: vec![0; MwlFile::MAX_SECTION_BYTES + 1],
            }]
        ),
        Err(MwlDocumentControllerError::File(
            MwlError::SectionTooLarge { .. }
        ))
    ));
    assert!(!controller.is_modified());
}

#[test]
fn immutable_save_acknowledgement_retains_newer_edits() {
    let mut controller = controller();
    controller
        .apply_edits(0, &[MwlDocumentEdit::SetFlags(8)])
        .unwrap();
    let saved = controller.begin_save().unwrap();
    controller
        .apply_edits(1, &[MwlDocumentEdit::SetFlags(9)])
        .unwrap();
    assert_eq!(
        controller.begin_save(),
        Err(MwlDocumentControllerError::SavePending)
    );
    assert!(matches!(
        controller.acknowledge_save(saved.request_id + 1),
        Err(MwlDocumentControllerError::StaleSave { .. })
    ));
    controller.acknowledge_save(saved.request_id).unwrap();
    assert!(controller.is_modified());
    let current = controller.begin_save().unwrap();
    controller.cancel_save(current.request_id).unwrap();
    assert!(!controller.save_pending());
}

#[test]
fn whole_file_history_restores_saved_baseline_and_invalidates_divergent_redo() {
    let mut controller = controller();
    let original = controller.value().clone();
    controller
        .apply_edits(
            0,
            &[
                MwlDocumentEdit::SetFlags(8),
                MwlDocumentEdit::ReplaceSection {
                    section: MwlSectionKind::Layer1,
                    bytes: vec![9, 8, 7],
                },
            ],
        )
        .unwrap();
    assert!(controller.can_undo());
    assert!(controller.undo(1).unwrap());
    assert_eq!(controller.value(), &original);
    assert!(!controller.is_modified());
    assert!(controller.redo(2).unwrap());
    assert_eq!(controller.value().flags, 8);
    assert_eq!(
        controller.value().sections[MwlSectionKind::Layer1 as usize].bytes,
        [9, 8, 7]
    );
    assert!(controller.undo(3).unwrap());
    controller
        .apply_edits(4, &[MwlDocumentEdit::SetLevelNumber(0x01aa)])
        .unwrap();
    assert!(!controller.can_redo());
    assert!(!controller.redo(5).unwrap());
    assert_eq!(controller.revision(), 5);
}

#[test]
fn stale_and_empty_history_navigation_are_atomic() {
    let mut controller = controller();
    assert!(!controller.undo(0).unwrap());
    controller
        .apply_edits(0, &[MwlDocumentEdit::SetFlags(8)])
        .unwrap();
    let before = controller.value().clone();
    assert!(matches!(
        controller.undo(0),
        Err(MwlDocumentControllerError::StaleRevision { .. })
    ));
    assert_eq!(controller.value(), &before);
    assert_eq!(controller.revision(), 1);
}
