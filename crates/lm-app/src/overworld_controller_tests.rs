use super::*;
use crate::{AppError, AppState, Command, FrontendEffect};
use lm_graphics::{Bgr555, CompactExAnimation, ExAnimationRecord, Palette, PaletteEntryOwner};
use lm_overworld::{EventRevealTable, OverworldLayer, OverworldMessage, Submap};
use lm_project::{
    CompleteOverworldSaveOptions, EndpointRomLayout, EndpointSaveOptions, EventRevealRomLayout,
    EventRevealSaveOptions, ExAnimationRomLayout, ExAnimationSaveOptions, LevelPointerTable,
    MessageRomLayout, MessageSaveOptions, OverworldLayers, OverworldLayersRomLayout,
    OverworldSaveOptions, PaletteRomLayout, PaletteSaveOptions, Project, RatsOwnershipManifest,
    SpriteRomLayout, SpriteSaveOptions,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::RomImage;

const MODES: [bool; 256] = [false; 256];
const POINTERS: [usize; 9] = [
    0x200, 0x210, 0x220, 0x230, 0x240, 0x250, 0x260, 0x270, 0x280,
];

fn table(offset: usize) -> LevelPointerTable {
    LevelPointerTable {
        offset,
        entries: 1,
        stride: 3,
    }
}

fn layout() -> CompleteOverworldRomLayout {
    CompleteOverworldRomLayout {
        layers: OverworldLayersRomLayout {
            mapper: Mapper::LoRom,
            layer1: table(POINTERS[0]),
            layer2: table(POINTERS[1]),
            width: 2,
            height: 2,
        },
        event_reveals: EventRevealRomLayout {
            mapper: Mapper::LoRom,
            sources: table(POINTERS[2]),
            destinations: table(POINTERS[3]),
            entries_per_slot: 1,
        },
        endpoints: EndpointRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(POINTERS[4]),
            endpoints_per_slot: 1,
        },
        messages: MessageRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(POINTERS[5]),
            messages_per_slot: 1,
        },
        sprites: SpriteRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(POINTERS[6]),
            sprites_per_slot: 1,
            record_len: 9,
        },
        palette: PaletteRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(POINTERS[7]),
            colors_per_palette: 16,
        },
        animation: ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(POINTERS[8]),
            maximum_records: 32,
            maximum_encoded_len: 0x4000,
        },
    }
}

fn shape() -> CompleteOverworldShape {
    CompleteOverworldShape {
        width: 2,
        height: 2,
        event_reveals: 1,
        endpoints: 1,
        messages: 1,
        sprites: 1,
        sprite_record_len: 9,
        palette_colors: 16,
    }
}

fn animation_record(value: u8) -> ExAnimationRecord {
    ExAnimationRecord::new(1, 0, 0, 0x1234, false, &[value, value + 1], false).unwrap()
}

fn data() -> CompleteOverworldData {
    CompleteOverworldData {
        layers: OverworldLayers {
            layer1: OverworldLayer::new(2, 2, vec![1, 2, 3, 4]).unwrap(),
            layer2: OverworldLayer::new(2, 2, vec![5, 6, 7, 8]).unwrap(),
        },
        event_reveals: EventRevealTable {
            entries: vec![EventReveal {
                source_tile: 1,
                destination_tile: 2,
            }],
        },
        endpoints: vec![OverworldEndpoint {
            x: 1,
            y: 2,
            submap: 0,
        }],
        messages: vec![OverworldMessage::decode(&[0x11; OverworldMessage::ENCODED_LEN]).unwrap()],
        sprites: vec![OverworldSprite {
            id: 1,
            x: 2,
            y: 3,
            submap: Submap::Main,
            extra: vec![0xaa, 0xbb],
        }],
        palette: Palette {
            colors: (0_u16..16).map(Bgr555).collect(),
        },
        animation: CompactExAnimation {
            setting: 1,
            header_value: 0x1234_5678,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![animation_record(4)],
        },
    }
}

fn save_options(search: std::ops::Range<usize>) -> CompleteOverworldSaveOptions {
    let allocation = AllocationPolicy {
        search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: POINTERS
            .iter()
            .map(|offset| ProtectedRange(*offset..*offset + 3))
            .chain(std::iter::once(ProtectedRange(0x7fdc..0x7fe0)))
            .collect(),
    };
    CompleteOverworldSaveOptions {
        layers: OverworldSaveOptions {
            layer1_allocation: allocation.clone(),
            layer2_allocation: allocation.clone(),
            previous_layer1: None,
            previous_layer2: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        event_reveals: EventRevealSaveOptions {
            source_allocation: allocation.clone(),
            destination_allocation: allocation.clone(),
            previous_sources: None,
            previous_destinations: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        endpoints: EndpointSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        messages: MessageSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        sprites: SpriteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        palette: PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        animation: ExAnimationSaveOptions {
            allocation,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    }
}

fn tagged_test_rom() -> (Vec<u8>, RatsOwnershipManifest) {
    let mut bytes = vec![0xff; 0x8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let saved = project
        .save_complete_overworld(0, &data(), layout(), &save_options(0x1000..0x7000), &MODES)
        .unwrap();
    project.refresh_checksum(0x7fdc).unwrap();
    (
        project.save_snapshot(),
        RatsOwnershipManifest {
            owned: vec![
                saved.layer1.block,
                saved.layer2.block,
                saved.event_sources.block,
                saved.event_destinations.block,
                saved.endpoints.block,
                saved.messages.block,
                saved.sprites.block,
                saved.palette.block,
                saved.animation.block,
            ],
            retained: Vec::new(),
        },
    )
}

fn test_rom() -> Vec<u8> {
    tagged_test_rom().0
}

#[test]
fn mixed_domains_expand_dispatch_reload_and_undo_as_one() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowOverworld).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = OverworldController::decode(
        &snapshot,
        0,
        layout(),
        &MODES,
        PaletteOwnership::editable(16),
    )
    .unwrap();
    controller
        .apply_edits(&[
            OverworldControllerEdit::SetLayerTile {
                layer: OverworldLayerId::Layer2,
                x: 1,
                y: 0,
                tile: 0x1234,
            },
            OverworldControllerEdit::ReplaceEventReveal {
                index: 0,
                reveal: EventReveal {
                    source_tile: 9,
                    destination_tile: 10,
                },
            },
            OverworldControllerEdit::SetMessageTile {
                message: 0,
                column: 17,
                row: 7,
                tile: 0x44,
            },
            OverworldControllerEdit::ReplaceSprite {
                index: 0,
                sprite: OverworldSprite {
                    id: 7,
                    x: 8,
                    y: 9,
                    submap: Submap::StarWorld,
                    extra: vec![0xcc, 0xdd],
                },
            },
            OverworldControllerEdit::PaletteChanges(vec![PaletteChange {
                index: 3,
                color: Bgr555(0x9234),
            }]),
            OverworldControllerEdit::Animation(vec![
                ExAnimationControllerEdit::SetTrigger {
                    trigger: 4,
                    value: Some(0xaa),
                },
                ExAnimationControllerEdit::ReplaceRecord {
                    index: 0,
                    record: animation_record(12),
                },
            ]),
        ])
        .unwrap();
    let prepared = controller
        .prepare_commit("Edit complete overworld", &save_options(0x8000..0x10000))
        .unwrap();
    assert_eq!(prepared.mutation.appended.len(), 0x8000);
    assert_eq!(
        app.dispatch(prepared.into_command()).unwrap(),
        [FrontendEffect::ProjectChanged {
            description: "Edit complete overworld".into(),
            mode: EditorMode::Overworld,
            revision: 1,
        }]
    );
    assert_eq!(
        app.project()
            .unwrap()
            .load_complete_overworld(0, layout(), &MODES)
            .unwrap(),
        *controller.data()
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

fn every_payload_edit() -> Vec<OverworldControllerEdit> {
    vec![
        OverworldControllerEdit::SetLayerTile {
            layer: OverworldLayerId::Layer1,
            x: 0,
            y: 0,
            tile: 0x1111,
        },
        OverworldControllerEdit::SetLayerTile {
            layer: OverworldLayerId::Layer2,
            x: 1,
            y: 0,
            tile: 0x2222,
        },
        OverworldControllerEdit::ReplaceEventReveal {
            index: 0,
            reveal: EventReveal {
                source_tile: 9,
                destination_tile: 10,
            },
        },
        OverworldControllerEdit::ReplaceEndpoint {
            index: 0,
            endpoint: OverworldEndpoint {
                x: 9,
                y: 10,
                submap: 2,
            },
        },
        OverworldControllerEdit::ReplaceMessage {
            index: 0,
            message: OverworldMessage::decode(&[0x42; OverworldMessage::ENCODED_LEN]).unwrap(),
        },
        OverworldControllerEdit::ReplaceSprite {
            index: 0,
            sprite: OverworldSprite {
                id: 7,
                x: 8,
                y: 9,
                submap: Submap::StarWorld,
                extra: vec![0xcc, 0xdd],
            },
        },
        OverworldControllerEdit::PaletteChanges(vec![PaletteChange {
            index: 3,
            color: Bgr555(0x1234),
        }]),
        OverworldControllerEdit::Animation(vec![ExAnimationControllerEdit::ReplaceRecord {
            index: 0,
            record: animation_record(12),
        }]),
    ]
}

#[test]
fn complete_file_replacement_is_atomic_validated_and_slot_agnostic() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowOverworld).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut owners = vec![PaletteEntryOwner::Editable; 16];
    owners[2] = PaletteEntryOwner::Fixed;
    let mut controller = OverworldController::decode(
        &snapshot,
        0,
        layout(),
        &MODES,
        PaletteOwnership::from_owners(owners),
    )
    .unwrap();
    let baseline = controller.data().clone();
    let mut imported = data();
    imported.layers.layer1.tiles[0] = 0x1234;
    let file = CompleteOverworldFile {
        source_slot: 0x1ff,
        shape: shape(),
        data: imported.clone(),
    };
    controller.replace_complete_file(&file, shape()).unwrap();
    assert_eq!(controller.data(), &imported);
    assert!(controller.is_modified());

    let mut wrong_shape = file.clone();
    wrong_shape.shape.width += 1;
    assert!(
        controller
            .replace_complete_file(&wrong_shape, shape())
            .is_err()
    );
    assert_eq!(controller.data(), &imported);

    let mut protected_palette = file.clone();
    protected_palette.data.palette.colors[2].0 ^= 1;
    let protected_error = controller
        .replace_complete_file(&protected_palette, shape())
        .unwrap_err();
    assert!(
        matches!(protected_error, OverworldControllerError::Palette { .. }),
        "{protected_error:?}"
    );
    assert_eq!(controller.data(), &imported);

    let mut too_many = file;
    too_many.data.animation.records = vec![animation_record(1); 33];
    assert!(
        controller
            .replace_complete_file(&too_many, shape())
            .is_err()
    );
    assert_eq!(controller.data(), &imported);
    assert_ne!(controller.data(), &baseline);
}

#[test]
fn owned_complete_overworld_reclaims_nine_snapshot_blocks_and_undo_restores_them() {
    let (rom, manifest) = tagged_test_rom();
    let mut app = AppState::default();
    app.load_rom(rom).unwrap();
    app.dispatch(Command::ShowOverworld).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = OverworldController::decode(
        &snapshot,
        0,
        layout(),
        &MODES,
        PaletteOwnership::editable(16),
    )
    .unwrap();
    assert_eq!(
        controller.previous_blocks,
        std::array::from_fn(|index| Some(manifest.owned[index].clone()))
    );
    controller.apply_edits(&every_payload_edit()).unwrap();
    let prepared = controller
        .prepare_commit_with_reclamation(
            "Owned complete overworld edit",
            &save_options(0x8000..0x10000),
            &manifest,
        )
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
            .load_complete_overworld(0, layout(), &MODES)
            .unwrap(),
        *controller.data()
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
fn late_cross_domain_failure_rolls_back_and_stale_commit_is_rejected() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowOverworld).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut owners = vec![PaletteEntryOwner::Editable; 16];
    owners[2] = PaletteEntryOwner::Fixed;
    let mut controller = OverworldController::decode(
        &snapshot,
        0,
        layout(),
        &MODES,
        PaletteOwnership::from_owners(owners),
    )
    .unwrap();
    assert!(!controller.is_modified());
    assert!(
        controller
            .prepare_commit("unchanged", &save_options(0x8000..0x10000))
            .unwrap()
            .mutation
            .is_empty()
    );
    let original = controller.data().clone();
    assert!(matches!(
        controller.apply_edits(&[
            OverworldControllerEdit::SetLayerTile {
                layer: OverworldLayerId::Layer1,
                x: 0,
                y: 0,
                tile: 99,
            },
            OverworldControllerEdit::PaletteChanges(vec![PaletteChange {
                index: 2,
                color: Bgr555(7),
            }]),
        ]),
        Err(OverworldControllerError::Palette { command: 1, .. })
    ));
    assert_eq!(controller.data(), &original);
    let prepared = controller
        .prepare_commit("stale", &save_options(0x8000..0x10000))
        .unwrap();
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
fn whole_message_replacement_is_atomic_and_rolls_back_on_a_late_invalid_index() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowOverworld).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = OverworldController::decode(
        &snapshot,
        0,
        layout(),
        &MODES,
        PaletteOwnership::editable(16),
    )
    .unwrap();
    let replacement = OverworldMessage::decode(&[0x42; OverworldMessage::ENCODED_LEN]).unwrap();
    controller
        .apply_edits(&[OverworldControllerEdit::ReplaceMessage {
            index: 0,
            message: replacement.clone(),
        }])
        .unwrap();
    assert_eq!(controller.data().messages[0], replacement);
    assert!(controller.is_modified());

    let before_failed_batch = controller.data().clone();
    assert!(matches!(
        controller.apply_edits(&[
            OverworldControllerEdit::ReplaceMessage {
                index: 0,
                message: OverworldMessage::decode(&[0x77; OverworldMessage::ENCODED_LEN]).unwrap(),
            },
            OverworldControllerEdit::ReplaceMessage {
                index: 1,
                message: replacement,
            },
        ]),
        Err(OverworldControllerError::Edit {
            command: 1,
            error: OverworldEditError::IndexOutOfBounds { index: 1, len: 1 }
        })
    ));
    assert_eq!(controller.data(), &before_failed_batch);
}

#[test]
fn event_source_that_would_normalize_rolls_back_earlier_domain_edits() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowOverworld).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = OverworldController::decode(
        &snapshot,
        0,
        layout(),
        &MODES,
        PaletteOwnership::editable(16),
    )
    .unwrap();
    let original = controller.data().clone();
    assert!(matches!(
        controller.apply_edits(&[
            OverworldControllerEdit::SetLayerTile {
                layer: OverworldLayerId::Layer1,
                x: 0,
                y: 0,
                tile: 99,
            },
            OverworldControllerEdit::ReplaceEventReveal {
                index: 0,
                reveal: EventReveal {
                    source_tile: EventRevealTable::MAX_TILE + 1,
                    destination_tile: 10,
                },
            },
        ]),
        Err(OverworldControllerError::Edit {
            command: 1,
            error: OverworldEditError::EventReveal(_)
        })
    ));
    assert_eq!(controller.data(), &original);
    assert!(!controller.is_modified());
}

#[test]
fn wrong_mode_mapper_modes_and_sprite_shape_are_rejected() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let level_snapshot = app.controller_snapshot().unwrap();
    assert!(matches!(
        OverworldController::decode(
            &level_snapshot,
            0,
            layout(),
            &MODES,
            PaletteOwnership::editable(16)
        ),
        Err(OverworldControllerError::WrongMode(EditorMode::Level(
            0x105
        )))
    ));
    app.dispatch(Command::ShowOverworld).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut wrong_mapper = layout();
    wrong_mapper.layers.mapper = Mapper::Sa1;
    assert!(matches!(
        OverworldController::decode(
            &snapshot,
            0,
            wrong_mapper,
            &MODES,
            PaletteOwnership::editable(16)
        ),
        Err(OverworldControllerError::MapperMismatch { .. })
    ));
    assert!(matches!(
        OverworldController::decode(
            &snapshot,
            0,
            layout(),
            &[false; 255],
            PaletteOwnership::editable(16)
        ),
        Err(OverworldControllerError::SizeModeCount(255))
    ));
    let mut controller = OverworldController::decode(
        &snapshot,
        0,
        layout(),
        &MODES,
        PaletteOwnership::editable(16),
    )
    .unwrap();
    assert!(matches!(
        controller.apply_edits(&[OverworldControllerEdit::ReplaceSprite {
            index: 0,
            sprite: OverworldSprite {
                id: 1,
                x: 2,
                y: 3,
                submap: Submap::Main,
                extra: vec![0],
            },
        }]),
        Err(OverworldControllerError::Edit { .. })
    ));
}
