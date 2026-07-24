use super::{put, put_hex};
use crate::Observation;
use lm_level::{Map16Page, Map16PageFile, Map16Set};

/// Produces a canonical semantic snapshot of one complete Map16 page.
#[must_use]
pub fn observe_map16_page(page: &Map16Page) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "map16/tile-count", page.tiles.len());
    for (index, tile) in page.tiles.iter().enumerate() {
        let base = format!("map16/tiles/{index:04x}");
        put_hex(
            &mut result,
            &format!("{base}/graphics"),
            &tile.encode_graphics(),
        );
        put(&mut result, &format!("{base}/acts-like"), tile.acts_like);
    }
    result
}

/// Produces a canonical snapshot of a page artifact including its source-page identity.
///
/// # Panics
///
/// Panics only if the fixed source identity path collides with a page-observer path, which would
/// indicate an internal observer schema regression.
#[must_use]
pub fn observe_map16_page_file(file: &Map16PageFile) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "map16/source-page", file.source_page);
    for (path, value) in observe_map16_page(&file.page).entries() {
        result
            .insert(path, value)
            .expect("source identity and page fields are disjoint");
    }
    result
}

/// Produces a canonical page/tile-addressable snapshot of a complete Map16 workspace.
#[must_use]
pub fn observe_map16_set(set: &Map16Set) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "map16/pages/count", set.pages.len());
    for (page_index, page) in set.pages.iter().enumerate() {
        put(
            &mut result,
            &format!("map16/pages/{page_index:04x}/tile-count"),
            page.tiles.len(),
        );
        for (tile_index, tile) in page.tiles.iter().enumerate() {
            let base = format!("map16/pages/{page_index:04x}/tiles/{tile_index:04x}");
            put_hex(
                &mut result,
                &format!("{base}/graphics"),
                &tile.encode_graphics(),
            );
            put(&mut result, &format!("{base}/acts-like"), tile.acts_like);
        }
    }
    result
}
