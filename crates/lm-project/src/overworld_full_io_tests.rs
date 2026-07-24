use super::*;
use crate::LevelPointerTable;
use lm_graphics::{Bgr555, ExAnimationError};
use lm_overworld::{EventReveal, EventTableError, OverworldLayer, Submap};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::RomImage;

const POINTERS: [usize; 9] = [0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0];

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
            entries_per_slot: 2,
        },
        endpoints: EndpointRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(POINTERS[4]),
            endpoints_per_slot: 2,
        },
        messages: MessageRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(POINTERS[5]),
            messages_per_slot: 2,
        },
        sprites: SpriteRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(POINTERS[6]),
            sprites_per_slot: 2,
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

fn policy() -> AllocationPolicy {
    AllocationPolicy {
        search: 0x100..0x1_8000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: POINTERS
            .iter()
            .map(|offset| ProtectedRange(*offset..*offset + 3))
            .collect(),
    }
}

fn options() -> CompleteOverworldSaveOptions {
    let allocation = policy();
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

fn data() -> CompleteOverworldData {
    CompleteOverworldData {
        layers: OverworldLayers {
            layer1: OverworldLayer::new(2, 2, vec![1, 2, 3, 4]).unwrap(),
            layer2: OverworldLayer::new(2, 2, vec![5, 6, 7, 8]).unwrap(),
        },
        event_reveals: EventRevealTable {
            entries: vec![
                EventReveal {
                    source_tile: 1,
                    destination_tile: 2,
                },
                EventReveal {
                    source_tile: 3,
                    destination_tile: 4,
                },
            ],
        },
        endpoints: vec![
            OverworldEndpoint {
                x: 1,
                y: 2,
                submap: 0,
            },
            OverworldEndpoint {
                x: 3,
                y: 4,
                submap: 1,
            },
        ],
        messages: vec![
            OverworldMessage::decode(&[0x11; OverworldMessage::ENCODED_LEN]).unwrap(),
            OverworldMessage::decode(&[0x22; OverworldMessage::ENCODED_LEN]).unwrap(),
        ],
        sprites: vec![
            OverworldSprite {
                id: 1,
                x: 2,
                y: 3,
                submap: Submap::Main,
                extra: vec![0xaa, 0xbb],
            },
            OverworldSprite {
                id: 4,
                x: 5,
                y: 6,
                submap: Submap::StarWorld,
                extra: vec![0xcc, 0xdd],
            },
        ],
        palette: Palette {
            colors: (0_u16..16).map(Bgr555).collect(),
        },
        animation: CompactExAnimation {
            setting: 3,
            header_value: 0x1234_5678,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: Vec::new(),
        },
    }
}

#[test]
fn complete_save_load_and_undo_are_one_atomic_operation() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x1_8000]).unwrap());
    let original = project.save_snapshot();
    project
        .save_complete_overworld(0, &data(), layout(), &options(), &[false; 256])
        .unwrap();
    assert_eq!(
        project
            .load_complete_overworld(0, layout(), &[false; 256])
            .unwrap(),
        data()
    );
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn aggregate_load_rejects_wrong_length_tagged_component() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x1_8000]).unwrap());
    let saved = project
        .save_complete_overworld(0, &data(), layout(), &options(), &[false; 256])
        .unwrap();
    project
        .rom
        .write(
            saved.layer1.block.header_offset,
            &lm_rats::make_header(saved.layer1.block.payload.len() + 1).unwrap(),
        )
        .unwrap();
    assert!(matches!(
        project.load_complete_overworld(0, layout(), &[false; 256]),
        Err(CompleteOverworldIoError::Layers(OverworldIoError::Load(
            crate::PayloadLoadError::TaggedLengthMismatch {
                actual: 9,
                expected: 8,
                ..
            }
        )))
    ));
}

#[test]
fn late_sprite_validation_failure_preserves_everything() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x1_8000]).unwrap());
    let original = project.save_snapshot();
    let mut invalid = data();
    invalid.sprites[1].extra.clear();
    assert!(matches!(
        project.save_complete_overworld(0, &invalid, layout(), &options(), &[false; 256]),
        Err(CompleteOverworldIoError::Sprites(SpriteIoError::Codec(_)))
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn invalid_event_source_is_rejected_before_complete_allocation() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x1_8000]).unwrap());
    let original = project.save_snapshot();
    let mut invalid = data();
    invalid.event_reveals.entries[1].source_tile = EventRevealTable::MAX_TILE + 1;
    assert!(matches!(
        project.save_complete_overworld(0, &invalid, layout(), &options(), &[false; 256]),
        Err(CompleteOverworldIoError::Events(
            EventRevealIoError::Decode(EventTableError::InvalidSourceTile {
                index: 1,
                tile: 0x800
            })
        ))
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn mixed_mapper_layout_is_rejected_before_mutation() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x1_8000]).unwrap());
    let original = project.save_snapshot();
    let mut mixed = layout();
    mixed.messages.mapper = Mapper::ExLoRom;
    assert!(matches!(
        project.save_complete_overworld(0, &data(), mixed, &options(), &[false; 256]),
        Err(CompleteOverworldIoError::MapperMismatch {
            domain: "messages",
            ..
        })
    ));
    assert_eq!(project.save_snapshot(), original);
}

#[test]
fn final_animation_validation_failure_preserves_everything() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x1_8000]).unwrap());
    let original = project.save_snapshot();
    let mut too_small = layout();
    too_small.animation.maximum_encoded_len = 7;
    assert!(matches!(
        project.save_complete_overworld(0, &data(), too_small, &options(), &[false; 256]),
        Err(CompleteOverworldIoError::Animation(
            ExAnimationIoError::EncodedLimit { .. }
        ))
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn unrepresented_animation_state_is_rejected_before_complete_allocation() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x1_8000]).unwrap());
    let original = project.save_snapshot();
    let mut invalid = data();
    invalid.animation.trigger_values[7] = 0xaa;
    assert!(matches!(
        project.save_complete_overworld(0, &invalid, layout(), &options(), &[false; 256]),
        Err(CompleteOverworldIoError::Animation(
            ExAnimationIoError::Animation(ExAnimationError::DisabledTriggerValue {
                trigger: 7,
                value: 0xaa
            })
        ))
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}
