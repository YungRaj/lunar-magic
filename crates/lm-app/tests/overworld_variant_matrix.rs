use lm_app::{
    ControllerSnapshot, EditorMode, ExAnimationControllerEdit,
    NativeCustomOverworldSpriteController, NativeCustomOverworldSpriteEdit, OverworldController,
    OverworldControllerEdit, OverworldLayerId,
};
use lm_graphics::{
    Bgr555, CompactExAnimation, ExAnimationRecord, Palette, PaletteChange, PaletteOwnership,
};
use lm_overworld::{
    EventReveal, EventRevealTable, NativeCustomOverworldSprite, NativeCustomOverworldSpriteTable,
    OverworldEndpoint, OverworldLayer, OverworldMessage, OverworldSprite, Submap,
};
use lm_project::{
    CompleteOverworldData, CompleteOverworldFile, CompleteOverworldRomLayout,
    CompleteOverworldSaveOptions, CompleteOverworldShape, EndpointRomLayout, EndpointSaveOptions,
    EventRevealRomLayout, EventRevealSaveOptions, ExAnimationRomLayout, ExAnimationSaveOptions,
    LevelPointerTable, MessageRomLayout, MessageSaveOptions, NativeCustomOverworldSpriteRomLayout,
    NativeCustomOverworldSpriteSaveOptions, OverworldLayers, OverworldLayersRomLayout,
    OverworldSaveOptions, PaletteRomLayout, PaletteSaveOptions, Project, SpriteRomLayout,
    SpriteSaveOptions,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, compute_snes_checksum, detect_identity};

const MODES: [bool; 256] = [false; 256];
const COPIER_PREFIX: [u8; 512] = {
    let mut prefix = [0_u8; 512];
    prefix[0] = 0x40;
    prefix[8] = 0xaa;
    prefix[9] = 0xbb;
    prefix[10] = 0x04;
    prefix
};

#[derive(Clone, Copy)]
struct IdentityCase {
    title: &'static [u8; 21],
    region: u8,
    map_mode: u8,
}

fn mapper(map_mode: u8) -> Mapper {
    match map_mode {
        0x20 | 0x30 => Mapper::LoRom,
        0x23 => Mapper::Sa1,
        0x32 => Mapper::ExLoRom,
        _ => unreachable!(),
    }
}

fn pointers(base: usize) -> [usize; 9] {
    std::array::from_fn(|index| base + index * 0x10)
}

fn table(offset: usize) -> LevelPointerTable {
    LevelPointerTable {
        offset,
        entries: 1,
        stride: 3,
    }
}

fn layout(mapper: Mapper, pointer_base: usize) -> CompleteOverworldRomLayout {
    let pointers = pointers(pointer_base);
    CompleteOverworldRomLayout {
        layers: OverworldLayersRomLayout {
            mapper,
            layer1: table(pointers[0]),
            layer2: table(pointers[1]),
            width: 2,
            height: 2,
        },
        event_reveals: EventRevealRomLayout {
            mapper,
            sources: table(pointers[2]),
            destinations: table(pointers[3]),
            entries_per_slot: 1,
        },
        endpoints: EndpointRomLayout {
            mapper,
            pointers: table(pointers[4]),
            endpoints_per_slot: 1,
        },
        messages: MessageRomLayout {
            mapper,
            pointers: table(pointers[5]),
            messages_per_slot: 1,
        },
        sprites: SpriteRomLayout {
            mapper,
            pointers: table(pointers[6]),
            sprites_per_slot: 1,
            record_len: 9,
        },
        palette: PaletteRomLayout {
            mapper,
            pointers: table(pointers[7]),
            colors_per_palette: 16,
        },
        animation: ExAnimationRomLayout {
            mapper,
            pointers: table(pointers[8]),
            maximum_records: 32,
            maximum_encoded_len: 0x2000,
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

fn animation_record(seed: u8) -> ExAnimationRecord {
    ExAnimationRecord::new(
        1,
        0,
        0,
        0x1200 + u16::from(seed),
        false,
        &[seed, seed.wrapping_add(1)],
        false,
    )
    .unwrap()
}

fn data(seed: u8) -> CompleteOverworldData {
    CompleteOverworldData {
        layers: OverworldLayers {
            layer1: OverworldLayer::new(
                2,
                2,
                (0..4).map(|value| u16::from(seed) + value).collect(),
            )
            .unwrap(),
            layer2: OverworldLayer::new(
                2,
                2,
                (4..8).map(|value| u16::from(seed) + value).collect(),
            )
            .unwrap(),
        },
        event_reveals: EventRevealTable {
            entries: vec![EventReveal {
                source_tile: u16::from(seed) + 8,
                destination_tile: u16::from(seed) + 9,
            }],
        },
        endpoints: vec![OverworldEndpoint {
            x: u16::from(seed) + 10,
            y: u16::from(seed) + 11,
            submap: seed % 7,
        }],
        messages: vec![OverworldMessage::decode(&[seed; OverworldMessage::ENCODED_LEN]).unwrap()],
        sprites: vec![OverworldSprite {
            id: u16::from(seed),
            x: u16::from(seed) + 1,
            y: u16::from(seed) + 2,
            submap: Submap::Main,
            extra: vec![seed.wrapping_add(3), seed.wrapping_add(4)],
        }],
        palette: Palette {
            colors: (0_u16..16)
                .map(|value| Bgr555(value + u16::from(seed)))
                .collect(),
        },
        animation: CompactExAnimation {
            setting: seed & 7,
            header_value: 0x1234_5600 | u32::from(seed),
            trigger_mask: 1,
            trigger_values: {
                let mut values = [0; 16];
                values[0] = seed;
                values
            },
            records: vec![animation_record(seed)],
        },
    }
}

fn options(search: std::ops::Range<usize>, pointer_base: usize) -> CompleteOverworldSaveOptions {
    let allocation = AllocationPolicy {
        search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: pointers(pointer_base)
            .into_iter()
            .map(|offset| ProtectedRange(offset..offset + 3))
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

fn variant_rom(case: IdentityCase, pointer_base: usize, copier_header: bool) -> Vec<u8> {
    let mapper = mapper(case.map_mode);
    let logical_len = if case.map_mode == 0x32 {
        0x40_8000
    } else {
        0x8000
    };
    let mut logical = vec![0xff; logical_len];
    let header = 0x7fc0;
    logical[header..header + 21].copy_from_slice(case.title);
    logical[header + 0x15] = case.map_mode;
    logical[header + 0x19] = case.region;
    logical[header + 0x1b] = 0;
    let checksum = compute_snes_checksum(&logical, header + 0x1c).unwrap();
    logical[header + 0x1c..header + 0x20].copy_from_slice(&checksum.encoded());

    let mut project = Project::new(RomImage::from_bytes(logical).unwrap());
    project
        .save_complete_overworld_with_checksum(
            0,
            &data(1),
            layout(mapper, pointer_base),
            &options(0x1000..0x4000, pointer_base),
            &MODES,
            header + 0x1c,
        )
        .unwrap();
    let logical = project.rom.logical_bytes();
    if copier_header {
        let mut physical = COPIER_PREFIX.to_vec();
        physical.extend_from_slice(logical);
        physical
    } else {
        logical.to_vec()
    }
}

fn edit_variant(physical: Vec<u8>, pointer_base: usize) -> Vec<u8> {
    let original = physical.clone();
    let image = RomImage::from_bytes(physical.clone()).unwrap();
    let identity = detect_identity(&image).unwrap();
    let layout = layout(identity.mapper, pointer_base);
    let snapshot = ControllerSnapshot {
        revision: 9,
        mode: EditorMode::Overworld,
        identity,
        document_path: None,
        rom_bytes: physical.clone(),
    };
    let mut controller =
        OverworldController::decode(&snapshot, 0, layout, &MODES, PaletteOwnership::editable(16))
            .unwrap();
    let encoded = CompleteOverworldFile {
        source_slot: 0x1ff,
        shape: shape(),
        data: data(0x20),
    }
    .encode(&MODES)
    .unwrap();
    let imported = CompleteOverworldFile::decode(&encoded, 32, &MODES).unwrap();
    controller
        .replace_complete_file(&imported, shape())
        .unwrap();
    let expected = controller.data().clone();
    let prepared = controller
        .prepare_commit(
            "Complete overworld supported-variant matrix",
            &options(0x4000..0x7000, pointer_base),
        )
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(physical).unwrap());
    project
        .apply_mutation(
            "Complete overworld supported-variant matrix",
            &prepared.mutation,
        )
        .unwrap();
    assert_eq!(
        project.load_complete_overworld(0, layout, &MODES).unwrap(),
        expected
    );
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    let edited = project.rom.as_file_bytes().to_vec();
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), original);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), edited);
    edited
}

fn edit_interactively_variant(physical: Vec<u8>, pointer_base: usize) -> Vec<u8> {
    let original = physical.clone();
    let image = RomImage::from_bytes(physical.clone()).unwrap();
    let identity = detect_identity(&image).unwrap();
    let layout = layout(identity.mapper, pointer_base);
    let snapshot = ControllerSnapshot {
        revision: 10,
        mode: EditorMode::Overworld,
        identity,
        document_path: None,
        rom_bytes: physical.clone(),
    };
    let mut controller =
        OverworldController::decode(&snapshot, 0, layout, &MODES, PaletteOwnership::editable(16))
            .unwrap();
    controller
        .apply_edits(&[
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
        ])
        .unwrap();
    let expected = controller.data().clone();
    let prepared = controller
        .prepare_commit(
            "Interactive overworld supported-variant matrix",
            &options(0x4000..0x7000, pointer_base),
        )
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(physical).unwrap());
    project
        .apply_mutation(
            "Interactive overworld supported-variant matrix",
            &prepared.mutation,
        )
        .unwrap();
    assert_eq!(
        project.load_complete_overworld(0, layout, &MODES).unwrap(),
        expected
    );
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    let edited = project.rom.as_file_bytes().to_vec();
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), original);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), edited);
    edited
}

fn native_sprite_layout(
    mapper: Mapper,
    pointer_offset: usize,
) -> NativeCustomOverworldSpriteRomLayout {
    NativeCustomOverworldSpriteRomLayout {
        mapper,
        pointer_offset,
        maximum_payload_len: 0x0fff,
    }
}

fn native_sprite(seed: u8, x: u16, y: u16) -> NativeCustomOverworldSprite {
    NativeCustomOverworldSprite {
        id: seed & 0x7f,
        x,
        y,
        screen: (seed & 3) * 8,
        extra: vec![seed.wrapping_add(0x40)],
    }
}

fn native_sprite_table(seed: u8) -> NativeCustomOverworldSpriteTable {
    NativeCustomOverworldSpriteTable {
        maps: std::array::from_fn(|map| {
            vec![native_sprite(
                seed.wrapping_add(u8::try_from(map).unwrap()),
                0x40 + u16::try_from(map).unwrap() * 8,
                0x80 + u16::try_from(map).unwrap() * 8,
            )]
        }),
    }
}

fn native_sprite_options(pointer_offset: usize) -> NativeCustomOverworldSpriteSaveOptions {
    NativeCustomOverworldSpriteSaveOptions {
        allocation: AllocationPolicy {
            search: 0x7000..0x7f00,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![
                ProtectedRange(pointer_offset..pointer_offset + 3),
                ProtectedRange(0x7fdc..0x7fe0),
            ],
        },
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    }
}

fn native_sprite_variant_rom(
    case: IdentityCase,
    pointer_offset: usize,
    copier_header: bool,
) -> Vec<u8> {
    let mut physical = variant_rom(case, 0x200, copier_header);
    let image = RomImage::from_bytes(physical).unwrap();
    let identity = detect_identity(&image).unwrap();
    let mut project = Project::new(image);
    project
        .save_native_custom_overworld_sprites(
            &native_sprite_table(1),
            &[4; 128],
            native_sprite_layout(identity.mapper, pointer_offset),
            &native_sprite_options(pointer_offset),
        )
        .unwrap();
    project
        .refresh_checksum(identity.internal_header_offset + 0x1c)
        .unwrap();
    physical = project.save_snapshot();
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    physical
}

fn edit_native_sprite_variant(physical: Vec<u8>, pointer_offset: usize) -> Vec<u8> {
    let original = physical.clone();
    let image = RomImage::from_bytes(physical.clone()).unwrap();
    let identity = detect_identity(&image).unwrap();
    let layout = native_sprite_layout(identity.mapper, pointer_offset);
    let snapshot = ControllerSnapshot {
        revision: 11,
        mode: EditorMode::Overworld,
        identity: identity.clone(),
        document_path: None,
        rom_bytes: physical.clone(),
    };
    let mut controller =
        NativeCustomOverworldSpriteController::decode(&snapshot, layout, [4; 128]).unwrap();
    controller
        .apply_edits(&[
            NativeCustomOverworldSpriteEdit::Insert {
                map: 0,
                index: 1,
                sprite: native_sprite(0x21, 0x100, 0x120),
            },
            NativeCustomOverworldSpriteEdit::MoveBefore {
                map: 0,
                from: 1,
                before: 0,
            },
            NativeCustomOverworldSpriteEdit::Replace {
                map: 3,
                index: 0,
                sprite: native_sprite(0x32, 0x188, 0x1a0),
            },
            NativeCustomOverworldSpriteEdit::Remove { map: 6, index: 0 },
        ])
        .unwrap();
    let expected = controller.table().clone();
    assert_eq!(expected.maps[0][0].id, 0x21);
    assert_eq!(expected.maps[0][1].id, 1);
    assert_eq!(expected.maps[3][0].id, 0x32);
    assert!(expected.maps[6].is_empty());

    let prepared = controller
        .prepare_commit(
            "Native custom overworld sprite supported-variant matrix",
            &native_sprite_options(pointer_offset),
        )
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(physical).unwrap());
    project
        .apply_mutation(
            "Native custom overworld sprite supported-variant matrix",
            &prepared.mutation,
        )
        .unwrap();
    assert_eq!(
        project
            .load_native_custom_overworld_sprites(layout, &[4; 128])
            .unwrap()
            .table,
        expected
    );
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    let edited = project.save_snapshot();

    let reopened_identity = detect_identity(&project.rom).unwrap();
    let reopened = NativeCustomOverworldSpriteController::decode(
        &ControllerSnapshot {
            revision: 12,
            mode: EditorMode::Overworld,
            identity: reopened_identity,
            document_path: None,
            rom_bytes: edited.clone(),
        },
        layout,
        [4; 128],
    )
    .unwrap();
    assert_eq!(reopened.table(), &expected);

    assert!(project.undo().unwrap());
    assert_eq!(project.save_snapshot(), original);
    assert!(project.redo().unwrap());
    assert_eq!(project.save_snapshot(), edited);
    edited
}

#[test]
fn complete_overworld_transfer_matches_every_supported_identity_and_layout_variant() {
    const SMW: &[u8; 21] = b"SUPER MARIOWORLD     ";
    const ALL_STARS_WORLD: &[u8; 21] = b"ALL_STARS + WORLD    ";
    let identities = [(SMW, 0), (SMW, 1), (ALL_STARS_WORLD, 1)];
    for &(title, region) in &identities {
        for map_mode in [0x20, 0x30, 0x23, 0x32] {
            for pointer_base in [0x200, 0x400] {
                let case = IdentityCase {
                    title,
                    region,
                    map_mode,
                };
                let headerless = variant_rom(case, pointer_base, false);
                let headered = variant_rom(case, pointer_base, true);
                let edited_headerless = edit_variant(headerless, pointer_base);
                let edited_headered = edit_variant(headered, pointer_base);
                assert_eq!(&edited_headered[..512], &COPIER_PREFIX);
                assert_eq!(&edited_headered[512..], edited_headerless);
            }
        }
    }
}

#[test]
fn interactive_overworld_edits_match_every_supported_identity_and_layout_variant() {
    const SMW: &[u8; 21] = b"SUPER MARIOWORLD     ";
    const ALL_STARS_WORLD: &[u8; 21] = b"ALL_STARS + WORLD    ";
    let identities = [(SMW, 0), (SMW, 1), (ALL_STARS_WORLD, 1)];
    for &(title, region) in &identities {
        for map_mode in [0x20, 0x30, 0x23, 0x32] {
            for pointer_base in [0x200, 0x400] {
                let case = IdentityCase {
                    title,
                    region,
                    map_mode,
                };
                let headerless = variant_rom(case, pointer_base, false);
                let headered = variant_rom(case, pointer_base, true);
                let edited_headerless = edit_interactively_variant(headerless, pointer_base);
                let edited_headered = edit_interactively_variant(headered, pointer_base);
                assert_eq!(&edited_headered[..512], &COPIER_PREFIX);
                assert_eq!(&edited_headered[512..], edited_headerless);
            }
        }
    }
}

#[test]
fn native_custom_sprite_edits_match_every_supported_identity_mapper_header_and_storage_variant() {
    const SMW: &[u8; 21] = b"SUPER MARIOWORLD     ";
    const ALL_STARS_WORLD: &[u8; 21] = b"ALL_STARS + WORLD    ";
    let identities = [(SMW, 0), (SMW, 1), (ALL_STARS_WORLD, 1)];
    for &(title, region) in &identities {
        for map_mode in [0x20, 0x30, 0x23, 0x32] {
            for pointer_offset in [0x600, 0x680] {
                let case = IdentityCase {
                    title,
                    region,
                    map_mode,
                };
                let headerless = native_sprite_variant_rom(case, pointer_offset, false);
                let headered = native_sprite_variant_rom(case, pointer_offset, true);
                let edited_headerless = edit_native_sprite_variant(headerless, pointer_offset);
                let edited_headered = edit_native_sprite_variant(headered, pointer_offset);
                assert_eq!(&edited_headered[..512], &COPIER_PREFIX);
                assert_eq!(&edited_headered[512..], edited_headerless);
            }
        }
    }
}
