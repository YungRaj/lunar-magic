use lm_app::{
    ControllerSnapshot, EditorMode, NativeMap16BitmapImportSession,
    NativeMap16BitmapImportSessionRequest,
};
use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette, Rgba8};
use lm_level::{LevelObjectData, Map16Page, Map16Tile, NativeSpriteStream, Subtile};
use lm_profile::RevisionProfile;
use lm_project::{
    GraphicsSaveOptions, InstalledLayout, LevelPointerTable, LevelSaveOptions, LoadedLevelSlot,
    Map16SaveOptions, PaletteSaveOptions, Project,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, Region, RomImage, SupportedGame, compute_snes_checksum, detect_identity};

const SMW: &[u8; 21] = b"SUPER MARIOWORLD     ";
const ALL_STARS_WORLD: &[u8; 21] = b"ALL_STARS + WORLD    ";
const LOGICAL_LEN: usize = 0x40_000;
const CHECKSUM_FIELD: usize = 0x7fdc;
const COPIER_PREFIX: [u8; 512] = {
    let mut prefix = [0_u8; 512];
    prefix[0] = 0x40;
    prefix[8] = 0xaa;
    prefix[9] = 0xbb;
    prefix[10] = 0x04;
    prefix
};

fn pointer(offset: usize, entries: usize) -> LevelPointerTable {
    LevelPointerTable {
        offset,
        entries,
        stride: 3,
    }
}

fn allocation() -> AllocationPolicy {
    AllocationPolicy {
        search: 0x1_0000..0x3_0000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x100..0x1900),
            ProtectedRange(0x7fc0..0x8000),
        ],
    }
}

fn routed_profile(
    mapper: Mapper,
    game: SupportedGame,
    region: Region,
    revision: u8,
) -> RevisionProfile {
    let mut profile = lm_profile::test_support::profile();
    profile.game = game;
    profile.region = region;
    profile.revision = revision;
    profile.mapper = mapper;
    profile.level.mapper = mapper;
    profile.level.layer1 = pointer(0x100, 1);
    profile.level.sprites = pointer(0x110, 1).into();
    profile.level.expanded_sprites = false;
    profile.map16.mapper = mapper;
    profile.map16.graphics = pointer(0x120, 1);
    profile.map16.acts_like = pointer(0x130, 1);
    profile.graphics.mapper = mapper;
    profile.graphics.pointers = pointer(0x140, 6);
    profile.graphics.split_pointer_planes = None;
    profile.object_tileset_graphics_offset = Some(0x1800);
    profile.palette.mapper = mapper;
    profile.palette.pointers = pointer(0x160, 1);
    profile.palette.colors_per_palette = 256;
    profile.palette_installation = InstalledLayout::Unconditional(profile.palette);
    profile.exanimation.mapper = mapper;
    profile.exanimation_installation = InstalledLayout::Absent;
    profile.exanimation_feature_installation = InstalledLayout::Absent;
    if let Some(layer2) = profile.layer2.as_mut() {
        layer2.mapper = mapper;
    }
    if let Some(expanded) = profile.expanded_settings.as_mut() {
        expanded.mapper = mapper;
    }
    profile.overworld.layers.mapper = mapper;
    profile.overworld.event_reveals.mapper = mapper;
    profile.overworld.endpoints.mapper = mapper;
    profile.overworld.messages.mapper = mapper;
    profile.overworld.sprites.mapper = mapper;
    profile.overworld.palette.mapper = mapper;
    profile.overworld.animation.mapper = mapper;
    profile
}

fn seeded_logical_rom(
    title: &[u8; 21],
    region_byte: u8,
    map_mode: u8,
    mapper: Mapper,
) -> (Vec<u8>, RevisionProfile) {
    let mut logical = vec![0xff; LOGICAL_LEN];
    logical[0x7fc0..0x7fc0 + 21].copy_from_slice(title);
    logical[0x7fc0 + 0x15] = map_mode;
    logical[0x7fc0 + 0x19] = region_byte;
    logical[0x7fc0 + 0x1b] = 0;
    logical[0x1800..0x1804].copy_from_slice(&[0, 1, 2, 3]);
    let checksum = compute_snes_checksum(&logical, CHECKSUM_FIELD).unwrap();
    logical[CHECKSUM_FIELD..CHECKSUM_FIELD + 4].copy_from_slice(&checksum.encoded());
    let identity = detect_identity(&RomImage::from_bytes(logical.clone()).unwrap()).unwrap();
    let profile = routed_profile(mapper, identity.game, identity.region, identity.revision);
    let mut project = Project::new(RomImage::from_bytes(logical).unwrap());
    let common = allocation();
    let level_options = LevelSaveOptions {
        layer1_allocation: common.clone(),
        sprite_allocation: common.clone(),
        previous_layer1: None,
        previous_sprites: None,
        reuse_identical: false,
        erase_fill: 0xff,
    };
    project
        .save_level_slot(
            profile.level,
            &LoadedLevelSlot {
                number: 0,
                layer1: LevelObjectData::default(),
                sprites: NativeSpriteStream::default(),
            },
            &profile.sprite_lengths,
            &level_options,
        )
        .unwrap();
    let graphics_options = GraphicsSaveOptions {
        allocation: common.clone(),
        previous_block: None,
        reuse_identical: false,
        erase_fill: 0xff,
    };
    for file_number in 0..6 {
        let pixel = if file_number < 4 {
            u8::try_from(file_number + 1).unwrap()
        } else {
            0
        };
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([pixel; 64]); 0x80],
        };
        project
            .save_graphics_file(file_number, &graphics, profile.graphics, &graphics_options)
            .unwrap();
    }
    project
        .save_palette(
            0,
            &Palette {
                colors: (0..256).map(|value| Bgr555(value & 0x7fff)).collect(),
            },
            profile.palette,
            &PaletteSaveOptions {
                allocation: common.clone(),
                previous_block: None,
                reuse_identical: false,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    project
        .save_map16_page(
            0,
            &Map16Page::new(vec![
                Map16Tile {
                    top_left: Subtile(0x1004),
                    top_right: Subtile(0x1004),
                    bottom_left: Subtile(0x1004),
                    bottom_right: Subtile(0x1004),
                    acts_like: 0,
                };
                256
            ])
            .unwrap(),
            profile.map16,
            &Map16SaveOptions {
                graphics_allocation: common.clone(),
                acts_like_allocation: common,
                previous_graphics: None,
                previous_acts_like: None,
                reuse_identical: false,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    project.refresh_checksum(CHECKSUM_FIELD).unwrap();
    (project.rom.logical_bytes().to_vec(), profile)
}

fn exercise_variant(physical: Vec<u8>, profile: RevisionProfile) -> Vec<u8> {
    let image = RomImage::from_bytes(physical.clone()).unwrap();
    let identity = detect_identity(&image).unwrap();
    let snapshot = ControllerSnapshot {
        revision: 7,
        mode: EditorMode::Map16,
        identity,
        document_path: None,
        rom_bytes: physical.clone(),
    };
    let session = NativeMap16BitmapImportSession::new(
        snapshot,
        profile.clone(),
        NativeMap16BitmapImportSessionRequest {
            level: 0,
            start_map16_tile: 0,
            extra_graphics: [Some(4), Some(5)],
            pixels: (0..16 * 16)
                .map(|index| {
                    if (index / 16 + index % 16) & 1 == 0 {
                        Rgba8 {
                            red: 248,
                            green: 24,
                            blue: 96,
                            alpha: 255,
                        }
                    } else {
                        Rgba8 {
                            red: 16,
                            green: 232,
                            blue: 144,
                            alpha: 255,
                        }
                    }
                })
                .collect(),
            width: 16,
            height: 16,
            palette_row: 0,
        },
    )
    .unwrap();
    let prepared = session.prepare_commit(0x3_0000..0x3_8000).unwrap();
    assert_eq!(prepared.expected_revision, 7);
    assert!(!prepared.mutation.is_empty());
    let mut project = Project::new(RomImage::from_bytes(physical.clone()).unwrap());
    project
        .apply_mutation(&prepared.description, &prepared.mutation)
        .unwrap();
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    let reopened = project.load_map16_page(0, profile.map16).unwrap();
    assert_ne!(reopened.tiles[0], Map16Tile::default());
    let graphics = project.load_graphics_file(4, profile.graphics).unwrap();
    assert_ne!(graphics.tiles[0].pixels(), &[0; 64]);
    let palette = project.load_palette(0, profile.palette).unwrap();
    assert_ne!(
        palette,
        Palette {
            colors: (0..256).map(|value| Bgr555(value & 0x7fff)).collect(),
        }
    );
    let edited = project.rom.as_file_bytes().to_vec();
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), physical);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), edited);
    edited
}

#[test]
fn installed_bitmap_import_commits_reopens_and_undoes_across_all_supported_variants() {
    let identities = [(SMW, 0), (SMW, 1), (ALL_STARS_WORLD, 1)];
    let mappings = [
        (0x20, Mapper::LoRom),
        (0x30, Mapper::LoRom),
        (0x23, Mapper::Sa1),
        (0x32, Mapper::ExLoRom),
    ];
    let mut cases = 0;
    for (title, region) in identities {
        for (map_mode, mapper) in mappings {
            let (logical, profile) = seeded_logical_rom(title, region, map_mode, mapper);
            let mut headerless_results: [Option<Vec<u8>>; 2] = [None, None];
            for headered in [false, true] {
                for corrupt_checksum in [false, true] {
                    let mut source = logical.clone();
                    if corrupt_checksum {
                        source[0x40] ^= 1;
                    }
                    let physical = if headered {
                        let mut physical = COPIER_PREFIX.to_vec();
                        physical.extend(source);
                        physical
                    } else {
                        source
                    };
                    let edited = exercise_variant(physical, profile.clone());
                    if headered {
                        assert_eq!(&edited[..512], &COPIER_PREFIX);
                        assert_eq!(
                            &edited[512..],
                            headerless_results[usize::from(corrupt_checksum)]
                                .as_deref()
                                .unwrap()
                        );
                    } else {
                        headerless_results[usize::from(corrupt_checksum)] = Some(edited);
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 48);
}
