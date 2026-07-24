use crate::{Canvas, Rgba, WorldRect};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridOverlay {
    pub origin_x: i64,
    pub origin_y: i64,
    pub cell_width: u32,
    pub cell_height: u32,
    pub color: Rgba,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionOverlay {
    /// Half-open screen-space selection rectangle.
    pub bounds: WorldRect,
    pub light: Rgba,
    pub dark: Rgba,
    pub dash_length: u32,
    pub phase: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorOverlay {
    Grid(GridOverlay),
    Selection(SelectionOverlay),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorOverlayError {
    ZeroGridSpacing,
    ZeroDashLength,
    InvalidSelectionBounds,
}

impl std::fmt::Display for EditorOverlayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid editor overlay: {self:?}")
    }
}

impl std::error::Error for EditorOverlayError {}

/// Draws painter-ordered editor overlays over an existing viewport canvas.
///
/// All coordinates are signed screen pixels. Geometry outside the canvas is clipped, and colors
/// use deterministic straight-alpha source-over compositing.
///
/// # Errors
///
/// Rejects zero grid spacing, zero-length dash patterns, and inverted or empty selections.
pub fn draw_editor_overlays(
    canvas: &mut Canvas,
    overlays: &[EditorOverlay],
) -> Result<(), EditorOverlayError> {
    validate_editor_overlays(overlays)?;
    for overlay in overlays {
        match *overlay {
            EditorOverlay::Grid(grid) => draw_grid(canvas, grid),
            EditorOverlay::Selection(selection) => draw_selection(canvas, selection),
        }
    }
    Ok(())
}

/// Validates an overlay batch without allocating or drawing.
///
/// # Errors
///
/// Rejects zero grid spacing, zero-length dash patterns, and inverted or empty selections.
pub fn validate_editor_overlays(overlays: &[EditorOverlay]) -> Result<(), EditorOverlayError> {
    for overlay in overlays {
        match overlay {
            EditorOverlay::Grid(grid) if grid.cell_width == 0 || grid.cell_height == 0 => {
                return Err(EditorOverlayError::ZeroGridSpacing);
            }
            EditorOverlay::Selection(selection) if selection.dash_length == 0 => {
                return Err(EditorOverlayError::ZeroDashLength);
            }
            EditorOverlay::Selection(selection)
                if selection.bounds.left >= selection.bounds.right
                    || selection.bounds.top >= selection.bounds.bottom =>
            {
                return Err(EditorOverlayError::InvalidSelectionBounds);
            }
            _ => {}
        }
    }
    Ok(())
}

fn draw_grid(canvas: &mut Canvas, grid: GridOverlay) {
    let first_x = residue(grid.origin_x, grid.cell_width);
    for x in (first_x..canvas.width()).step_by(grid.cell_width as usize) {
        for y in 0..canvas.height() {
            blend_pixel(canvas, x, y, grid.color);
        }
    }
    let first_y = residue(grid.origin_y, grid.cell_height);
    for y in (first_y..canvas.height()).step_by(grid.cell_height as usize) {
        for x in 0..canvas.width() {
            if x >= first_x && (x - first_x) % grid.cell_width as usize == 0 {
                continue;
            }
            blend_pixel(canvas, x, y, grid.color);
        }
    }
}

fn residue(origin: i64, spacing: u32) -> usize {
    usize::try_from(origin.rem_euclid(i64::from(spacing))).unwrap_or(0)
}

fn draw_selection(canvas: &mut Canvas, selection: SelectionOverlay) {
    let left = selection.bounds.left;
    let top = selection.bounds.top;
    let right = selection.bounds.right - 1;
    let bottom = selection.bounds.bottom - 1;
    for y in 0..canvas.height() {
        let screen_y = i64::try_from(y).unwrap_or(i64::MAX);
        if screen_y < top || screen_y > bottom {
            continue;
        }
        for x in 0..canvas.width() {
            let screen_x = i64::try_from(x).unwrap_or(i64::MAX);
            if screen_x < left || screen_x > right {
                continue;
            }
            if screen_x != left && screen_x != right && screen_y != top && screen_y != bottom {
                continue;
            }
            let stripe =
                (i128::from(screen_x) + i128::from(screen_y) + i128::from(selection.phase))
                    .div_euclid(i128::from(selection.dash_length));
            let color = if stripe.rem_euclid(2) == 0 {
                selection.light
            } else {
                selection.dark
            };
            blend_pixel(canvas, x, y, color);
        }
    }
}

fn blend_pixel(canvas: &mut Canvas, x: usize, y: usize, source: Rgba) {
    let Some(destination) = canvas.get(x, y) else {
        return;
    };
    canvas.set(x, y, source_over(source, destination));
}

fn source_over(source: Rgba, destination: Rgba) -> Rgba {
    let source_alpha = u32::from(source.alpha);
    let destination_alpha = u32::from(destination.alpha);
    let inverse = 255 - source_alpha;
    let output_alpha = source_alpha + divide_255(destination_alpha * inverse);
    if output_alpha == 0 {
        return Rgba::default();
    }
    let channel = |source_channel: u8, destination_channel: u8| {
        let premultiplied = u32::from(source_channel) * source_alpha * 255
            + u32::from(destination_channel) * destination_alpha * inverse;
        let denominator = output_alpha * 255;
        u8::try_from((premultiplied + denominator / 2) / denominator).unwrap_or(u8::MAX)
    };
    Rgba {
        red: channel(source.red, destination.red),
        green: channel(source.green, destination.green),
        blue: channel(source.blue, destination.blue),
        alpha: u8::try_from(output_alpha).unwrap_or(u8::MAX),
    }
}

const fn divide_255(value: u32) -> u32 {
    (value + 127) / 255
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgba = Rgba {
        red: 255,
        green: 0,
        blue: 0,
        alpha: 255,
    };
    const BLUE: Rgba = Rgba {
        red: 0,
        green: 0,
        blue: 255,
        alpha: 255,
    };

    #[test]
    fn signed_grid_origin_repeats_and_intersections_do_not_overflow() {
        let mut canvas = Canvas::try_new(5, 4).unwrap();
        draw_editor_overlays(
            &mut canvas,
            &[EditorOverlay::Grid(GridOverlay {
                origin_x: -1,
                origin_y: 1,
                cell_width: 3,
                cell_height: 2,
                color: RED,
            })],
        )
        .unwrap();
        for (x, y) in [(2, 0), (2, 1), (2, 3), (0, 1), (4, 3)] {
            assert_eq!(canvas.get(x, y), Some(RED));
        }
        assert_eq!(canvas.get(0, 0), Some(Rgba::default()));
    }

    #[test]
    fn selection_is_half_open_clipped_and_phase_animated() {
        let mut first = Canvas::try_new(4, 3).unwrap();
        let selection = SelectionOverlay {
            bounds: WorldRect {
                left: -1,
                top: 0,
                right: 3,
                bottom: 2,
            },
            light: RED,
            dark: BLUE,
            dash_length: 1,
            phase: 0,
        };
        draw_editor_overlays(&mut first, &[EditorOverlay::Selection(selection)]).unwrap();
        assert_eq!(first.get(0, 0), Some(RED));
        assert_eq!(first.get(1, 0), Some(BLUE));
        assert_eq!(first.get(2, 1), Some(BLUE));
        assert_eq!(first.get(3, 0), Some(Rgba::default()));

        let mut second = Canvas::try_new(4, 3).unwrap();
        draw_editor_overlays(
            &mut second,
            &[EditorOverlay::Selection(SelectionOverlay {
                phase: 1,
                ..selection
            })],
        )
        .unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn alpha_compositing_handles_opaque_and_transparent_destinations() {
        let half_red = Rgba { alpha: 128, ..RED };
        assert_eq!(
            source_over(half_red, BLUE),
            Rgba {
                red: 128,
                green: 0,
                blue: 127,
                alpha: 255
            }
        );
        assert_eq!(
            source_over(half_red, Rgba::default()),
            Rgba { alpha: 128, ..RED }
        );
    }

    #[test]
    fn every_overlay_is_validated_before_drawing() {
        let mut canvas = Canvas::try_new(2, 2).unwrap();
        let original = canvas.clone();
        let overlays = [
            EditorOverlay::Grid(GridOverlay {
                origin_x: 0,
                origin_y: 0,
                cell_width: 1,
                cell_height: 1,
                color: RED,
            }),
            EditorOverlay::Selection(SelectionOverlay {
                bounds: WorldRect {
                    left: 0,
                    top: 0,
                    right: 1,
                    bottom: 1,
                },
                light: RED,
                dark: BLUE,
                dash_length: 0,
                phase: 0,
            }),
        ];
        assert_eq!(
            draw_editor_overlays(&mut canvas, &overlays),
            Err(EditorOverlayError::ZeroDashLength)
        );
        assert_eq!(canvas, original);

        assert_eq!(
            draw_editor_overlays(
                &mut canvas,
                &[EditorOverlay::Grid(GridOverlay {
                    origin_x: i64::MIN,
                    origin_y: i64::MAX,
                    cell_width: 0,
                    cell_height: 1,
                    color: RED,
                })]
            ),
            Err(EditorOverlayError::ZeroGridSpacing)
        );
        assert_eq!(
            draw_editor_overlays(
                &mut canvas,
                &[EditorOverlay::Selection(SelectionOverlay {
                    bounds: WorldRect {
                        left: 4,
                        top: 0,
                        right: 4,
                        bottom: 1,
                    },
                    light: RED,
                    dark: BLUE,
                    dash_length: 1,
                    phase: 0,
                })]
            ),
            Err(EditorOverlayError::InvalidSelectionBounds)
        );
        assert_eq!(canvas, original);
    }
}
