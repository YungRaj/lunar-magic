use super::*;
use crate::{AppError, AppState, Command, FrontendEffect};
use lm_graphics::PaletteEntryOwner;
use lm_project::{LevelPointerTable, PaletteSaveOptions, Project, RatsOwnershipManifest};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::RomImage;

fn layout() -> PaletteRomLayout {
    PaletteRomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: 0x200,
            entries: 2,
            stride: 3,
        },
        colors_per_palette: 32,
    }
}

fn palette() -> Palette {
    Palette {
        colors: (0_u16..32)
            .map(|value| Bgr555(value | if value % 3 == 0 { 0x8000 } else { 0 }))
            .collect(),
    }
}

fn test_rom() -> Vec<u8> {
    let mut bytes = vec![0xff; 0x8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    project
        .save_palette(
            1,
            &palette(),
            layout(),
            &PaletteSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x1000..0x4000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![ProtectedRange(0x200..0x206)],
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

fn options() -> PaletteSaveOptions {
    PaletteSaveOptions {
        allocation: AllocationPolicy {
            search: 0x8000..0x10000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x200..0x206), ProtectedRange(0x7fdc..0x7fe0)],
        },
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    }
}

#[test]
fn palette_saves_semantically_on_a_growing_recovery_project() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowPalette(1)).unwrap();
    let mut palette_controller = PaletteController::decode(
        &app.controller_snapshot().unwrap(),
        layout(),
        PaletteOwnership::editable(32),
    )
    .unwrap();
    palette_controller
        .apply_edits(&[PaletteControllerEdit::ReplaceRange {
            start: 5,
            colors: vec![Bgr555(0x1234)],
        }])
        .unwrap();
    let baseline = app.project().unwrap().save_snapshot();
    let mut staged = app.project().unwrap().clone();
    palette_controller
        .save_to_project(&mut staged, &options())
        .unwrap();

    assert!(staged.rom.logical_len() > baseline.len());
    assert_eq!(
        staged.load_palette(1, layout()).unwrap(),
        *palette_controller.palette()
    );
    assert_eq!(app.project().unwrap().save_snapshot(), baseline);
    let logical = staged.rom.logical_bytes();
    assert_eq!(
        lm_rom::SnesChecksum::decode(logical, 0x7fdc).unwrap(),
        lm_rom::compute_snes_checksum(logical, 0x7fdc).unwrap()
    );
}

#[test]
fn exact_words_edit_expand_dispatch_reload_and_undo() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowPalette(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        PaletteController::decode(&snapshot, layout(), PaletteOwnership::editable(32)).unwrap();
    let untouched_raw = controller.palette().colors[3];
    assert_eq!(untouched_raw, Bgr555(0x8003));
    controller
        .apply_edits(&[PaletteControllerEdit::ReplaceRange {
            start: 4,
            colors: vec![Bgr555(0xffff), Bgr555(0x9234)],
        }])
        .unwrap();
    assert_eq!(controller.palette().colors[3], untouched_raw);
    let prepared = controller
        .prepare_commit("Edit palette 001", &options())
        .unwrap();
    assert_eq!(prepared.mutation.appended.len(), 0x8000);
    assert_eq!(
        app.dispatch(prepared.into_command()).unwrap(),
        [FrontendEffect::ProjectChanged {
            description: "Edit palette 001".into(),
            mode: EditorMode::Palette(1),
            revision: 1,
        }]
    );
    assert_eq!(
        app.project().unwrap().load_palette(1, layout()).unwrap(),
        *controller.palette()
    );
    assert_eq!(
        app.project()
            .unwrap()
            .load_palette(1, layout())
            .unwrap()
            .colors[3],
        Bgr555(0x8003)
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
    app.dispatch(Command::Redo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x10000);
}

#[test]
fn owned_palette_commit_reclaims_snapshot_block_and_undo_restores_it() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowPalette(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        PaletteController::decode(&snapshot, layout(), PaletteOwnership::editable(32)).unwrap();
    let previous = controller.previous_block.clone().unwrap();
    controller
        .apply_edits(&[PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
            index: 4,
            color: Bgr555(0x4321),
        }])])
        .unwrap();
    let prepared = controller
        .prepare_commit_with_reclamation(
            "Owned palette edit",
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
        app.project().unwrap().load_palette(1, layout()).unwrap(),
        *controller.palette()
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
fn protected_late_edit_rolls_back_and_stale_commit_is_rejected() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowPalette(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut owners = vec![PaletteEntryOwner::Editable; 32];
    owners[7] = PaletteEntryOwner::ExAnimation { record: 2 };
    let mut controller =
        PaletteController::decode(&snapshot, layout(), PaletteOwnership::from_owners(owners))
            .unwrap();
    assert!(!controller.is_modified());
    assert!(
        controller
            .prepare_commit("unchanged", &options())
            .unwrap()
            .mutation
            .is_empty()
    );
    let original = controller.palette().clone();
    assert!(matches!(
        controller.apply_edits(&[
            PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
                index: 1,
                color: Bgr555(9),
            }]),
            PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
                index: 7,
                color: Bgr555(10),
            }]),
        ]),
        Err(PaletteControllerError::Edit { command: 1, .. })
    ));
    assert_eq!(controller.palette(), &original);
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
        PaletteController::decode(&level_snapshot, layout(), PaletteOwnership::editable(32)),
        Err(PaletteControllerError::WrongMode(EditorMode::Level(0x105)))
    ));
    app.dispatch(Command::ShowPalette(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut wrong_mapper = layout();
    wrong_mapper.mapper = Mapper::Sa1;
    assert!(matches!(
        PaletteController::decode(&snapshot, wrong_mapper, PaletteOwnership::editable(32)),
        Err(PaletteControllerError::MapperMismatch { .. })
    ));
    assert!(matches!(
        PaletteController::decode(&snapshot, layout(), PaletteOwnership::editable(31)),
        Err(PaletteControllerError::Edit {
            error: PaletteBatchEditError::OwnershipShape { .. },
            ..
        })
    ));
}

#[test]
fn raw_import_is_masked_row_zero_canonical_and_ownership_atomic() {
    let palette = Palette {
        colors: vec![Bgr555(0x7fff); RawSnesPaletteFile::COLOR_COUNT],
    };
    let mut owners = vec![lm_graphics::PaletteEntryOwner::Editable; palette.colors.len()];
    owners[2] = lm_graphics::PaletteEntryOwner::Fixed;
    let mut controller = PaletteController {
        revision: 0,
        palette_number: 0,
        layout: PaletteRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0,
                entries: 1,
                stride: 3,
            },
            colors_per_palette: RawSnesPaletteFile::COLOR_COUNT,
        },
        checksum_field_offset: 0,
        source_file_bytes: Vec::new(),
        baseline: palette.clone(),
        palette,
        ownership: PaletteOwnership::from_owners(owners),
        previous_block: None,
    };
    let source = RawSnesPaletteFile {
        palette: Palette {
            colors: (0_u16..=256).map(Bgr555).collect(),
        },
    };
    let mut mask = vec![0; PaletteMaskFile::FILE_LEN];
    mask[0] = 1;
    mask[1] = 0x80;
    mask[16] = 2;
    mask[256] = 1;
    controller
        .import_raw_palette(&source, &PaletteMaskFile::decode(&mask).unwrap())
        .unwrap();
    assert_eq!(controller.palette().colors[0], Bgr555(0));
    assert_eq!(controller.palette().colors[1], Bgr555(1));
    assert_eq!(controller.palette().colors[16], Bgr555(0));
    assert_eq!(controller.palette().colors[256], Bgr555(256));
    assert_eq!(controller.palette().colors[2], Bgr555(0x7fff));

    let accepted = controller.palette().clone();
    mask[2] = 1;
    assert!(
        controller
            .import_raw_palette(&source, &PaletteMaskFile::decode(&mask).unwrap())
            .is_err()
    );
    assert_eq!(controller.palette(), &accepted);
}

#[test]
fn supported_palette_mapping_matches_retained_tpl_oracle_and_is_ownership_atomic() {
    let palette = Palette {
        colors: (0_u16..=256).map(Bgr555).collect(),
    };
    let mut owners = vec![lm_graphics::PaletteEntryOwner::Editable; palette.colors.len()];
    owners[34] = lm_graphics::PaletteEntryOwner::Fixed;
    let mut controller = PaletteController {
        revision: 0,
        palette_number: 0,
        layout: PaletteRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0,
                entries: 1,
                stride: 3,
            },
            colors_per_palette: RawSnesPaletteFile::COLOR_COUNT,
        },
        checksum_field_offset: 0,
        source_file_bytes: Vec::new(),
        baseline: palette.clone(),
        palette,
        ownership: PaletteOwnership::from_owners(owners),
        previous_block: None,
    };
    let exported = controller.supported_palette().unwrap();
    assert_eq!(exported.colors[0], Bgr555(0));
    assert_eq!(exported.colors[1], Bgr555(2));
    assert_eq!(exported.colors[255], Bgr555(256));

    let mut imported = exported.clone();
    imported.colors[0] = Bgr555(0x1234);
    imported.colors[1] = Bgr555(0x2345);
    imported.colors[33] = Bgr555(34);
    controller.import_supported_palette(&imported).unwrap();
    assert_eq!(controller.palette().colors[0], Bgr555(0x1234));
    assert_eq!(controller.palette().colors[1], Bgr555(1));
    assert_eq!(controller.palette().colors[2], Bgr555(0x2345));
    for row in 1..16 {
        assert_eq!(
            controller.palette().colors[row * Palette::COLORS_PER_ROW + 1],
            Bgr555(0)
        );
    }

    let accepted = controller.palette().clone();
    imported.colors[33] = Bgr555(0x3456);
    assert!(controller.import_supported_palette(&imported).is_err());
    assert_eq!(controller.palette(), &accepted);
}

#[test]
fn supported_palette_mask_uses_file_indices_and_preserves_unselected_rows() {
    let palette = Palette {
        colors: (0_u16..=256).map(Bgr555).collect(),
    };
    let mut controller = PaletteController {
        revision: 0,
        palette_number: 0,
        layout: PaletteRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0,
                entries: 1,
                stride: 3,
            },
            colors_per_palette: RawSnesPaletteFile::COLOR_COUNT,
        },
        checksum_field_offset: 0,
        source_file_bytes: Vec::new(),
        baseline: palette.clone(),
        palette,
        ownership: PaletteOwnership::editable(RawSnesPaletteFile::COLOR_COUNT),
        previous_block: None,
    };
    let source = Palette {
        colors: vec![Bgr555(0x1234); 256],
    };
    let mut entries = vec![0; PaletteMaskFile::FILE_LEN];
    entries[0] = 1;
    entries[1] = 2;
    entries[16] = 0x80;
    entries[17] = 1;
    entries[32] = 0;
    entries[256] = 1;
    let mask = PaletteMaskFile::decode(&entries).unwrap();

    controller
        .import_supported_palette_with_mask(&source, &mask)
        .unwrap();
    assert_eq!(controller.palette().colors[0], Bgr555(0x1234));
    assert_eq!(controller.palette().colors[1], Bgr555(1));
    assert_eq!(controller.palette().colors[2], Bgr555(0x1234));
    assert_eq!(controller.palette().colors[17], Bgr555(0));
    assert_eq!(controller.palette().colors[18], Bgr555(0x1234));
    assert_eq!(controller.palette().colors[33], Bgr555(33));
    assert_eq!(controller.palette().colors[256], Bgr555(256));
}
