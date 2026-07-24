use crate::{Canvas, draw_indexed_tile};
use lm_graphics::{IndexedTile, Palette};
use lm_level::Map16Tile;

pub fn draw_map16_tile(
    canvas: &mut Canvas,
    definition: Map16Tile,
    tiles: &[IndexedTile],
    palettes: &[Palette],
    x: usize,
    y: usize,
) {
    for (subtile, dx, dy) in [
        (definition.top_left, 0, 0),
        (definition.top_right, 8, 0),
        (definition.bottom_left, 0, 8),
        (definition.bottom_right, 8, 8),
    ] {
        let Some(tile) = tiles.get(usize::from(subtile.tile_number())) else {
            continue;
        };
        let Some(palette) = palettes.get(usize::from(subtile.palette())) else {
            continue;
        };
        let (Some(target_x), Some(target_y)) = (x.checked_add(dx), y.checked_add(dy)) else {
            continue;
        };
        let flipped = tile.flipped(subtile.x_flip(), subtile.y_flip());
        draw_indexed_tile(canvas, &flipped, palette, target_x, target_y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rgba;
    use lm_graphics::Bgr555;
    use lm_level::Subtile;

    #[test]
    fn composes_quadrants_and_honors_flip_bits() {
        let mut pixels = [0; IndexedTile::PIXEL_COUNT];
        pixels[0] = 1;
        let tiles = [IndexedTile::new(pixels)];
        let palettes = [Palette {
            colors: vec![Bgr555(0), Bgr555(0x7c00)],
        }];
        let definition = Map16Tile {
            top_left: Subtile(0),
            top_right: Subtile(0x4000),
            bottom_left: Subtile(0x8000),
            bottom_right: Subtile(0xc000),
            acts_like: 0,
        };
        let mut canvas = Canvas::try_new(16, 16).unwrap();
        draw_map16_tile(&mut canvas, definition, &tiles, &palettes, 0, 0);
        let blue = Rgba {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        };
        assert_eq!(canvas.get(0, 0), Some(blue));
        assert_eq!(canvas.get(15, 0), Some(blue));
        assert_eq!(canvas.get(0, 15), Some(blue));
        assert_eq!(canvas.get(15, 15), Some(blue));
    }

    #[test]
    fn quadrant_origin_overflow_is_clipped_without_panicking() {
        let tiles = [IndexedTile::new([1; IndexedTile::PIXEL_COUNT])];
        let palettes = [Palette {
            colors: vec![Bgr555(0), Bgr555(0x001f)],
        }];
        let definition = Map16Tile {
            top_left: Subtile(0),
            top_right: Subtile(0),
            bottom_left: Subtile(0),
            bottom_right: Subtile(0),
            acts_like: 0,
        };
        let mut canvas = Canvas::try_new(1, 1).unwrap();
        draw_map16_tile(
            &mut canvas,
            definition,
            &tiles,
            &palettes,
            usize::MAX,
            usize::MAX,
        );
        assert_eq!(canvas.get(0, 0), Some(Rgba::default()));
    }
}
