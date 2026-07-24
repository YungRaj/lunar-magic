use crate::{Canvas, PortableMap16RenderError, render_portable_map16_page};
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::{DscDisplayContext, DscResolvedTable, Map16Page, Map16PageFile, Map16SetFile};
use std::fmt;

const PAGE_COLUMNS: usize = 16;
const TILE_PIXELS: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortableDscMap16RenderError {
    MissingSourcePage(usize),
    WrongSourceTileCount(usize),
    MissingDisplayTile { source: u16, target: u16 },
    Render(PortableMap16RenderError),
}

impl fmt::Display for PortableDscMap16RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "portable DSC Map16 render failed: {self:?}")
    }
}

impl std::error::Error for PortableDscMap16RenderError {}

/// Renders a Map16 page after applying Lunar Magic's direct custom-display substitutions.
///
/// # Errors
///
/// Rejects a missing or malformed source page, a mapped tile outside the supplied complete Map16
/// set, or any ordinary portable Map16 render error.
pub fn render_portable_map16_page_with_dsc(
    graphics: &GraphicsInterchangeFile,
    palette: &PaletteInterchangeFile,
    map16: &Map16SetFile,
    page_index: usize,
    dsc: &DscResolvedTable,
    context: DscDisplayContext,
) -> Result<Canvas, PortableDscMap16RenderError> {
    let source_page = map16
        .set
        .pages
        .get(page_index)
        .ok_or(PortableDscMap16RenderError::MissingSourcePage(page_index))?;
    if source_page.tiles.len() != Map16Page::TILE_COUNT {
        return Err(PortableDscMap16RenderError::WrongSourceTileCount(
            source_page.tiles.len(),
        ));
    }
    let page_base = page_index
        .checked_mul(Map16Page::TILE_COUNT)
        .ok_or(PortableDscMap16RenderError::MissingSourcePage(page_index))?;
    let mut rendered = Vec::with_capacity(Map16Page::TILE_COUNT);
    let mut blended = Vec::with_capacity(Map16Page::TILE_COUNT);
    for index in 0..Map16Page::TILE_COUNT {
        let source = u16::try_from(page_base + index)
            .map_err(|_| PortableDscMap16RenderError::MissingSourcePage(page_index))?;
        let resolution = dsc.resolve_display(source, context);
        let target = usize::from(resolution.tile_id);
        let definition = map16
            .set
            .pages
            .get(target / Map16Page::TILE_COUNT)
            .and_then(|page| page.tiles.get(target % Map16Page::TILE_COUNT))
            .copied()
            .ok_or(PortableDscMap16RenderError::MissingDisplayTile {
                source,
                target: resolution.tile_id,
            })?;
        rendered.push(definition);
        blended.push(resolution.blended);
    }
    let page = Map16PageFile {
        source_page: u16::try_from(page_index)
            .map_err(|_| PortableDscMap16RenderError::MissingSourcePage(page_index))?,
        page: Map16Page::new(rendered)
            .map_err(|_| PortableDscMap16RenderError::WrongSourceTileCount(0))?,
    };
    let mut canvas = render_portable_map16_page(graphics, palette, &page)
        .map_err(PortableDscMap16RenderError::Render)?;
    apply_black_average(&mut canvas, &blended);
    Ok(canvas)
}

fn apply_black_average(canvas: &mut Canvas, blended: &[bool]) {
    for (index, blend) in blended.iter().copied().enumerate() {
        if !blend {
            continue;
        }
        let origin_x = index % PAGE_COLUMNS * TILE_PIXELS;
        let origin_y = index / PAGE_COLUMNS * TILE_PIXELS;
        for y in origin_y..origin_y + TILE_PIXELS {
            for x in origin_x..origin_x + TILE_PIXELS {
                if let Some(mut pixel) = canvas.get(x, y) {
                    pixel.red >>= 1;
                    pixel.green >>= 1;
                    pixel.blue >>= 1;
                    canvas.set(x, y, pixel);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette};
    use lm_level::{DscDescriptionStyle, DscSidecar, Map16Set, Map16Tile, Subtile};

    fn assets() -> (GraphicsInterchangeFile, PaletteInterchangeFile) {
        let mut first = [0; IndexedTile::PIXEL_COUNT];
        first[0] = 1;
        let mut second = [0; IndexedTile::PIXEL_COUNT];
        second[0] = 2;
        (
            GraphicsInterchangeFile {
                source_slot: 0,
                graphics: GraphicsFile4bpp {
                    tiles: vec![IndexedTile::new(first), IndexedTile::new(second)],
                },
            },
            PaletteInterchangeFile {
                source_palette: 0,
                palette: Palette {
                    colors: [Bgr555(0), Bgr555(0x001f), Bgr555(0x03e0)]
                        .into_iter()
                        .chain(std::iter::repeat_n(Bgr555(0), 125))
                        .collect(),
                },
            },
        )
    }

    #[test]
    fn mapping_selects_another_definition_and_blends_it() {
        let mut tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        tiles[1] = Map16Tile {
            top_left: Subtile(1),
            top_right: Subtile(1),
            bottom_left: Subtile(1),
            bottom_right: Subtile(1),
            acts_like: 0,
        };
        let map16 = Map16SetFile {
            set: Map16Set {
                pages: vec![Map16Page::new(tiles).unwrap()],
            },
        };
        let source = DscSidecar::decode(b"0\t4\t1\n").unwrap();
        let dsc = DscResolvedTable::from_sidecar(
            &source,
            DscDescriptionStyle {
                background: 0,
                detail: 0,
                foreground: 0,
                mode: 0,
            },
        );
        let (graphics, palette) = assets();
        let canvas = render_portable_map16_page_with_dsc(
            &graphics,
            &palette,
            &map16,
            0,
            &dsc,
            DscDisplayContext {
                first_feature_enabled: true,
                ..DscDisplayContext::default()
            },
        )
        .unwrap();
        let pixel = canvas.get(0, 0).unwrap();
        assert_eq!((pixel.red, pixel.green, pixel.blue), (0, 127, 0));
    }

    #[test]
    fn missing_mapping_target_is_an_explicit_error() {
        let map16 = Map16SetFile {
            set: Map16Set {
                pages: vec![
                    Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
                ],
            },
        };
        let source = DscSidecar::decode(b"0\t4\t100\n").unwrap();
        let dsc = DscResolvedTable::from_sidecar(
            &source,
            DscDescriptionStyle {
                background: 0,
                detail: 0,
                foreground: 0,
                mode: 0,
            },
        );
        let (graphics, palette) = assets();
        assert!(matches!(
            render_portable_map16_page_with_dsc(
                &graphics,
                &palette,
                &map16,
                0,
                &dsc,
                DscDisplayContext {
                    first_feature_enabled: true,
                    ..DscDisplayContext::default()
                }
            ),
            Err(PortableDscMap16RenderError::MissingDisplayTile {
                source: 0,
                target: 0x100
            })
        ));
    }
}
