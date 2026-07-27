use lm_project::{
    LevelLayer2RomLayout, LevelLayer2TilemapEncoding, LevelPointerTable, LevelRomLayout,
    PayloadReadPolicy, Project, SpritePointerTable,
};
use lm_rom::{Mapper, RomImage};
use std::path::Path;

pub fn inspect(
    bytes: Vec<u8>,
    mapper: Mapper,
    number: usize,
    layer1_table: usize,
    sprite_table: usize,
    expanded_sprites: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    const LEVEL_SLOTS: usize = 0x200;
    let project = Project::new(RomImage::from_bytes(bytes)?);
    let table = |offset| LevelPointerTable {
        offset,
        entries: LEVEL_SLOTS,
        stride: 3,
    };
    let level = project.load_level_slot(
        number,
        LevelRomLayout {
            mapper,
            layer1: table(layer1_table),
            sprites: table(sprite_table).into(),
            expanded_sprites,
        },
        &lm_level::SpriteLengthTable::standard(),
    )?;
    println!("level: {:#05x}", level.number);
    println!("header: {:02x?}", level.layer1.header.encoded());
    println!("objects: {}", level.layer1.objects.records.len());
    println!("sprite-header: {:#04x}", level.sprites.header);
    println!("sprite-tokens: {}", level.sprites.tokens.len());
    println!("expanded-sprites: {}", level.sprites.expanded);
    Ok(())
}

pub fn inspect_split_bank(
    bytes: Vec<u8>,
    mapper: Mapper,
    number: usize,
    layer1_table: usize,
    sprite_low_table: usize,
    sprite_bank_table: usize,
    expanded_sprites: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    const LEVEL_SLOTS: usize = 0x200;
    let project = Project::new(RomImage::from_bytes(bytes)?);
    let level = project.load_level_slot(
        number,
        LevelRomLayout {
            mapper,
            layer1: LevelPointerTable {
                offset: layer1_table,
                entries: LEVEL_SLOTS,
                stride: 3,
            },
            sprites: SpritePointerTable::SplitBankTable {
                low_words: LevelPointerTable {
                    offset: sprite_low_table,
                    entries: LEVEL_SLOTS,
                    stride: 2,
                },
                banks: LevelPointerTable {
                    offset: sprite_bank_table,
                    entries: LEVEL_SLOTS,
                    stride: 1,
                },
            },
            expanded_sprites,
        },
        &lm_level::SpriteLengthTable::standard(),
    )?;
    print_level(&level);
    Ok(())
}

fn print_level(level: &lm_project::LoadedLevelSlot) {
    println!("level: {:#05x}", level.number);
    println!("header: {:02x?}", level.layer1.header.encoded());
    println!("objects: {}", level.layer1.objects.records.len());
    println!("sprite-header: {:#04x}", level.sprites.header);
    println!("sprite-tokens: {}", level.sprites.tokens.len());
    println!("expanded-sprites: {}", level.sprites.expanded);
}

pub fn export_layer2(
    bytes: Vec<u8>,
    mapper: Mapper,
    number: usize,
    layer1_table: usize,
    layer2_table: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    const LEVEL_SLOTS: usize = 0x200;
    let project = Project::new(RomImage::from_bytes(bytes)?);
    let layer1 = LevelPointerTable {
        offset: layer1_table,
        entries: LEVEL_SLOTS,
        stride: 3,
    };
    let payload = project.load_payload(
        layer1.pointer_offset(number)?,
        mapper,
        &PayloadReadPolicy::TaggedOrTerminated {
            terminator: vec![0xff],
            maximum_len: 0x8000,
            bank_size: Some(0x8000),
        },
    )?;
    let objects = lm_level::LevelObjectData::parse(&payload.bytes)?;
    let layer2 = project.load_level_layer2(
        number,
        objects.header.level_mode(),
        LevelLayer2RomLayout {
            mapper,
            pointers: LevelPointerTable {
                offset: layer2_table,
                entries: LEVEL_SLOTS,
                stride: 3,
            },
            background_bank_substitution: None,
            descriptor_table: None,
            maximum_compressed_len: 0x8000,
            tilemap_encoding: LevelLayer2TilemapEncoding::Legacy { high_byte: 0 },
        },
    )?;
    let encoded = layer2.encode_mwl()?;
    crate::atomic_output::write_new(output, &encoded)?;
    println!("level: {number:#05x}");
    println!(
        "layer2-storage: {}",
        match layer2 {
            lm_level::NativeLayer2Data::Objects(_) => "objects",
            lm_level::NativeLayer2Data::Tilemap(_) => "compressed-tilemap",
        }
    );
    println!("decoded-bytes: {}", encoded.len());
    println!("output: {}", output.display());
    Ok(())
}
