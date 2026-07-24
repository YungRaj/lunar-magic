#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rgba {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<Rgba>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanvasError {
    DimensionOverflow,
    TooManyPixels(usize),
    WrongPixelCount { expected: usize, actual: usize },
}

impl std::fmt::Display for CanvasError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid render canvas: {self:?}")
    }
}

impl std::error::Error for CanvasError {}

impl Canvas {
    pub const MAX_PIXELS: usize = 0x400_0000;

    /// Constructs a bounded canvas without permitting dimension overflow or unbounded allocation.
    ///
    /// # Errors
    ///
    /// Returns [`CanvasError`] when the dimensions overflow or exceed [`Self::MAX_PIXELS`].
    pub fn try_new(width: usize, height: usize) -> Result<Self, CanvasError> {
        let count = checked_pixel_count(width, height)?;
        Ok(Self {
            width,
            height,
            pixels: vec![Rgba::default(); count],
        })
    }

    /// Constructs a canvas from an exact row-major RGBA raster.
    ///
    /// # Errors
    ///
    /// Returns [`CanvasError`] for invalid dimensions or a mismatched pixel count.
    pub fn from_pixels(
        width: usize,
        height: usize,
        pixels: Vec<Rgba>,
    ) -> Result<Self, CanvasError> {
        let expected = checked_pixel_count(width, height)?;
        if pixels.len() != expected {
            return Err(CanvasError::WrongPixelCount {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    pub fn set(&mut self, x: usize, y: usize, color: Rgba) {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x] = color;
        }
    }

    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> Option<Rgba> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels
            .get(y.checked_mul(self.width)?.checked_add(x)?)
            .copied()
    }

    #[must_use]
    pub fn pixels(&self) -> &[Rgba] {
        &self.pixels
    }
}

fn checked_pixel_count(width: usize, height: usize) -> Result<usize, CanvasError> {
    let count = width
        .checked_mul(height)
        .ok_or(CanvasError::DimensionOverflow)?;
    if count > Canvas::MAX_PIXELS {
        Err(CanvasError::TooManyPixels(count))
    } else {
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates_do_not_wrap_into_another_row() {
        let mut canvas = Canvas::try_new(2, 2).unwrap();
        let color = Rgba {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 4,
        };
        canvas.set(1, 1, color);
        canvas.set(2, 0, Rgba { red: 9, ..color });
        assert_eq!(canvas.get(1, 1), Some(color));
        assert_eq!(canvas.get(2, 0), None);
        assert_eq!(canvas.get(0, 2), None);
    }

    #[test]
    fn fallible_construction_rejects_overflow_limits_and_wrong_rasters() {
        assert_eq!(
            Canvas::try_new(usize::MAX, 2),
            Err(CanvasError::DimensionOverflow)
        );
        assert!(matches!(
            Canvas::try_new(Canvas::MAX_PIXELS + 1, 1),
            Err(CanvasError::TooManyPixels(_))
        ));
        assert_eq!(
            Canvas::from_pixels(2, 2, vec![Rgba::default(); 3]),
            Err(CanvasError::WrongPixelCount {
                expected: 4,
                actual: 3
            })
        );
    }
}
