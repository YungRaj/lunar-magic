use crate::atomic_output::write_new;
use lm_project::{LevelPointerTable, MessageRomLayout, Project, SpriteRomLayout};
use lm_rom::{Mapper, RomImage};

pub fn inspect_messages(
    bytes: Vec<u8>,
    mapper: Mapper,
    slot: usize,
    pointer_table: usize,
    count: usize,
    observation: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new(RomImage::from_bytes(bytes)?);
    let messages = project.load_overworld_messages(
        slot,
        MessageRomLayout {
            mapper,
            pointers: table(pointer_table, slot)?,
            messages_per_slot: count,
        },
    )?;
    println!("messages: {}", messages.len());
    println!("encoded-bytes: {}", messages.len().saturating_mul(144));
    if let Some(path) = observation {
        write_new(
            path,
            lm_oracle::observe_overworld_messages(&messages).to_text(),
        )?;
        println!("observation: {}", path.display());
    }
    Ok(())
}

pub fn inspect_sprites(
    bytes: Vec<u8>,
    mapper: Mapper,
    slot: usize,
    pointer_table: usize,
    count: usize,
    record_len: usize,
    observation: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::new(RomImage::from_bytes(bytes)?);
    let sprites = project.load_overworld_sprites(
        slot,
        SpriteRomLayout {
            mapper,
            pointers: table(pointer_table, slot)?,
            sprites_per_slot: count,
            record_len,
        },
    )?;
    println!("sprites: {}", sprites.len());
    for (index, sprite) in sprites.iter().enumerate() {
        println!(
            "sprite-{index:02x}: id={:#06x} x={:#06x} y={:#06x} submap={:?} extra-bytes={}",
            sprite.id,
            sprite.x,
            sprite.y,
            sprite.submap,
            sprite.extra.len()
        );
    }
    if let Some(path) = observation {
        write_new(
            path,
            lm_oracle::observe_overworld_sprites(&sprites).to_text(),
        )?;
        println!("observation: {}", path.display());
    }
    Ok(())
}

fn table(offset: usize, slot: usize) -> Result<LevelPointerTable, Box<dyn std::error::Error>> {
    Ok(LevelPointerTable {
        offset,
        entries: slot.checked_add(1).ok_or("slot count overflow")?,
        stride: 3,
    })
}
