use crate::{NativeLevelMap16Cache, NativeLevelMap16CacheError, NativeLevelMap16Layout};
use lm_level::ObjectStream;
use std::collections::BTreeSet;
use std::fmt;

pub const STANDARD_OBJECT_COMMANDS: usize = 0x40;

/// A compact expandable Map16 pattern for one standard-object command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardObjectPattern {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardObjectDefinitionSet {
    definitions: Vec<Option<StandardObjectPattern>>,
    extended: Vec<Option<StandardObjectPattern>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardObjectRenderReport {
    pub cache: NativeLevelMap16Cache,
    pub rendered_objects: usize,
    pub missing_commands: BTreeSet<u8>,
    pub missing_extended_objects: BTreeSet<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StandardObjectRenderError {
    InvalidCommand(u8),
    InvalidPatternShape {
        width: usize,
        height: usize,
        tiles: usize,
    },
    CoordinateOverflow,
    Cache(NativeLevelMap16CacheError),
}

impl fmt::Display for StandardObjectRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot render standard object: {self:?}")
    }
}

impl std::error::Error for StandardObjectRenderError {}

impl From<NativeLevelMap16CacheError> for StandardObjectRenderError {
    fn from(value: NativeLevelMap16CacheError) -> Self {
        Self::Cache(value)
    }
}

impl StandardObjectDefinitionSet {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            definitions: vec![None; STANDARD_OBJECT_COMMANDS],
            extended: vec![None; 0x100],
        }
    }

    /// Installs one command definition.
    ///
    /// # Errors
    ///
    /// Rejects commands above 0x3f and empty/inconsistent pattern shapes.
    pub fn set(
        &mut self,
        command: u8,
        pattern: StandardObjectPattern,
    ) -> Result<(), StandardObjectRenderError> {
        validate_pattern(&pattern)?;
        let slot = self
            .definitions
            .get_mut(usize::from(command))
            .ok_or(StandardObjectRenderError::InvalidCommand(command))?;
        *slot = Some(pattern);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, command: u8) -> Option<&StandardObjectPattern> {
        self.definitions.get(usize::from(command))?.as_ref()
    }

    /// Installs one command-zero extended-object definition.
    ///
    /// # Errors
    ///
    /// Applies the same exact shape validation as [`Self::set`].
    pub fn set_extended(
        &mut self,
        object: u8,
        pattern: StandardObjectPattern,
    ) -> Result<(), StandardObjectRenderError> {
        validate_pattern(&pattern)?;
        self.extended[usize::from(object)] = Some(pattern);
        Ok(())
    }

    #[must_use]
    pub fn get_extended(&self, object: u8) -> Option<&StandardObjectPattern> {
        self.extended[usize::from(object)].as_ref()
    }
}

impl Default for StandardObjectDefinitionSet {
    fn default() -> Self {
        Self::empty()
    }
}

fn validate_pattern(pattern: &StandardObjectPattern) -> Result<(), StandardObjectRenderError> {
    let expected = pattern.width.checked_mul(pattern.height).ok_or(
        StandardObjectRenderError::InvalidPatternShape {
            width: pattern.width,
            height: pattern.height,
            tiles: pattern.tiles.len(),
        },
    )?;
    if pattern.width == 0 || pattern.height == 0 || pattern.tiles.len() != expected {
        return Err(StandardObjectRenderError::InvalidPatternShape {
            width: pattern.width,
            height: pattern.height,
            tiles: pattern.tiles.len(),
        });
    }
    Ok(())
}

/// Expands every known standard object into a Lunar Magic-compatible cell cache.
///
/// Objects are processed in stream order, so later objects overwrite earlier cells. Commands with
/// no recovered definition are reported and do not fabricate tiles.
///
/// # Errors
///
/// Returns a typed coordinate/cache error without a partial result.
pub fn render_standard_object_stream(
    stream: &ObjectStream,
    definitions: &StandardObjectDefinitionSet,
    layout: NativeLevelMap16Layout,
    blank_tile: u16,
) -> Result<StandardObjectRenderReport, StandardObjectRenderError> {
    let mut cache = NativeLevelMap16Cache::filled(blank_tile);
    let mut rendered_objects = 0;
    let mut missing_commands = BTreeSet::new();
    let mut missing_extended_objects = BTreeSet::new();
    for placement in stream.native_placements() {
        let record = &stream.records[placement.record_index];
        let command = record.command_id();
        let pattern = if command == 0 {
            definitions.get_extended(record.parameter())
        } else {
            definitions.get(command)
        };
        let Some(pattern) = pattern else {
            if command == 0 {
                missing_extended_objects.insert(record.parameter());
            } else {
                missing_commands.insert(command);
            }
            continue;
        };
        let (major_span, minor_span) = if command == 0 {
            (1, 1)
        } else {
            (placement.major_span, placement.minor_span)
        };
        render_pattern(
            &mut cache, layout, placement, major_span, minor_span, pattern,
        )?;
        rendered_objects += 1;
    }
    Ok(StandardObjectRenderReport {
        cache,
        rendered_objects,
        missing_commands,
        missing_extended_objects,
    })
}

fn render_pattern(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    major_span: u8,
    minor_span: u8,
    pattern: &StandardObjectPattern,
) -> Result<(), StandardObjectRenderError> {
    for major_offset in 0..usize::from(major_span) {
        for minor_offset in 0..usize::from(minor_span) {
            let major = usize::from(placement.major)
                .checked_add(major_offset)
                .ok_or(StandardObjectRenderError::CoordinateOverflow)?;
            let minor = usize::from(placement.minor)
                .checked_add(minor_offset)
                .ok_or(StandardObjectRenderError::CoordinateOverflow)?;
            let (x, y) = if layout.vertical {
                (minor, major)
            } else {
                (major, minor)
            };
            cache.set(layout, x, y, pattern.tile_for(major_offset, minor_offset))?;
        }
    }
    Ok(())
}

/// Installs the shared command-zero single-tile definitions recovered from
/// `PlaceCommandMappedSingleTile`.
///
/// The 0x10–0x50 selector range uses page 0 below 0x23 and page 1 thereafter.
///
/// # Errors
///
/// Returns a definition error if the recovered single-tile pattern cannot be installed.
pub fn install_lunar_magic_shared_extended_objects(
    definitions: &mut StandardObjectDefinitionSet,
) -> Result<(), StandardObjectRenderError> {
    const TILES: [u8; 0x41] = [
        0x1f, 0x22, 0x24, 0x42, 0x43, 0x27, 0x29, 0x25, 0x6e, 0x6f, 0x70, 0x71, 0x72, 0x45, 0x46,
        0x47, 0x48, 0x36, 0x37, 0x11, 0x12, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
        0x29, 0x1d, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x25, 0x26, 0x27, 0x28, 0x2a, 0xde, 0xe0, 0xe2,
        0xe4, 0xec, 0xed, 0x2c, 0x25, 0x2d, 0x8a, 0x38, 0xe9, 0x10, 0x85, 0x00, 0xe0, 0x18, 0x90,
        0x2c, 0xe0, 0x1d, 0xb0, 0x28,
    ];
    for (selector, tile) in (0x10_u8..=0x50).zip(TILES) {
        definitions.set_extended(
            selector,
            StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![u16::from(tile) + if selector < 0x23 { 0 } else { 0x100 }],
            },
        )?;
    }
    Ok(())
}

impl StandardObjectPattern {
    fn tile_for(&self, x: usize, y: usize) -> u16 {
        let source_x = expandable_axis_index(x, self.width);
        let source_y = expandable_axis_index(y, self.height);
        self.tiles[source_y * self.width + source_x]
    }
}

fn expandable_axis_index(position: usize, pattern_len: usize) -> usize {
    match pattern_len {
        1 => 0,
        2 => position.min(1),
        _ if position == 0 => 0,
        _ => 1 + (position - 1) % (pattern_len - 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{ObjectRecord, ObjectStream};

    fn layout() -> NativeLevelMap16Layout {
        NativeLevelMap16Layout {
            width: 32,
            height: 16,
            page_stride: 0x1b0,
            base_cell: 0,
            vertical: false,
        }
    }

    #[test]
    fn expandable_pattern_repeats_interior() {
        let pattern = StandardObjectPattern {
            width: 3,
            height: 3,
            tiles: (1..=9).collect(),
        };
        assert_eq!(pattern.tile_for(0, 0), 1);
        assert_eq!(pattern.tile_for(1, 1), 5);
        assert_eq!(pattern.tile_for(4, 3), 5);
    }

    #[test]
    fn stream_order_overwrites_and_unknown_commands_are_reported() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        definitions
            .set(
                1,
                StandardObjectPattern {
                    width: 1,
                    height: 1,
                    tiles: vec![0x123],
                },
            )
            .unwrap();
        definitions
            .set(
                3,
                StandardObjectPattern {
                    width: 1,
                    height: 1,
                    tiles: vec![0x456],
                },
            )
            .unwrap();
        let stream = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0, 0x10, 0x11]).unwrap(),
                ObjectRecord::new(vec![0, 0x30, 0]).unwrap(),
                ObjectRecord::new(vec![1, 0x20, 0]).unwrap(),
            ],
        };
        let report = render_standard_object_stream(&stream, &definitions, layout(), 0x25).unwrap();
        assert_eq!(report.rendered_objects, 2);
        assert_eq!(report.missing_commands, BTreeSet::from([2]));
        assert!(report.missing_extended_objects.is_empty());
        let first = NativeLevelMap16Cache::cell_index(layout(), 0, 0);
        assert_eq!(report.cache.cells()[first], 0x456);
        assert_eq!(
            report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 1, 0)],
            0x123
        );
    }

    #[test]
    fn recovered_extended_object_lookup_uses_the_native_page_boundary() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_extended_objects(&mut definitions).unwrap();
        assert_eq!(definitions.get_extended(0x10).unwrap().tiles, [0x1f]);
        assert_eq!(definitions.get_extended(0x22).unwrap().tiles, [0x37]);
        assert_eq!(definitions.get_extended(0x23).unwrap().tiles, [0x111]);
        assert_eq!(definitions.get_extended(0x50).unwrap().tiles, [0x128]);
        assert!(definitions.get_extended(0x0f).is_none());
        assert!(definitions.get_extended(0x51).is_none());
    }
}
