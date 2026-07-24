use crate::{Canvas, Point, Rgba, Scene, Viewport, ViewportError};
use lm_graphics::{IndexedTile, Palette};

/// Renders a painter-ordered scene through an exact viewport transform.
///
/// This is the deterministic reference path for zoomed editor views. It samples the world with
/// nearest-neighbor semantics and treats palette index zero as transparent.
///
/// # Errors
///
/// Returns [`ViewportError`] if a screen coordinate cannot be transformed into world space.
pub fn draw_scene_viewport(
    canvas: &mut Canvas,
    viewport: Viewport,
    scene: &Scene,
    tiles: &[IndexedTile],
    palettes: &[Palette],
) -> Result<(), ViewportError> {
    let width = canvas.width().min(viewport.width as usize);
    let height = canvas.height().min(viewport.height as usize);
    for screen_y in 0..height {
        for screen_x in 0..width {
            let world = viewport.screen_to_world(Point {
                x: i64::try_from(screen_x).map_err(|_| ViewportError::CoordinateOverflow)?,
                y: i64::try_from(screen_y).map_err(|_| ViewportError::CoordinateOverflow)?,
            })?;
            if let Some(color) = sample_scene(world, scene, tiles, palettes) {
                canvas.set(screen_x, screen_y, color);
            }
        }
    }
    Ok(())
}

fn sample_scene(
    world: Point,
    scene: &Scene,
    tiles: &[IndexedTile],
    palettes: &[Palette],
) -> Option<Rgba> {
    for instance in scene.instances.iter().rev() {
        let (Some(local_x), Some(local_y)) = (
            world.x.checked_sub(i64::from(instance.x)),
            world.y.checked_sub(i64::from(instance.y)),
        ) else {
            continue;
        };
        if !(0..8).contains(&local_x) || !(0..8).contains(&local_y) {
            continue;
        }
        let (Some(tile), Some(palette)) = (
            tiles.get(instance.tile_index),
            palettes.get(instance.palette_index),
        ) else {
            continue;
        };
        let mut x = usize::try_from(local_x).ok()?;
        let mut y = usize::try_from(local_y).ok()?;
        if instance.x_flip {
            x = IndexedTile::WIDTH - 1 - x;
        }
        if instance.y_flip {
            y = IndexedTile::HEIGHT - 1 - y;
        }
        let index = usize::from(tile.pixels()[y * IndexedTile::WIDTH + x]);
        if index == 0 {
            continue;
        }
        let Some(color) = palette.colors.get(index) else {
            continue;
        };
        let rgb = color.to_rgb8();
        return Some(Rgba {
            red: rgb.red,
            green: rgb.green,
            blue: rgb.blue,
            alpha: 255,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TileInstance;
    use lm_graphics::Bgr555;

    fn assets() -> ([IndexedTile; 1], [Palette; 1]) {
        let mut pixels = [0; IndexedTile::PIXEL_COUNT];
        pixels[0] = 1;
        (
            [IndexedTile::new(pixels)],
            [Palette {
                colors: vec![Bgr555(0), Bgr555(0x001f)],
            }],
        )
    }

    #[test]
    fn integer_zoom_repeats_source_pixels_exactly() {
        let (tiles, palettes) = assets();
        let scene = Scene {
            instances: vec![TileInstance {
                tile_index: 0,
                palette_index: 0,
                x: 0,
                y: 0,
                x_flip: false,
                y_flip: false,
            }],
        };
        let viewport = Viewport::new(Point::default(), 16, 16, 2, 1).unwrap();
        let mut canvas = Canvas::try_new(16, 16).unwrap();
        draw_scene_viewport(&mut canvas, viewport, &scene, &tiles, &palettes).unwrap();
        let red = Rgba {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        assert_eq!(canvas.get(0, 0), Some(red));
        assert_eq!(canvas.get(1, 0), Some(red));
        assert_eq!(canvas.get(0, 1), Some(red));
        assert_eq!(canvas.get(2, 0), Some(Rgba::default()));
    }

    #[test]
    fn viewport_origin_pans_world_under_screen() {
        let (tiles, palettes) = assets();
        let scene = Scene {
            instances: vec![TileInstance {
                tile_index: 0,
                palette_index: 0,
                x: -4,
                y: 9,
                x_flip: false,
                y_flip: false,
            }],
        };
        let viewport = Viewport::new(Point { x: -4, y: 9 }, 1, 1, 1, 1).unwrap();
        let mut canvas = Canvas::try_new(1, 1).unwrap();
        draw_scene_viewport(&mut canvas, viewport, &scene, &tiles, &palettes).unwrap();
        assert_eq!(canvas.get(0, 0).unwrap().alpha, 255);
    }

    #[test]
    fn extreme_world_instance_subtraction_does_not_overflow() {
        let (tiles, palettes) = assets();
        let scene = Scene {
            instances: vec![TileInstance {
                tile_index: 0,
                palette_index: 0,
                x: i32::MAX,
                y: i32::MIN,
                x_flip: false,
                y_flip: false,
            }],
        };
        assert_eq!(
            sample_scene(
                Point {
                    x: i64::MIN,
                    y: i64::MAX,
                },
                &scene,
                &tiles,
                &palettes,
            ),
            None
        );
    }
}
