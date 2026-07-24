use super::*;

fn controller() -> Map16PageDocumentController {
    let file = Map16PageFile {
        source_page: 0x12,
        page: Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
    };
    Map16PageDocumentController::decode("page.map16".into(), &file.encode().unwrap()).unwrap()
}

#[test]
fn mixed_page_local_edits_are_atomic_canonical_and_preserve_identity() {
    let mut controller = controller();
    controller
        .apply_edits(
            0,
            &[
                Map16PageDocumentEdit::ReplaceTile {
                    tile: 1,
                    value: Map16Tile {
                        top_left: Subtile(1),
                        top_right: Subtile(2),
                        bottom_left: Subtile(3),
                        bottom_right: Subtile(4),
                        acts_like: 0xabcd,
                    },
                },
                Map16PageDocumentEdit::SetSubtile {
                    tile: 1,
                    quadrant: Map16Quadrant::BottomRight,
                    value: Subtile(0x9234),
                },
                Map16PageDocumentEdit::SetActsLike {
                    tile: 2,
                    value: 0xffff,
                },
            ],
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert_eq!(controller.value().source_page, 0x12);
    assert_eq!(
        controller.value().page.tiles[1].bottom_right,
        Subtile(0x9234)
    );
    assert_eq!(controller.value().page.tiles[2].acts_like, 0xffff);
    let snapshot = controller.begin_save().unwrap();
    assert_eq!(
        Map16PageFile::decode(&snapshot.bytes).unwrap(),
        *controller.value()
    );
}

#[test]
fn late_bad_index_and_stale_revision_leave_the_page_unchanged() {
    let mut controller = controller();
    let original = controller.value().clone();
    assert!(matches!(
        controller.apply_edits(
            0,
            &[
                Map16PageDocumentEdit::SetActsLike { tile: 0, value: 1 },
                Map16PageDocumentEdit::SetActsLike {
                    tile: 256,
                    value: 2
                },
            ]
        ),
        Err(Map16PageDocumentControllerError::TileOutOfRange {
            command: 1,
            tile: 256
        })
    ));
    assert_eq!(controller.value(), &original);
    assert!(matches!(
        controller.apply_edits(1, &[]),
        Err(Map16PageDocumentControllerError::StaleRevision { .. })
    ));
}

#[test]
fn immutable_save_acknowledgement_retains_newer_edits_and_cancel_retries() {
    let mut controller = controller();
    controller
        .apply_edits(
            0,
            &[Map16PageDocumentEdit::SetActsLike { tile: 0, value: 1 }],
        )
        .unwrap();
    let first = controller.begin_save().unwrap();
    controller
        .apply_edits(
            1,
            &[Map16PageDocumentEdit::SetActsLike { tile: 0, value: 2 }],
        )
        .unwrap();
    assert!(matches!(
        controller.begin_save(),
        Err(Map16PageDocumentControllerError::SavePending)
    ));
    assert!(controller.acknowledge_save(first.request_id + 1).is_err());
    controller.acknowledge_save(first.request_id).unwrap();
    assert!(controller.is_modified());
    let second = controller.begin_save().unwrap();
    controller.cancel_save(second.request_id).unwrap();
    assert!(!controller.save_pending());
}

#[test]
fn history_restores_saved_page_and_invalidates_divergent_redo() {
    let edit = |value| Map16PageDocumentEdit::SetActsLike { tile: 0, value };
    let mut controller = controller();
    controller.apply_edits(0, &[edit(1)]).unwrap();
    let snapshot = controller.begin_save().unwrap();
    controller.acknowledge_save(snapshot.request_id).unwrap();
    controller.apply_edits(1, &[edit(2)]).unwrap();
    assert!(controller.undo(2).unwrap());
    assert!(!controller.is_modified());
    assert!(controller.redo(3).unwrap());
    assert!(controller.undo(4).unwrap());
    controller.apply_edits(5, &[edit(3)]).unwrap();
    assert!(!controller.can_redo());
    assert!(matches!(
        controller.undo(5),
        Err(Map16PageDocumentControllerError::StaleRevision { .. })
    ));
}
