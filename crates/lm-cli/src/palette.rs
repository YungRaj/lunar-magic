use crate::atomic_output::write_new;
use lm_project::{LevelPointerTable, PaletteRomLayout, Project};
use lm_rom::{Mapper, RomImage};

pub fn inspect(
    bytes: Vec<u8>,
    mapper: Mapper,
    index: usize,
    pointer_table: usize,
    colors: usize,
    observation: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    const PALETTE_SLOTS: usize = 0x200;
    let project = Project::new(RomImage::from_bytes(bytes)?);
    let palette = project.load_palette(
        index,
        PaletteRomLayout {
            mapper,
            pointers: LevelPointerTable {
                offset: pointer_table,
                entries: PALETTE_SLOTS,
                stride: 3,
            },
            colors_per_palette: colors,
        },
    )?;
    println!("palette: {index:#05x}");
    println!("colors: {}", palette.colors.len());
    println!("rows: {}", palette.colors.len().div_ceil(16));
    if let Some(path) = observation {
        write_new(path, lm_oracle::observe_palette(&palette).to_text())?;
        println!("observation: {}", path.display());
    }
    Ok(())
}
