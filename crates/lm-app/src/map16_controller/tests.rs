use super::*;
use crate::{AppError, AppState, Command, FrontendEffect};
use lm_level::{Map16Page, Map16Tile};
use lm_project::{LevelPointerTable, Map16SetSaveOptions, Project, RatsOwnershipManifest};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::RomImage;

fn layout() -> Map16RomLayout {
    Map16RomLayout {
        mapper: Mapper::LoRom,
        graphics: LevelPointerTable {
            offset: 0x200,
            entries: 2,
            stride: 3,
        },
        acts_like: LevelPointerTable {
            offset: 0x300,
            entries: 2,
            stride: 3,
        },
    }
}

fn page(seed: u16) -> Map16Page {
    let mut tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
    tiles[0].top_left = Subtile(seed);
    Map16Page::new(tiles).unwrap()
}

fn test_rom() -> Vec<u8> {
    let mut bytes = vec![0xff; 0x8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
    for (page_number, (graphics_pc, acts_pc)) in
        [(0x1000, 0x1800), (0x2000, 0x2800)].into_iter().enumerate()
    {
        let graphics_pointer = layout().graphics.pointer_offset(page_number).unwrap();
        let acts_pointer = layout().acts_like.pointer_offset(page_number).unwrap();
        let graphics_snes = lm_rom::pc_to_snes(Mapper::LoRom, graphics_pc)
            .unwrap()
            .to_le_bytes();
        let acts_snes = lm_rom::pc_to_snes(Mapper::LoRom, acts_pc)
            .unwrap()
            .to_le_bytes();
        bytes[graphics_pointer..graphics_pointer + 3].copy_from_slice(&graphics_snes[..3]);
        bytes[acts_pointer..acts_pointer + 3].copy_from_slice(&acts_snes[..3]);
        let (graphics, acts_like) = page(u16::try_from(page_number + 1).unwrap())
            .encode()
            .unwrap();
        bytes[graphics_pc..graphics_pc + graphics.len()].copy_from_slice(&graphics);
        bytes[acts_pc..acts_pc + acts_like.len()].copy_from_slice(&acts_like);
    }
    let checksum = lm_rom::compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    bytes
}

fn options() -> Map16SetSaveOptions {
    let policy = AllocationPolicy {
        search: 0x8000..0x10000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x200..0x206),
            ProtectedRange(0x300..0x306),
            ProtectedRange(0x7fdc..0x7fe0),
        ],
    };
    Map16SetSaveOptions {
        graphics_allocation: policy.clone(),
        acts_like_allocation: policy,
        previous_graphics: Vec::new(),
        previous_acts_like: Vec::new(),
        reuse_identical: true,
        erase_fill: 0xff,
    }
}

fn tagged_test_rom() -> (Vec<u8>, RatsOwnershipManifest) {
    let mut project = Project::new(RomImage::from_bytes(test_rom()).unwrap());
    let set = project.load_map16_set(layout()).unwrap();
    let policy = AllocationPolicy {
        search: 0x3000..0x7000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x200..0x206),
            ProtectedRange(0x300..0x306),
            ProtectedRange(0x7fdc..0x7fe0),
        ],
    };
    let saved = project
        .save_map16_set_with_checksum(
            &set,
            layout(),
            0x7fdc,
            &Map16SetSaveOptions {
                graphics_allocation: policy.clone(),
                acts_like_allocation: policy,
                previous_graphics: Vec::new(),
                previous_acts_like: Vec::new(),
                reuse_identical: false,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    let owned = saved
        .pages
        .into_iter()
        .flat_map(|page| [page.graphics.block, page.acts_like.block])
        .collect();
    (
        project.save_snapshot(),
        RatsOwnershipManifest {
            owned,
            retained: Vec::new(),
        },
    )
}

#[test]
fn edit_expands_dispatches_reloads_and_undoes_complete_set() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = Map16Controller::decode(&snapshot, layout()).unwrap();
    controller
        .apply_edits(&[
            Map16ControllerEdit::SetSubtile {
                address: Map16Address { page: 1, tile: 7 },
                quadrant: Map16Quadrant::BottomRight,
                subtile: Subtile(0x4321),
                resolution_limit: 512,
            },
            Map16ControllerEdit::SetActsLike {
                address: Map16Address { page: 1, tile: 7 },
                acts_like: 3,
                resolution_limit: 512,
            },
        ])
        .unwrap();
    let prepared = controller.prepare_commit("Edit Map16", &options()).unwrap();
    assert_eq!(prepared.mutation.appended.len(), 0x8000);
    assert_eq!(
        app.dispatch(prepared.into_command()).unwrap(),
        [FrontendEffect::ProjectChanged {
            description: "Edit Map16".into(),
            mode: EditorMode::Map16,
            revision: 1,
        }]
    );
    assert_eq!(
        app.project().unwrap().load_map16_set(layout()).unwrap(),
        *controller.set()
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
fn map16_saves_semantically_after_independent_recovery_growth() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let mut controller =
        Map16Controller::decode(&app.controller_snapshot().unwrap(), layout()).unwrap();
    controller
        .apply_edits(&[Map16ControllerEdit::SetSubtile {
            address: Map16Address { page: 1, tile: 9 },
            quadrant: Map16Quadrant::TopLeft,
            subtile: Subtile(0x3456),
            resolution_limit: 512,
        }])
        .unwrap();
    let baseline = app.project().unwrap().save_snapshot();
    let mut staged = app.project().unwrap().clone();
    staged
        .expand_rom(Mapper::LoRom, 0x1_0000, 0xff, 0x7fdc)
        .unwrap();
    let mut grown_options = options();
    grown_options.graphics_allocation.search = 0x1_0000..0x1_8000;
    grown_options.acts_like_allocation.search = 0x1_0000..0x1_8000;
    controller
        .save_to_project(&mut staged, &grown_options)
        .unwrap();

    assert_eq!(staged.load_map16_set(layout()).unwrap(), *controller.set());
    assert_eq!(staged.rom.logical_len(), 0x1_8000);
    assert_eq!(app.project().unwrap().save_snapshot(), baseline);
    let logical = staged.rom.logical_bytes();
    assert_eq!(
        lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
        lm_rom::compute_snes_checksum(logical, 0x7fdc).unwrap()
    );
}

#[test]
fn owned_complete_set_commit_reclaims_every_snapshot_plane_and_undo_restores_it() {
    let (rom, manifest) = tagged_test_rom();
    let mut app = AppState::default();
    app.load_rom(rom).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = Map16Controller::decode(&snapshot, layout()).unwrap();
    assert_eq!(
        controller.previous_graphics,
        vec![
            Some(manifest.owned[0].clone()),
            Some(manifest.owned[2].clone())
        ]
    );
    assert_eq!(
        controller.previous_acts_like,
        vec![
            Some(manifest.owned[1].clone()),
            Some(manifest.owned[3].clone())
        ]
    );
    controller
        .apply_edits(&[
            Map16ControllerEdit::SetSubtile {
                address: Map16Address { page: 0, tile: 0 },
                quadrant: Map16Quadrant::TopLeft,
                subtile: Subtile(0x1111),
                resolution_limit: 512,
            },
            Map16ControllerEdit::SetActsLike {
                address: Map16Address { page: 0, tile: 7 },
                acts_like: 8,
                resolution_limit: 512,
            },
            Map16ControllerEdit::SetSubtile {
                address: Map16Address { page: 1, tile: 0 },
                quadrant: Map16Quadrant::TopLeft,
                subtile: Subtile(0x2222),
                resolution_limit: 512,
            },
            Map16ControllerEdit::SetActsLike {
                address: Map16Address { page: 1, tile: 7 },
                acts_like: 8,
                resolution_limit: 512,
            },
        ])
        .unwrap();
    let prepared = controller
        .prepare_commit_with_reclamation("Owned Map16 edit", &options(), &manifest)
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
        app.project().unwrap().load_map16_set(layout()).unwrap(),
        *controller.set()
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
fn late_invalid_edit_rolls_back_and_stale_commit_cannot_expand() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = Map16Controller::decode(&snapshot, layout()).unwrap();
    assert!(!controller.is_modified());
    assert!(
        controller
            .prepare_commit("unchanged", &options())
            .unwrap()
            .mutation
            .is_empty()
    );
    let original = controller.set().clone();
    assert!(matches!(
        controller.apply_edits(&[
            Map16ControllerEdit::SetSubtile {
                address: Map16Address { page: 0, tile: 0 },
                quadrant: Map16Quadrant::TopLeft,
                subtile: Subtile(7),
                resolution_limit: 512,
            },
            Map16ControllerEdit::SetActsLike {
                address: Map16Address { page: 9, tile: 0 },
                acts_like: 0,
                resolution_limit: 512,
            },
        ]),
        Err(Map16ControllerError::Edit { command: 1, .. })
    ));
    assert_eq!(controller.set(), &original);
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
fn wrong_mode_mapper_and_table_shapes_are_rejected() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let level_snapshot = app.controller_snapshot().unwrap();
    assert!(matches!(
        Map16Controller::decode(&level_snapshot, layout()),
        Err(Map16ControllerError::WrongMode(EditorMode::Level(0x105)))
    ));
    app.dispatch(Command::ShowMap16).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut wrong_mapper = layout();
    wrong_mapper.mapper = Mapper::Sa1;
    assert!(matches!(
        Map16Controller::decode(&snapshot, wrong_mapper),
        Err(Map16ControllerError::MapperMismatch { .. })
    ));
    let mut unequal = layout();
    unequal.acts_like.entries = 1;
    assert!(matches!(
        Map16Controller::decode(&snapshot, unequal),
        Err(Map16ControllerError::Io(Map16SetIoError::TableCount { .. }))
    ));
}
