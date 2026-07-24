use crate::atomic_output::write_new;
use lm_project::{GraphicsCompression, GraphicsRomLayout, LevelPointerTable, Project};
use lm_rom::{Mapper, RomImage};

#[derive(Clone, Copy)]
pub struct GraphicsInspectOptions<'a> {
    pub mapper: Mapper,
    pub file: usize,
    pub pointer_table: usize,
    pub maximum_compressed_len: usize,
    pub maximum_decompressed_len: usize,
    pub compression: GraphicsCompression,
    pub observation: Option<&'a std::path::Path>,
}

pub fn inspect(
    bytes: Vec<u8>,
    options: GraphicsInspectOptions<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    const GRAPHICS_SLOTS: usize = 0x100;
    let project = Project::new(RomImage::from_bytes(bytes)?);
    let graphics = project.load_graphics_file(
        options.file,
        GraphicsRomLayout {
            mapper: options.mapper,
            pointers: LevelPointerTable {
                offset: options.pointer_table,
                entries: GRAPHICS_SLOTS,
                stride: 3,
            },
            compression: options.compression,
            maximum_compressed_len: options.maximum_compressed_len,
            maximum_decompressed_len: options.maximum_decompressed_len,
        },
    )?;
    println!("file: {:#04x}", options.file);
    println!("tiles: {}", graphics.tiles.len());
    println!("decoded-bytes: {:#x}", graphics.encode()?.len());
    if let Some(path) = options.observation {
        write_new(path, lm_oracle::observe_graphics(&graphics).to_text())?;
        println!("observation: {}", path.display());
    }
    Ok(())
}
