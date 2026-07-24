mod blending;
mod model;
mod scene_builder;

pub(crate) use blending::build_level_scene_with_cell_blends;
pub use model::{
    EntityAppearance, EntitySource, GridPlacement, LevelRenderError, LevelSceneLayout,
    resolve_entity_appearances,
};
pub use scene_builder::{build_level_scene, build_level_scene_with_layer3};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Canvas, Layer3Placement, MaterializedLayer3Plane, Point, Rgba, TileInstance, Viewport,
        draw_scene_viewport,
    };
    use lm_graphics::{Bgr555, IndexedTile, Palette};
    use lm_level::{
        LayerData, Level, Map16Tile, ObjectRecord, ObjectStream, SpriteRecord, SpriteStream,
        Subtile,
    };

    fn definition(tile: u16) -> Map16Tile {
        Map16Tile {
            top_left: Subtile(tile),
            top_right: Subtile(tile + 1),
            bottom_left: Subtile(tile + 2),
            bottom_right: Subtile(tile + 3),
            acts_like: 0,
        }
    }

    #[test]
    fn portable_appearance_records_preserve_sources_and_coordinates() {
        use lm_level::{AppearanceSource, EntityAppearanceFile, EntityAppearanceRecord};
        let file = EntityAppearanceFile {
            appearances: vec![EntityAppearanceRecord {
                source: AppearanceSource::Layer2Object(7),
                tile_index: 0x123,
                palette_index: 6,
                x: -12,
                y: 34,
                x_flip: true,
                y_flip: false,
            }],
        };
        assert_eq!(
            resolve_entity_appearances(&file),
            [EntityAppearance {
                source: EntitySource::Layer2Object(7),
                tile_index: 0x123,
                palette_index: 6,
                x: -12,
                y: 34,
                x_flip: true,
                y_flip: false,
            }]
        );
    }

    fn level() -> Level {
        Level {
            layer1: LayerData {
                objects: ObjectStream {
                    records: vec![ObjectRecord::new(vec![1, 2, 3]).unwrap()],
                },
                raw_tilemap: vec![1],
            },
            layer2: LayerData {
                objects: ObjectStream::default(),
                raw_tilemap: vec![0],
            },
            sprites: SpriteStream {
                header: 0,
                records: vec![SpriteRecord {
                    encoded: vec![4, 5, 6],
                }],
            },
            ..Level::default()
        }
    }

    fn layout() -> LevelSceneLayout {
        LevelSceneLayout {
            layer1: GridPlacement {
                width: 1,
                height: 1,
                origin_x: 16,
                origin_y: 0,
            },
            layer2: GridPlacement {
                width: 1,
                height: 1,
                origin_x: 0,
                origin_y: 0,
            },
        }
    }

    #[test]
    fn layers_and_valid_entities_follow_painter_order() {
        let scene = build_level_scene(
            &level(),
            layout(),
            &[definition(0), definition(4)],
            &[
                EntityAppearance {
                    source: EntitySource::Sprite(0),
                    tile_index: 9,
                    palette_index: 2,
                    x: 7,
                    y: 8,
                    x_flip: true,
                    y_flip: false,
                },
                EntityAppearance {
                    source: EntitySource::Layer2Object(99),
                    tile_index: 10,
                    palette_index: 0,
                    x: 0,
                    y: 0,
                    x_flip: false,
                    y_flip: false,
                },
            ],
        )
        .unwrap();
        assert_eq!(scene.instances.len(), 9);
        assert_eq!(scene.instances[0].tile_index, 0);
        assert_eq!(scene.instances[4].tile_index, 4);
        assert_eq!(scene.instances[4].x, 16);
        assert_eq!(scene.instances[8].tile_index, 9);
    }

    #[test]
    fn materialized_layer_three_uses_explicit_painter_boundary() {
        let plane = |placement| MaterializedLayer3Plane {
            source_digest: [0; 32],
            placement,
            instances: vec![TileInstance {
                tile_index: 99,
                palette_index: 0,
                x: 0,
                y: 0,
                x_flip: false,
                y_flip: false,
            }],
        };
        assert!(matches!(
            build_level_scene_with_layer3(
                &level(),
                layout(),
                &[definition(0), definition(4)],
                &[],
                Some(&plane(Layer3Placement::AboveLayer1)),
            ),
            Err(LevelRenderError::Layer3StateMissing)
        ));
        let mut level = level();
        level.layer3 = Some(lm_level::Layer3Data::default());
        let scene = build_level_scene_with_layer3(
            &level,
            layout(),
            &[definition(0), definition(4)],
            &[],
            Some(&plane(Layer3Placement::BetweenLayer2AndLayer1)),
        )
        .unwrap();
        assert_eq!(scene.instances[4].tile_index, 99);
        let scene = build_level_scene_with_layer3(
            &level,
            layout(),
            &[definition(0), definition(4)],
            &[],
            Some(&plane(Layer3Placement::AboveEntities)),
        )
        .unwrap();
        assert_eq!(scene.instances.last().unwrap().tile_index, 99);
    }

    #[test]
    fn malformed_shapes_and_missing_map16_are_safe() {
        let mut malformed = level();
        malformed.layer1.raw_tilemap.push(2);
        assert!(matches!(
            build_level_scene(&malformed, layout(), &[], &[]),
            Err(LevelRenderError::InvalidLayerShape { layer: 1, .. })
        ));
        let mut level = level();
        level.layer1.raw_tilemap[0] = 99;
        let scene = build_level_scene(&level, layout(), &[definition(0)], &[]).unwrap();
        assert_eq!(scene.instances.len(), 4);
    }

    #[test]
    fn level_scene_renders_through_negative_origin_viewport() {
        let mut layout = layout();
        layout.layer2.origin_x = -16;
        let scene =
            build_level_scene(&level(), layout, &[definition(0), definition(0)], &[]).unwrap();
        let tiles: Vec<_> = (0..4)
            .map(|_| IndexedTile::new([1; IndexedTile::PIXEL_COUNT]))
            .collect();
        let palettes = [Palette {
            colors: vec![Bgr555(0), Bgr555(0x001f)],
        }];
        let viewport = Viewport::new(Point { x: -16, y: 0 }, 1, 1, 1, 1).unwrap();
        let mut canvas = Canvas::try_new(1, 1).unwrap();
        draw_scene_viewport(&mut canvas, viewport, &scene, &tiles, &palettes).unwrap();
        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255,
            })
        );
    }

    #[test]
    fn high_priority_subtiles_are_emitted_after_low_priority_peers() {
        let mut level = Level::default();
        level.layer1.raw_tilemap = vec![0];
        let layout = LevelSceneLayout {
            layer1: GridPlacement {
                width: 1,
                height: 1,
                origin_x: 0,
                origin_y: 0,
            },
            layer2: GridPlacement {
                width: 0,
                height: 0,
                origin_x: 0,
                origin_y: 0,
            },
        };
        let definition = Map16Tile {
            top_left: Subtile(0x2007),
            top_right: Subtile(1),
            bottom_left: Subtile(2),
            bottom_right: Subtile(3),
            acts_like: 0,
        };
        let scene = build_level_scene(&level, layout, &[definition], &[]).unwrap();
        assert_eq!(
            scene
                .instances
                .iter()
                .map(|instance| instance.tile_index)
                .collect::<Vec<_>>(),
            [1, 2, 3, 7]
        );
    }
}
