use lm_overworld::{OverworldLayer, OverworldSprite, SpriteAppearanceFile};
use std::fmt;

/// Rendering metadata supplied by the sprite-definition layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpriteAppearance {
    pub sprite_index: usize,
    pub tile_index: usize,
    pub palette_index: usize,
    pub x_offset: i32,
    pub y_offset: i32,
    pub x_flip: bool,
    pub y_flip: bool,
}

/// Resolves definition parts for every matching sprite record in model order.
#[must_use]
pub fn resolve_sprite_appearances(
    sprites: &[OverworldSprite],
    definitions: &SpriteAppearanceFile,
) -> Vec<SpriteAppearance> {
    sprites
        .iter()
        .enumerate()
        .flat_map(|(sprite_index, sprite)| {
            definitions
                .definition(sprite.id)
                .into_iter()
                .flat_map(move |definition| {
                    definition.parts.iter().map(move |part| SpriteAppearance {
                        sprite_index,
                        tile_index: usize::from(part.tile_index),
                        palette_index: usize::from(part.palette_index),
                        x_offset: i32::from(part.x_offset),
                        y_offset: i32::from(part.y_offset),
                        x_flip: part.x_flip,
                        y_flip: part.y_flip,
                    })
                })
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldRenderError {
    InvalidLayerShape {
        layer: u8,
        width: usize,
        height: usize,
        tiles: usize,
    },
    EventCoordinateOutOfRange {
        event: u8,
        x: u16,
        y: u16,
    },
    CoordinateOverflow,
}

impl fmt::Display for OverworldRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot render overworld: {self:?}")
    }
}

impl std::error::Error for OverworldRenderError {}

pub(super) fn validate_layer(
    number: u8,
    layer: &OverworldLayer,
) -> Result<(), OverworldRenderError> {
    if layer.width.checked_mul(layer.height) != Some(layer.tiles.len()) {
        return Err(OverworldRenderError::InvalidLayerShape {
            layer: number,
            width: layer.width,
            height: layer.height,
            tiles: layer.tiles.len(),
        });
    }
    Ok(())
}
