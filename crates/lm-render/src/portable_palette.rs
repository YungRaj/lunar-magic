use crate::{Canvas, CanvasError, Rgba};
use lm_graphics::PaletteInterchangeFile;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PortablePaletteRenderError {
    EmptyPalette,
    ZeroColumns,
    ZeroCellSize,
    DimensionOverflow,
    Canvas(CanvasError),
}

impl fmt::Display for PortablePaletteRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "portable palette render failed: {self:?}")
    }
}

impl std::error::Error for PortablePaletteRenderError {}

/// Renders exact SNES colors as an opaque row-major swatch grid.
///
/// # Errors
///
/// Rejects empty palettes, zero layout values, dimension overflow, and excessive canvases.
pub fn render_portable_palette(
    palette: &PaletteInterchangeFile,
    columns: usize,
    cell_size: usize,
) -> Result<Canvas, PortablePaletteRenderError> {
    if palette.palette.colors.is_empty() {
        return Err(PortablePaletteRenderError::EmptyPalette);
    }
    if columns == 0 {
        return Err(PortablePaletteRenderError::ZeroColumns);
    }
    if cell_size == 0 {
        return Err(PortablePaletteRenderError::ZeroCellSize);
    }
    let rows = palette
        .palette
        .colors
        .len()
        .checked_add(columns - 1)
        .and_then(|value| value.checked_div(columns))
        .ok_or(PortablePaletteRenderError::DimensionOverflow)?;
    let width = columns
        .checked_mul(cell_size)
        .ok_or(PortablePaletteRenderError::DimensionOverflow)?;
    let height = rows
        .checked_mul(cell_size)
        .ok_or(PortablePaletteRenderError::DimensionOverflow)?;
    let mut canvas = Canvas::try_new(width, height).map_err(PortablePaletteRenderError::Canvas)?;
    for (index, color) in palette.palette.colors.iter().enumerate() {
        let rgb = color.to_rgb8();
        let rgba = Rgba {
            red: rgb.red,
            green: rgb.green,
            blue: rgb.blue,
            alpha: 255,
        };
        let origin_x = index % columns * cell_size;
        let origin_y = index / columns * cell_size;
        for y in origin_y..origin_y + cell_size {
            for x in origin_x..origin_x + cell_size {
                canvas.set(x, y, rgba);
            }
        }
    }
    Ok(canvas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, Palette};

    #[test]
    fn renders_opaque_swatch_grid_with_partial_row() {
        let palette = PaletteInterchangeFile {
            source_palette: 0,
            palette: Palette {
                colors: vec![Bgr555(0), Bgr555(0x001f), Bgr555(0x03e0)],
            },
        };
        let canvas = render_portable_palette(&palette, 2, 3).unwrap();
        assert_eq!((canvas.width(), canvas.height()), (6, 6));
        assert_eq!(canvas.get(0, 0).unwrap().alpha, 255);
        assert_eq!(canvas.get(3, 0).unwrap().red, 255);
        assert_eq!(canvas.get(0, 3).unwrap().green, 255);
        assert_eq!(canvas.get(3, 3), Some(Rgba::default()));
    }

    #[test]
    fn empty_and_zero_layouts_fail() {
        let empty = PaletteInterchangeFile {
            source_palette: 0,
            palette: lm_graphics::Palette { colors: vec![] },
        };
        assert!(render_portable_palette(&empty, 1, 1).is_err());
        let one = PaletteInterchangeFile {
            source_palette: 0,
            palette: lm_graphics::Palette {
                colors: vec![Bgr555(0)],
            },
        };
        assert!(render_portable_palette(&one, 0, 1).is_err());
        assert!(render_portable_palette(&one, 1, 0).is_err());
    }
}
