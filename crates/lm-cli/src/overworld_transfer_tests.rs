use super::*;
use lm_graphics::{Bgr555, CompactExAnimation, Palette};
use lm_overworld::{
    EventReveal, EventRevealTable, OverworldEndpoint, OverworldLayer, OverworldMessage,
    OverworldSprite, Submap,
};
use lm_project::{CompleteOverworldShape, OverworldLayers, Project, RatsOwnershipManifest};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{compute_snes_checksum, pc_to_snes};

fn descriptor() -> OverworldLayoutDescriptor {
    OverworldLayoutDescriptor {
        layer1_table: 0x20,
        layer2_table: 0x620,
        event_source_table: 0xc20,
        event_destination_table: 0x1220,
        endpoint_table: 0x1820,
        message_table: 0x1e20,
        sprite_table: 0x2420,
        palette_table: 0x2a20,
        animation_table: 0x3020,
        width: 2,
        height: 2,
        event_reveals: 2,
        endpoints: 2,
        messages: 2,
        sprites: 2,
        sprite_record_len: 9,
        palette_colors: 16,
        animation_max_records: 32,
        animation_max_encoded: 0x4000,
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
            setting: 1,
            header_value: 2,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: Vec::new(),
        },
    }
}

fn input_rom(modes: &[bool]) -> Vec<u8> {
    let descriptor = descriptor();
    let data = data();
    let (sources, destinations) = data.event_reveals.encode().unwrap();
    let payloads = [
        data.layers.layer1.encode_le().unwrap(),
        data.layers.layer2.encode_le().unwrap(),
        sources,
        destinations,
        OverworldEndpoint::encode_all(&data.endpoints).unwrap(),
        OverworldMessage::encode_all(&data.messages).unwrap(),
        OverworldSprite::encode_all(&data.sprites, descriptor.sprite_record_len).unwrap(),
        data.palette.encode_snes().unwrap(),
        data.animation.encode(modes).unwrap(),
    ];
    let locations = [
        0x4000, 0x4100, 0x4200, 0x4300, 0x4400, 0x4500, 0x4700, 0x4800, 0x4900,
    ];
    let mut bytes = vec![0xff; 0x2_0000];
    for ((table, location), payload) in descriptor
        .pointer_tables()
        .into_iter()
        .zip(locations)
        .zip(payloads)
    {
        let snes = pc_to_snes(Mapper::LoRom, location).unwrap().to_le_bytes();
        bytes[table + 3..table + 6].copy_from_slice(&snes[..3]);
        bytes[location..location + payload.len()].copy_from_slice(&payload);
    }
    bytes[0x7fdc..0x7fe0].fill(0);
    bytes
}

#[test]
fn import_commits_nine_payloads_repairs_checksum_and_reopens() {
    let modes = [false; 256];
    let descriptor = descriptor();
    let output = import_image(
        input_rom(&modes),
        OverworldTargetSpec {
            slot: 1,
            descriptor,
            mapper: Mapper::LoRom,
        }
        .interpreted(&modes),
        ImportDocument {
            data: &data(),
            shape: descriptor.shape(),
        },
        ImportPolicy {
            checksum_field: 0x7fdc,
            search: 0x1_0000..0x1_f000,
        },
        None,
    )
    .unwrap();
    let project = Project::new(RomImage::from_bytes(output).unwrap());
    assert_eq!(
        project
            .load_complete_overworld(1, descriptor.rom_layout(Mapper::LoRom), &modes)
            .unwrap(),
        data()
    );
    let checksum = compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap();
    assert_eq!(
        &project.rom.logical_bytes()[0x7fdc..0x7fe0],
        &[
            checksum.complement.to_le_bytes(),
            checksum.checksum.to_le_bytes()
        ]
        .concat()
    );
}

#[test]
fn shape_mismatch_is_rejected_before_pointer_loading() {
    let descriptor = descriptor();
    let modes = [false; 256];
    assert!(
        import_image(
            vec![0xff; 0x2_0000],
            OverworldTargetSpec {
                slot: 1,
                descriptor,
                mapper: Mapper::LoRom,
            }
            .interpreted(&modes),
            ImportDocument {
                data: &data(),
                shape: CompleteOverworldShape {
                    width: 3,
                    ..descriptor.shape()
                },
            },
            ImportPolicy {
                checksum_field: 0x7fdc,
                search: 0x1_0000..0x1_f000,
            },
            None,
        )
        .is_err()
    );
}

fn tagged_input(modes: &[bool]) -> (Vec<u8>, RatsOwnershipManifest) {
    let descriptor = descriptor();
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x2_0000]).unwrap());
    let allocation = AllocationPolicy {
        search: 0x1_0000..0x1_f000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![ProtectedRange(0x7fdc..0x7fe0)],
    };
    let saved = project
        .save_complete_overworld_with_checksum(
            1,
            &data(),
            descriptor.rom_layout(Mapper::LoRom),
            &save_options(allocation, PreviousBlocks::default()),
            modes,
            0x7fdc,
        )
        .unwrap();
    let mut owned = vec![
        saved.layer1.block,
        saved.layer2.block,
        saved.event_sources.block,
        saved.event_destinations.block,
        saved.endpoints.block,
        saved.messages.block,
        saved.sprites.block,
        saved.palette.block,
        saved.animation.block,
    ];
    owned.sort_by_key(|block| block.header_offset);
    owned.dedup();
    (
        project.save_snapshot(),
        RatsOwnershipManifest {
            owned,
            retained: Vec::new(),
        },
    )
}

#[test]
fn ownership_backed_import_reclaims_all_displaced_overworld_blocks() {
    let modes = [false; 256];
    let descriptor = descriptor();
    let (input, manifest) = tagged_input(&modes);
    let mut replacement = data();
    replacement.layers.layer1.tiles[0] = 0x101;
    replacement.layers.layer2.tiles[0] = 0x202;
    replacement.event_reveals.entries[0].source_tile = 0x303;
    replacement.event_reveals.entries[0].destination_tile = 0x404;
    replacement.endpoints[0].x = 9;
    replacement.messages[0] =
        OverworldMessage::decode(&[0x33; OverworldMessage::ENCODED_LEN]).unwrap();
    replacement.sprites[0].id = 9;
    replacement.palette.colors[0] = Bgr555(0x1234);
    replacement.animation.header_value = 9;
    let output = import_image(
        input,
        OverworldTargetSpec {
            slot: 1,
            descriptor,
            mapper: Mapper::LoRom,
        }
        .interpreted(&modes),
        ImportDocument {
            data: &replacement,
            shape: descriptor.shape(),
        },
        ImportPolicy {
            checksum_field: 0x7fdc,
            search: 0x1_0000..0x1_f000,
        },
        Some(&manifest),
    )
    .unwrap();
    for block in &manifest.owned {
        assert!(output[block.full_range()].iter().all(|byte| *byte == 0xff));
    }
    let reopened = Project::new(RomImage::from_bytes(output.clone()).unwrap());
    assert_eq!(
        reopened
            .load_complete_overworld(1, descriptor.rom_layout(Mapper::LoRom), &modes)
            .unwrap(),
        replacement
    );
    assert_eq!(
        &output[0x7fdc..0x7fe0],
        &compute_snes_checksum(&output, 0x7fdc).unwrap().encoded()
    );
}

#[test]
fn ownership_backed_import_retains_every_reused_overworld_block() {
    let modes = [false; 256];
    let descriptor = descriptor();
    let (input, manifest) = tagged_input(&modes);
    let output = import_image(
        input,
        OverworldTargetSpec {
            slot: 1,
            descriptor,
            mapper: Mapper::LoRom,
        }
        .interpreted(&modes),
        ImportDocument {
            data: &data(),
            shape: descriptor.shape(),
        },
        ImportPolicy {
            checksum_field: 0x7fdc,
            search: 0x1_0000..0x1_f000,
        },
        Some(&manifest),
    )
    .unwrap();
    for block in &manifest.owned {
        assert_eq!(
            lm_rats::parse_at(&output, block.header_offset).unwrap(),
            *block
        );
    }
    let reopened = Project::new(RomImage::from_bytes(output).unwrap());
    assert_eq!(
        reopened
            .load_complete_overworld(1, descriptor.rom_layout(Mapper::LoRom), &modes)
            .unwrap(),
        data()
    );
}
