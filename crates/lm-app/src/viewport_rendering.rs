use lm_render::{
    Canvas, EditorOverlay, EditorOverlayError, Viewport, ViewportRasterError, draw_editor_overlays,
    rasterize_canvas_viewport,
};

use crate::LevelViewport;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorPreviewError {
    Raster(ViewportRasterError),
    Overlay(EditorOverlayError),
}

impl std::fmt::Display for EditorPreviewError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "failed to compose editor preview: {self:?}")
    }
}

impl std::error::Error for EditorPreviewError {}

/// Renders a world-space canvas using the camera state stored by the application shell.
///
/// # Errors
///
/// Returns [`ViewportRasterError`] when dimensions cannot be allocated or viewport arithmetic
/// overflows.
pub fn render_editor_viewport(
    source: &Canvas,
    camera: LevelViewport,
    width: u32,
    height: u32,
) -> Result<Canvas, ViewportRasterError> {
    let (zoom_numerator, zoom_denominator) = camera.zoom();
    let viewport = Viewport::new(
        camera.origin,
        width,
        height,
        zoom_numerator,
        zoom_denominator,
    )
    .map_err(ViewportRasterError::Viewport)?;
    rasterize_canvas_viewport(source, viewport)
}

/// Backward-compatible level-editor name for [`render_editor_viewport`].
///
/// # Errors
///
/// Returns [`ViewportRasterError`] under the same conditions as [`render_editor_viewport`].
pub fn render_level_viewport(
    source: &Canvas,
    camera: LevelViewport,
    width: u32,
    height: u32,
) -> Result<Canvas, ViewportRasterError> {
    render_editor_viewport(source, camera, width, height)
}

/// Renders one viewport and then composites painter-ordered screen-space editor overlays.
///
/// # Errors
///
/// Returns [`EditorPreviewError`] for viewport allocation/arithmetic failures or malformed overlay
/// geometry.
pub fn render_editor_preview(
    source: &Canvas,
    camera: LevelViewport,
    width: u32,
    height: u32,
    overlays: &[EditorOverlay],
) -> Result<Canvas, EditorPreviewError> {
    let mut canvas = render_editor_viewport(source, camera, width, height)
        .map_err(EditorPreviewError::Raster)?;
    draw_editor_overlays(&mut canvas, overlays).map_err(EditorPreviewError::Overlay)?;
    Ok(canvas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_render::{Point, Rgba, ViewportError};

    #[test]
    fn application_camera_drives_exact_renderer_zoom_and_pan() {
        let source = Canvas::from_pixels(
            3,
            1,
            [10, 20, 30]
                .into_iter()
                .map(|red| Rgba {
                    red,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                })
                .collect(),
        )
        .unwrap();
        let camera = LevelViewport::new(Point { x: 1, y: 0 }, 2, 1).unwrap();
        let rendered = render_editor_viewport(&source, camera, 4, 1).unwrap();
        assert_eq!(
            rendered
                .pixels()
                .iter()
                .map(|pixel| pixel.red)
                .collect::<Vec<_>>(),
            vec![20, 20, 30, 30]
        );
    }

    #[test]
    fn renderer_preserves_signed_application_origins() {
        let source = Canvas::from_pixels(
            1,
            1,
            vec![Rgba {
                red: 7,
                green: 0,
                blue: 0,
                alpha: 255,
            }],
        )
        .unwrap();
        let camera = LevelViewport::new(Point { x: -1, y: 0 }, 1, 1).unwrap();
        let rendered = render_editor_viewport(&source, camera, 2, 1).unwrap();
        assert_eq!(rendered.get(0, 0), Some(Rgba::default()));
        assert_eq!(rendered.get(1, 0), source.get(0, 0));
    }

    #[test]
    fn impossible_output_size_is_reported_without_allocation() {
        let source = Canvas::try_new(1, 1).unwrap();
        let error = render_editor_viewport(&source, LevelViewport::default(), u32::MAX, u32::MAX)
            .unwrap_err();
        assert!(matches!(error, ViewportRasterError::Canvas(_)));
    }

    #[test]
    fn renderer_error_type_retains_viewport_failures() {
        let error = ViewportRasterError::Viewport(ViewportError::CoordinateOverflow);
        assert!(error.to_string().contains("CoordinateOverflow"));
    }

    #[test]
    fn preview_composes_screen_overlays_after_camera_sampling() {
        use lm_render::{EditorOverlay, GridOverlay};

        let source = Canvas::from_pixels(
            1,
            1,
            vec![Rgba {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 255,
            }],
        )
        .unwrap();
        let overlay = EditorOverlay::Grid(GridOverlay {
            origin_x: 1,
            origin_y: 1,
            cell_width: 2,
            cell_height: 2,
            color: Rgba {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            },
        });
        let rendered =
            render_editor_preview(&source, LevelViewport::default(), 2, 2, &[overlay]).unwrap();
        assert_eq!(rendered.get(0, 0), source.get(0, 0));
        assert_eq!(rendered.get(1, 0).unwrap().alpha, 255);
        assert_eq!(rendered.get(0, 1).unwrap().alpha, 255);
    }
}
