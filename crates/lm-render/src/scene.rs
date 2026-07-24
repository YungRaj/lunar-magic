use crate::indexed::draw_indexed_tile_clipped_average;
use crate::{Canvas, draw_indexed_tile_clipped};
use lm_graphics::{IndexedTile, Palette};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileInstance {
    pub tile_index: usize,
    pub palette_index: usize,
    pub x: i32,
    pub y: i32,
    pub x_flip: bool,
    pub y_flip: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Scene {
    /// Instances are painter-ordered; later entries appear above earlier entries.
    pub instances: Vec<TileInstance>,
}

/// Renders an immutable tile scene using the software reference backend.
pub fn draw_scene(canvas: &mut Canvas, scene: &Scene, tiles: &[IndexedTile], palettes: &[Palette]) {
    for instance in &scene.instances {
        let (Some(tile), Some(palette)) = (
            tiles.get(instance.tile_index),
            palettes.get(instance.palette_index),
        ) else {
            continue;
        };
        let tile = tile.flipped(instance.x_flip, instance.y_flip);
        draw_indexed_tile_clipped(canvas, &tile, palette, instance.x, instance.y);
    }
}

pub(crate) fn draw_scene_with_average(
    canvas: &mut Canvas,
    scene: &Scene,
    average: &[bool],
    tiles: &[IndexedTile],
    palettes: &[Palette],
) {
    for (instance, average) in scene.instances.iter().zip(average.iter().copied()) {
        let (Some(tile), Some(palette)) = (
            tiles.get(instance.tile_index),
            palettes.get(instance.palette_index),
        ) else {
            continue;
        };
        let tile = tile.flipped(instance.x_flip, instance.y_flip);
        if average {
            draw_indexed_tile_clipped_average(canvas, &tile, palette, instance.x, instance.y);
        } else {
            draw_indexed_tile_clipped(canvas, &tile, palette, instance.x, instance.y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rgba;
    use lm_graphics::Bgr555;

    #[test]
    fn signed_positions_clip_and_later_instances_win() {
        let tiles = [IndexedTile::new([1; IndexedTile::PIXEL_COUNT])];
        let palettes = [
            Palette {
                colors: vec![Bgr555(0), Bgr555(0x001f)],
            },
            Palette {
                colors: vec![Bgr555(0), Bgr555(0x03e0)],
            },
        ];
        let scene = Scene {
            instances: vec![
                TileInstance {
                    tile_index: 0,
                    palette_index: 0,
                    x: -7,
                    y: -7,
                    x_flip: false,
                    y_flip: false,
                },
                TileInstance {
                    tile_index: 0,
                    palette_index: 1,
                    x: 0,
                    y: 0,
                    x_flip: false,
                    y_flip: false,
                },
            ],
        };
        let mut canvas = Canvas::try_new(8, 8).unwrap();
        draw_scene(&mut canvas, &scene, &tiles, &palettes);
        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 0,
                green: 255,
                blue: 0,
                alpha: 255,
            })
        );
    }

    #[test]
    fn missing_assets_are_safely_ignored() {
        let scene = Scene {
            instances: vec![TileInstance {
                tile_index: 99,
                palette_index: 99,
                x: 0,
                y: 0,
                x_flip: false,
                y_flip: false,
            }],
        };
        let mut canvas = Canvas::try_new(1, 1).unwrap();
        draw_scene(&mut canvas, &scene, &[], &[]);
        assert_eq!(canvas.get(0, 0), Some(Rgba::default()));
    }
}
