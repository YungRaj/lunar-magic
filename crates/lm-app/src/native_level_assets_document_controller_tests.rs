use super::*;
use crate::{ExAnimationControllerEdit, PaletteControllerEdit};
use lm_graphics::{Bgr555, CompactExAnimation, Palette, PaletteChange};
use lm_level::{ExpandedLevelSettingsRecord, LevelObjectData, NativeSpriteStream};
use lm_project::{LoadedLevelSlot, LoadedNativeLevelAssets};

fn file() -> NativeLevelAssetsFile {
    NativeLevelAssetsFile {
        source_slot: 3,
        assets: LoadedNativeLevelAssets {
            level: LoadedLevelSlot {
                number: 3,
                layer1: LevelObjectData::default(),
                sprites: NativeSpriteStream::default(),
            },
            palette: Palette {
                colors: vec![Bgr555(0), Bgr555(1)],
            },
            exanimation: CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: Vec::new(),
            },
            expanded_settings: None,
        },
    }
}

fn controller() -> NativeLevelAssetsDocumentController {
    let bytes = file().encode(&[false; 256]).unwrap();
    NativeLevelAssetsDocumentController::decode(
        "level.lmnative".into(),
        &bytes,
        SpriteLengthTable::standard(),
        8,
        &[false; 256],
    )
    .unwrap()
}

fn palette_edit(value: u16) -> NativeLevelAssetsControllerEdit {
    NativeLevelAssetsControllerEdit::Palette(vec![PaletteControllerEdit::ApplyChanges(vec![
        PaletteChange {
            index: 1,
            color: Bgr555(value),
        },
    ])])
}

#[test]
fn mixed_edits_are_revisioned_and_reopen_canonically() {
    let mut controller = controller();
    controller
        .apply_edits(
            0,
            &[
                palette_edit(0x1234),
                NativeLevelAssetsControllerEdit::ExAnimation(vec![
                    ExAnimationControllerEdit::SetSetting(7),
                ]),
            ],
            &PaletteOwnership::editable(2),
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert!(controller.is_modified());
    let snapshot = controller.begin_save().unwrap();
    let reopened = NativeLevelAssetsFile::decode(
        &snapshot.bytes,
        &SpriteLengthTable::standard(),
        8,
        &[false; 256],
    )
    .unwrap();
    assert_eq!(&reopened, controller.value());
}

#[test]
fn portable_boundary_interaction_is_semantic_lossless_and_undoable() {
    let mut source = file();
    let mut bytes = [0; ExpandedLevelSettingsRecord::ENCODED_LEN];
    bytes[16..18].copy_from_slice(&0xb123_u16.to_le_bytes());
    source.assets.expanded_settings = Some(ExpandedLevelSettingsRecord::from_encoded(bytes));
    let encoded = source.encode(&[false; 256]).unwrap();
    let mut controller = NativeLevelAssetsDocumentController::decode(
        "level.lmnative".into(),
        &encoded,
        SpriteLengthTable::standard(),
        8,
        &[false; 256],
    )
    .unwrap();

    let descriptor = lm_level::Layer3TilemapGraphicsDescriptor::new(0xabc, 2, 3).unwrap();
    controller
        .apply_edits(
            0,
            &[
                NativeLevelAssetsControllerEdit::SpriteBoundaryInteractionAir(true),
                NativeLevelAssetsControllerEdit::Layer3TilemapSettings {
                    enabled: true,
                    descriptor,
                },
            ],
            &PaletteOwnership::editable(2),
        )
        .unwrap();
    assert_eq!(
        controller
            .value()
            .assets
            .expanded_settings
            .as_ref()
            .unwrap()
            .word(8)
            .unwrap(),
        0xf123
    );
    let settings = controller
        .value()
        .assets
        .expanded_settings
        .as_ref()
        .unwrap();
    assert!(settings.layer3_tilemap_enabled());
    assert_eq!(
        settings.layer3_tilemap_graphics_descriptor().unwrap(),
        descriptor
    );
    assert!(controller.undo(1).unwrap());
    assert_eq!(
        controller
            .value()
            .assets
            .expanded_settings
            .as_ref()
            .unwrap()
            .word(8)
            .unwrap(),
        0xb123
    );
    let settings = controller
        .value()
        .assets
        .expanded_settings
        .as_ref()
        .unwrap();
    assert!(!settings.layer3_tilemap_enabled());
    assert_eq!(settings.word(1).unwrap(), 0);
}

#[test]
fn stale_late_invalid_and_no_op_batches_are_atomic() {
    let mut controller = controller();
    assert!(matches!(
        controller.apply_edits(4, &[], &PaletteOwnership::editable(2)),
        Err(NativeLevelAssetsDocumentControllerError::StaleRevision { .. })
    ));
    let before = controller.value().clone();
    let result = controller.apply_edits(
        0,
        &[
            palette_edit(0x2222),
            NativeLevelAssetsControllerEdit::ExpandedSettingsWords(vec![(0, 4)]),
        ],
        &PaletteOwnership::editable(2),
    );
    assert!(matches!(
        result,
        Err(NativeLevelAssetsDocumentControllerError::Edit(
            NativeLevelAssetsControllerError::ExpandedSettingsUnavailable { .. }
        ))
    ));
    assert_eq!(controller.value(), &before);
    let result = controller.apply_edits(
        0,
        &[NativeLevelAssetsControllerEdit::SpriteSpawnProperties {
            vertical_range: 3,
            smart_spawn: true,
        }],
        &PaletteOwnership::editable(2),
    );
    assert!(matches!(
        result,
        Err(NativeLevelAssetsDocumentControllerError::Edit(
            NativeLevelAssetsControllerError::SpriteSpawnSettingsUnavailable { command: 0 }
        ))
    ));
    assert_eq!(controller.value(), &before);
    controller
        .apply_edits(0, &[], &PaletteOwnership::editable(2))
        .unwrap();
    assert_eq!(controller.revision(), 0);
}

#[test]
fn save_snapshots_are_correlated_and_do_not_hide_newer_edits() {
    let mut controller = controller();
    controller
        .apply_edits(0, &[palette_edit(2)], &PaletteOwnership::editable(2))
        .unwrap();
    let save = controller.begin_save().unwrap();
    assert!(matches!(
        controller.begin_save(),
        Err(NativeLevelAssetsDocumentControllerError::SavePending)
    ));
    assert!(controller.cancel_save(save.request_id + 1).is_err());
    assert!(controller.save_pending());
    controller
        .apply_edits(1, &[palette_edit(3)], &PaletteOwnership::editable(2))
        .unwrap();
    assert!(controller.acknowledge_save(save.request_id + 1).is_err());
    controller.acknowledge_save(save.request_id).unwrap();
    assert!(controller.is_modified());
    assert!(!controller.save_pending());
    assert!(matches!(
        controller.cancel_save(save.request_id),
        Err(NativeLevelAssetsDocumentControllerError::NoPendingSave)
    ));
}

#[test]
fn decode_rejects_wrong_mode_count_and_malformed_input() {
    let bytes = file().encode(&[false; 256]).unwrap();
    assert!(matches!(
        NativeLevelAssetsDocumentController::decode(
            "level.lmnative".into(),
            &bytes,
            SpriteLengthTable::standard(),
            8,
            &[false; 12],
        ),
        Err(NativeLevelAssetsDocumentControllerError::SizeModeCount(12))
    ));
    assert!(matches!(
        NativeLevelAssetsDocumentController::decode(
            "level.lmnative".into(),
            b"broken",
            SpriteLengthTable::standard(),
            8,
            &[false; 256],
        ),
        Err(NativeLevelAssetsDocumentControllerError::File(_))
    ));
}

#[test]
fn undo_redo_are_monotonic_bounded_and_track_the_saved_baseline() {
    let mut controller = controller();
    controller
        .apply_edits(0, &[palette_edit(2)], &PaletteOwnership::editable(2))
        .unwrap();
    let saved_value = controller.value().clone();
    let save = controller.begin_save().unwrap();
    controller.acknowledge_save(save.request_id).unwrap();
    controller
        .apply_edits(1, &[palette_edit(3)], &PaletteOwnership::editable(2))
        .unwrap();
    assert!(controller.is_modified());
    assert!(controller.can_undo());
    assert!(controller.undo(2).unwrap());
    assert_eq!(controller.revision(), 3);
    assert_eq!(controller.value(), &saved_value);
    assert!(!controller.is_modified());
    assert!(controller.can_redo());
    assert!(controller.redo(3).unwrap());
    assert_eq!(controller.revision(), 4);
    assert!(controller.is_modified());
    assert!(!controller.redo(4).unwrap());
    assert_eq!(controller.revision(), 4);
    assert!(matches!(
        controller.undo(3),
        Err(NativeLevelAssetsDocumentControllerError::StaleRevision { .. })
    ));
}

#[test]
fn divergent_edits_clear_redo_without_losing_the_undo_chain() {
    let mut controller = controller();
    controller
        .apply_edits(0, &[palette_edit(2)], &PaletteOwnership::editable(2))
        .unwrap();
    controller
        .apply_edits(1, &[palette_edit(3)], &PaletteOwnership::editable(2))
        .unwrap();
    controller.undo(2).unwrap();
    controller
        .apply_edits(3, &[palette_edit(4)], &PaletteOwnership::editable(2))
        .unwrap();
    assert!(!controller.can_redo());
    assert!(controller.can_undo());
    assert_eq!(controller.value().assets.palette.colors[1], Bgr555(4));
}

#[test]
fn aggregate_history_retains_only_the_document_limit() {
    let mut controller = controller();
    for revision in 0..=NativeLevelAssetsDocumentController::HISTORY_LIMIT {
        let color = u16::from(revision % 2 == 0) + 2;
        controller
            .apply_edits(
                u64::try_from(revision).unwrap(),
                &[palette_edit(color)],
                &PaletteOwnership::editable(2),
            )
            .unwrap();
    }
    for _ in 0..NativeLevelAssetsDocumentController::HISTORY_LIMIT {
        let revision = controller.revision();
        assert!(controller.undo(revision).unwrap());
    }
    let revision = controller.revision();
    assert!(!controller.undo(revision).unwrap());
    assert_eq!(controller.revision(), revision);
}
