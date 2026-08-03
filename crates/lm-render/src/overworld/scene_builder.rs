use super::{OverworldRenderError, SpriteAppearance, validate_layer};
use crate::{Scene, TileInstance};
use lm_level::{Map16Tile, Subtile};
use lm_overworld::{OverworldLayer, OverworldSprite};

/// Builds a painter-ordered overworld scene: layer 2, layer 1, then sprite appearances.
///
/// Missing Map16 definitions and missing sprite appearances are retained as model data but skipped
/// by this reference renderer. Each Map16 cell expands into its four 8x8 subtiles.
///
/// # Errors
///
/// Returns [`OverworldRenderError`] for malformed layer shapes or unrepresentable coordinates.
pub fn build_overworld_scene(
    layer1: &OverworldLayer,
    layer2: &OverworldLayer,
    map16: &[Map16Tile],
    sprites: &[OverworldSprite],
    appearances: &[SpriteAppearance],
) -> Result<Scene, OverworldRenderError> {
    validate_layer(1, layer1)?;
    validate_layer(2, layer2)?;
    let tile_capacity = layer1
        .tiles
        .len()
        .checked_add(layer2.tiles.len())
        .and_then(|count| count.checked_mul(4))
        .and_then(|count| count.checked_add(appearances.len()))
        .ok_or(OverworldRenderError::CoordinateOverflow)?;
    let mut scene = Scene {
        instances: Vec::with_capacity(tile_capacity),
    };
    append_layer(&mut scene, layer2, map16)?;
    append_layer(&mut scene, layer1, map16)?;
    for appearance in appearances {
        let Some(sprite) = sprites.get(appearance.sprite_index) else {
            continue;
        };
        let x = i32::from(sprite.x)
            .checked_add(appearance.x_offset)
            .ok_or(OverworldRenderError::CoordinateOverflow)?;
        let y = i32::from(sprite.y)
            .checked_add(appearance.y_offset)
            .ok_or(OverworldRenderError::CoordinateOverflow)?;
        scene.instances.push(TileInstance {
            tile_index: appearance.tile_index,
            palette_index: appearance.palette_index,
            x,
            y,
            x_flip: appearance.x_flip,
            y_flip: appearance.y_flip,
        });
    }
    Ok(scene)
}

/// Builds one painter-ordered overworld layer without fabricating another layer or sprites.
///
/// # Errors
///
/// Returns [`OverworldRenderError`] for a malformed layer or unrepresentable coordinates.
pub fn build_overworld_layer_scene(
    layer_number: u8,
    layer: &OverworldLayer,
    map16: &[Map16Tile],
) -> Result<Scene, OverworldRenderError> {
    validate_layer(layer_number, layer)?;
    let capacity = layer
        .tiles
        .len()
        .checked_mul(4)
        .ok_or(OverworldRenderError::CoordinateOverflow)?;
    let mut scene = Scene {
        instances: Vec::with_capacity(capacity),
    };
    append_layer(&mut scene, layer, map16)?;
    Ok(scene)
}

fn append_layer(
    scene: &mut Scene,
    layer: &OverworldLayer,
    map16: &[Map16Tile],
) -> Result<(), OverworldRenderError> {
    for high_priority in [false, true] {
        for (index, map16_index) in layer.tiles.iter().copied().enumerate() {
            let Some(definition) = map16.get(usize::from(map16_index)) else {
                continue;
            };
            let cell_x = index % layer.width;
            let cell_y = index / layer.width;
            let x = i32::try_from(
                cell_x
                    .checked_mul(16)
                    .ok_or(OverworldRenderError::CoordinateOverflow)?,
            )
            .map_err(|_| OverworldRenderError::CoordinateOverflow)?;
            let y = i32::try_from(
                cell_y
                    .checked_mul(16)
                    .ok_or(OverworldRenderError::CoordinateOverflow)?,
            )
            .map_err(|_| OverworldRenderError::CoordinateOverflow)?;
            for (subtile, dx, dy) in [
                (definition.top_left, 0, 0),
                (definition.top_right, 8, 0),
                (definition.bottom_left, 0, 8),
                (definition.bottom_right, 8, 8),
            ]
            .into_iter()
            .filter(|(subtile, _, _)| subtile.priority() == high_priority)
            {
                push_subtile(scene, subtile, x + dx, y + dy);
            }
        }
    }
    Ok(())
}

fn push_subtile(scene: &mut Scene, subtile: Subtile, x: i32, y: i32) {
    scene.instances.push(TileInstance {
        tile_index: usize::from(subtile.tile_number()),
        palette_index: usize::from(subtile.palette()),
        x,
        y,
        x_flip: subtile.x_flip(),
        y_flip: subtile.y_flip(),
    });
}
