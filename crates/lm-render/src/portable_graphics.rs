use crate::{Canvas, CanvasError, draw_indexed_tile};
use lm_graphics::{GraphicsInterchangeFile, IndexedTile, Palette, PaletteInterchangeFile};
use std::fmt;

const PALETTE_ROW_COLORS: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortableGraphicsRenderError {
    EmptyGraphics,
    ZeroColumns,
    InvalidPaletteShape(usize),
    MissingPaletteRow { requested: usize, available: usize },
    DimensionOverflow,
    Canvas(CanvasError),
}

impl fmt::Display for PortableGraphicsRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "portable graphics render failed: {self:?}")
    }
}

impl std::error::Error for PortableGraphicsRenderError {}

/// Renders one portable 4bpp graphics file as a deterministic tile sheet.
///
/// # Errors
///
/// Rejects empty graphics, zero columns, malformed or missing palette rows, dimension overflow,
/// and canvases above the renderer allocation bound.
pub fn render_portable_graphics(
    graphics: &GraphicsInterchangeFile,
    palette: &PaletteInterchangeFile,
    palette_row: usize,
    columns: usize,
) -> Result<Canvas, PortableGraphicsRenderError> {
    if graphics.graphics.tiles.is_empty() {
        return Err(PortableGraphicsRenderError::EmptyGraphics);
    }
    if columns == 0 {
        return Err(PortableGraphicsRenderError::ZeroColumns);
    }
    let palette = palette_row_value(&palette.palette, palette_row)?;
    let rows = graphics
        .graphics
        .tiles
        .len()
        .checked_add(columns - 1)
        .and_then(|value| value.checked_div(columns))
        .ok_or(PortableGraphicsRenderError::DimensionOverflow)?;
    let width = columns
        .checked_mul(IndexedTile::WIDTH)
        .ok_or(PortableGraphicsRenderError::DimensionOverflow)?;
    let height = rows
        .checked_mul(IndexedTile::HEIGHT)
        .ok_or(PortableGraphicsRenderError::DimensionOverflow)?;
    let mut canvas = Canvas::try_new(width, height).map_err(PortableGraphicsRenderError::Canvas)?;
    for (index, tile) in graphics.graphics.tiles.iter().enumerate() {
        draw_indexed_tile(
            &mut canvas,
            tile,
            &palette,
            index % columns * IndexedTile::WIDTH,
            index / columns * IndexedTile::HEIGHT,
        );
    }
    Ok(canvas)
}

fn palette_row_value(
    palette: &Palette,
    row: usize,
) -> Result<Palette, PortableGraphicsRenderError> {
    if palette.colors.len() % PALETTE_ROW_COLORS != 0 {
        return Err(PortableGraphicsRenderError::InvalidPaletteShape(
            palette.colors.len(),
        ));
    }
    let available = palette.colors.len() / PALETTE_ROW_COLORS;
    let start = row
        .checked_mul(PALETTE_ROW_COLORS)
        .ok_or(PortableGraphicsRenderError::DimensionOverflow)?;
    let end = start
        .checked_add(PALETTE_ROW_COLORS)
        .ok_or(PortableGraphicsRenderError::DimensionOverflow)?;
    let colors =
        palette
            .colors
            .get(start..end)
            .ok_or(PortableGraphicsRenderError::MissingPaletteRow {
                requested: row,
                available,
            })?;
    Ok(Palette {
        colors: colors.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile};

    #[test]
    fn renders_partial_final_row_with_selected_palette() {
        let graphics = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([1; 64]); 3],
            },
        };
        let palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette {
                colors: (0..32)
                    .map(|index| {
                        if index == 17 {
                            Bgr555(0x001f)
                        } else {
                            Bgr555(0)
                        }
                    })
                    .collect(),
            },
        };
        let canvas = render_portable_graphics(&graphics, &palette, 1, 2).unwrap();
        assert_eq!((canvas.width(), canvas.height()), (16, 16));
        assert_eq!(canvas.get(0, 0).unwrap().red, 255);
        assert_eq!(canvas.get(8, 8), Some(crate::Rgba::default()));
    }

    #[test]
    fn invalid_shapes_rows_and_dimensions_fail() {
        let graphics = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: lm_graphics::GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([0; 64])],
            },
        };
        let mut palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette { colors: vec![] },
        };
        assert!(render_portable_graphics(&graphics, &palette, 0, 0).is_err());
        assert!(render_portable_graphics(&graphics, &palette, 0, 1).is_err());
        palette.palette.colors = vec![Bgr555(0); 17];
        assert!(render_portable_graphics(&graphics, &palette, 0, 1).is_err());
    }
}
