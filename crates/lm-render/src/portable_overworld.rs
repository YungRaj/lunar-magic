use crate::{
    Canvas, CanvasError, OverworldRenderError, apply_event_reveals, build_overworld_layer_scene,
    build_overworld_scene, draw_scene, resolve_sprite_appearances,
};
use lm_graphics::{
    GraphicsInterchangeFile, MaterializedAnimationFrame, MaterializedFrameError, Palette,
};
use lm_level::{Map16SetFile, Map16Tile};
use lm_overworld::SpriteAppearanceFile;
use lm_project::CompleteOverworldFile;
use std::fmt;

const PALETTE_ROW_COLORS: usize = 16;

#[derive(Debug)]
pub enum PortableOverworldRenderError {
    TooManyCompletedReveals { requested: usize, available: usize },
    MissingMap16Tile { layer: u8, tile: u16 },
    InvalidPaletteShape(usize),
    EmptyPalette,
    MissingGraphicsTile { instance: usize, tile: usize },
    MissingPaletteRow { instance: usize, row: usize },
    DimensionOverflow,
    Animation(MaterializedFrameError),
    Scene(OverworldRenderError),
    Canvas(CanvasError),
}

impl fmt::Display for PortableOverworldRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "portable overworld render failed: {self:?}")
    }
}

impl std::error::Error for PortableOverworldRenderError {}

/// Renders one complete portable overworld snapshot, optionally at an animation frame.
///
/// # Errors
///
/// Rejects reveal counts outside the model, malformed layers or palettes, missing Map16,
/// graphics, or palette references, invalid animation overrides, and excessive dimensions.
pub fn render_portable_overworld(
    overworld: &CompleteOverworldFile,
    map16: &Map16SetFile,
    graphics: &GraphicsInterchangeFile,
    appearance_definitions: Option<&SpriteAppearanceFile>,
    animation_frame: Option<&MaterializedAnimationFrame>,
    completed_reveals: usize,
) -> Result<Canvas, PortableOverworldRenderError> {
    let available = overworld.data.event_reveals.entries.len();
    if completed_reveals > available {
        return Err(PortableOverworldRenderError::TooManyCompletedReveals {
            requested: completed_reveals,
            available,
        });
    }
    let definitions: Vec<Map16Tile> = map16
        .set
        .pages
        .iter()
        .flat_map(|page| page.tiles.iter().copied())
        .collect();
    let layer1 = apply_event_reveals(
        &overworld.data.layers.layer1,
        &overworld.data.event_reveals.entries,
        completed_reveals,
    );
    validate_map16_references(1, &layer1.tiles, definitions.len())?;
    validate_map16_references(2, &overworld.data.layers.layer2.tiles, definitions.len())?;
    let appearances = appearance_definitions
        .map(|values| resolve_sprite_appearances(&overworld.data.sprites, values))
        .unwrap_or_default();
    let scene = build_overworld_scene(
        &layer1,
        &overworld.data.layers.layer2,
        &definitions,
        &overworld.data.sprites,
        &appearances,
    )
    .map_err(PortableOverworldRenderError::Scene)?;
    let (animated_graphics, animated_palette);
    let (tiles, palette) = if let Some(frame) = animation_frame {
        (animated_graphics, animated_palette) = frame
            .apply(&graphics.graphics, &overworld.data.palette)
            .map_err(PortableOverworldRenderError::Animation)?;
        (&animated_graphics.tiles, &animated_palette)
    } else {
        (&graphics.graphics.tiles, &overworld.data.palette)
    };
    let palettes = palette_rows(palette)?;
    validate_scene_assets(&scene.instances, tiles.len(), palettes.len())?;
    let width = overworld
        .shape
        .width
        .checked_mul(16)
        .ok_or(PortableOverworldRenderError::DimensionOverflow)?;
    let height = overworld
        .shape
        .height
        .checked_mul(16)
        .ok_or(PortableOverworldRenderError::DimensionOverflow)?;
    let mut canvas =
        Canvas::try_new(width, height).map_err(PortableOverworldRenderError::Canvas)?;
    draw_scene(&mut canvas, &scene, tiles, &palettes);
    Ok(canvas)
}

/// Renders one authentic overworld layer without requiring or fabricating another layer.
///
/// # Errors
///
/// Rejects malformed layer dimensions, missing Map16/graphics/palette references, invalid palette
/// shapes, or excessive output dimensions.
pub fn render_portable_overworld_layer(
    layer_number: u8,
    layer: &lm_overworld::OverworldLayer,
    map16: &Map16SetFile,
    graphics: &GraphicsInterchangeFile,
    palette: &Palette,
) -> Result<Canvas, PortableOverworldRenderError> {
    let definitions: Vec<Map16Tile> = map16
        .set
        .pages
        .iter()
        .flat_map(|page| page.tiles.iter().copied())
        .collect();
    validate_map16_references(layer_number, &layer.tiles, definitions.len())?;
    let scene = build_overworld_layer_scene(layer_number, layer, &definitions)
        .map_err(PortableOverworldRenderError::Scene)?;
    let palettes = palette_rows(palette)?;
    validate_scene_assets(
        &scene.instances,
        graphics.graphics.tiles.len(),
        palettes.len(),
    )?;
    let width = layer
        .width
        .checked_mul(16)
        .ok_or(PortableOverworldRenderError::DimensionOverflow)?;
    let height = layer
        .height
        .checked_mul(16)
        .ok_or(PortableOverworldRenderError::DimensionOverflow)?;
    let mut canvas =
        Canvas::try_new(width, height).map_err(PortableOverworldRenderError::Canvas)?;
    draw_scene(&mut canvas, &scene, &graphics.graphics.tiles, &palettes);
    Ok(canvas)
}

/// Renders SMW's gameplay-consumed `$7F4000-$7F7FFF` Layer 2 tilemap directly as packed SNES
/// 8x8 tilemap words. This storage is not Map16 and must not pass through Map16 definitions.
///
/// # Errors
///
/// Rejects malformed layer dimensions, missing graphics/palette references, malformed palettes,
/// or excessive output dimensions.
pub fn render_smw_overworld_layer2_tilemap(
    layer: &lm_overworld::OverworldLayer,
    graphics: &GraphicsInterchangeFile,
    palette: &Palette,
) -> Result<Canvas, PortableOverworldRenderError> {
    let expected = layer
        .width
        .checked_mul(layer.height)
        .ok_or(PortableOverworldRenderError::DimensionOverflow)?;
    if layer.tiles.len() != expected {
        return Err(PortableOverworldRenderError::Scene(
            OverworldRenderError::InvalidLayerShape {
                layer: 2,
                width: layer.width,
                height: layer.height,
                tiles: layer.tiles.len(),
            },
        ));
    }
    let palettes = palette_rows(palette)?;
    let mut scene = crate::Scene::default();
    for high_priority in [false, true] {
        for (index, &word) in layer.tiles.iter().enumerate() {
            let subtile = lm_level::Subtile(word);
            if subtile.priority() != high_priority {
                continue;
            }
            scene.instances.push(crate::TileInstance {
                tile_index: usize::from(subtile.tile_number()),
                palette_index: usize::from(subtile.palette()),
                x: i32::try_from(index % layer.width)
                    .map_err(|_| PortableOverworldRenderError::DimensionOverflow)?
                    * 8,
                y: i32::try_from(index / layer.width)
                    .map_err(|_| PortableOverworldRenderError::DimensionOverflow)?
                    * 8,
                x_flip: subtile.x_flip(),
                y_flip: subtile.y_flip(),
            });
        }
    }
    validate_scene_assets(
        &scene.instances,
        graphics.graphics.tiles.len(),
        palettes.len(),
    )?;
    let width = layer
        .width
        .checked_mul(8)
        .ok_or(PortableOverworldRenderError::DimensionOverflow)?;
    let height = layer
        .height
        .checked_mul(8)
        .ok_or(PortableOverworldRenderError::DimensionOverflow)?;
    let mut canvas =
        Canvas::try_new(width, height).map_err(PortableOverworldRenderError::Canvas)?;
    draw_scene(&mut canvas, &scene, &graphics.graphics.tiles, &palettes);
    Ok(canvas)
}

fn validate_map16_references(
    layer: u8,
    tiles: &[u16],
    definition_count: usize,
) -> Result<(), PortableOverworldRenderError> {
    if let Some(tile) = tiles
        .iter()
        .find(|tile| usize::from(**tile) >= definition_count)
    {
        return Err(PortableOverworldRenderError::MissingMap16Tile { layer, tile: *tile });
    }
    Ok(())
}

fn palette_rows(palette: &Palette) -> Result<Vec<Palette>, PortableOverworldRenderError> {
    if palette.colors.len() % PALETTE_ROW_COLORS != 0 {
        return Err(PortableOverworldRenderError::InvalidPaletteShape(
            palette.colors.len(),
        ));
    }
    let rows = palette
        .colors
        .chunks_exact(PALETTE_ROW_COLORS)
        .map(|colors| Palette {
            colors: colors.to_vec(),
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return Err(PortableOverworldRenderError::EmptyPalette);
    }
    Ok(rows)
}

fn validate_scene_assets(
    instances: &[crate::TileInstance],
    graphics_count: usize,
    palette_count: usize,
) -> Result<(), PortableOverworldRenderError> {
    for (instance, value) in instances.iter().enumerate() {
        if value.tile_index >= graphics_count {
            return Err(PortableOverworldRenderError::MissingGraphicsTile {
                instance,
                tile: value.tile_index,
            });
        }
        if value.palette_index >= palette_count {
            return Err(PortableOverworldRenderError::MissingPaletteRow {
                instance,
                row: value.palette_index,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, GraphicsFile4bpp, IndexedTile};
    use lm_level::{Map16Page, Map16Set, Map16Tile, Subtile};
    use lm_overworld::OverworldLayer;

    fn assets() -> (Map16SetFile, GraphicsInterchangeFile, Palette) {
        (
            Map16SetFile {
                set: Map16Set {
                    pages: vec![Map16Page {
                        tiles: vec![Map16Tile {
                            top_left: Subtile(0),
                            top_right: Subtile(0),
                            bottom_left: Subtile(0),
                            bottom_right: Subtile(0),
                            acts_like: 0,
                        }],
                    }],
                },
            },
            GraphicsInterchangeFile {
                source_slot: 0,
                graphics: GraphicsFile4bpp {
                    tiles: vec![IndexedTile::new([1; IndexedTile::PIXEL_COUNT])],
                },
            },
            Palette {
                colors: (0..16)
                    .map(|index| {
                        if index == 1 {
                            Bgr555(0x001f)
                        } else {
                            Bgr555(0)
                        }
                    })
                    .collect(),
            },
        )
    }

    #[test]
    fn single_layer_render_does_not_require_a_fabricated_companion_layer() {
        let layer = OverworldLayer::new(2, 1, vec![0, 0]).unwrap();
        let (map16, graphics, palette) = assets();
        let canvas =
            render_portable_overworld_layer(2, &layer, &map16, &graphics, &palette).unwrap();
        assert_eq!((canvas.width(), canvas.height()), (32, 16));
        assert_eq!(canvas.get(0, 0).unwrap().red, 255);
        assert_eq!(canvas.get(31, 15).unwrap().red, 255);
    }

    #[test]
    fn single_layer_render_retains_strict_map16_reference_validation() {
        let layer = OverworldLayer::new(1, 1, vec![1]).unwrap();
        let (map16, graphics, palette) = assets();
        assert!(matches!(
            render_portable_overworld_layer(2, &layer, &map16, &graphics, &palette),
            Err(PortableOverworldRenderError::MissingMap16Tile { layer: 2, tile: 1 })
        ));
    }

    #[test]
    fn gameplay_layer2_renderer_decodes_packed_8x8_words_without_map16() {
        let layer = OverworldLayer::new(2, 1, vec![0, 1 | (1 << 10) | 0xc000]).unwrap();
        let graphics = GraphicsInterchangeFile {
            source_slot: 0,
            graphics: GraphicsFile4bpp {
                tiles: vec![
                    IndexedTile::new([1; IndexedTile::PIXEL_COUNT]),
                    IndexedTile::new([2; IndexedTile::PIXEL_COUNT]),
                ],
            },
        };
        let mut colors = vec![Bgr555(0); 32];
        colors[1] = Bgr555(0x001f);
        colors[16 + 2] = Bgr555(0x03e0);
        let canvas =
            render_smw_overworld_layer2_tilemap(&layer, &graphics, &Palette { colors }).unwrap();
        assert_eq!((canvas.width(), canvas.height()), (16, 8));
        assert_eq!(canvas.get(0, 0).unwrap().red, 255);
        assert_eq!(canvas.get(15, 7).unwrap().green, 255);
    }

    #[test]
    fn gameplay_layer2_renderer_rejects_missing_direct_tile_reference() {
        let layer = OverworldLayer::new(1, 1, vec![1]).unwrap();
        let (_, graphics, palette) = assets();
        assert!(matches!(
            render_smw_overworld_layer2_tilemap(&layer, &graphics, &palette),
            Err(PortableOverworldRenderError::MissingGraphicsTile { tile: 1, .. })
        ));
    }
}
