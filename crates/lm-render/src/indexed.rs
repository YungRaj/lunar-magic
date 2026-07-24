use crate::{Canvas, Rgba};
use lm_graphics::{IndexedTile, Palette};

pub fn draw_indexed_tile(
    canvas: &mut Canvas,
    tile: &IndexedTile,
    palette: &Palette,
    x: usize,
    y: usize,
) {
    for row in 0..8 {
        for column in 0..8 {
            let index = usize::from(tile.pixels()[row * 8 + column]);
            if index == 0 {
                continue;
            }
            if let Some(color) = palette.colors.get(index) {
                let rgb = color.to_rgb8();
                let (Some(target_x), Some(target_y)) = (x.checked_add(column), y.checked_add(row))
                else {
                    continue;
                };
                canvas.set(
                    target_x,
                    target_y,
                    Rgba {
                        red: rgb.red,
                        green: rgb.green,
                        blue: rgb.blue,
                        alpha: 255,
                    },
                );
            }
        }
    }
}

pub fn draw_indexed_tile_clipped(
    canvas: &mut Canvas,
    tile: &IndexedTile,
    palette: &Palette,
    x: i32,
    y: i32,
) {
    for row in 0..IndexedTile::HEIGHT {
        for column in 0..IndexedTile::WIDTH {
            let index = usize::from(tile.pixels()[row * IndexedTile::WIDTH + column]);
            if index == 0 {
                continue;
            }
            let Some(target_x) = x.checked_add(i32::try_from(column).unwrap_or(i32::MAX)) else {
                continue;
            };
            let Some(target_y) = y.checked_add(i32::try_from(row).unwrap_or(i32::MAX)) else {
                continue;
            };
            let (Ok(target_x), Ok(target_y)) =
                (usize::try_from(target_x), usize::try_from(target_y))
            else {
                continue;
            };
            if let Some(color) = palette.colors.get(index) {
                let rgb = color.to_rgb8();
                canvas.set(
                    target_x,
                    target_y,
                    Rgba {
                        red: rgb.red,
                        green: rgb.green,
                        blue: rgb.blue,
                        alpha: 255,
                    },
                );
            }
        }
    }
}

pub(crate) fn draw_indexed_tile_clipped_average(
    canvas: &mut Canvas,
    tile: &IndexedTile,
    palette: &Palette,
    x: i32,
    y: i32,
) {
    for row in 0..IndexedTile::HEIGHT {
        for column in 0..IndexedTile::WIDTH {
            let index = usize::from(tile.pixels()[row * IndexedTile::WIDTH + column]);
            if index == 0 {
                continue;
            }
            let (Some(target_x), Some(target_y)) = (
                x.checked_add(i32::try_from(column).unwrap_or(i32::MAX)),
                y.checked_add(i32::try_from(row).unwrap_or(i32::MAX)),
            ) else {
                continue;
            };
            let (Ok(target_x), Ok(target_y)) =
                (usize::try_from(target_x), usize::try_from(target_y))
            else {
                continue;
            };
            let (Some(color), Some(existing)) =
                (palette.colors.get(index), canvas.get(target_x, target_y))
            else {
                continue;
            };
            let rgb = color.to_rgb8();
            canvas.set(
                target_x,
                target_y,
                Rgba {
                    red: (rgb.red & 0xfe) / 2 + (existing.red & 0xfe) / 2,
                    green: (rgb.green & 0xfe) / 2 + (existing.green & 0xfe) / 2,
                    blue: (rgb.blue & 0xfe) / 2 + (existing.blue & 0xfe) / 2,
                    alpha: 255,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::Bgr555;

    #[test]
    fn color_zero_is_transparent_and_others_are_opaque() {
        let mut pixels = [0; IndexedTile::PIXEL_COUNT];
        pixels[1] = 1;
        let tile = IndexedTile::new(pixels);
        let palette = Palette {
            colors: vec![Bgr555(0), Bgr555(0x001f)],
        };
        let mut canvas = Canvas::try_new(8, 8).unwrap();
        draw_indexed_tile(&mut canvas, &tile, &palette, 0, 0);
        assert_eq!(canvas.get(0, 0), Some(Rgba::default()));
        assert_eq!(
            canvas.get(1, 0),
            Some(Rgba {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255,
            })
        );
    }

    #[test]
    fn unsigned_draw_origin_overflow_is_clipped_without_panicking() {
        let tile = IndexedTile::new([1; IndexedTile::PIXEL_COUNT]);
        let palette = Palette {
            colors: vec![Bgr555(0), Bgr555(0x001f)],
        };
        let mut canvas = Canvas::try_new(1, 1).unwrap();
        draw_indexed_tile(&mut canvas, &tile, &palette, usize::MAX, usize::MAX);
        assert_eq!(canvas.get(0, 0), Some(Rgba::default()));
    }

    #[test]
    fn signed_clipping_matches_a_wide_integer_reference_model() {
        let tile = IndexedTile::new(std::array::from_fn(|index| {
            u8::try_from(index % 16).unwrap()
        }));
        let palette = Palette {
            colors: (0_u16..16).map(Bgr555).collect(),
        };
        let origins = (-10..=6)
            .flat_map(|x| (-10..=6).map(move |y| (x, y)))
            .chain([
                (i32::MIN, i32::MIN),
                (i32::MIN, i32::MAX),
                (i32::MAX, i32::MIN),
                (i32::MAX, i32::MAX),
            ]);
        for (origin_x, origin_y) in origins {
            let mut actual = Canvas::try_new(5, 5).unwrap();
            draw_indexed_tile_clipped(&mut actual, &tile, &palette, origin_x, origin_y);

            let mut expected = Canvas::try_new(5, 5).unwrap();
            for row in 0..IndexedTile::HEIGHT {
                for column in 0..IndexedTile::WIDTH {
                    let index = usize::from(tile.pixels()[row * IndexedTile::WIDTH + column]);
                    if index == 0 {
                        continue;
                    }
                    let x = i64::from(origin_x) + i64::try_from(column).unwrap();
                    let y = i64::from(origin_y) + i64::try_from(row).unwrap();
                    if !(0..5).contains(&x) || !(0..5).contains(&y) {
                        continue;
                    }
                    let rgb = palette.colors[index].to_rgb8();
                    expected.set(
                        usize::try_from(x).unwrap(),
                        usize::try_from(y).unwrap(),
                        Rgba {
                            red: rgb.red,
                            green: rgb.green,
                            blue: rgb.blue,
                            alpha: 255,
                        },
                    );
                }
            }
            assert_eq!(actual, expected, "origin ({origin_x}, {origin_y})");
        }
    }
}
