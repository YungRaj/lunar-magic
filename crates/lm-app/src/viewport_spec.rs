use crate::spec_text::{self, Fields, SpecError};
use lm_app::LevelViewport;
use lm_render::{Canvas, EditorOverlayFile, Point, draw_editor_overlays};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ViewportSpec {
    pub camera: LevelViewport,
    pub width: u32,
    pub height: u32,
}

impl ViewportSpec {
    pub(crate) fn from_optional_parts(
        origin_x: Option<i64>,
        origin_y: Option<i64>,
        width: Option<u32>,
        height: Option<u32>,
        zoom_numerator: Option<u32>,
        zoom_denominator: Option<u32>,
    ) -> Result<Option<Self>, SpecError> {
        let supplied = [
            origin_x.is_some(),
            origin_y.is_some(),
            width.is_some(),
            height.is_some(),
            zoom_numerator.is_some(),
            zoom_denominator.is_some(),
        ];
        if supplied.iter().all(|value| !value) {
            return Ok(None);
        }
        if !supplied.iter().all(|value| *value) {
            return Err(spec_text::error(
                "viewport-origin-x, viewport-origin-y, viewport-width, viewport-height, zoom-numerator, and zoom-denominator must be supplied together",
            ));
        }
        let (
            Some(origin_x),
            Some(origin_y),
            Some(width),
            Some(height),
            Some(numerator),
            Some(denominator),
        ) = (
            origin_x,
            origin_y,
            width,
            height,
            zoom_numerator,
            zoom_denominator,
        )
        else {
            unreachable!("all viewport fields were checked above");
        };
        if width == 0 || height == 0 {
            return Err(spec_text::error("viewport dimensions must be nonzero"));
        }
        let camera = LevelViewport::new(
            Point {
                x: origin_x,
                y: origin_y,
            },
            numerator,
            denominator,
        )
        .map_err(|error| spec_text::error(error.to_string()))?;
        Ok(Some(Self {
            camera,
            width,
            height,
        }))
    }
}

pub(crate) fn take_optional(fields: &mut Fields) -> Result<Option<ViewportSpec>, SpecError> {
    ViewportSpec::from_optional_parts(
        take_number(fields, "viewport-origin-x")?,
        take_number(fields, "viewport-origin-y")?,
        take_number(fields, "viewport-width")?,
        take_number(fields, "viewport-height")?,
        take_number(fields, "zoom-numerator")?,
        take_number(fields, "zoom-denominator")?,
    )
}

pub(crate) fn render(
    canvas: Canvas,
    viewport: Option<ViewportSpec>,
    overlays: Option<&Path>,
) -> Result<Canvas, Box<dyn std::error::Error>> {
    let mut canvas = if let Some(viewport) = viewport {
        lm_app::render_editor_viewport(&canvas, viewport.camera, viewport.width, viewport.height)?
    } else {
        canvas
    };
    if let Some(path) = overlays {
        let bytes =
            crate::read_bounded_bytes(path, EditorOverlayFile::MAX_FILE_LEN, "editor overlays")?;
        let file = EditorOverlayFile::decode(&bytes)?;
        draw_editor_overlays(&mut canvas, &file.overlays)?;
    }
    Ok(canvas)
}

fn take_number<T: std::str::FromStr>(
    fields: &mut Fields,
    key: &str,
) -> Result<Option<T>, SpecError> {
    fields
        .remove(key)
        .map(|value| {
            value.parse().map_err(|_| {
                spec_text::error(format!(
                    "portable specification has invalid decimal {key}: {value:?}"
                ))
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_or_complete_camera_is_accepted() {
        assert_eq!(
            ViewportSpec::from_optional_parts(None, None, None, None, None, None).unwrap(),
            None
        );
        let value = ViewportSpec::from_optional_parts(
            Some(-7),
            Some(9),
            Some(640),
            Some(448),
            Some(3),
            Some(2),
        )
        .unwrap()
        .unwrap();
        assert_eq!(value.camera.origin, Point { x: -7, y: 9 });
        assert_eq!(value.camera.zoom(), (3, 2));
        assert_eq!((value.width, value.height), (640, 448));
    }

    #[test]
    fn partial_zero_and_out_of_range_cameras_are_rejected() {
        assert!(ViewportSpec::from_optional_parts(Some(0), None, None, None, None, None).is_err());
        assert!(
            ViewportSpec::from_optional_parts(Some(0), Some(0), Some(0), Some(1), Some(1), Some(1))
                .is_err()
        );
        assert!(
            ViewportSpec::from_optional_parts(
                Some(0),
                Some(0),
                Some(1),
                Some(1),
                Some(51),
                Some(1)
            )
            .is_err()
        );
    }

    #[test]
    fn field_parser_consumes_camera_keys_before_unknown_validation() {
        let mut fields = spec_text::parse_fields(
            "V\nviewport-origin-x -1\nviewport-origin-y 2\nviewport-width 3\nviewport-height 4\nzoom-numerator 5\nzoom-denominator 2\n",
            "V",
        )
        .unwrap();
        let value = take_optional(&mut fields).unwrap().unwrap();
        assert_eq!(value.camera.origin, Point { x: -1, y: 2 });
        assert!(fields.is_empty());
    }

    #[test]
    fn absent_camera_preserves_the_owned_canvas() {
        let canvas = Canvas::try_new(2, 3).unwrap();
        assert_eq!(render(canvas.clone(), None, None).unwrap(), canvas);
    }
}
