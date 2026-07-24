use lm_level::{AppearanceSource, EntityAppearanceFile};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridPlacement {
    pub width: usize,
    pub height: usize,
    pub origin_x: i32,
    pub origin_y: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelSceneLayout {
    pub layer1: GridPlacement,
    pub layer2: GridPlacement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntitySource {
    Layer1Object(usize),
    Layer2Object(usize),
    Sprite(usize),
}

/// One tile emitted by an object/sprite definition renderer for editor preview.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntityAppearance {
    pub source: EntitySource,
    pub tile_index: usize,
    pub palette_index: usize,
    pub x: i32,
    pub y: i32,
    pub x_flip: bool,
    pub y_flip: bool,
}

/// Converts portable resolved preview records into renderer-native appearances.
///
/// Records with source indices that do not fit the host are omitted; the scene builder separately
/// validates that each remaining source exists in the decoded level.
#[must_use]
pub fn resolve_entity_appearances(file: &EntityAppearanceFile) -> Vec<EntityAppearance> {
    file.appearances
        .iter()
        .filter_map(|record| {
            let source = match record.source {
                AppearanceSource::Layer1Object(index) => {
                    EntitySource::Layer1Object(usize::try_from(index).ok()?)
                }
                AppearanceSource::Layer2Object(index) => {
                    EntitySource::Layer2Object(usize::try_from(index).ok()?)
                }
                AppearanceSource::Sprite(index) => {
                    EntitySource::Sprite(usize::try_from(index).ok()?)
                }
            };
            Some(EntityAppearance {
                source,
                tile_index: usize::from(record.tile_index),
                palette_index: usize::from(record.palette_index),
                x: record.x,
                y: record.y,
                x_flip: record.x_flip,
                y_flip: record.y_flip,
            })
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelRenderError {
    InvalidLayerShape {
        layer: u8,
        width: usize,
        height: usize,
        tiles: usize,
    },
    CoordinateOverflow,
    Layer3StateMissing,
    BlendShape {
        layer: u8,
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for LevelRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot render level: {self:?}")
    }
}

impl std::error::Error for LevelRenderError {}
