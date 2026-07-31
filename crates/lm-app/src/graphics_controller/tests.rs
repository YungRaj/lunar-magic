use super::*;
use crate::{AppError, AppState, Command, FrontendEffect};
use lm_graphics::{GraphicsTileOwner, IndexedTile};
use lm_project::{
    GraphicsCompression, GraphicsSaveOptions, LevelPointerTable, Project, RatsOwnershipManifest,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::RomImage;

fn layout() -> GraphicsRomLayout {
    GraphicsRomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: 0x200,
            entries: 4,
            stride: 3,
        },
        split_pointer_planes: None,
        compression: GraphicsCompression::Lz2,
        maximum_compressed_len: 0x8000,
        maximum_decompressed_len: 0x10000,
    }
}

fn graphics() -> GraphicsFile4bpp {
    GraphicsFile4bpp {
        tiles: vec![
            IndexedTile::new([0; 64]),
            IndexedTile::new(std::array::from_fn(|index| {
                u8::try_from(index % 16).unwrap()
            })),
            IndexedTile::new([2; 64]),
        ],
    }
}

fn test_rom() -> Vec<u8> {
    test_rom_for(layout())
}

fn test_rom_for(layout: GraphicsRomLayout) -> Vec<u8> {
    let mut bytes = vec![0xff; 0x8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    project
        .save_graphics_file(
            2,
            &graphics(),
            layout,
            &GraphicsSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x1000..0x4000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![ProtectedRange(0x200..0x20c)],
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    project.refresh_checksum(0x7fdc).unwrap();
    project.save_snapshot()
}

fn options() -> GraphicsSaveOptions {
    GraphicsSaveOptions {
        allocation: AllocationPolicy {
            search: 0x8000..0x10000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x200..0x20c), ProtectedRange(0x7fdc..0x7fe0)],
        },
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    }
}

fn graphics_snapshot() -> crate::ControllerSnapshot {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowGraphics(2)).unwrap();
    app.controller_snapshot().unwrap()
}

#[test]
fn edits_compresses_expands_dispatches_and_reloads() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowGraphics(2)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        GraphicsController::decode(&snapshot, layout(), GraphicsOwnership::editable(3)).unwrap();
    controller
        .apply_edits(&[GraphicsControllerEdit::ReplaceRange {
            start: 1,
            tiles: vec![IndexedTile::new([7; 64]), IndexedTile::new([8; 64])],
        }])
        .unwrap();
    let prepared = controller
        .prepare_commit("Edit graphics 02", &options())
        .unwrap();
    assert_eq!(prepared.mutation.appended.len(), 0x8000);
    assert_eq!(
        app.dispatch(prepared.into_command()).unwrap(),
        [FrontendEffect::ProjectChanged {
            description: "Edit graphics 02".into(),
            mode: EditorMode::Graphics(2),
            revision: 1,
        }]
    );
    assert_eq!(
        app.project()
            .unwrap()
            .load_graphics_file(2, layout())
            .unwrap(),
        *controller.graphics()
    );
    let logical = app.project().unwrap().rom.logical_bytes();
    assert_eq!(
        lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
        lm_rom::compute_snes_checksum(logical, 0x7fdc).unwrap()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
    app.dispatch(Command::Redo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x10000);
}

#[test]
fn controller_edits_and_reopens_profile_selected_lz3_graphics() {
    let mut lz3 = layout();
    lz3.compression = GraphicsCompression::Lz3;
    let mut app = AppState::default();
    app.load_rom(test_rom_for(lz3)).unwrap();
    app.dispatch(Command::ShowGraphics(2)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        GraphicsController::decode(&snapshot, lz3, GraphicsOwnership::editable(3)).unwrap();
    controller
        .apply_edits(&[GraphicsControllerEdit::ApplyChanges(vec![
            GraphicsTileChange {
                index: 1,
                tile: IndexedTile::new([9; 64]),
            },
        ])])
        .unwrap();
    let prepared = controller
        .prepare_commit("Edit LZ3 graphics", &options())
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    assert_eq!(
        app.project().unwrap().load_graphics_file(2, lz3).unwrap(),
        *controller.graphics()
    );
}

#[test]
fn owned_commit_reclaims_snapshot_block_and_undo_restores_it() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowGraphics(2)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        GraphicsController::decode(&snapshot, layout(), GraphicsOwnership::editable(3)).unwrap();
    let previous = controller.previous_block.clone().unwrap();
    controller
        .apply_edits(&[GraphicsControllerEdit::ApplyChanges(vec![
            GraphicsTileChange {
                index: 1,
                tile: IndexedTile::new([12; 64]),
            },
        ])])
        .unwrap();
    let prepared = controller
        .prepare_commit_with_reclamation(
            "Owned graphics edit",
            &options(),
            &RatsOwnershipManifest {
                owned: vec![previous.clone()],
                retained: Vec::new(),
            },
        )
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    assert!(
        app.project().unwrap().rom.logical_bytes()[previous.full_range()]
            .iter()
            .all(|byte| *byte == 0xff)
    );
    assert_eq!(
        app.project()
            .unwrap()
            .load_graphics_file(2, layout())
            .unwrap(),
        *controller.graphics()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(
        lm_rats::parse_at(
            app.project().unwrap().rom.logical_bytes(),
            previous.header_offset
        )
        .unwrap(),
        previous
    );
}

#[test]
fn editable_display_decode_derives_exact_ownership_shape() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowGraphics(2)).unwrap();
    let controller =
        GraphicsController::decode_editable(&app.controller_snapshot().unwrap(), layout()).unwrap();
    assert_eq!(controller.graphics(), &graphics());
    assert_eq!(controller.ownership().len(), graphics().tiles.len());
    assert!((0..controller.ownership().len()).all(|index| matches!(
        controller.ownership().owner(index),
        Some(GraphicsTileOwner::Editable)
    )));
}

#[test]
fn ownership_failure_rolls_back_batch_and_stale_commit_is_rejected() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowGraphics(2)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let ownership = GraphicsOwnership::from_owners(vec![
        GraphicsTileOwner::Editable,
        GraphicsTileOwner::Fixed,
        GraphicsTileOwner::Editable,
    ]);
    let mut controller = GraphicsController::decode(&snapshot, layout(), ownership).unwrap();
    assert!(!controller.is_modified());
    assert!(
        controller
            .prepare_commit("unchanged", &options())
            .unwrap()
            .mutation
            .is_empty()
    );
    let original = controller.graphics().clone();
    assert!(matches!(
        controller.apply_edits(&[
            GraphicsControllerEdit::ApplyChanges(vec![GraphicsTileChange {
                index: 0,
                tile: IndexedTile::new([4; 64]),
            }]),
            GraphicsControllerEdit::ApplyChanges(vec![GraphicsTileChange {
                index: 1,
                tile: IndexedTile::new([5; 64]),
            }]),
        ]),
        Err(GraphicsControllerError::Edit { command: 1, .. })
    ));
    assert_eq!(controller.graphics(), &original);
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
        Err(AppError::StaleProjectRevision { .. })
    ));
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
}

#[test]
fn wrong_mode_mapper_and_ownership_shape_are_rejected() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let level_snapshot = app.controller_snapshot().unwrap();
    assert!(matches!(
        GraphicsController::decode(&level_snapshot, layout(), GraphicsOwnership::editable(3)),
        Err(GraphicsControllerError::WrongMode(EditorMode::Level(0x105)))
    ));
    app.dispatch(Command::ShowGraphics(2)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut wrong_mapper = layout();
    wrong_mapper.mapper = Mapper::Sa1;
    assert!(matches!(
        GraphicsController::decode(&snapshot, wrong_mapper, GraphicsOwnership::editable(3)),
        Err(GraphicsControllerError::MapperMismatch { .. })
    ));
    assert!(matches!(
        GraphicsController::decode(&snapshot, layout(), GraphicsOwnership::editable(2)),
        Err(GraphicsControllerError::Edit {
            error: GraphicsEditError::OwnershipShape { .. },
            ..
        })
    ));
}

#[test]
fn raw_import_round_trips_and_preserves_protected_tiles() {
    let snapshot = graphics_snapshot();
    let ownership = GraphicsOwnership::from_owners(vec![
        GraphicsTileOwner::Editable,
        GraphicsTileOwner::Fixed,
        GraphicsTileOwner::Editable,
    ]);
    let mut controller = GraphicsController::decode(&snapshot, layout(), ownership).unwrap();
    let original = controller.export_raw().unwrap();
    controller.import_raw(&original).unwrap();
    assert!(!controller.is_modified());

    let mut edited = original.clone();
    edited[0] ^= 0x80;
    controller.import_raw(&edited).unwrap();
    assert!(controller.is_modified());
    assert_eq!(controller.export_raw().unwrap(), edited);

    let before_rejection = controller.export_raw().unwrap();
    let mut protected = before_rejection.clone();
    protected[GraphicsFile4bpp::BYTES_PER_TILE] ^= 0x80;
    assert!(controller.import_raw(&protected).is_err());
    assert_eq!(controller.export_raw().unwrap(), before_rejection);
}

#[test]
fn raw_import_rejects_partial_and_wrong_sized_files_atomically() {
    let snapshot = graphics_snapshot();
    let mut controller =
        GraphicsController::decode(&snapshot, layout(), GraphicsOwnership::editable(3)).unwrap();
    let before = controller.export_raw().unwrap();
    assert!(matches!(
        controller.import_raw(&before[..before.len() - 1]),
        Err(GraphicsControllerError::File(_))
    ));
    assert!(matches!(
        controller.import_raw(&before[..GraphicsFile4bpp::BYTES_PER_TILE]),
        Err(GraphicsControllerError::ImportedTileCount {
            expected: 3,
            actual: 1
        })
    ));
    assert_eq!(controller.export_raw().unwrap(), before);
}
