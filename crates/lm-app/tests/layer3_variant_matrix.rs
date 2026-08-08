use lm_app::{
    ControllerSnapshot, EditorMode, NativeLevelAssetsController, NativeLevelAssetsControllerEdit,
};
use lm_graphics::PaletteOwnership;
use lm_level::{Layer3ExpandedModeFlags, Layer3TilemapGraphicsDescriptor, SpriteLengthTable};
use lm_project::{
    ExAnimationRomLayout, ExAnimationSaveOptions, ExpandedLevelSettingsLayout, LevelPointerTable,
    LevelRomLayout, LevelSaveOptions, NativeLevelAssetsLayout, NativeLevelAssetsSaveOptions,
    PaletteRomLayout, PaletteSaveOptions, Project,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, compute_snes_checksum, detect_identity, pc_to_snes};

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

fn pointer_table(offset: usize) -> LevelPointerTable {
    LevelPointerTable {
        offset,
        entries: 1,
        stride: 3,
    }
}

fn write_pointer(bytes: &mut [u8], mapper: Mapper, offset: usize, target: usize) {
    let pointer = pc_to_snes(mapper, target).unwrap().to_le_bytes();
    bytes[offset..offset + 3].copy_from_slice(&pointer[..3]);
}

fn layout(mapper: Mapper, settings_offset: usize) -> NativeLevelAssetsLayout {
    NativeLevelAssetsLayout {
        level: LevelRomLayout {
            mapper,
            layer1: pointer_table(0x20),
            sprites: pointer_table(0x30).into(),
            expanded_sprites: false,
        },
        palette: PaletteRomLayout {
            mapper,
            pointers: pointer_table(0x40),
            colors_per_palette: 2,
        },
        exanimation: ExAnimationRomLayout {
            mapper,
            pointers: pointer_table(0x50),
            maximum_records: 8,
            maximum_encoded_len: 0x100,
        },
        expanded_settings: Some(ExpandedLevelSettingsLayout {
            mapper,
            table_offset: settings_offset,
            entries: 1,
            stride: 32,
        }),
    }
}

fn variant_rom(case: IdentityCase, settings_offset: usize, copier_header: bool) -> Vec<u8> {
    let mapper = match case.map_mode {
        0x23 => Mapper::Sa1,
        0x32 => Mapper::ExLoRom,
        0x20 | 0x30 => Mapper::LoRom,
        _ => unreachable!(),
    };
    let logical_len = if case.map_mode == 0x32 {
        0x40_8000
    } else {
        0x8000
    };
    let mut logical = vec![0xff; logical_len];
    write_pointer(&mut logical, mapper, 0x20, 0x100);
    write_pointer(&mut logical, mapper, 0x30, 0x120);
    write_pointer(&mut logical, mapper, 0x40, 0x140);
    write_pointer(&mut logical, mapper, 0x50, 0x160);
    logical[0x100..0x109].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 0xff]);
    logical[0x120..0x125].copy_from_slice(&[0x10, 0, 1, 2, 0xff]);
    logical[0x140..0x144].copy_from_slice(&[1, 0, 2, 0]);
    logical[0x160..0x187].fill(0);
    logical[settings_offset..settings_offset + 32].fill(0x5a);

    let header = 0x7fc0;
    logical[header..header + 21].copy_from_slice(case.title);
    logical[header + 0x15] = case.map_mode;
    logical[header + 0x19] = case.region;
    logical[header + 0x1b] = 0;
    let checksum = compute_snes_checksum(&logical, header + 0x1c).unwrap();
    logical[header + 0x1c..header + 0x20].copy_from_slice(&checksum.encoded());

    if copier_header {
        let mut physical = COPIER_PREFIX.to_vec();
        physical.extend(logical);
        physical
    } else {
        logical
    }
}

fn options(settings_offset: usize) -> NativeLevelAssetsSaveOptions {
    let allocation = AllocationPolicy {
        search: 0x3000..0x7000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x20..0x53),
            ProtectedRange(settings_offset..settings_offset + 32),
            ProtectedRange(0x7fc0..0x8000),
        ],
    };
    NativeLevelAssetsSaveOptions {
        level: LevelSaveOptions {
            layer1_allocation: allocation.clone(),
            sprite_allocation: allocation.clone(),
            previous_layer1: None,
            previous_sprites: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        palette: PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        exanimation: ExAnimationSaveOptions {
            allocation,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    }
}

fn edit_variant(
    physical: Vec<u8>,
    settings_offset: usize,
) -> (
    Vec<u8>,
    Layer3TilemapGraphicsDescriptor,
    Layer3ExpandedModeFlags,
) {
    let image = RomImage::from_bytes(physical.clone()).unwrap();
    let identity = detect_identity(&image).unwrap();
    let layout = layout(identity.mapper, settings_offset);
    let snapshot = ControllerSnapshot {
        revision: 7,
        mode: EditorMode::Level(0),
        identity: identity.clone(),
        document_path: None,
        rom_bytes: physical.clone(),
    };
    let mut controller = NativeLevelAssetsController::decode(
        &snapshot,
        layout,
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let descriptor = Layer3TilemapGraphicsDescriptor::new(0xabc, 2, 3).unwrap();
    let mode = Layer3ExpandedModeFlags::from_packed(0x89ab_cdef);
    controller
        .apply_edits(&[
            NativeLevelAssetsControllerEdit::Layer3TilemapSettings {
                enabled: true,
                descriptor,
            },
            NativeLevelAssetsControllerEdit::Layer3ExpandedMode(mode),
        ])
        .unwrap();
    let prepared = controller
        .prepare_commit(
            "Layer 3 supported-variant matrix",
            &options(settings_offset),
        )
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(physical.clone()).unwrap());
    project
        .apply_mutation("Layer 3 supported-variant matrix", &prepared.mutation)
        .unwrap();
    let reopened = project
        .load_native_level_assets(0, layout, &SpriteLengthTable::standard(), &[false; 256])
        .unwrap();
    let settings = reopened.expanded_settings.as_ref().unwrap();
    assert!(settings.layer3_tilemap_enabled());
    assert_eq!(
        settings.layer3_tilemap_graphics_descriptor().unwrap(),
        descriptor
    );
    assert_eq!(settings.layer3_expanded_mode_flags(), mode);
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    let edited = project.rom.as_file_bytes().to_vec();
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), physical);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), edited);
    (edited, descriptor, mode)
}

#[test]
fn layer3_edit_reopens_and_undoes_across_every_supported_identity_and_table_variant() {
    const SMW: &[u8; 21] = b"SUPER MARIOWORLD     ";
    const ALL_STARS_WORLD: &[u8; 21] = b"ALL_STARS + WORLD    ";
    let identities = [(SMW, 0), (SMW, 1), (ALL_STARS_WORLD, 1)];
    for &(title, region) in &identities {
        for map_mode in [0x20, 0x30, 0x23, 0x32] {
            for settings_offset in [0x60, 0x2000] {
                let case = IdentityCase {
                    title,
                    region,
                    map_mode,
                };
                let headerless = variant_rom(case, settings_offset, false);
                let headered = variant_rom(case, settings_offset, true);
                let (edited_headerless, _, _) = edit_variant(headerless, settings_offset);
                let (edited_headered, _, _) = edit_variant(headered, settings_offset);
                assert_eq!(&edited_headered[..512], &COPIER_PREFIX);
                assert_eq!(&edited_headered[512..], edited_headerless);
            }
        }
    }
}
