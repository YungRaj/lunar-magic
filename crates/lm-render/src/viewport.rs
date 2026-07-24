use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldRect {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Viewport {
    pub origin: Point,
    pub width: u32,
    pub height: u32,
    zoom_numerator: u32,
    zoom_denominator: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewportError {
    ZeroZoom,
    CoordinateOverflow,
}

impl fmt::Display for ViewportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid viewport operation: {self:?}")
    }
}

impl std::error::Error for ViewportError {}

impl Viewport {
    /// Constructs a viewport using an exact rational zoom factor.
    ///
    /// # Errors
    ///
    /// Returns [`ViewportError::ZeroZoom`] when either zoom component is zero.
    pub const fn new(
        origin: Point,
        width: u32,
        height: u32,
        zoom_numerator: u32,
        zoom_denominator: u32,
    ) -> Result<Self, ViewportError> {
        if zoom_numerator == 0 || zoom_denominator == 0 {
            return Err(ViewportError::ZeroZoom);
        }
        Ok(Self {
            origin,
            width,
            height,
            zoom_numerator,
            zoom_denominator,
        })
    }

    #[must_use]
    pub const fn zoom(self) -> (u32, u32) {
        (self.zoom_numerator, self.zoom_denominator)
    }

    /// Converts a world coordinate into a signed screen-pixel coordinate.
    ///
    /// # Errors
    ///
    /// Returns [`ViewportError::CoordinateOverflow`] when intermediate or output math overflows.
    pub fn world_to_screen(self, world: Point) -> Result<Point, ViewportError> {
        Ok(Point {
            x: scale(
                world
                    .x
                    .checked_sub(self.origin.x)
                    .ok_or(ViewportError::CoordinateOverflow)?,
                self.zoom_numerator,
                self.zoom_denominator,
            )?,
            y: scale(
                world
                    .y
                    .checked_sub(self.origin.y)
                    .ok_or(ViewportError::CoordinateOverflow)?,
                self.zoom_numerator,
                self.zoom_denominator,
            )?,
        })
    }

    /// Converts a screen pixel to the world coordinate containing that pixel.
    ///
    /// # Errors
    ///
    /// Returns [`ViewportError::CoordinateOverflow`] when intermediate or output math overflows.
    pub fn screen_to_world(self, screen: Point) -> Result<Point, ViewportError> {
        Ok(Point {
            x: self
                .origin
                .x
                .checked_add(scale(screen.x, self.zoom_denominator, self.zoom_numerator)?)
                .ok_or(ViewportError::CoordinateOverflow)?,
            y: self
                .origin
                .y
                .checked_add(scale(screen.y, self.zoom_denominator, self.zoom_numerator)?)
                .ok_or(ViewportError::CoordinateOverflow)?,
        })
    }

    /// Returns the half-open world rectangle touched by the viewport.
    ///
    /// # Errors
    ///
    /// Returns [`ViewportError::CoordinateOverflow`] for unrepresentable bounds.
    pub fn visible_world(self) -> Result<WorldRect, ViewportError> {
        let world_width = visible_span(self.width, self.zoom_denominator, self.zoom_numerator)?;
        let world_height = visible_span(self.height, self.zoom_denominator, self.zoom_numerator)?;
        Ok(WorldRect {
            left: self.origin.x,
            top: self.origin.y,
            right: self
                .origin
                .x
                .checked_add(world_width)
                .ok_or(ViewportError::CoordinateOverflow)?,
            bottom: self
                .origin
                .y
                .checked_add(world_height)
                .ok_or(ViewportError::CoordinateOverflow)?,
        })
    }

    /// Pans by an exact world-coordinate delta.
    ///
    /// # Errors
    ///
    /// Returns [`ViewportError::CoordinateOverflow`] when the new origin is unrepresentable.
    pub fn pan(&mut self, delta: Point) -> Result<(), ViewportError> {
        let origin = Point {
            x: self
                .origin
                .x
                .checked_add(delta.x)
                .ok_or(ViewportError::CoordinateOverflow)?,
            y: self
                .origin
                .y
                .checked_add(delta.y)
                .ok_or(ViewportError::CoordinateOverflow)?,
        };
        self.origin = origin;
        Ok(())
    }

    /// Changes zoom while keeping the world point below `screen_anchor` stationary.
    ///
    /// # Errors
    ///
    /// Returns [`ViewportError`] for zero zoom or coordinate overflow.
    pub fn zoom_at(
        &mut self,
        screen_anchor: Point,
        numerator: u32,
        denominator: u32,
    ) -> Result<(), ViewportError> {
        let world_anchor = self.screen_to_world(screen_anchor)?;
        let mut changed = Self::new(self.origin, self.width, self.height, numerator, denominator)?;
        let changed_anchor = changed.screen_to_world(screen_anchor)?;
        changed.pan(Point {
            x: world_anchor
                .x
                .checked_sub(changed_anchor.x)
                .ok_or(ViewportError::CoordinateOverflow)?,
            y: world_anchor
                .y
                .checked_sub(changed_anchor.y)
                .ok_or(ViewportError::CoordinateOverflow)?,
        })?;
        *self = changed;
        Ok(())
    }
}

fn visible_span(
    screen_pixels: u32,
    zoom_denominator: u32,
    zoom_numerator: u32,
) -> Result<i64, ViewportError> {
    let scaled = u128::from(screen_pixels) * u128::from(zoom_denominator);
    let rounded = scaled.div_ceil(u128::from(zoom_numerator));
    i64::try_from(rounded).map_err(|_| ViewportError::CoordinateOverflow)
}

fn scale(value: i64, numerator: u32, denominator: u32) -> Result<i64, ViewportError> {
    let scaled = i128::from(value) * i128::from(numerator);
    let quotient = scaled.div_euclid(i128::from(denominator));
    i64::try_from(quotient).map_err(|_| ViewportError::CoordinateOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_coordinates_use_euclidean_pixel_buckets() {
        let viewport = Viewport::new(Point { x: 10, y: -4 }, 320, 224, 3, 2).unwrap();
        assert_eq!(
            viewport.world_to_screen(Point { x: 9, y: -5 }).unwrap(),
            Point { x: -2, y: -2 }
        );
        assert_eq!(
            viewport.screen_to_world(Point { x: -2, y: -2 }).unwrap(),
            Point { x: 8, y: -6 }
        );
    }

    #[test]
    fn zoom_anchor_stays_on_the_same_world_coordinate() {
        let anchor = Point { x: 160, y: 112 };
        let mut viewport = Viewport::new(Point { x: -50, y: 20 }, 320, 224, 1, 1).unwrap();
        let before = viewport.screen_to_world(anchor).unwrap();
        viewport.zoom_at(anchor, 4, 1).unwrap();
        assert_eq!(viewport.screen_to_world(anchor).unwrap(), before);
        assert_eq!(viewport.zoom(), (4, 1));
    }

    #[test]
    fn invalid_zoom_and_overflow_do_not_mutate_viewport() {
        assert!(matches!(
            Viewport::new(Point::default(), 1, 1, 0, 1),
            Err(ViewportError::ZeroZoom)
        ));
        let mut viewport = Viewport::new(Point { x: i64::MAX, y: 0 }, 1, 1, 1, 1).unwrap();
        let original = viewport;
        assert!(viewport.pan(Point { x: 1, y: 0 }).is_err());
        assert_eq!(viewport, original);
    }

    #[test]
    fn wide_intermediates_accept_representable_extreme_results() {
        for value in [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX] {
            assert_eq!(scale(value, 2, 2).unwrap(), value);
            assert_eq!(scale(value, u32::MAX, u32::MAX).unwrap(), value);
        }
        assert_eq!(
            scale(i64::MAX, 2, 1),
            Err(ViewportError::CoordinateOverflow)
        );
        assert_eq!(
            scale(i64::MIN, 2, 1),
            Err(ViewportError::CoordinateOverflow)
        );
        assert_eq!(
            visible_span(u32::MAX, u32::MAX, u32::MAX).unwrap(),
            i64::from(u32::MAX)
        );
        assert_eq!(
            visible_span(u32::MAX, u32::MAX, 1),
            Err(ViewportError::CoordinateOverflow)
        );
    }

    #[test]
    fn visible_bounds_follow_pan_and_zoom() {
        let viewport = Viewport::new(Point { x: 100, y: 200 }, 320, 224, 2, 1).unwrap();
        assert_eq!(
            viewport.visible_world().unwrap(),
            WorldRect {
                left: 100,
                top: 200,
                right: 260,
                bottom: 312,
            }
        );
    }

    #[test]
    fn visible_bounds_round_fractional_world_spans_outward() {
        let zoomed_in = Viewport::new(Point { x: -5, y: 7 }, 1, 1, 2, 1).unwrap();
        assert_eq!(
            zoomed_in.visible_world().unwrap(),
            WorldRect {
                left: -5,
                top: 7,
                right: -4,
                bottom: 8,
            }
        );

        let fractional = Viewport::new(Point::default(), 5, 7, 3, 2).unwrap();
        assert_eq!(
            fractional.visible_world().unwrap(),
            WorldRect {
                left: 0,
                top: 0,
                right: 4,
                bottom: 5,
            }
        );
        assert_eq!(
            Viewport::new(Point::default(), 0, 0, 7, 3)
                .unwrap()
                .visible_world()
                .unwrap(),
            WorldRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0
            }
        );
    }

    #[test]
    fn visible_bounds_overflow_is_typed() {
        let viewport = Viewport::new(Point { x: i64::MAX, y: 0 }, 1, 1, 2, 1).unwrap();
        assert_eq!(
            viewport.visible_world(),
            Err(ViewportError::CoordinateOverflow)
        );
        let enormous_ratio = Viewport::new(Point::default(), u32::MAX, 1, 1, u32::MAX).unwrap();
        assert_eq!(
            enormous_ratio.visible_world(),
            Err(ViewportError::CoordinateOverflow)
        );
    }
}
