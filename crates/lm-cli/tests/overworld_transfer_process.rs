use lm_graphics::{Bgr555, CompactExAnimation, Palette};
use lm_overworld::{
    EventReveal, EventRevealTable, OverworldEndpoint, OverworldLayer, OverworldMessage,
    OverworldSprite, Submap,
};
use lm_project::{
    CompleteOverworldData, CompleteOverworldFile, CompleteOverworldRomLayout,
    CompleteOverworldSaveOptions, CompleteOverworldShape, EndpointRomLayout, EventRevealRomLayout,
    ExAnimationRomLayout, LevelPointerTable, MessageRomLayout, OverworldLayers,
    OverworldLayersRomLayout, PaletteRomLayout, Project, RatsOwnershipManifest,
    RatsOwnershipManifestFile, SavedCompleteOverworld, SpriteRomLayout,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SnesChecksum, compute_snes_checksum, pc_to_snes};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SLOT: usize = 1;
const MODES: [bool; 256] = [false; 256];
const TABLES: [usize; 9] = [
    0x20, 0x620, 0xc20, 0x1220, 0x1820, 0x1e20, 0x2420, 0x2a20, 0x3020,
];
const LOCATIONS: [usize; 9] = [
    0x4000, 0x4100, 0x4200, 0x4300, 0x4400, 0x4500, 0x4700, 0x4800, 0x4900,
];

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

fn replacement() -> CompleteOverworldData {
    let mut value = data();
    value.layers.layer1.tiles[0] = 0x101;
    value.layers.layer2.tiles[0] = 0x202;
    value.event_reveals.entries[0] = EventReveal {
        source_tile: 0x303,
        destination_tile: 0x404,
    };
    value.endpoints[0].x = 9;
    value.messages[0] = OverworldMessage::decode(&[0x33; OverworldMessage::ENCODED_LEN]).unwrap();
    value.sprites[0].id = 9;
    value.palette.colors[0] = Bgr555(0x1234);
    value.animation.header_value = 9;
    value
}

fn shape() -> CompleteOverworldShape {
    CompleteOverworldShape {
        width: 2,
        height: 2,
        event_reveals: 2,
        endpoints: 2,
        messages: 2,
        sprites: 2,
        sprite_record_len: 9,
        palette_colors: 16,
    }
}

fn layout() -> CompleteOverworldRomLayout {
    let table = |offset| LevelPointerTable {
        offset,
        entries: 0x200,
        stride: 3,
    };
    CompleteOverworldRomLayout {
        layers: OverworldLayersRomLayout {
            mapper: Mapper::LoRom,
            layer1: table(TABLES[0]),
            layer2: table(TABLES[1]),
            width: 2,
            height: 2,
        },
        event_reveals: EventRevealRomLayout {
            mapper: Mapper::LoRom,
            sources: table(TABLES[2]),
            destinations: table(TABLES[3]),
            entries_per_slot: 2,
        },
        endpoints: EndpointRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(TABLES[4]),
            endpoints_per_slot: 2,
        },
        messages: MessageRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(TABLES[5]),
            messages_per_slot: 2,
        },
        sprites: SpriteRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(TABLES[6]),
            sprites_per_slot: 2,
            record_len: 9,
        },
        palette: PaletteRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(TABLES[7]),
            colors_per_palette: 16,
        },
        animation: ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(TABLES[8]),
            maximum_records: 32,
            maximum_encoded_len: 0x4000,
        },
    }
}

fn write_fixture(path: &Path) {
    let value = data();
    let (sources, destinations) = value.event_reveals.encode().unwrap();
    let payloads = [
        value.layers.layer1.encode_le().unwrap(),
        value.layers.layer2.encode_le().unwrap(),
        sources,
        destinations,
        OverworldEndpoint::encode_all(&value.endpoints).unwrap(),
        OverworldMessage::encode_all(&value.messages).unwrap(),
        OverworldSprite::encode_all(&value.sprites, 9).unwrap(),
        value.palette.encode_snes().unwrap(),
        value.animation.encode(&MODES).unwrap(),
    ];
    let mut bytes = vec![0xff; 0x2_0000];
    for ((table, location), payload) in TABLES.into_iter().zip(LOCATIONS).zip(payloads) {
        let pointer = pc_to_snes(Mapper::LoRom, location).unwrap().to_le_bytes();
        bytes[table + SLOT * 3..table + SLOT * 3 + 3].copy_from_slice(&pointer[..3]);
        bytes[location..location + payload.len()].copy_from_slice(&payload);
    }
    let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    fs::write(path, bytes).unwrap();
}

fn descriptor() -> String {
    [
        "layer1_table=0x20",
        "layer2_table=0x620",
        "event_source_table=0xc20",
        "event_destination_table=0x1220",
        "endpoint_table=0x1820",
        "message_table=0x1e20",
        "sprite_table=0x2420",
        "palette_table=0x2a20",
        "animation_table=0x3020",
        "width=2",
        "height=2",
        "event_reveals=2",
        "endpoints=2",
        "messages=2",
        "sprites=2",
        "sprite_record_len=9",
        "palette_colors=16",
        "animation_max_records=32",
        "animation_max_encoded=0x4000",
    ]
    .join("\n")
}

fn invoke(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .args(arguments)
        .output()
        .unwrap()
}

fn saved_blocks(saved: SavedCompleteOverworld) -> Vec<lm_rats::RatsBlock> {
    vec![
        saved.layer1.block,
        saved.layer2.block,
        saved.event_sources.block,
        saved.event_destinations.block,
        saved.endpoints.block,
        saved.messages.block,
        saved.sprites.block,
        saved.palette.block,
        saved.animation.block,
    ]
}

#[test]
fn built_binary_transfers_all_complete_overworld_domains() {
    let directory = std::env::temp_dir().join(format!(
        "lm-overworld-transfer-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Input game.smc");
    let layout_file = directory.join("World layout.txt");
    let modes = directory.join("Size modes.bin");
    let exported = directory.join("Exported world.lmow");
    let replacement_file = directory.join("Replacement world.lmow");
    let output = directory.join("Imported game.smc");
    write_fixture(&input);
    fs::write(&layout_file, descriptor()).unwrap();
    fs::write(&modes, [0; 256]).unwrap();

    let export = invoke(&[
        "overworld-export",
        input.to_str().unwrap(),
        "lorom",
        "1",
        layout_file.to_str().unwrap(),
        modes.to_str().unwrap(),
        exported.to_str().unwrap(),
    ]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let decoded = CompleteOverworldFile::decode(&fs::read(&exported).unwrap(), 32, &MODES).unwrap();
    assert_eq!(decoded.data, data());

    fs::write(
        &replacement_file,
        CompleteOverworldFile {
            source_slot: 1,
            shape: shape(),
            data: replacement(),
        }
        .encode(&MODES)
        .unwrap(),
    )
    .unwrap();
    let import_arguments = [
        "overworld-import",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "1",
        layout_file.to_str().unwrap(),
        modes.to_str().unwrap(),
        replacement_file.to_str().unwrap(),
        "7fdc",
        "10000",
        "1f000",
    ];
    let import = invoke(&import_arguments);
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    let reopened = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
    assert_eq!(
        reopened
            .load_complete_overworld(SLOT, layout(), &MODES)
            .unwrap(),
        replacement()
    );
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    assert!(!invoke(&import_arguments).status.success());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn built_binary_owned_import_reclaims_all_nine_displaced_overworld_blocks() {
    let directory = std::env::temp_dir().join(format!(
        "lm-overworld-owned-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Owned input.smc");
    let layout_file = directory.join("World layout.txt");
    let modes = directory.join("Size modes.bin");
    let replacement_file = directory.join("Owned replacement.lmow");
    let manifest = directory.join("Ownership.lmrats");
    let output = directory.join("Owned output.smc");

    let mut protected: Vec<_> = TABLES
        .into_iter()
        .map(|start| ProtectedRange(start..start + 0x600))
        .collect();
    protected.push(ProtectedRange(0x7fdc..0x7fe0));
    let allocation = AllocationPolicy {
        search: 0x4000..0xf000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected,
    };
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x2_0000]).unwrap());
    let displaced_blocks = saved_blocks(
        project
            .save_complete_overworld(
                SLOT,
                &data(),
                layout(),
                &CompleteOverworldSaveOptions::uniform_allocation(allocation),
                &MODES,
            )
            .unwrap(),
    );
    project.refresh_checksum(0x7fdc).unwrap();
    fs::write(&input, project.save_snapshot()).unwrap();
    fs::write(&layout_file, descriptor()).unwrap();
    fs::write(&modes, [0; 256]).unwrap();
    fs::write(
        &replacement_file,
        CompleteOverworldFile {
            source_slot: u16::try_from(SLOT).unwrap(),
            shape: shape(),
            data: replacement(),
        }
        .encode(&MODES)
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &manifest,
        RatsOwnershipManifestFile(RatsOwnershipManifest {
            owned: displaced_blocks.clone(),
            retained: Vec::new(),
        })
        .encode()
        .unwrap(),
    )
    .unwrap();

    let import = invoke(&[
        "overworld-import-owned",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "1",
        layout_file.to_str().unwrap(),
        modes.to_str().unwrap(),
        replacement_file.to_str().unwrap(),
        "7fdc",
        "10000",
        "1f000",
        manifest.to_str().unwrap(),
    ]);
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    for block in &displaced_blocks {
        assert!(bytes[block.full_range()].iter().all(|byte| *byte == 0xff));
    }
    let reopened = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
    assert_eq!(
        reopened
            .load_complete_overworld(SLOT, layout(), &MODES)
            .unwrap(),
        replacement()
    );
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}
