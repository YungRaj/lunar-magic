use crate::{Canvas, CanvasError, draw_map16_tile};
use lm_graphics::{GraphicsInterchangeFile, Palette, PaletteInterchangeFile};
use lm_level::{Map16Page, Map16PageFile};
use std::fmt;

const PAGE_COLUMNS: usize = 16;
const TILE_PIXELS: usize = 16;
const PALETTE_ROW_COLORS: usize = 16;
const REQUIRED_PALETTE_ROWS: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortableMap16RenderError {
    WrongTileCount(usize),
    InvalidPaletteShape(usize),
    TooFewPaletteRows(usize),
    MissingGraphicsTile { definition: usize, tile: u16 },
    MissingPaletteRow { definition: usize, row: u8 },
    DimensionOverflow,
    Canvas(CanvasError),
}

impl fmt::Display for PortableMap16RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "portable Map16 render failed: {self:?}")
    }
}

impl std::error::Error for PortableMap16RenderError {}

/// Renders one exact portable Map16 page as a 16-by-16 tile sheet.
///
/// # Errors
///
/// Rejects malformed page or palette shapes, missing asset references, dimension overflow, and
/// canvases exceeding the renderer's allocation bound.
pub fn render_portable_map16_page(
    graphics: &GraphicsInterchangeFile,
    palette: &PaletteInterchangeFile,
    page: &Map16PageFile,
) -> Result<Canvas, PortableMap16RenderError> {
    if page.page.tiles.len() != Map16Page::TILE_COUNT {
        return Err(PortableMap16RenderError::WrongTileCount(
            page.page.tiles.len(),
        ));
    }
    let palettes = palette_rows(&palette.palette)?;
    validate_assets(page, graphics, &palettes)?;
    let dimension = PAGE_COLUMNS
        .checked_mul(TILE_PIXELS)
        .ok_or(PortableMap16RenderError::DimensionOverflow)?;
    let mut canvas =
        Canvas::try_new(dimension, dimension).map_err(PortableMap16RenderError::Canvas)?;
    for (index, definition) in page.page.tiles.iter().enumerate() {
        draw_map16_tile(
            &mut canvas,
            *definition,
            &graphics.graphics.tiles,
            &palettes,
            index % PAGE_COLUMNS * TILE_PIXELS,
            index / PAGE_COLUMNS * TILE_PIXELS,
        );
    }
    Ok(canvas)
}

fn palette_rows(palette: &Palette) -> Result<Vec<Palette>, PortableMap16RenderError> {
    if palette.colors.len() % PALETTE_ROW_COLORS != 0 {
        return Err(PortableMap16RenderError::InvalidPaletteShape(
            palette.colors.len(),
        ));
    }
    let rows = palette
        .colors
        .chunks_exact(PALETTE_ROW_COLORS)
        .map(|colors| Palette {
            colors: colors.to_vec(),
        })
        .collect::<Vec<_>>();
    if rows.len() < REQUIRED_PALETTE_ROWS {
        return Err(PortableMap16RenderError::TooFewPaletteRows(rows.len()));
    }
    Ok(rows)
}

fn validate_assets(
    page: &Map16PageFile,
    graphics: &GraphicsInterchangeFile,
    palettes: &[Palette],
) -> Result<(), PortableMap16RenderError> {
    for (definition, value) in page.page.tiles.iter().enumerate() {
        for subtile in [
            value.top_left,
            value.top_right,
            value.bottom_left,
            value.bottom_right,
        ] {
            if usize::from(subtile.tile_number()) >= graphics.graphics.tiles.len() {
                return Err(PortableMap16RenderError::MissingGraphicsTile {
                    definition,
                    tile: subtile.tile_number(),
                });
            }
            if usize::from(subtile.palette()) >= palettes.len() {
                return Err(PortableMap16RenderError::MissingPaletteRow {
                    definition,
                    row: subtile.palette(),
                });
            }
        }
    }
    Ok(())
}
