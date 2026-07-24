use lm_level::{LevelObjectData, NativeLevelFile, NativeSpriteStream, SpriteLengthTable};
use lm_project::{
    LevelPointerTable, LevelRomLayout, LevelSaveOptions, LoadedLevelSlot, Project,
    RatsOwnershipManifest, RatsOwnershipManifestFile, SpritePointerTable,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SnesChecksum, compute_snes_checksum, pc_to_snes};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const LEVEL: usize = 1;
const LAYER1_TABLE: usize = 0x200;
const SPRITE_TABLE: usize = 0x800;

fn level(layer_tail: [u8; 3], sprite: u8) -> NativeLevelFile {
    NativeLevelFile {
        source_level: 0x105,
        layer1: LevelObjectData::parse(&[
            1,
            2,
            3,
            4,
            5,
            layer_tail[0],
            layer_tail[1],
            layer_tail[2],
            0xff,
        ])
        .unwrap(),
        sprites: NativeSpriteStream::parse(
            &[0x10, 0x00, sprite, 0x01, 0xff],
            false,
            &SpriteLengthTable::standard(),
        )
        .unwrap(),
    }
}

fn layout() -> LevelRomLayout {
    let pointers = |offset| LevelPointerTable {
        offset,
        entries: 0x200,
        stride: 3,
    };
    LevelRomLayout {
        mapper: Mapper::LoRom,
        layer1: pointers(LAYER1_TABLE),
        sprites: SpritePointerTable::Contiguous(pointers(SPRITE_TABLE)),
        expanded_sprites: false,
    }
}

fn write_fixture(path: &Path, value: &NativeLevelFile) {
    let layer = value.layer1.encode().unwrap();
    let sprites = value.sprites.encode_checked().unwrap();
    let mut bytes = vec![0xff; 0x10000];
    for (table, target) in [(LAYER1_TABLE, 0x2000), (SPRITE_TABLE, 0x3000)] {
        let pointer = pc_to_snes(Mapper::LoRom, target).unwrap().to_le_bytes();
        bytes[table + LEVEL * 3..table + LEVEL * 3 + 3].copy_from_slice(&pointer[..3]);
    }
    bytes[0x2000..0x2000 + layer.len()].copy_from_slice(&layer);
    bytes[0x3000..0x3000 + sprites.len()].copy_from_slice(&sprites);
    let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    fs::write(path, bytes).unwrap();
}

fn invoke(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn built_binary_transfers_native_level_object_and_sprite_streams() {
    let directory = std::env::temp_dir().join(format!(
        "lm-level-transfer-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Input game.smc");
    let exported = directory.join("Exported level.lmlvl");
    let replacement = directory.join("Replacement level.lmlvl");
    let output = directory.join("Imported game.smc");
    write_fixture(&input, &level([9, 8, 7], 0x20));

    let export = invoke(&[
        "level-export",
        input.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "800",
        "legacy",
        "standard",
        exported.to_str().unwrap(),
    ]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let decoded = NativeLevelFile::decode(
        &fs::read(&exported).unwrap(),
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    assert_eq!(decoded.layer1, level([9, 8, 7], 0x20).layer1);
    assert_eq!(decoded.sprites, level([9, 8, 7], 0x20).sprites);

    fs::write(&replacement, level([6, 5, 4], 0x30).encode().unwrap()).unwrap();
    let import_arguments = [
        "level-import",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "800",
        "legacy",
        "standard",
        replacement.to_str().unwrap(),
        "7fdc",
        "4000",
        "7800",
    ];
    let import = invoke(&import_arguments);
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    let reopened = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
    let loaded = reopened
        .load_level_slot(LEVEL, layout(), &SpriteLengthTable::standard())
        .unwrap();
    assert_eq!(loaded.layer1, level([6, 5, 4], 0x30).layer1);
    assert_eq!(loaded.sprites, level([6, 5, 4], 0x30).sprites);
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    assert!(!invoke(&import_arguments).status.success());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn built_binary_owned_import_reclaims_both_displaced_level_streams() {
    let directory = std::env::temp_dir().join(format!(
        "lm-level-owned-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Owned input.smc");
    let replacement = directory.join("Owned replacement.lmlvl");
    let manifest = directory.join("Ownership.lmrats");
    let output = directory.join("Owned output.smc");

    let allocation = AllocationPolicy {
        search: 0x1000..0x3800,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(LAYER1_TABLE..LAYER1_TABLE + 0x600),
            ProtectedRange(SPRITE_TABLE..SPRITE_TABLE + 0x600),
        ],
    };
    let original = level([3, 2, 1], 0x22);
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let displaced = project
        .save_level_slot(
            layout(),
            &LoadedLevelSlot {
                number: LEVEL,
                layer1: original.layer1,
                sprites: original.sprites,
            },
            &SpriteLengthTable::standard(),
            &LevelSaveOptions {
                layer1_allocation: allocation.clone(),
                sprite_allocation: allocation,
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: false,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    let displaced_blocks = vec![displaced.layer1.block, displaced.sprites.block];
    project.refresh_checksum(0x7fdc).unwrap();
    fs::write(&input, project.save_snapshot()).unwrap();
    let expected = level([8, 7, 6], 0x44);
    fs::write(&replacement, expected.encode().unwrap()).unwrap();
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
        "level-import-owned",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "800",
        "legacy",
        "standard",
        replacement.to_str().unwrap(),
        "7fdc",
        "4000",
        "7000",
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
    let loaded = reopened
        .load_level_slot(LEVEL, layout(), &SpriteLengthTable::standard())
        .unwrap();
    assert_eq!(loaded.layer1, expected.layer1);
    assert_eq!(loaded.sprites, expected.sprites);
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}
