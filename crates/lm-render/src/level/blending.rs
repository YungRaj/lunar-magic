use super::{
    EntityAppearance, EntitySource, LevelRenderError, LevelSceneLayout,
    build_level_scene_with_layer3,
};
use crate::{Layer3Placement, MaterializedLayer3Plane, Scene};
use lm_level::{LayerData, Level, Map16Tile};

pub(crate) struct BlendedLevelScene {
    pub scene: Scene,
    pub average: Vec<bool>,
}

pub(crate) fn build_level_scene_with_cell_blends(
    level: &Level,
    layout: LevelSceneLayout,
    map16: &[Map16Tile],
    appearances: &[EntityAppearance],
    layer3: Option<&MaterializedLayer3Plane>,
    layer1_average: &[bool],
    layer2_average: &[bool],
) -> Result<BlendedLevelScene, LevelRenderError> {
    for (number, layer, average) in [
        (1, &level.layer1, layer1_average),
        (2, &level.layer2, layer2_average),
    ] {
        if layer.raw_tilemap.len() != average.len() {
            return Err(LevelRenderError::BlendShape {
                layer: number,
                expected: layer.raw_tilemap.len(),
                actual: average.len(),
            });
        }
    }
    let scene = build_level_scene_with_layer3(level, layout, map16, appearances, layer3)?;
    let mut average = Vec::with_capacity(scene.instances.len());
    append_layer3_average(&mut average, layer3, Layer3Placement::BehindLayer2);
    append_layer_average(&mut average, &level.layer2, map16, layer2_average);
    append_layer3_average(
        &mut average,
        layer3,
        Layer3Placement::BetweenLayer2AndLayer1,
    );
    append_layer_average(&mut average, &level.layer1, map16, layer1_average);
    append_layer3_average(&mut average, layer3, Layer3Placement::AboveLayer1);
    average.extend(
        appearances
            .iter()
            .filter(|appearance| source_exists(level, appearance.source))
            .map(|_| false),
    );
    append_layer3_average(&mut average, layer3, Layer3Placement::AboveEntities);
    debug_assert_eq!(scene.instances.len(), average.len());
    Ok(BlendedLevelScene { scene, average })
}

fn append_layer_average(
    output: &mut Vec<bool>,
    layer: &LayerData,
    map16: &[Map16Tile],
    average: &[bool],
) {
    for high_priority in [false, true] {
        for (index, definition_index) in layer.raw_tilemap.iter().copied().enumerate() {
            let Some(definition) = map16.get(usize::from(definition_index)) else {
                continue;
            };
            output.extend(
                [
                    definition.top_left,
                    definition.top_right,
                    definition.bottom_left,
                    definition.bottom_right,
                ]
                .into_iter()
                .filter(|subtile| subtile.priority() == high_priority)
                .map(|_| average[index]),
            );
        }
    }
}

fn append_layer3_average(
    output: &mut Vec<bool>,
    layer3: Option<&MaterializedLayer3Plane>,
    placement: Layer3Placement,
) {
    if let Some(layer3) = layer3.filter(|plane| plane.placement == placement) {
        output.resize(output.len() + layer3.instances.len(), false);
    }
}

fn source_exists(level: &Level, source: EntitySource) -> bool {
    match source {
        EntitySource::Layer1Object(index) => index < level.layer1.objects.records.len(),
        EntitySource::Layer2Object(index) => index < level.layer2.objects.records.len(),
        EntitySource::Sprite(index) => index < level.sprites.records.len(),
    }
}
