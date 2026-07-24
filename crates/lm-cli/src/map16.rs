use crate::atomic_output::write_new;
use lm_project::{LevelPointerTable, Map16RomLayout, Project};
use lm_rom::{Mapper, RomImage};

pub fn inspect(
    bytes: Vec<u8>,
    mapper: Mapper,
    page: usize,
    graphics_table: usize,
    acts_like_table: usize,
    observation: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    const PAGE_SLOTS: usize = 0x100;
    let table = |offset| LevelPointerTable {
        offset,
        entries: PAGE_SLOTS,
        stride: 3,
    };
    let project = Project::new(RomImage::from_bytes(bytes)?);
    let page_data = project.load_map16_page(
        page,
        Map16RomLayout {
            mapper,
            graphics: table(graphics_table),
            acts_like: table(acts_like_table),
        },
    )?;
    let nonblank = page_data
        .tiles
        .iter()
        .filter(|tile| **tile != lm_level::Map16Tile::default())
        .count();
    println!("page: {page:#04x}");
    println!("tiles: {}", page_data.tiles.len());
    println!("nonblank-tiles: {nonblank}");
    if let Some(path) = observation {
        write_new(path, lm_oracle::observe_map16_page(&page_data).to_text())?;
        println!("observation: {}", path.display());
    }
    Ok(())
}
