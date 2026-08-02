use super::*;
use lm_app::{
    ExternalTool, RevisionProfile, ShortcutBinding, ShortcutConfig, ShortcutGesture, ShortcutKey,
    ShortcutModifiers, ToolEvent, ToolbarAction, ToolbarConfig, ToolbarItem, UiTextKey,
};
use lm_graphics::{
    Bgr555, CompactExAnimation, ExAnimationRecord, GraphicsFile4bpp, IndexedTile, Palette,
};
use lm_overworld::{
    EventReveal, EventRevealTable, OverworldEndpoint, OverworldLayer, OverworldMessage,
    OverworldSprite, Submap,
};
use lm_project::{
    CompleteOverworldData, CompleteOverworldSaveOptions, ExAnimationSaveOptions,
    GraphicsSaveOptions, LevelPointerTable, OverworldLayers, Project,
};
use lm_rats::AllocationPolicy;
use lm_rom::{Mapper, RomImage, compute_snes_checksum, pc_to_snes};

fn pointer_tables(profile: &RevisionProfile) -> [LevelPointerTable; 16] {
    [
        profile.level.layer1,
        profile.level.sprites.low_or_contiguous_table(),
        profile.map16.graphics,
        profile.map16.acts_like,
        profile.graphics.pointers,
        profile.palette.pointers,
        profile.exanimation.pointers,
        profile.overworld.layers.layer1,
        profile.overworld.layers.layer2,
        profile.overworld.event_reveals.sources,
        profile.overworld.event_reveals.destinations,
        profile.overworld.endpoints.pointers,
        profile.overworld.messages.pointers,
        profile.overworld.sprites.pointers,
        profile.overworld.palette.pointers,
        profile.overworld.animation.pointers,
    ]
}

fn fixture_overworld(profile: &RevisionProfile) -> CompleteOverworldData {
    let shape = profile.overworld_shape;
    let layer_len = shape.width * shape.height;
    CompleteOverworldData {
        layers: OverworldLayers {
            layer1: OverworldLayer::new(shape.width, shape.height, vec![1; layer_len]).unwrap(),
            layer2: OverworldLayer::new(shape.width, shape.height, vec![2; layer_len]).unwrap(),
        },
        event_reveals: EventRevealTable {
            entries: vec![
                EventReveal {
                    source_tile: 1,
                    destination_tile: 2,
                };
                shape.event_reveals
            ],
        },
        endpoints: vec![
            OverworldEndpoint {
                x: 1,
                y: 2,
                submap: 0,
            };
            shape.endpoints
        ],
        messages: vec![
            OverworldMessage::decode(&[0x11; OverworldMessage::ENCODED_LEN]).unwrap();
            shape.messages
        ],
        sprites: vec![
            OverworldSprite {
                id: 1,
                x: 2,
                y: 3,
                submap: Submap::Main,
                extra: vec![0xaa; shape.sprite_record_len - 7],
            };
            shape.sprites
        ],
        palette: Palette {
            colors: (0..shape.palette_colors)
                .map(|value| Bgr555(u16::try_from(value).unwrap()))
                .collect(),
        },
        animation: CompactExAnimation {
            setting: 1,
            header_value: 0x8765_4321,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: vec![ExAnimationRecord::new(1, 0, 0, 0x1111, false, &[4, 0], false).unwrap()],
        },
    }
}

#[allow(clippy::too_many_lines)]
fn profiled_rom(profile: &RevisionProfile) -> Vec<u8> {
    let mut bytes = vec![0xff; 0x40_8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x32;
    bytes[0x7fd6] = 2;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
    let default_pointer = pc_to_snes(Mapper::ExLoRom, 0x6200).unwrap().to_le_bytes();
    for table in pointer_tables(profile) {
        for index in 0..table.entries {
            let offset = table.pointer_offset(index).unwrap();
            bytes[offset..offset + 3].copy_from_slice(&default_pointer[..3]);
        }
    }
    let graphics_target = pc_to_snes(Mapper::ExLoRom, 0x7000).unwrap().to_le_bytes();
    let acts_target = pc_to_snes(Mapper::ExLoRom, 0x7800).unwrap().to_le_bytes();
    for index in 0..profile.map16.graphics.entries {
        let graphics = profile.map16.graphics.pointer_offset(index).unwrap();
        let acts = profile.map16.acts_like.pointer_offset(index).unwrap();
        bytes[graphics..graphics + 3].copy_from_slice(&graphics_target[..3]);
        bytes[acts..acts + 3].copy_from_slice(&acts_target[..3]);
    }
    bytes[0x7000..0x7a00].fill(0);
    let number = 0x105;
    let layer_pointer = profile.level.layer1.pointer_offset(number).unwrap();
    let sprite_pointer = profile
        .level
        .sprites
        .low_or_contiguous_table()
        .pointer_offset(number)
        .unwrap();
    let layer_target = pc_to_snes(Mapper::ExLoRom, 0x6000).unwrap().to_le_bytes();
    let sprite_target = pc_to_snes(Mapper::ExLoRom, 0x6100).unwrap().to_le_bytes();
    bytes[layer_pointer..layer_pointer + 3].copy_from_slice(&layer_target[..3]);
    bytes[sprite_pointer..sprite_pointer + 3].copy_from_slice(&sprite_target[..3]);
    bytes[0x6000..0x6009].copy_from_slice(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]);
    bytes[0x6100..0x6107].copy_from_slice(&[0x30, 0, 0, 1, 0xff, 0xfe, 0xff]);
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    project
        .save_graphics_file(
            2,
            &GraphicsFile4bpp {
                tiles: vec![
                    IndexedTile::new([0; 64]),
                    IndexedTile::new([1; 64]),
                    IndexedTile::new([2; 64]),
                ],
            },
            profile.graphics,
            &GraphicsSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x20_0000..0x21_0000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: Vec::new(),
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    project
        .save_exanimation(
            1,
            &CompactExAnimation {
                setting: 3,
                header_value: 0x1234_5678,
                trigger_mask: 1,
                trigger_values: [9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                records: vec![
                    ExAnimationRecord::new(1, 1, 0, 0x1234, true, &[1, 0, 2, 0], false).unwrap(),
                ],
            },
            profile.exanimation,
            &profile.exanimation_double_size_modes,
            &ExAnimationSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x22_0000..0x23_0000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: Vec::new(),
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    project
        .save_complete_overworld(
            0,
            &fixture_overworld(profile),
            profile.overworld,
            &CompleteOverworldSaveOptions::uniform_allocation(AllocationPolicy {
                search: 0x24_0000..0x26_0000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: Vec::new(),
            }),
            &profile.exanimation_double_size_modes,
        )
        .unwrap();
    project.refresh_checksum(0x7fdc).unwrap();
    project.save_snapshot()
}

#[path = "main_tests_documents.rs"]
mod documents;
#[path = "main_tests_native.rs"]
mod native;
#[path = "main_tests_policy.rs"]
mod policy;
