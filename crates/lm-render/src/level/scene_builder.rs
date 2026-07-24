use super::{EntityAppearance, EntitySource, GridPlacement, LevelRenderError, LevelSceneLayout};
use crate::{Layer3Placement, MaterializedLayer3Plane, Scene, TileInstance};
use lm_level::{LayerData, Level, Map16Tile, Subtile};

/// Builds a painter-ordered level scene from decoded tilemaps and definition-rendered entities.
///
/// Layer 2 is emitted first, then Layer 1, followed by valid entity appearances in caller order.
/// Missing Map16 definitions and appearances referring to absent model records are skipped. This
/// lets editor plugins provide previews without making the reference renderer interpret custom
/// object or sprite formats.
///
/// # Errors
///
/// Returns [`LevelRenderError`] for mismatched layer dimensions or coordinate overflow.
pub fn build_level_scene(
    level: &Level,
    layout: LevelSceneLayout,
    map16: &[Map16Tile],
    appearances: &[EntityAppearance],
) -> Result<Scene, LevelRenderError> {
    build_level_scene_with_layer3(level, layout, map16, appearances, None)
}

/// Builds a level scene with an optional provider-resolved Layer 3 plane.
///
/// The plane's explicit placement determines which painter boundary receives its instances. The
/// caller remains responsible for verifying its source digest against the exact Layer 3 model.
///
/// # Errors
///
/// Returns [`LevelRenderError`] for mismatched layer dimensions, coordinate overflow, or a plane
/// supplied for a level without Layer 3 semantic state.
pub fn build_level_scene_with_layer3(
    level: &Level,
    layout: LevelSceneLayout,
    map16: &[Map16Tile],
    appearances: &[EntityAppearance],
    layer3: Option<&MaterializedLayer3Plane>,
) -> Result<Scene, LevelRenderError> {
    validate_layer(1, &level.layer1, layout.layer1)?;
    validate_layer(2, &level.layer2, layout.layer2)?;
    if layer3.is_some() && level.layer3.is_none() {
        return Err(LevelRenderError::Layer3StateMissing);
    }
    let capacity = level
        .layer1
        .raw_tilemap
        .len()
        .checked_add(level.layer2.raw_tilemap.len())
        .and_then(|tiles| tiles.checked_mul(4))
        .and_then(|tiles| tiles.checked_add(appearances.len()))
        .and_then(|tiles| tiles.checked_add(layer3.map_or(0, |plane| plane.instances.len())))
        .ok_or(LevelRenderError::CoordinateOverflow)?;
    let mut scene = Scene {
        instances: Vec::with_capacity(capacity),
    };
    append_layer3(&mut scene, layer3, Layer3Placement::BehindLayer2);
    append_layer(&mut scene, &level.layer2, layout.layer2, map16)?;
    append_layer3(&mut scene, layer3, Layer3Placement::BetweenLayer2AndLayer1);
    append_layer(&mut scene, &level.layer1, layout.layer1, map16)?;
    append_layer3(&mut scene, layer3, Layer3Placement::AboveLayer1);
    scene.instances.extend(
        appearances
            .iter()
            .filter(|appearance| source_exists(level, appearance.source))
            .map(|appearance| TileInstance {
                tile_index: appearance.tile_index,
                palette_index: appearance.palette_index,
                x: appearance.x,
                y: appearance.y,
                x_flip: appearance.x_flip,
                y_flip: appearance.y_flip,
            }),
    );
    append_layer3(&mut scene, layer3, Layer3Placement::AboveEntities);
    Ok(scene)
}

fn append_layer3(
    scene: &mut Scene,
    layer3: Option<&MaterializedLayer3Plane>,
    placement: Layer3Placement,
) {
    if let Some(layer3) = layer3.filter(|plane| plane.placement == placement) {
        scene.instances.extend_from_slice(&layer3.instances);
    }
}

fn validate_layer(
    number: u8,
    layer: &LayerData,
    placement: GridPlacement,
) -> Result<(), LevelRenderError> {
    if placement.width.checked_mul(placement.height) != Some(layer.raw_tilemap.len()) {
        return Err(LevelRenderError::InvalidLayerShape {
            layer: number,
            width: placement.width,
            height: placement.height,
            tiles: layer.raw_tilemap.len(),
        });
    }
    Ok(())
}

fn append_layer(
    scene: &mut Scene,
    layer: &LayerData,
    placement: GridPlacement,
    map16: &[Map16Tile],
) -> Result<(), LevelRenderError> {
    for high_priority in [false, true] {
        for (index, definition_index) in layer.raw_tilemap.iter().copied().enumerate() {
            let Some(definition) = map16.get(usize::from(definition_index)) else {
                continue;
            };
            let column = index % placement.width;
            let row = index / placement.width;
            let x = cell_coordinate(column, placement.origin_x)?;
            let y = cell_coordinate(row, placement.origin_y)?;
            for (subtile, dx, dy) in [
                (definition.top_left, 0, 0),
                (definition.top_right, 8, 0),
                (definition.bottom_left, 0, 8),
                (definition.bottom_right, 8, 8),
            ]
            .into_iter()
            .filter(|(subtile, _, _)| subtile.priority() == high_priority)
            {
                scene.instances.push(subtile_instance(
                    subtile,
                    x.checked_add(dx)
                        .ok_or(LevelRenderError::CoordinateOverflow)?,
                    y.checked_add(dy)
                        .ok_or(LevelRenderError::CoordinateOverflow)?,
                ));
            }
        }
    }
    Ok(())
}

fn cell_coordinate(cell: usize, origin: i32) -> Result<i32, LevelRenderError> {
    i32::try_from(
        cell.checked_mul(16)
            .ok_or(LevelRenderError::CoordinateOverflow)?,
    )
    .map_err(|_| LevelRenderError::CoordinateOverflow)?
    .checked_add(origin)
    .ok_or(LevelRenderError::CoordinateOverflow)
}

fn subtile_instance(subtile: Subtile, x: i32, y: i32) -> TileInstance {
    TileInstance {
        tile_index: usize::from(subtile.tile_number()),
        palette_index: usize::from(subtile.palette()),
        x,
        y,
        x_flip: subtile.x_flip(),
        y_flip: subtile.y_flip(),
    }
}

fn source_exists(level: &Level, source: EntitySource) -> bool {
    match source {
        EntitySource::Layer1Object(index) => index < level.layer1.objects.records.len(),
        EntitySource::Layer2Object(index) => index < level.layer2.objects.records.len(),
        EntitySource::Sprite(index) => index < level.sprites.records.len(),
    }
}
