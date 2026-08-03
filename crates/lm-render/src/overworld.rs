mod events;
mod model;
mod scene_builder;

pub use events::{apply_event_changes, apply_event_reveals};
use model::validate_layer;
pub use model::{OverworldRenderError, SpriteAppearance, resolve_sprite_appearances};
pub use scene_builder::{build_overworld_layer_scene, build_overworld_scene};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Point, Rgba, TileInstance, Viewport, draw_scene_viewport};
    use lm_graphics::{Bgr555, IndexedTile, Palette};
    use lm_level::{Map16Tile, Subtile};
    use lm_overworld::{
        EventId, EventReveal, EventTileChange, OverworldLayer, OverworldSprite,
        SpriteAppearanceFile, Submap,
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
    fn event_application_is_ordered_bounded_and_stale_safe() {
        let layer = OverworldLayer::new(2, 1, vec![3, 9]).unwrap();
        let events = [
            EventTileChange {
                event: EventId(1),
                x: 0,
                y: 0,
                before: 3,
                after: 4,
                raw_flags: 0,
            },
            EventTileChange {
                event: EventId(2),
                x: 0,
                y: 0,
                before: 4,
                after: 5,
                raw_flags: 0,
            },
            EventTileChange {
                event: EventId(2),
                x: 1,
                y: 0,
                before: 8,
                after: 7,
                raw_flags: 0,
            },
        ];
        assert_eq!(
            apply_event_changes(&layer, &events, 1).unwrap().tiles,
            [4, 9]
        );
        assert_eq!(
            apply_event_changes(&layer, &events, 2).unwrap().tiles,
            [5, 9]
        );
        let outside = [EventTileChange {
            event: EventId(0),
            x: 2,
            y: 0,
            before: 0,
            after: 1,
            raw_flags: 0,
        }];
        assert!(matches!(
            apply_event_changes(&layer, &outside, 0),
            Err(OverworldRenderError::EventCoordinateOutOfRange { .. })
        ));
    }

    #[test]
    fn reveal_pairs_apply_globally_in_table_order() {
        let layer = OverworldLayer::new(3, 1, vec![1, 1, 2]).unwrap();
        let reveals = [
            EventReveal {
                source_tile: 1,
                destination_tile: 2,
            },
            EventReveal {
                source_tile: 2,
                destination_tile: 3,
            },
        ];
        assert_eq!(apply_event_reveals(&layer, &reveals, 1).tiles, [2, 2, 2]);
        assert_eq!(apply_event_reveals(&layer, &reveals, 2).tiles, [3, 3, 3]);
    }

    #[test]
    fn layers_expand_in_painter_order_and_sprites_are_last() {
        let layer1 = OverworldLayer::new(1, 1, vec![1]).unwrap();
        let layer2 = OverworldLayer::new(1, 1, vec![0]).unwrap();
        let map16 = [definition(0), definition(4)];
        let sprites = [OverworldSprite {
            id: 9,
            x: 10,
            y: 20,
            submap: Submap::Main,
            extra: vec![],
        }];
        let scene = build_overworld_scene(
            &layer1,
            &layer2,
            &map16,
            &sprites,
            &[SpriteAppearance {
                sprite_index: 0,
                tile_index: 12,
                palette_index: 3,
                x_offset: -2,
                y_offset: 1,
                x_flip: true,
                y_flip: false,
            }],
        )
        .unwrap();
        assert_eq!(scene.instances.len(), 9);
        assert_eq!(scene.instances[0].tile_index, 0);
        assert_eq!(scene.instances[4].tile_index, 4);
        assert_eq!(
            scene.instances[8],
            TileInstance {
                tile_index: 12,
                palette_index: 3,
                x: 8,
                y: 21,
                x_flip: true,
                y_flip: false
            }
        );
    }

    #[test]
    fn appearance_definitions_expand_for_each_matching_sprite() {
        use lm_overworld::{SpriteAppearanceDefinition, SpriteAppearancePart};
        let sprites = [
            OverworldSprite {
                id: 3,
                x: 1,
                y: 2,
                submap: Submap::Main,
                extra: vec![],
            },
            OverworldSprite {
                id: 3,
                x: 4,
                y: 5,
                submap: Submap::Main,
                extra: vec![],
            },
        ];
        let definitions = SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 3,
                parts: vec![SpriteAppearancePart {
                    tile_index: 9,
                    palette_index: 2,
                    x_offset: -1,
                    y_offset: 8,
                    x_flip: false,
                    y_flip: true,
                }],
            }],
        };
        let appearances = resolve_sprite_appearances(&sprites, &definitions);
        assert_eq!(appearances.len(), 2);
        assert_eq!(appearances[1].sprite_index, 1);
        assert_eq!(appearances[1].y_offset, 8);
    }

    #[test]
    fn malformed_shapes_and_missing_assets_are_safe() {
        let malformed = OverworldLayer {
            tiles: vec![0],
            width: 2,
            height: 2,
        };
        let empty = OverworldLayer::new(0, 0, vec![]).unwrap();
        assert!(matches!(
            build_overworld_scene(&malformed, &empty, &[], &[], &[]),
            Err(OverworldRenderError::InvalidLayerShape { layer: 1, .. })
        ));
        let layer = OverworldLayer::new(1, 1, vec![99]).unwrap();
        assert!(
            build_overworld_scene(&layer, &empty, &[], &[], &[])
                .unwrap()
                .instances
                .is_empty()
        );
    }

    #[test]
    fn built_scene_renders_through_a_panned_viewport() {
        let layer1 = OverworldLayer::new(1, 1, vec![0]).unwrap();
        let layer2 = OverworldLayer::new(0, 0, vec![]).unwrap();
        let scene = build_overworld_scene(&layer1, &layer2, &[definition(0)], &[], &[]).unwrap();
        let tiles: Vec<_> = (0..4)
            .map(|_| IndexedTile::new([1; IndexedTile::PIXEL_COUNT]))
            .collect();
        let palettes = [Palette {
            colors: vec![Bgr555(0), Bgr555(0x7c00)],
        }];
        let viewport = Viewport::new(Point { x: 8, y: 8 }, 2, 2, 2, 1).unwrap();
        let mut canvas = Canvas::try_new(2, 2).unwrap();
        draw_scene_viewport(&mut canvas, viewport, &scene, &tiles, &palettes).unwrap();
        assert_eq!(
            canvas.get(0, 0),
            Some(Rgba {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            })
        );
        assert_eq!(canvas.get(1, 1), canvas.get(0, 0));
    }
}
