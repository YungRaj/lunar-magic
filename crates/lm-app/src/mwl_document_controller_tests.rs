use super::*;
use lm_graphics::{Bgr555, CompactExAnimation, ExAnimationFrame, ExAnimationRecord, Palette};
use lm_level::{ExpandedLevelSettingsRecord, Layer3TilemapGraphicsDescriptor};
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
