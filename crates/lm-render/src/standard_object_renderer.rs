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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardObjectRenderReport {
    pub cache: NativeLevelMap16Cache,
    pub rendered_objects: usize,
    pub missing_commands: BTreeSet<u8>,
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
}

impl Default for StandardObjectDefinitionSet {
    fn default() -> Self {
        Self::empty()
    }
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
    for placement in stream.native_placements() {
        let record = &stream.records[placement.record_index];
        let command = record.command_id();
        let Some(pattern) = definitions.get(command) else {
            missing_commands.insert(command);
            continue;
        };
        render_pattern(&mut cache, layout, placement, pattern)?;
        rendered_objects += 1;
    }
    Ok(StandardObjectRenderReport {
        cache,
        rendered_objects,
        missing_commands,
    })
}

fn render_pattern(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    pattern: &StandardObjectPattern,
) -> Result<(), StandardObjectRenderError> {
    for major_offset in 0..usize::from(placement.major_span) {
        for minor_offset in 0..usize::from(placement.minor_span) {
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
        let first = NativeLevelMap16Cache::cell_index(layout(), 0, 0);
        assert_eq!(report.cache.cells()[first], 0x456);
        assert_eq!(
            report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 1, 0)],
            0x123
        );
    }
}
