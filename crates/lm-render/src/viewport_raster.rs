use crate::{Canvas, CanvasError, Point, Viewport, ViewportError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewportRasterError {
    Canvas(CanvasError),
    Viewport(ViewportError),
}

impl std::fmt::Display for ViewportRasterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "failed to rasterize viewport: {self:?}")
    }
}

impl std::error::Error for ViewportRasterError {}

impl From<CanvasError> for ViewportRasterError {
    fn from(error: CanvasError) -> Self {
        Self::Canvas(error)
    }
}

impl From<ViewportError> for ViewportRasterError {
    fn from(error: ViewportError) -> Self {
        Self::Viewport(error)
    }
}

/// Samples an existing world-space canvas through an exact viewport transform.
///
/// Sampling is deterministic nearest-neighbor. World coordinates outside `source` are transparent,
/// making signed camera origins safe without requiring a padded source raster.
///
/// # Errors
///
/// Returns [`ViewportRasterError`] if the output allocation exceeds [`Canvas`] limits or a screen
/// coordinate cannot be represented by the viewport transform.
pub fn rasterize_canvas_viewport(
    source: &Canvas,
    viewport: Viewport,
) -> Result<Canvas, ViewportRasterError> {
    let width = usize::try_from(viewport.width).map_err(|_| CanvasError::DimensionOverflow)?;
    let height = usize::try_from(viewport.height).map_err(|_| CanvasError::DimensionOverflow)?;
    let mut output = Canvas::try_new(width, height)?;

    for screen_y in 0..height {
        for screen_x in 0..width {
            let world = viewport.screen_to_world(Point {
                x: i64::try_from(screen_x).map_err(|_| ViewportError::CoordinateOverflow)?,
                y: i64::try_from(screen_y).map_err(|_| ViewportError::CoordinateOverflow)?,
            })?;
            let (Ok(source_x), Ok(source_y)) = (usize::try_from(world.x), usize::try_from(world.y))
            else {
                continue;
            };
            if let Some(color) = source.get(source_x, source_y) {
                output.set(screen_x, screen_y, color);
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rgba;

    fn color(value: u8) -> Rgba {
        Rgba {
            red: value,
            green: 0,
            blue: 0,
            alpha: 255,
        }
    }

    fn source() -> Canvas {
        Canvas::from_pixels(3, 2, (1..=6).map(color).collect()).unwrap()
    }

    #[test]
    fn unity_viewport_crops_the_source() {
        let viewport = Viewport::new(Point { x: 1, y: 0 }, 2, 2, 1, 1).unwrap();
        let output = rasterize_canvas_viewport(&source(), viewport).unwrap();
        assert_eq!(output.pixels(), &[color(2), color(3), color(5), color(6)]);
    }

    #[test]
    fn integer_zoom_repeats_pixels_exactly() {
        let viewport = Viewport::new(Point::default(), 4, 2, 2, 1).unwrap();
        let output = rasterize_canvas_viewport(&source(), viewport).unwrap();
        assert_eq!(
            output.pixels(),
            &[
                color(1),
                color(1),
                color(2),
                color(2),
                color(1),
                color(1),
                color(2),
                color(2),
            ]
        );
    }

    #[test]
    fn fractional_zoom_uses_exact_nearest_neighbor_buckets() {
        let viewport = Viewport::new(Point::default(), 3, 1, 3, 2).unwrap();
        let output = rasterize_canvas_viewport(&source(), viewport).unwrap();
        assert_eq!(output.pixels(), &[color(1), color(1), color(2)]);
    }

    #[test]
    fn signed_origins_and_source_edges_are_transparent() {
        let viewport = Viewport::new(Point { x: -1, y: -1 }, 3, 3, 1, 1).unwrap();
        let output = rasterize_canvas_viewport(&source(), viewport).unwrap();
        let clear = Rgba::default();
        assert_eq!(
            output.pixels(),
            &[
                clear,
                clear,
                clear,
                clear,
                color(1),
                color(2),
                clear,
                color(4),
                color(5),
            ]
        );
    }

    #[test]
    fn output_allocation_and_coordinate_overflow_are_typed() {
        let too_large = Viewport::new(Point::default(), u32::MAX, u32::MAX, 1, 1).unwrap();
        assert!(matches!(
            rasterize_canvas_viewport(&source(), too_large),
            Err(ViewportRasterError::Canvas(_))
        ));

        let overflowing = Viewport::new(Point { x: i64::MAX, y: 0 }, 2, 1, 1, 1).unwrap();
        assert_eq!(
            rasterize_canvas_viewport(&source(), overflowing),
            Err(ViewportRasterError::Viewport(
                ViewportError::CoordinateOverflow
            ))
        );
    }
}
