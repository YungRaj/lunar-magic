use crate::{
    atomic_output::write_new_batch,
    oracle_input::{read_bounded, read_exact},
};
use lm_graphics::{GraphicsInterchangeFile, GraphicsOwnership, IndexedBitmapImport};
use lm_level::{Map16Page, Map16PageFile, Map16Tile, Subtile};
#[cfg(test)]
use std::fs;
use std::io;
use std::path::Path;

const PIXEL_WIDTH: usize = 256;
const PIXEL_HEIGHT: usize = 256;
const PIXEL_COUNT: usize = PIXEL_WIDTH * PIXEL_HEIGHT;
const TILE_PLANE_WIDTH: usize = PIXEL_WIDTH / 8;

#[derive(Clone, Copy)]
pub(crate) struct IndexedMap16Import<'a> {
    pub indices: &'a Path,
    pub graphics: &'a Path,
    pub occupancy: &'a Path,
    pub palette_row: u8,
    pub acts_like: u16,
    pub source_page: u16,
    pub graphics_output: &'a Path,
    pub occupancy_output: &'a Path,
    pub page_output: &'a Path,
}

pub(crate) fn execute(request: IndexedMap16Import<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let paths = [
        request.indices,
        request.graphics,
        request.occupancy,
        request.graphics_output,
        request.occupancy_output,
        request.page_output,
    ];
    if paths
        .iter()
        .enumerate()
        .any(|(index, path)| paths[..index].contains(path))
    {
        return Err("indexed Map16 inputs and outputs must all differ".into());
    }
    if request.palette_row > 7 {
        return Err("Map16 palette row must be in 0..=7".into());
    }
    let pixels = read_exact(request.indices, PIXEL_COUNT, "indexed Map16 page")?;
    let graphics_file = GraphicsInterchangeFile::decode(&read_bounded(
        request.graphics,
        GraphicsInterchangeFile::MAX_FILE_LEN,
    )?)?;
    let occupancy_bytes = read_exact(
        request.occupancy,
        graphics_file.graphics.tiles.len(),
        "graphics occupancy",
    )?;
    if occupancy_bytes.iter().any(|value| *value > 1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "occupancy must contain one canonical 0/1 byte per graphics tile",
        )
        .into());
    }
    let occupied = occupancy_bytes
        .iter()
        .map(|value| *value != 0)
        .collect::<Vec<_>>();
    let imported = IndexedBitmapImport::materialize(
        PIXEL_WIDTH,
        PIXEL_HEIGHT,
        &pixels,
        &graphics_file.graphics,
        &GraphicsOwnership::editable(graphics_file.graphics.tiles.len()),
        &occupied,
    )?;
    let page = build_page(&imported, request.palette_row, request.acts_like)?;
    let encoded_graphics = GraphicsInterchangeFile {
        source_slot: graphics_file.source_slot,
        graphics: imported.graphics,
    }
    .encode()?;
    let encoded_occupancy = imported
        .occupied
        .iter()
        .map(|occupied| u8::from(*occupied))
        .collect::<Vec<_>>();
    let encoded_page = Map16PageFile {
        source_page: request.source_page,
        page,
    }
    .encode()?;
    write_new_batch(&[
        (request.graphics_output, encoded_graphics.as_slice()),
        (request.occupancy_output, encoded_occupancy.as_slice()),
        (request.page_output, encoded_page.as_slice()),
    ])?;
    Ok(())
}

pub(crate) fn build_page(
    imported: &IndexedBitmapImport,
    palette_row: u8,
    acts_like: u16,
) -> Result<Map16Page, io::Error> {
    if imported.width_in_tiles != 32 || imported.height_in_tiles != 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "materialized page must contain 32 by 32 subtiles",
        ));
    }
    let mut tiles = Vec::with_capacity(Map16Page::TILE_COUNT);
    for tile_y in 0..16 {
        for tile_x in 0..16 {
            let top_left = tile_y * 2 * TILE_PLANE_WIDTH + tile_x * 2;
            tiles.push(Map16Tile {
                top_left: descriptor(imported.placements[top_left], palette_row),
                top_right: descriptor(imported.placements[top_left + 1], palette_row),
                bottom_left: descriptor(
                    imported.placements[top_left + TILE_PLANE_WIDTH],
                    palette_row,
                ),
                bottom_right: descriptor(
                    imported.placements[top_left + TILE_PLANE_WIDTH + 1],
                    palette_row,
                ),
                acts_like,
            });
        }
    }
    Map16Page::new(tiles).map_err(|tiles| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("wrong imported Map16 tile count: {}", tiles.len()),
        )
    })
}

fn descriptor(placement: lm_graphics::ImportedTilePlacement, palette_row: u8) -> Subtile {
    let mut word = placement.tile | (u16::from(palette_row) << 10);
    if placement.x_flip {
        word |= 0x4000;
    }
    if placement.y_flip {
        word |= 0x8000;
    }
    Subtile(word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{GraphicsFile4bpp, IndexedTile};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-indexed-map16-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn indexed_page_materializes_graphics_occupancy_and_map16_together() {
        let directory = directory();
        let indices = directory.join("page.idx");
        let graphics = directory.join("base.lmgfx");
        let occupancy = directory.join("base.occ");
        let output_graphics = directory.join("result.lmgfx");
        let output_occupancy = directory.join("result.occ");
        let output_page = directory.join("result.map16");
        fs::write(&indices, vec![3; PIXEL_COUNT]).unwrap();
        fs::write(
            &graphics,
            GraphicsInterchangeFile {
                source_slot: 0x32,
                graphics: GraphicsFile4bpp {
                    tiles: vec![IndexedTile::new([0; 64]); 4],
                },
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
        fs::write(&occupancy, [0; 4]).unwrap();
        execute(IndexedMap16Import {
            indices: &indices,
            graphics: &graphics,
            occupancy: &occupancy,
            palette_row: 3,
            acts_like: 0x130,
            source_page: 0x20,
            graphics_output: &output_graphics,
            occupancy_output: &output_occupancy,
            page_output: &output_page,
        })
        .unwrap();
        let page = Map16PageFile::decode(&fs::read(output_page).unwrap()).unwrap();
        assert_eq!(page.source_page, 0x20);
        assert_eq!(page.page.tiles[0].top_left.palette(), 3);
        assert_eq!(page.page.tiles[0].top_left.tile_number(), 0);
        assert_eq!(page.page.tiles[255].acts_like, 0x130);
        assert_eq!(fs::read(output_occupancy).unwrap(), [1, 0, 0, 0]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn aliases_palette_and_occupancy_fail_without_outputs() {
        let same = Path::new("same");
        assert!(
            execute(IndexedMap16Import {
                indices: same,
                graphics: same,
                occupancy: same,
                palette_row: 0,
                acts_like: 0,
                source_page: 0,
                graphics_output: same,
                occupancy_output: same,
                page_output: same,
            })
            .is_err()
        );
        let directory = directory();
        let indices = directory.join("page.idx");
        let graphics = directory.join("base.lmgfx");
        let occupancy = directory.join("base.occ");
        let output_graphics = directory.join("result.lmgfx");
        let output_occupancy = directory.join("result.occ");
        let output_page = directory.join("result.map16");
        fs::write(&indices, vec![0; PIXEL_COUNT]).unwrap();
        fs::write(&graphics, b"invalid").unwrap();
        fs::write(&occupancy, [2]).unwrap();
        assert!(
            execute(IndexedMap16Import {
                indices: &indices,
                graphics: &graphics,
                occupancy: &occupancy,
                palette_row: 8,
                acts_like: 0,
                source_page: 0,
                graphics_output: &output_graphics,
                occupancy_output: &output_occupancy,
                page_output: &output_page,
            })
            .is_err()
        );
        assert!(!output_graphics.exists());
        assert!(!output_occupancy.exists());
        assert!(!output_page.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
