use super::*;
use lm_level::{Map16Page, Map16Set};

fn controller() -> Map16DocumentController {
    let value = Map16SetFile {
        set: Map16Set {
            pages: vec![Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap()],
        },
    };
    Map16DocumentController::decode("all.lm16set".into(), &value.encode().unwrap()).unwrap()
}

fn edit(subtile: u16) -> Map16DocumentEdit {
    Map16DocumentEdit::SetSubtile {
        address: Map16Address { page: 0, tile: 0 },
        quadrant: Map16Quadrant::TopLeft,
        subtile: Subtile(subtile),
        resolution_limit: Map16Page::TILE_COUNT,
    }
}

#[test]
fn edits_are_revisioned_atomic_and_canonically_reopened() {
    let mut value = controller();
    value.apply_edits(0, &[edit(7)]).unwrap();
    assert_eq!(value.revision(), 1);
    assert!(value.is_modified());
    let before = value.value().clone();
    assert!(value.apply_edits(0, &[edit(8)]).is_err());
    assert_eq!(value.value(), &before);
    assert_eq!(
        Map16SetFile::decode(&value.value().encode().unwrap()).unwrap(),
        *value.value()
    );
}

#[test]
fn immutable_save_snapshot_does_not_acknowledge_later_edits() {
    let mut value = controller();
    value.apply_edits(0, &[edit(7)]).unwrap();
    let save = value.begin_save().unwrap();
    value.apply_edits(1, &[edit(8)]).unwrap();
    assert!(value.begin_save().is_err());
    assert!(value.acknowledge_save(save.request_id + 1).is_err());
    value.acknowledge_save(save.request_id).unwrap();
    assert!(value.is_modified());
    assert_eq!(
        Map16SetFile::decode(&save.bytes).unwrap().set.pages[0].tiles[0].top_left,
        Subtile(7)
    );
}

#[test]
fn page_growth_and_removal_are_canonical_revisioned_edits() {
    let mut value = controller();
    let blank = Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap();
    value
        .apply_edits(
            0,
            &[Map16DocumentEdit::AppendPage {
                page: blank,
                resolution_limit: Map16Page::TILE_COUNT * 2,
            }],
        )
        .unwrap();
    assert_eq!(value.value().set.pages.len(), 2);
    assert_eq!(value.revision(), 1);
    assert!(value.can_undo());
    assert_eq!(
        Map16SetFile::decode(&value.value().encode().unwrap()).unwrap(),
        *value.value()
    );

    value
        .apply_edits(
            1,
            &[Map16DocumentEdit::RemoveLastPage {
                resolution_limit: Map16Page::TILE_COUNT,
            }],
        )
        .unwrap();
    assert_eq!(value.value().set.pages.len(), 1);
    assert_eq!(value.revision(), 2);
    value.undo(2).unwrap();
    assert_eq!(value.value().set.pages.len(), 2);
}

#[test]
fn page_removal_with_retained_dangling_link_is_atomic() {
    let mut value = controller();
    let mut tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
    tiles[0].acts_like = 0x100;
    let blank = Map16Page::new(tiles).unwrap();
    value
        .apply_edits(
            0,
            &[Map16DocumentEdit::AppendPage {
                page: blank,
                resolution_limit: Map16Page::TILE_COUNT * 2,
            }],
        )
        .unwrap();
    value
        .apply_edits(
            1,
            &[Map16DocumentEdit::SetActsLike {
                address: Map16Address { page: 0, tile: 0 },
                acts_like: 0x100,
                resolution_limit: Map16Page::TILE_COUNT * 2,
            }],
        )
        .unwrap();
    let before = value.value().clone();
    assert!(matches!(
        value.apply_edits(
            2,
            &[Map16DocumentEdit::RemoveLastPage {
                resolution_limit: Map16Page::TILE_COUNT,
            }]
        ),
        Err(Map16DocumentControllerError::Edit { command: 0, .. })
    ));
    assert_eq!(value.value(), &before);
    assert_eq!(value.revision(), 2);
}

#[test]
fn history_restores_saved_state_and_invalidates_divergent_redo() {
    let mut value = controller();
    value.apply_edits(0, &[edit(7)]).unwrap();
    let saved = value.value().clone();
    let snapshot = value.begin_save().unwrap();
    value.acknowledge_save(snapshot.request_id).unwrap();
    value.apply_edits(1, &[edit(8)]).unwrap();
    assert!(value.undo(2).unwrap());
    assert_eq!(value.revision(), 3);
    assert_eq!(value.value(), &saved);
    assert!(!value.is_modified());
    assert!(value.redo(3).unwrap());
    assert_eq!(value.value().set.pages[0].tiles[0].top_left, Subtile(8));
    assert!(value.undo(4).unwrap());
    value.apply_edits(5, &[edit(9)]).unwrap();
    assert!(!value.can_redo());
    assert!(value.can_undo());
    assert!(matches!(
        value.undo(5),
        Err(Map16DocumentControllerError::StaleRevision { .. })
    ));
}
