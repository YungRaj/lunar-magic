use crate::{
    CompleteLevelFile, ExpandedLevelHeader, HeaderValueError, Layer1VerticalScrollMode, Level,
    lunar_magic_canonical_level_mode,
};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelLayer {
    Layer1,
    Layer2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayerDimensions {
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileCoordinate {
    pub x: usize,
    pub y: usize,
}

/// A mutation of one proven bitfield in the legacy five-byte level header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyHeaderEdit {
    BackgroundPalette(u8),
    LastScreen(u8),
    LevelMode(u8),
    BackgroundColor(u8),
    SpriteTileset(u8),
    DefaultMusicSelector(u8),
    TimeLimitSelector(u8),
    SpritePalette(u8),
    ForegroundPalette(u8),
    ObjectTileset(u8),
    Layer1VerticalScroll(Layer1VerticalScrollMode),
}

/// One ordered mutation in an atomic level-property batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelPropertyEdit {
    SetLevelNumber(u16),
    SetSpriteHeader(u8),
    LegacyHeader(LegacyHeaderEdit),
    /// Enables/replaces the exact expanded record or disables it without interpreting raw fields.
    SetExpandedHeader(Option<ExpandedLevelHeader>),
    SetExpandedField {
        index: usize,
        value: u16,
    },
    SetTile {
        layer: LevelLayer,
        dimensions: LayerDimensions,
        coordinate: TileCoordinate,
        tile: u16,
    },
    /// Replaces an entire tilemap after validating the supplied new dimensions.
    ReplaceTilemap {
        layer: LevelLayer,
        dimensions: LayerDimensions,
        tiles: Vec<u16>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelPropertyEditError {
    Header {
        command: usize,
        error: HeaderValueError,
    },
    MissingExpandedHeader {
        command: usize,
    },
    ExpandedFieldOutOfRange {
        command: usize,
        index: usize,
    },
    InvalidDimensions {
        command: usize,
        dimensions: LayerDimensions,
    },
    TilemapShapeMismatch {
        command: usize,
        layer: LevelLayer,
        expected: usize,
        actual: usize,
    },
    CoordinateOutOfRange {
        command: usize,
        coordinate: TileCoordinate,
        dimensions: LayerDimensions,
    },
}

impl fmt::Display for LevelPropertyEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid atomic level-property edit: {self:?}")
    }
}

impl std::error::Error for LevelPropertyEditError {}

impl Level {
    /// Atomically edits proven legacy-header fields, opaque expanded fields, and raw layer tiles.
    ///
    /// Tile coordinates always carry explicit dimensions because level mode interpretation is
    /// revision-specific. Each command observes prior commands. Invalid header values, shapes,
    /// coordinates, or expanded-field access leave the complete level unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`LevelPropertyEditError`] identifying the failing command and validation rule.
    pub fn apply_property_edits(
        &mut self,
        edits: &[LevelPropertyEdit],
    ) -> Result<(), LevelPropertyEditError> {
        if edits.is_empty() {
            return Ok(());
        }
        let mut staged = self.clone();
        for (command, edit) in edits.iter().enumerate() {
            match edit {
                LevelPropertyEdit::SetLevelNumber(value) => staged.number = *value,
                LevelPropertyEdit::SetSpriteHeader(value) => staged.sprites.header = *value,
                LevelPropertyEdit::LegacyHeader(edit) => {
                    apply_header_edit(&mut staged, *edit)
                        .map_err(|error| LevelPropertyEditError::Header { command, error })?;
                }
                LevelPropertyEdit::SetExpandedHeader(header) => {
                    staged.header.expanded = *header;
                }
                LevelPropertyEdit::SetExpandedField { index, value } => {
                    if *index >= ExpandedLevelHeader::FIELD_COUNT {
                        return Err(LevelPropertyEditError::ExpandedFieldOutOfRange {
                            command,
                            index: *index,
                        });
                    }
                    let Some(header) = staged.header.expanded.as_mut() else {
                        return Err(LevelPropertyEditError::MissingExpandedHeader { command });
                    };
                    header.fields[*index] = *value;
                }
                LevelPropertyEdit::SetTile {
                    layer,
                    dimensions,
                    coordinate,
                    tile,
                } => {
                    let area = checked_area(*dimensions, command)?;
                    let tiles = tilemap_mut(&mut staged, *layer);
                    if tiles.len() != area {
                        return Err(LevelPropertyEditError::TilemapShapeMismatch {
                            command,
                            layer: *layer,
                            expected: area,
                            actual: tiles.len(),
                        });
                    }
                    if coordinate.x >= dimensions.width || coordinate.y >= dimensions.height {
                        return Err(LevelPropertyEditError::CoordinateOutOfRange {
                            command,
                            coordinate: *coordinate,
                            dimensions: *dimensions,
                        });
                    }
                    let index = coordinate.y * dimensions.width + coordinate.x;
                    tiles[index] = *tile;
                }
                LevelPropertyEdit::ReplaceTilemap {
                    layer,
                    dimensions,
                    tiles,
                } => {
                    let area = checked_area(*dimensions, command)?;
                    if tiles.len() != area {
                        return Err(LevelPropertyEditError::TilemapShapeMismatch {
                            command,
                            layer: *layer,
                            expected: area,
                            actual: tiles.len(),
                        });
                    }
                    tilemap_mut(&mut staged, *layer).clone_from(tiles);
                }
            }
        }
        *self = staged;
        Ok(())
    }
}

fn apply_header_edit(level: &mut Level, edit: LegacyHeaderEdit) -> Result<(), HeaderValueError> {
    match edit {
        LegacyHeaderEdit::BackgroundPalette(value) => {
            level.header.legacy.set_background_palette(value)
        }
        LegacyHeaderEdit::LastScreen(value) => level.header.legacy.set_last_screen(value),
        LegacyHeaderEdit::LevelMode(value) => level
            .header
            .legacy
            .set_level_mode(lunar_magic_canonical_level_mode(value)),
        LegacyHeaderEdit::BackgroundColor(value) => level.header.legacy.set_background_color(value),
        LegacyHeaderEdit::SpriteTileset(value) => level.header.legacy.set_sprite_tileset(value),
        LegacyHeaderEdit::DefaultMusicSelector(value) => {
            level.header.legacy.set_default_music_selector(value)
        }
        LegacyHeaderEdit::TimeLimitSelector(value) => {
            level.header.legacy.set_time_limit_selector(value)
        }
        LegacyHeaderEdit::SpritePalette(value) => level.header.legacy.set_sprite_palette(value),
        LegacyHeaderEdit::ForegroundPalette(value) => {
            level.header.legacy.set_foreground_palette(value)
        }
        LegacyHeaderEdit::ObjectTileset(value) => level.header.legacy.set_object_tileset(value),
        LegacyHeaderEdit::Layer1VerticalScroll(mode) => {
            level.header.legacy.set_layer1_vertical_scroll(mode);
            Ok(())
        }
    }
}

fn checked_area(
    dimensions: LayerDimensions,
    command: usize,
) -> Result<usize, LevelPropertyEditError> {
    let Some(area) = dimensions.width.checked_mul(dimensions.height) else {
        return Err(LevelPropertyEditError::InvalidDimensions {
            command,
            dimensions,
        });
    };
    if dimensions.width == 0 || dimensions.height == 0 || area > CompleteLevelFile::MAX_RECORDS {
        return Err(LevelPropertyEditError::InvalidDimensions {
            command,
            dimensions,
        });
    }
    Ok(area)
}

fn tilemap_mut(level: &mut Level, layer: LevelLayer) -> &mut Vec<u16> {
    match layer {
        LevelLayer::Layer1 => &mut level.layer1.raw_tilemap,
        LevelLayer::Layer2 => &mut level.layer2.raw_tilemap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompleteLevelFile, LegacyLevelHeader};

    #[test]
    fn mixed_header_and_layer_batch_preserves_unknown_bits_and_round_trips() {
        let legacy = LegacyLevelHeader::decode(&[0x9f, 0xba, 0x77, 0xc7, 0x55]).unwrap();
        let mut level = Level::default();
        level.header.legacy = legacy;
        level.layer1.raw_tilemap = vec![1, 2, 3, 4];
        level.layer2.raw_tilemap = vec![5];
        level
            .apply_property_edits(&[
                LevelPropertyEdit::SetLevelNumber(0x123),
                LevelPropertyEdit::SetSpriteHeader(0x5a),
                LevelPropertyEdit::LegacyHeader(LegacyHeaderEdit::BackgroundPalette(2)),
                LevelPropertyEdit::LegacyHeader(LegacyHeaderEdit::LastScreen(0x1d)),
                LevelPropertyEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(3)),
                LevelPropertyEdit::LegacyHeader(LegacyHeaderEdit::DefaultMusicSelector(6)),
                LevelPropertyEdit::LegacyHeader(LegacyHeaderEdit::TimeLimitSelector(2)),
                LevelPropertyEdit::SetExpandedHeader(Some(ExpandedLevelHeader::default())),
                LevelPropertyEdit::SetExpandedField {
                    index: 3,
                    value: 0x1234,
                },
                LevelPropertyEdit::SetTile {
                    layer: LevelLayer::Layer1,
                    dimensions: LayerDimensions {
                        width: 2,
                        height: 2,
                    },
                    coordinate: TileCoordinate { x: 1, y: 0 },
                    tile: 9,
                },
                LevelPropertyEdit::ReplaceTilemap {
                    layer: LevelLayer::Layer2,
                    dimensions: LayerDimensions {
                        width: 1,
                        height: 2,
                    },
                    tiles: vec![6, 7],
                },
            ])
            .unwrap();
        let encoded_header = level.header.legacy.encoded();
        assert_eq!(level.header.legacy.last_screen(), 0x1d);
        assert_eq!(encoded_header[0], 0x5d);
        assert_eq!(encoded_header[1] >> 5, 0xba >> 5);
        assert_eq!(encoded_header[2], 0x67);
        assert_eq!(encoded_header[3], 0x87);
        assert_eq!(encoded_header[4], 0x55);
        assert_eq!(level.header.expanded.unwrap().fields[3], 0x1234);
        assert_eq!(level.number, 0x123);
        assert_eq!(level.sprites.header, 0x5a);
        assert_eq!(level.layer1.raw_tilemap, [1, 9, 3, 4]);
        assert_eq!(level.layer2.raw_tilemap, [6, 7]);
        let bytes = CompleteLevelFile(level.clone()).encode().unwrap();
        assert_eq!(CompleteLevelFile::decode(&bytes).unwrap().0, level);
    }

    #[test]
    fn late_shape_and_header_failures_roll_back_every_property() {
        let mut level = Level::default();
        level.layer1.raw_tilemap = vec![1, 2, 3, 4];
        let original = level.clone();
        assert!(matches!(
            level.apply_property_edits(&[
                LevelPropertyEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(3)),
                LevelPropertyEdit::SetTile {
                    layer: LevelLayer::Layer1,
                    dimensions: LayerDimensions {
                        width: 3,
                        height: 2,
                    },
                    coordinate: TileCoordinate { x: 0, y: 0 },
                    tile: 9,
                },
            ]),
            Err(LevelPropertyEditError::TilemapShapeMismatch { command: 1, .. })
        ));
        assert_eq!(level, original);

        assert!(matches!(
            level.apply_property_edits(&[
                LevelPropertyEdit::SetSpriteHeader(0x22),
                LevelPropertyEdit::LegacyHeader(LegacyHeaderEdit::LastScreen(0x20)),
            ]),
            Err(LevelPropertyEditError::Header { command: 1, .. })
        ));
        assert_eq!(level, original);

        assert!(matches!(
            level.apply_property_edits(&[
                LevelPropertyEdit::ReplaceTilemap {
                    layer: LevelLayer::Layer2,
                    dimensions: LayerDimensions {
                        width: 1,
                        height: 1,
                    },
                    tiles: vec![8],
                },
                LevelPropertyEdit::LegacyHeader(LegacyHeaderEdit::SpritePalette(8)),
            ]),
            Err(LevelPropertyEditError::Header { command: 1, .. })
        ));
        assert_eq!(level, original);
    }

    #[test]
    fn portable_semantic_mode_edit_uses_reserved_fallback_and_retains_bounds() {
        let mut level = Level::default();
        level.header.legacy = LegacyLevelHeader::decode(&[0, 0xe3, 0, 0, 0]).unwrap();
        level
            .apply_property_edits(&[LevelPropertyEdit::LegacyHeader(
                LegacyHeaderEdit::LevelMode(0x12),
            )])
            .unwrap();
        assert_eq!(level.header.legacy.level_mode(), 0);
        assert_eq!(level.header.legacy.background_color(), 7);
        let canonical = level.clone();
        assert!(
            level
                .apply_property_edits(&[LevelPropertyEdit::LegacyHeader(
                    LegacyHeaderEdit::LevelMode(0x20),
                )])
                .is_err()
        );
        assert_eq!(level, canonical);
    }

    #[test]
    fn coordinate_expanded_field_and_dimensions_are_bounded() {
        let mut level = Level::default();
        level.layer1.raw_tilemap = vec![1];
        let original = level.clone();
        assert!(matches!(
            level.apply_property_edits(&[LevelPropertyEdit::SetTile {
                layer: LevelLayer::Layer1,
                dimensions: LayerDimensions {
                    width: 1,
                    height: 1,
                },
                coordinate: TileCoordinate { x: 1, y: 0 },
                tile: 2,
            }]),
            Err(LevelPropertyEditError::CoordinateOutOfRange { .. })
        ));
        assert_eq!(level, original);
        assert_eq!(
            level.apply_property_edits(&[LevelPropertyEdit::SetExpandedField {
                index: 0,
                value: 1,
            }]),
            Err(LevelPropertyEditError::MissingExpandedHeader { command: 0 })
        );
        assert!(matches!(
            level.apply_property_edits(&[LevelPropertyEdit::ReplaceTilemap {
                layer: LevelLayer::Layer1,
                dimensions: LayerDimensions {
                    width: usize::MAX,
                    height: 2,
                },
                tiles: vec![],
            }]),
            Err(LevelPropertyEditError::InvalidDimensions { .. })
        ));
        assert_eq!(level, original);
    }

    #[test]
    fn ordered_expanded_header_commands_can_enable_edit_and_disable() {
        let mut level = Level::default();
        level
            .apply_property_edits(&[
                LevelPropertyEdit::SetExpandedHeader(Some(ExpandedLevelHeader::default())),
                LevelPropertyEdit::SetExpandedField {
                    index: ExpandedLevelHeader::FIELD_COUNT - 1,
                    value: 0xabcd,
                },
            ])
            .unwrap();
        assert_eq!(
            level.header.expanded.unwrap().fields[ExpandedLevelHeader::FIELD_COUNT - 1],
            0xabcd
        );
        level
            .apply_property_edits(&[LevelPropertyEdit::SetExpandedHeader(None)])
            .unwrap();
        assert!(level.header.expanded.is_none());
    }
}
