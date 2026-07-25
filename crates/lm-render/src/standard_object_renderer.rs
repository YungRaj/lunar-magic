use crate::{NativeLevelMap16Cache, NativeLevelMap16CacheError, NativeLevelMap16Layout};
use lm_level::ObjectStream;
use std::collections::BTreeSet;
use std::fmt;

pub const STANDARD_OBJECT_COMMANDS: usize = 78;
const SHARED_EXTENDED_TILES: [u8; 0x41] = [
    0x1f, 0x22, 0x24, 0x42, 0x43, 0x27, 0x29, 0x25, 0x6e, 0x6f, 0x70, 0x71, 0x72, 0x45, 0x46, 0x47,
    0x48, 0x36, 0x37, 0x11, 0x12, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x29, 0x1d,
    0x1f, 0x20, 0x21, 0x22, 0x23, 0x25, 0x26, 0x27, 0x28, 0x2a, 0xde, 0xe0, 0xe2, 0xe4, 0xec, 0xed,
    0x2c, 0x25, 0x2d, 0x8a, 0x38, 0xe9, 0x10, 0x85, 0x00, 0xe0, 0x18, 0x90, 0x2c, 0xe0, 0x1d, 0xb0,
    0x28,
];
const SHARED_SLOT_002_END_TILES: [u8; 8] = [0x3b, 0x3c, 0x3b, 0x3f, 0x3b, 0x3c, 0x3b, 0x3f];
const SHARED_SLOT_002_FILL_TILES: [u8; 8] = [0x3d, 0x3e, 0x3d, 0x3e, 0x3d, 0x3e, 0x3d, 0x3e];
const SHARED_SLOT_010_TILES: [u8; 16] = [
    0x05, 0x06, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0x85, 0x00, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_011_TOP_TILES: [u8; 28] = [
    0x00, 0x01, 0x04, 0x08, 0x02, 0x03, 0x05, 0x0b, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0x85, 0x00,
    0x85, 0x02, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a, 0x85, 0x01, 0x8a, 0x38,
];
const SHARED_SLOT_011_REMAINDER_TILES: [u8; 28] = [
    0x02, 0x03, 0x05, 0x0b, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0x85, 0x00, 0x85, 0x02, 0xa5, 0x59,
    0x4a, 0x4a, 0x4a, 0x4a, 0x85, 0x01, 0x8a, 0x38, 0xe9, 0x17, 0xaa, 0x20,
];
const SHARED_SLOT_014_BASE_TILES: [u8; 16] = [
    0x0a, 0x0c, 0xa4, 0x57, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a, 0x85, 0x00, 0xa5, 0x59, 0x29, 0x0f,
];
const SHARED_SLOT_014_TOP_ON_08: [u8; 16] = [
    0x07, 0x09, 0x1a, 0x19, 0x85, 0x0c, 0xb7, 0x6b, 0xc9, 0x08, 0xd0, 0x07, 0xbf, 0xd5, 0xb4, 0x0d,
];
const SHARED_SLOT_014_TOP_ON_0E: [u8; 16] = [
    0x1a, 0x19, 0x85, 0x0c, 0xb7, 0x6b, 0xc9, 0x08, 0xd0, 0x07, 0xbf, 0xd5, 0xb4, 0x0d, 0x50, 0x50,
];
const SHARED_SLOT_014_BOTTOM_ON_0E: [u8; 16] = [
    0x0d, 0x0f, 0x1c, 0x1b, 0x85, 0x0c, 0xb7, 0x6b, 0xc9, 0x0e, 0xd0, 0x07, 0xbf, 0xfa, 0xb4, 0x0d,
];
const SHARED_SLOT_014_BOTTOM_ON_08: [u8; 16] = [
    0x1c, 0x1b, 0x85, 0x0c, 0xb7, 0x6b, 0xc9, 0x0e, 0xd0, 0x07, 0xbf, 0xfa, 0xb4, 0x0d, 0x60, 0x45,
];
const SHARED_SLOT_017_START_TILES: [u8; 16] = [
    0x73, 0x7a, 0x85, 0x88, 0xc3, 0x74, 0x7b, 0x86, 0x89, 0xc3, 0x79, 0x80, 0x87, 0x8e, 0xc3, 0xa4,
];
const SHARED_SLOT_017_MIDDLE_TILES: [u8; 16] = [
    0x74, 0x7b, 0x86, 0x89, 0xc3, 0x79, 0x80, 0x87, 0x8e, 0xc3, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f,
];
const SHARED_SLOT_017_END_TILES: [u8; 16] = [
    0x79, 0x80, 0x87, 0x8e, 0xc3, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0x85, 0x00, 0xa5, 0x59, 0x4a,
];
const SHARED_SLOT_018_TILES: [[u8; 7]; 4] = [
    [0x07, 0x0a, 0x0a, 0x08, 0x0a, 0x0a, 0x09],
    [0x81, 0x82, 0x83, 0x81, 0x82, 0x83, 0x81],
    [0x81, 0x25, 0x84, 0x81, 0x25, 0x84, 0x81],
    [0x81, 0x25, 0x84, 0x81, 0x25, 0x84, 0x81],
];
const SHARED_SLOT_019_TILES: [u8; 16] = [
    0x93, 0x9c, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0x85, 0x00, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_020_TOP_TILES: [u8; 16] = [
    0x94, 0x8f, 0x9d, 0x98, 0x95, 0x90, 0x9e, 0x99, 0x8f, 0x8f, 0x98, 0x98, 0x90, 0x90, 0x99, 0x99,
];
const SHARED_SLOT_020_REMAINDER_TILES: [u8; 16] = [
    0x8f, 0x8f, 0x98, 0x98, 0x90, 0x90, 0x99, 0x99, 0xa4, 0x57, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_021_TILES: [u8; 16] = [
    0xc4, 0xc5, 0xc7, 0xec, 0xed, 0xc6, 0xc7, 0xee, 0x59, 0x5a, 0xef, 0xc7, 0xee, 0x59, 0x5b, 0x5c,
];
const SHARED_SLOT_026_EVEN_TILES: [u8; 16] = [
    0xbd, 0xbf, 0xbe, 0xc0, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0xaa, 0xa5, 0x59, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_026_ODD_TILES: [u8; 16] = [
    0xbe, 0xc0, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0xaa, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a, 0x85,
];
const SHARED_SLOT_029_TOP_TILES: [u8; 16] = [
    0x5f, 0x5e, 0x10, 0x0f, 0x60, 0x5d, 0xc5, 0xc4, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0xaa, 0xa5,
];
const SHARED_SLOT_029_REMAINDER_TILES: [u8; 16] = [
    0x60, 0x5d, 0xc5, 0xc4, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0xaa, 0xa5, 0x59, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_030_TILES: [[u8; 16]; 6] = [
    [0xb4; 16],
    [0xb4; 16],
    [0xb4; 16],
    [
        0xb4, 0xb4, 0xb4, 0xb4, 0xb5, 0xb3, 0xb5, 0xb3, 0xb3, 0xb4, 0xb4, 0xb5, 0xb3, 0xb4, 0xb4,
        0xb4,
    ],
    [
        0xb4, 0xb5, 0xb3, 0xb5, 0xb6, 0xb1, 0xb6, 0xb1, 0xb1, 0xb3, 0xb5, 0xb6, 0xb1, 0xb3, 0xb5,
        0xb3,
    ],
    [
        0xb5, 0xb6, 0xb1, 0xb6, 0x25, 0x25, 0x25, 0x25, 0x25, 0xb1, 0xb6, 0x25, 0x25, 0xb1, 0xb6,
        0xb1,
    ],
];
const SHARED_SLOT_033_START_TILES: [u8; 4] = [0xce, 0xd1, 0xcf, 0xd0];
const SHARED_SLOT_033_END_TILES: [u8; 4] = [0xf3, 0xf6, 0xf4, 0xf5];
const SHARED_SLOT_034_TILES: [u8; 16] = [
    0x5a, 0x59, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0x85, 0x00, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_035_TILES: [u8; 16] = [
    0x5b, 0x5c, 0x53, 0xa4, 0x57, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a, 0x85, 0x00, 0xa5, 0x59, 0x29,
];
const SHARED_SLOT_036_TILES: [[u8; 3]; 3] =
    [[0x5d, 0x5e, 0x5f], [0x60, 0x61, 0x62], [0x63, 0x64, 0x65]];
const SHARED_SLOT_041_TILES: [u8; 16] = [
    0x0c, 0x0d, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0x85, 0x00, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_042_TILES: [u8; 16] = [
    0x92, 0x93, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0x85, 0x00, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_043_TILES: [u8; 16] = [
    0x90, 0x91, 0xa2, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0xaa, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_052_TOP_TILES: [u8; 16] = [
    0x5a, 0x5b, 0x5b, 0x5b, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0xaa, 0xa5, 0x59, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_052_REMAINDER_TILES: [u8; 16] = [
    0x5b, 0x5b, 0xa4, 0x57, 0xa5, 0x59, 0x29, 0x0f, 0xaa, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a, 0x85,
];
const SHARED_SLOT_056_BODY_TILES: [u8; 16] = [
    0x50, 0x50, 0x51, 0x51, 0x4d, 0x50, 0x4f, 0x51, 0xa4, 0x57, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a,
];
const SHARED_SLOT_056_END_TILES: [u8; 16] = [
    0x4d, 0x50, 0x4f, 0x51, 0xa4, 0x57, 0xa5, 0x59, 0x4a, 0x4a, 0x4a, 0x4a, 0x85, 0x00, 0xa5, 0x59,
];
const SHARED_SLOT_008_TILES: [[[u8; 3]; 3]; 2] = [
    [[0x2f, 0x25, 0x32], [0x30, 0x25, 0x33], [0x31, 0x25, 0x34]],
    [[0x39, 0x25, 0x3c], [0x3a, 0x25, 0x3d], [0x3b, 0x25, 0x3e]],
];

/// A compact expandable Map16 pattern for one standard-object command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardObjectPattern {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObjectExtent {
    ParameterNibbles,
    HighNibbleByOne,
    TwoByLowNibble,
    OneByLowNibble,
    ThreeByParameterByte,
    FixedOne,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AxisExpansion {
    PreserveEdges,
    Clamp,
    FinalEdge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeRenderer {
    NoOp,
    Pattern,
    SharedSlot001,
    SharedSlot002,
    SharedSlot008,
    SharedSlot010,
    SharedSlot011,
    SharedSlot014,
    SharedSlot017,
    SharedSlot018,
    SharedSlot019,
    SharedSlot020,
    SharedSlot021,
    SharedSlot022,
    SharedSlot023,
    SharedSlot026,
    SharedSlot027,
    SharedSlot029,
    SharedSlot030,
    SharedSlot031,
    SharedSlot033,
    SharedSlot034,
    SharedSlot035,
    SharedSlot036,
    SharedSlot040,
    SharedSlot041,
    SharedSlot042,
    SharedSlot043,
    SharedSlot044,
    SharedSlot045,
    SharedSlot046,
    SharedSlot047,
    SharedSlot048,
    SharedSlot049,
    SharedSlot051,
    SharedSlot052,
    SharedSlot056,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StandardObjectDefinition {
    pattern: StandardObjectPattern,
    extent: ObjectExtent,
    major_expansion: AxisExpansion,
    minor_expansion: AxisExpansion,
    renderer: NativeRenderer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardObjectDefinitionSet {
    definitions: Vec<Option<StandardObjectDefinition>>,
    handler_definitions: Vec<Option<StandardObjectDefinition>>,
    extended: Vec<Option<StandardObjectDefinition>>,
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
            handler_definitions: vec![None; STANDARD_OBJECT_COMMANDS],
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
        *slot = Some(StandardObjectDefinition {
            pattern,
            extent: ObjectExtent::ParameterNibbles,
            major_expansion: AxisExpansion::PreserveEdges,
            minor_expansion: AxisExpansion::PreserveEdges,
            renderer: NativeRenderer::Pattern,
        });
        Ok(())
    }

    #[must_use]
    pub fn get(&self, command: u8) -> Option<&StandardObjectPattern> {
        Some(
            &self
                .definitions
                .get(usize::from(command))?
                .as_ref()?
                .pattern,
        )
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
        self.extended[usize::from(object)] = Some(StandardObjectDefinition {
            pattern,
            extent: ObjectExtent::FixedOne,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::Pattern,
        });
        Ok(())
    }

    #[must_use]
    pub fn get_extended(&self, object: u8) -> Option<&StandardObjectPattern> {
        Some(&self.extended[usize::from(object)].as_ref()?.pattern)
    }

    fn definition(&self, command: u8) -> Option<&StandardObjectDefinition> {
        self.definitions.get(usize::from(command))?.as_ref()
    }

    fn extended_definition(&self, object: u8) -> Option<&StandardObjectDefinition> {
        self.extended[usize::from(object)].as_ref()
    }

    fn handler_definition(&self, handler: u8) -> Option<&StandardObjectDefinition> {
        self.handler_definitions.get(usize::from(handler))?.as_ref()
    }

    fn set_native(
        &mut self,
        command: u8,
        definition: StandardObjectDefinition,
    ) -> Result<(), StandardObjectRenderError> {
        validate_pattern(&definition.pattern)?;
        let slot = self
            .definitions
            .get_mut(usize::from(command))
            .ok_or(StandardObjectRenderError::InvalidCommand(command))?;
        *slot = Some(definition);
        Ok(())
    }

    fn alias(&mut self, source: u8, target: u8) -> Result<(), StandardObjectRenderError> {
        let definition = self
            .definition(source)
            .cloned()
            .ok_or(StandardObjectRenderError::InvalidCommand(source))?;
        let slot = self
            .handler_definitions
            .get_mut(usize::from(target))
            .ok_or(StandardObjectRenderError::InvalidCommand(target))?;
        *slot = Some(definition);
        Ok(())
    }

    fn set_handler(
        &mut self,
        handler: u8,
        definition: StandardObjectDefinition,
    ) -> Result<(), StandardObjectRenderError> {
        validate_pattern(&definition.pattern)?;
        let slot = self
            .handler_definitions
            .get_mut(usize::from(handler))
            .ok_or(StandardObjectRenderError::InvalidCommand(handler))?;
        *slot = Some(definition);
        Ok(())
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
    render_standard_object_stream_with_map(stream, definitions, None, layout, blank_tile)
}

/// Renders standard objects after resolving each record ID through Lunar Magic's active
/// 64-entry family-specific handler map.
///
/// # Errors
///
/// Returns the same typed coordinate/cache errors as [`render_standard_object_stream`].
pub fn render_mapped_standard_object_stream(
    stream: &ObjectStream,
    definitions: &StandardObjectDefinitionSet,
    handler_map: &[u8; 64],
    layout: NativeLevelMap16Layout,
    blank_tile: u16,
) -> Result<StandardObjectRenderReport, StandardObjectRenderError> {
    render_standard_object_stream_with_map(
        stream,
        definitions,
        Some(handler_map),
        layout,
        blank_tile,
    )
}

fn render_standard_object_stream_with_map(
    stream: &ObjectStream,
    definitions: &StandardObjectDefinitionSet,
    handler_map: Option<&[u8; 64]>,
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
        let resolved_command = handler_map
            .and_then(|map| map.get(usize::from(command)).copied())
            .unwrap_or(command);
        let definition = if command == 0 {
            definitions.extended_definition(record.parameter())
        } else if handler_map.is_some() {
            definitions.handler_definition(resolved_command)
        } else {
            definitions.definition(resolved_command)
        };
        let Some(definition) = definition else {
            if command == 0 {
                missing_extended_objects.insert(record.parameter());
            } else {
                missing_commands.insert(command);
            }
            continue;
        };
        render_definition(
            &mut cache,
            layout,
            placement,
            command,
            record.parameter(),
            definition,
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

fn render_definition(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    command: u8,
    parameter: u8,
    definition: &StandardObjectDefinition,
) -> Result<(), StandardObjectRenderError> {
    if let Some(result) = dispatch_native_renderer(
        cache,
        layout,
        placement,
        command,
        parameter,
        definition.renderer,
    ) {
        return result;
    }
    let (major_span, minor_span) = match definition.extent {
        ObjectExtent::ParameterNibbles => (
            usize::from(placement.major_span),
            usize::from(placement.minor_span),
        ),
        ObjectExtent::HighNibbleByOne => (usize::from(placement.major_span), 1),
        ObjectExtent::TwoByLowNibble => (2, usize::from(placement.minor_span)),
        ObjectExtent::OneByLowNibble => (1, usize::from(placement.minor_span)),
        ObjectExtent::ThreeByParameterByte => (3, usize::from(parameter) + 1),
        ObjectExtent::FixedOne => (1, 1),
    };
    render_pattern(cache, layout, placement, major_span, minor_span, definition)
}

fn dispatch_native_renderer(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    command: u8,
    parameter: u8,
    renderer: NativeRenderer,
) -> Option<Result<(), StandardObjectRenderError>> {
    let result = match renderer {
        NativeRenderer::Pattern => return None,
        NativeRenderer::NoOp => Ok(()),
        NativeRenderer::SharedSlot001 => {
            render_shared_slot_001(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot002 => {
            render_shared_slot_002(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot008 => {
            render_shared_slot_008(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot010 => {
            render_shared_slot_010(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot011 => {
            render_shared_slot_011(cache, layout, placement, command, parameter)
        }
        NativeRenderer::SharedSlot014 => {
            render_shared_slot_014(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot017 => {
            render_shared_slot_017(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot018 => {
            render_shared_slot_018(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot019 => {
            render_shared_slot_019(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot020 => {
            render_shared_slot_020(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot021 => {
            render_shared_slot_021(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot022 => {
            render_shared_slot_022(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot023 => {
            render_shared_slot_023(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot026 => {
            render_shared_slot_026(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot027 => {
            render_shared_slot_027(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot029 => {
            render_shared_slot_029(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot030 => {
            render_shared_slot_030(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot031 => {
            render_shared_slot_031(cache, layout, placement, parameter)
        }
        _ => {
            return Some(dispatch_native_renderer_high(
                cache, layout, placement, parameter, renderer,
            ));
        }
    };
    Some(result)
}

fn dispatch_native_renderer_high(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
    renderer: NativeRenderer,
) -> Result<(), StandardObjectRenderError> {
    match renderer {
        NativeRenderer::SharedSlot033 => {
            render_shared_slot_033(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot034 => {
            render_shared_slot_034(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot035 => {
            render_shared_slot_035(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot036 => {
            render_shared_slot_036(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot040 => {
            render_shared_slot_040(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot041 => {
            render_shared_slot_041(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot042 => {
            render_shared_slot_042(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot043 => {
            render_shared_slot_043(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot044 => {
            render_shared_slot_044(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot045 => {
            render_shared_slot_045(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot046 => {
            render_shared_slot_046(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot047 => {
            render_shared_slot_047(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot048 => {
            render_shared_slot_048(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot049 => {
            render_shared_slot_049(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot051 => {
            render_shared_slot_051(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot052 => {
            render_shared_slot_052(cache, layout, placement, parameter)
        }
        NativeRenderer::SharedSlot056 => {
            render_shared_slot_056(cache, layout, placement, parameter)
        }
        _ => unreachable!("low native renderer routed to high dispatcher"),
    }
}

fn render_shared_slot_027(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    for major_offset in 0..=usize::from(parameter >> 4) {
        let pair = if major_offset & 1 == 0 {
            let existing_left = get_placement_cell(cache, layout, placement, major_offset, 0)?;
            let existing_right = get_placement_cell(cache, layout, placement, major_offset, 1)?;
            if existing_left.to_le_bytes()[0] == 0x0e {
                [0x10b, existing_right & 0xff00 | 0x0c]
            } else {
                [
                    existing_left & 0xff00 | 0xb9,
                    existing_right & 0xff00 | 0xba,
                ]
            }
        } else {
            [0x0bb, 0x0bc]
        };
        for (minor_offset, tile) in pair.into_iter().enumerate() {
            set_placement_cell(cache, layout, placement, major_offset, minor_offset, tile)?;
        }
    }
    Ok(())
}

fn render_shared_slot_026(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let variant = usize::from(parameter & 0x0f);
    for major_offset in 0..=usize::from(parameter >> 4) {
        let tile = if major_offset & 1 == 0 {
            let existing = get_placement_cell(cache, layout, placement, major_offset, 0)?;
            let low = existing.to_le_bytes()[0];
            if variant == 1 && (low == 0xb6 || low == 0xb1) {
                u16::from(low + 1)
            } else if variant == 0 && low == 0x0e {
                0x10d
            } else {
                u16::from(SHARED_SLOT_026_EVEN_TILES[variant])
            }
        } else {
            u16::from(SHARED_SLOT_026_ODD_TILES[variant])
        };
        set_placement_cell(cache, layout, placement, major_offset, 0, tile)?;
    }
    Ok(())
}

fn render_shared_slot_021(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let rows = usize::from(parameter >> 4) + 1;
    let mut table_index = 0;
    let mut row_width = 2;
    for major_offset in 0..rows {
        for column in 0..row_width {
            if table_index > 15 {
                table_index -= 5;
            }
            set_placement_cell_signed(
                cache,
                layout,
                placement,
                isize::try_from(major_offset)
                    .map_err(|_| StandardObjectRenderError::CoordinateOverflow)?,
                isize::try_from(column)
                    .map_err(|_| StandardObjectRenderError::CoordinateOverflow)?
                    - isize::try_from(major_offset)
                        .map_err(|_| StandardObjectRenderError::CoordinateOverflow)?,
                u16::from(SHARED_SLOT_021_TILES[table_index]) + 0x100,
            )?;
            table_index += 1;
        }
        row_width = if table_index == 2 { 4 } else { 5 };
    }
    let rows = isize::try_from(rows).map_err(|_| StandardObjectRenderError::CoordinateOverflow)?;
    set_placement_cell_signed(cache, layout, placement, rows, 1 - rows, 0x1eb)
}

fn render_shared_slot_022(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let top_left = adapt_three_way(get_placement_cell(cache, layout, placement, 0, 0)?, 0x1aa);
    set_placement_cell(cache, layout, placement, 0, 0, top_left)?;
    let existing_top_right = get_placement_cell(cache, layout, placement, 0, 1)?;
    let top_right = existing_top_right & 0xff00 | adapt_two_way(existing_top_right, 0x0a1);
    set_placement_cell(cache, layout, placement, 0, 1, top_right)?;

    let widening_rows = usize::from(parameter & 0x0f);
    let mut fill_width = 1;
    for row in 1..=widening_rows {
        let major = signed_offset(row)?;
        let left_minor = -major;
        let left = adapt_three_way(
            get_placement_cell_signed(cache, layout, placement, major, left_minor)?,
            0x1aa,
        );
        set_placement_cell_signed(cache, layout, placement, major, left_minor, left)?;
        set_placement_cell_signed(cache, layout, placement, major, left_minor + 1, 0x1e2)?;
        for fill in 0..fill_width {
            set_placement_cell_signed(
                cache,
                layout,
                placement,
                major,
                left_minor + 2 + signed_offset(fill)?,
                0x03f,
            )?;
        }
        let right_minor = left_minor + 2 + signed_offset(fill_width)?;
        let right = adapt_two_way(
            get_placement_cell_signed(cache, layout, placement, major, right_minor)?,
            0x0a6,
        );
        set_placement_cell_signed(cache, layout, placement, major, right_minor, right)?;
        fill_width += 2;
    }

    let bottom_major = signed_offset(widening_rows + 1)?;
    let mut bottom_minor = -signed_offset(widening_rows)?;
    render_adaptive_fill_row(
        cache,
        layout,
        placement,
        bottom_major,
        bottom_minor,
        fill_width,
        true,
    )?;
    for extra in 0..usize::from(parameter >> 4) {
        bottom_minor += 1;
        render_adaptive_fill_row(
            cache,
            layout,
            placement,
            bottom_major + 1 + signed_offset(extra)?,
            bottom_minor,
            fill_width,
            false,
        )?;
    }
    Ok(())
}

fn render_shared_slot_023(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let existing_top_left = get_placement_cell(cache, layout, placement, 0, 0)?;
    let top_left = existing_top_left & 0xff00 | adapt_two_way(existing_top_left, 0x0af);
    set_placement_cell(cache, layout, placement, 0, 0, top_left)?;
    let top_right = adapt_three_way(get_placement_cell(cache, layout, placement, 0, 1)?, 0x1af);
    set_placement_cell(cache, layout, placement, 0, 1, top_right)?;

    let widening_rows = usize::from(parameter & 0x0f);
    let mut fill_width = 1;
    for row in 1..=widening_rows {
        let major = signed_offset(row)?;
        let left_minor = -major;
        let left = adapt_two_way(
            get_placement_cell_signed(cache, layout, placement, major, left_minor)?,
            0x0a9,
        );
        set_placement_cell_signed(cache, layout, placement, major, left_minor, left)?;
        for fill in 0..fill_width {
            set_placement_cell_signed(
                cache,
                layout,
                placement,
                major,
                left_minor + 1 + signed_offset(fill)?,
                0x03f,
            )?;
        }
        let separator_minor = left_minor + 1 + signed_offset(fill_width)?;
        set_placement_cell_signed(cache, layout, placement, major, separator_minor, 0x1e4)?;
        let right_minor = separator_minor + 1;
        let right = adapt_three_way(
            get_placement_cell_signed(cache, layout, placement, major, right_minor)?,
            0x1af,
        );
        set_placement_cell_signed(cache, layout, placement, major, right_minor, right)?;
        fill_width += 2;
    }

    let bottom_major = signed_offset(widening_rows + 1)?;
    let mut bottom_minor = -signed_offset(widening_rows)?;
    render_handler_23_fill_row(
        cache,
        layout,
        placement,
        bottom_major,
        bottom_minor,
        fill_width,
        0x1f9,
    )?;
    for extra in 0..usize::from(parameter >> 4) {
        bottom_minor += 1;
        render_handler_23_fill_row(
            cache,
            layout,
            placement,
            bottom_major + 1 + signed_offset(extra)?,
            bottom_minor,
            fill_width,
            0x0ac,
        )?;
    }
    Ok(())
}

fn render_handler_23_fill_row(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    major: isize,
    left_minor: isize,
    fill_width: usize,
    right_base: u16,
) -> Result<(), StandardObjectRenderError> {
    let left = adapt_two_way(
        get_placement_cell_signed(cache, layout, placement, major, left_minor)?,
        0x0a9,
    );
    set_placement_cell_signed(cache, layout, placement, major, left_minor, left)?;
    for fill in 0..fill_width {
        set_placement_cell_signed(
            cache,
            layout,
            placement,
            major,
            left_minor + 1 + signed_offset(fill)?,
            0x03f,
        )?;
    }
    let right_minor = left_minor + 1 + signed_offset(fill_width)?;
    let right = adapt_two_way(
        get_placement_cell_signed(cache, layout, placement, major, right_minor)?,
        right_base,
    );
    set_placement_cell_signed(cache, layout, placement, major, right_minor, right)
}

fn render_adaptive_fill_row(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    major: isize,
    left_minor: isize,
    fill_width: usize,
    bottom_cap: bool,
) -> Result<(), StandardObjectRenderError> {
    let existing_left = get_placement_cell_signed(cache, layout, placement, major, left_minor)?;
    let left = if bottom_cap {
        adapt_three_way(existing_left, 0x1f7)
    } else {
        adapt_two_way(existing_left, 0x0a3)
    };
    set_placement_cell_signed(cache, layout, placement, major, left_minor, left)?;
    for fill in 0..fill_width {
        set_placement_cell_signed(
            cache,
            layout,
            placement,
            major,
            left_minor + 1 + signed_offset(fill)?,
            0x03f,
        )?;
    }
    let right_minor = left_minor + 1 + signed_offset(fill_width)?;
    let right = adapt_two_way(
        get_placement_cell_signed(cache, layout, placement, major, right_minor)?,
        0x0a6,
    );
    set_placement_cell_signed(cache, layout, placement, major, right_minor, right)
}

fn adapt_three_way(existing: u16, base: u16) -> u16 {
    base + match existing.to_le_bytes()[0] {
        0x3f => 1,
        0x01 => 3,
        0x03 => 4,
        _ => 0,
    }
}

fn adapt_two_way(existing: u16, base: u16) -> u16 {
    base + match existing.to_le_bytes()[0] {
        0x25 => 0,
        0x3f => 1,
        _ => 2,
    }
}

fn signed_offset(value: usize) -> Result<isize, StandardObjectRenderError> {
    isize::try_from(value).map_err(|_| StandardObjectRenderError::CoordinateOverflow)
}

fn render_shared_slot_030(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    for strip in 0..=usize::from(parameter) {
        for (major_offset, row) in SHARED_SLOT_030_TILES.iter().enumerate() {
            for (column, &low) in row.iter().enumerate() {
                let minor_offset = strip
                    .checked_mul(16)
                    .and_then(|offset| offset.checked_add(column))
                    .ok_or(StandardObjectRenderError::CoordinateOverflow)?;
                let existing =
                    get_placement_cell(cache, layout, placement, major_offset, minor_offset)?;
                set_placement_cell(
                    cache,
                    layout,
                    placement,
                    major_offset,
                    minor_offset,
                    existing & 0xff00 | u16::from(low),
                )?;
            }
        }
    }
    Ok(())
}

fn render_shared_slot_031(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    set_placement_pair(cache, layout, placement, 0, [0x161, 0x162])?;
    for major_offset in 1..=usize::from(parameter >> 4) {
        set_placement_pair(cache, layout, placement, major_offset, [0x163, 0x164])?;
    }
    Ok(())
}

fn render_shared_slot_034(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let tile = u16::from(SHARED_SLOT_034_TILES[usize::from(parameter >> 4)]) + 0x100;
    for minor_offset in 0..=usize::from(parameter & 0x0f) {
        set_placement_cell(cache, layout, placement, 0, minor_offset, tile)?;
    }
    Ok(())
}

fn render_shared_slot_033(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let variant = usize::from(parameter & 3);
    let start = u16::from(SHARED_SLOT_033_START_TILES[variant]) + 0x100;
    let end = u16::from(SHARED_SLOT_033_END_TILES[variant]) + 0x100;
    let expands_right = parameter & 2 != 0;
    set_placement_cell(cache, layout, placement, 0, 0, start)?;
    let rows = usize::from(parameter >> 4) + 1;
    for row in 1..=rows {
        let major = signed_offset(row)?;
        let direction = if expands_right { 1 } else { -1 };
        for fill in 0..row.saturating_sub(1) {
            set_placement_cell_signed(
                cache,
                layout,
                placement,
                major,
                direction * signed_offset(fill)?,
                0x03f,
            )?;
        }
        let end_minor = direction * signed_offset(row.saturating_sub(1))?;
        set_placement_cell_signed(cache, layout, placement, major, end_minor, end)?;
        if row < rows {
            set_placement_cell_signed(
                cache,
                layout,
                placement,
                major,
                direction * signed_offset(row)?,
                start,
            )?;
        }
    }
    Ok(())
}

fn render_shared_slot_035(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let tile = u16::from(SHARED_SLOT_035_TILES[usize::from(parameter & 0x0f)]) + 0x100;
    for major_offset in 0..=usize::from(parameter >> 4) {
        set_placement_cell(cache, layout, placement, major_offset, 0, tile)?;
    }
    Ok(())
}

fn render_shared_slot_036(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let encoded_middle = usize::from(parameter & 0x0f);
    let middle_count = if encoded_middle == 0 {
        0xff
    } else {
        encoded_middle.saturating_sub(1)
    };
    let row_count = usize::from(parameter >> 4) + 1;
    for major_offset in 0..row_count {
        let row_kind = if major_offset == 0 {
            0
        } else if major_offset + 1 == row_count {
            2
        } else {
            1
        };
        set_placement_cell(
            cache,
            layout,
            placement,
            major_offset,
            0,
            u16::from(SHARED_SLOT_036_TILES[row_kind][0]) + 0x100,
        )?;
        for minor_offset in 0..middle_count {
            set_placement_cell(
                cache,
                layout,
                placement,
                major_offset,
                minor_offset + 1,
                u16::from(SHARED_SLOT_036_TILES[row_kind][1]) + 0x100,
            )?;
        }
        set_placement_cell(
            cache,
            layout,
            placement,
            major_offset,
            middle_count + 1,
            u16::from(SHARED_SLOT_036_TILES[row_kind][2]) + 0x100,
        )?;
    }
    Ok(())
}

fn render_shared_slot_040(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let encoded_height = usize::from(parameter >> 4);
    let body_rows = if encoded_height == 0 {
        0x100
    } else {
        encoded_height
    };
    for major_offset in 0..body_rows {
        let pair = if major_offset == 0 {
            [0x133, 0x134]
        } else {
            [0x09d, 0x09e]
        };
        set_placement_pair(cache, layout, placement, major_offset, pair)?;
    }
    set_placement_pair(cache, layout, placement, body_rows, [0x133, 0x134])
}

fn render_shared_slot_041(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let tile = u16::from(SHARED_SLOT_041_TILES[usize::from(parameter >> 4)]) + 0x100;
    for minor_offset in 0..=usize::from(parameter & 0x0f) {
        set_placement_cell(cache, layout, placement, 0, minor_offset, tile)?;
    }
    Ok(())
}

fn render_shared_slot_042(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let tile = u16::from(SHARED_SLOT_042_TILES[usize::from(parameter >> 4)]);
    for minor_offset in 0..=usize::from(parameter & 0x0f) {
        set_placement_cell(cache, layout, placement, 0, minor_offset, tile)?;
    }
    Ok(())
}

fn render_shared_slot_043(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let tile = u16::from(SHARED_SLOT_043_TILES[usize::from(parameter & 0x0f)]);
    for major_offset in 0..=usize::from(parameter >> 4) {
        set_placement_cell(cache, layout, placement, major_offset, 0, tile)?;
    }
    Ok(())
}

fn render_shared_slot_044(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let rows = usize::from(parameter >> 4) + 1;
    let variant = parameter & 0x0f;
    for row in 0..rows {
        let major = signed_offset(row)?;
        match variant {
            0 => {
                let minor = -2 * signed_offset(row)?;
                set_placement_cell_signed(cache, layout, placement, major, minor, 0x08c)?;
                set_placement_cell_signed(cache, layout, placement, major, minor + 1, 0x08d)?;
            }
            1 | 4 => {
                let tile = if variant == 4 { 0x094 } else { 0x086 };
                set_placement_cell_signed(
                    cache,
                    layout,
                    placement,
                    major,
                    -signed_offset(row)?,
                    tile,
                )?;
            }
            2 => {
                let minor = 2 * signed_offset(row)?;
                set_placement_cell_signed(cache, layout, placement, major, minor, 0x08e)?;
                set_placement_cell_signed(cache, layout, placement, major, minor + 1, 0x08f)?;
            }
            3 | 5 => {
                let tile = if variant == 5 { 0x095 } else { 0x087 };
                set_placement_cell(cache, layout, placement, row, row, tile)?;
            }
            _ => set_placement_cell(cache, layout, placement, row, row, 0x095)?,
        }
    }
    Ok(())
}

fn render_shared_slot_045(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let pairs = usize::from(parameter & 0x0f) + 1;
    let expands_right = parameter >> 4 != 0;
    let tiles = if expands_right {
        [0x089, 0x08b]
    } else {
        [0x088, 0x08a]
    };
    let direction = if expands_right { 1 } else { -1 };
    for pair in 0..pairs {
        let major = signed_offset(
            pair.checked_mul(2)
                .ok_or(StandardObjectRenderError::CoordinateOverflow)?,
        )?;
        let minor = direction * signed_offset(pair)?;
        set_placement_cell_signed(cache, layout, placement, major, minor, tiles[0])?;
        set_placement_cell_signed(cache, layout, placement, major + 1, minor, tiles[1])?;
    }
    Ok(())
}

fn render_shared_slot_046(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let existing_start = get_placement_cell(cache, layout, placement, 0, 0)?;
    let start = if (0x73..=0x75).contains(&existing_start.to_le_bytes()[0]) {
        0x10a
    } else {
        0x107
    };
    let encoded_middle = usize::from(parameter & 0x0f);
    let run = if encoded_middle == 0 {
        0x100
    } else {
        encoded_middle
    };
    set_placement_cell(cache, layout, placement, 0, 0, start)?;
    for minor_offset in 1..run {
        set_placement_cell(cache, layout, placement, 0, minor_offset, 0x108)?;
    }
    let existing_end = get_placement_cell(cache, layout, placement, 0, run)?;
    let end = if (0x73..=0x75).contains(&existing_end.to_le_bytes()[0]) {
        0x10b
    } else {
        0x109
    };
    set_placement_cell(cache, layout, placement, 0, run, end)
}

fn render_shared_slot_047(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let rows = usize::from(parameter >> 4) + 1;
    for major_offset in 0..rows {
        render_capped_minor_run(
            cache,
            layout,
            placement,
            major_offset,
            parameter,
            [0x73, 0x74, 0x75],
        )?;
    }
    Ok(())
}

fn render_shared_slot_048(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    render_capped_minor_run(
        cache,
        layout,
        placement,
        0,
        parameter,
        [0x159, 0x15a, 0x15b],
    )
}

fn render_capped_minor_run(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    major_offset: usize,
    parameter: u8,
    tiles: [u16; 3],
) -> Result<(), StandardObjectRenderError> {
    let encoded_middle = usize::from(parameter & 0x0f);
    let run = if encoded_middle == 0 {
        0x100
    } else {
        encoded_middle
    };
    set_placement_cell(cache, layout, placement, major_offset, 0, tiles[0])?;
    for minor_offset in 1..run {
        set_placement_cell(
            cache,
            layout,
            placement,
            major_offset,
            minor_offset,
            tiles[1],
        )?;
    }
    set_placement_cell(cache, layout, placement, major_offset, run, tiles[2])
}

fn render_shared_slot_049(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let encoded_middle = usize::from(parameter >> 4);
    let run = if encoded_middle == 0 {
        0x100
    } else {
        encoded_middle
    };
    set_placement_cell(cache, layout, placement, 0, 0, 0x15c)?;
    for major_offset in 1..run {
        set_placement_cell(cache, layout, placement, major_offset, 0, 0x15d)?;
    }
    set_placement_cell(cache, layout, placement, run, 0, 0x15e)
}

fn render_shared_slot_051(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    for minor_offset in 0..=usize::from(parameter & 0x0f) {
        set_placement_cell(cache, layout, placement, 0, minor_offset, 0x0a3)?;
        set_placement_cell(cache, layout, placement, 1, minor_offset, 0x10e)?;
    }
    Ok(())
}

fn render_shared_slot_052(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let variant = usize::from(parameter & 0x0f);
    for major_offset in 0..=usize::from(parameter >> 4) {
        let tile = if major_offset == 0 {
            SHARED_SLOT_052_TOP_TILES[variant]
        } else {
            SHARED_SLOT_052_REMAINDER_TILES[variant]
        };
        set_placement_cell(
            cache,
            layout,
            placement,
            major_offset,
            0,
            u16::from(tile) + 0x100,
        )?;
    }
    Ok(())
}

fn render_shared_slot_056(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let variant = usize::from(parameter & 0x0f);
    let body_rows = usize::from(parameter >> 4);
    for major_offset in 0..body_rows {
        set_placement_cell(
            cache,
            layout,
            placement,
            major_offset,
            0,
            u16::from(SHARED_SLOT_056_BODY_TILES[variant]) + 0x100,
        )?;
    }
    set_placement_cell(
        cache,
        layout,
        placement,
        body_rows,
        0,
        u16::from(SHARED_SLOT_056_END_TILES[variant]) + 0x100,
    )
}

fn render_shared_slot_018(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let encoded_repetitions = usize::from(parameter & 0x0f);
    let repetitions = if encoded_repetitions == 0 {
        0x100
    } else {
        encoded_repetitions
    };
    for (major_offset, row) in SHARED_SLOT_018_TILES.iter().enumerate() {
        let page = if major_offset == 0 { 0x100 } else { 0 };
        let mut minor_offset = 0;
        for &tile in &row[..3] {
            set_placement_cell(
                cache,
                layout,
                placement,
                major_offset,
                minor_offset,
                u16::from(tile) + page,
            )?;
            minor_offset += 1;
        }
        for _ in 1..repetitions {
            for &tile in &row[3..6] {
                set_placement_cell(
                    cache,
                    layout,
                    placement,
                    major_offset,
                    minor_offset,
                    u16::from(tile) + page,
                )?;
                minor_offset += 1;
            }
        }
        set_placement_cell(
            cache,
            layout,
            placement,
            major_offset,
            minor_offset,
            u16::from(row[6]) + page,
        )?;
    }
    Ok(())
}

fn render_shared_slot_017(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let variant = usize::from(parameter >> 4);
    let encoded_middle = usize::from(parameter & 0x0f);
    let middle_count = if encoded_middle == 0 {
        0x100
    } else {
        encoded_middle.saturating_sub(1)
    };
    set_placement_cell(
        cache,
        layout,
        placement,
        0,
        0,
        u16::from(SHARED_SLOT_017_START_TILES[variant]),
    )?;
    for minor_offset in 0..middle_count {
        set_placement_cell(
            cache,
            layout,
            placement,
            0,
            minor_offset + 1,
            u16::from(SHARED_SLOT_017_MIDDLE_TILES[variant]),
        )?;
    }
    set_placement_cell(
        cache,
        layout,
        placement,
        0,
        middle_count + 1,
        u16::from(SHARED_SLOT_017_END_TILES[variant]),
    )
}

fn render_shared_slot_019(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let tile = u16::from(SHARED_SLOT_019_TILES[usize::from(parameter >> 4)]);
    for minor_offset in 0..=usize::from(parameter & 0x0f) {
        set_placement_cell(cache, layout, placement, 0, minor_offset, tile)?;
    }
    Ok(())
}

fn render_shared_slot_020(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let variant = usize::from(parameter & 0x0f);
    set_placement_cell(
        cache,
        layout,
        placement,
        0,
        0,
        u16::from(SHARED_SLOT_020_TOP_TILES[variant]),
    )?;
    for major_offset in 1..=usize::from(parameter >> 4) {
        set_placement_cell(
            cache,
            layout,
            placement,
            major_offset,
            0,
            u16::from(SHARED_SLOT_020_REMAINDER_TILES[variant]),
        )?;
    }
    Ok(())
}

fn render_shared_slot_029(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let variant = usize::from(parameter & 0x0f);
    set_placement_cell(
        cache,
        layout,
        placement,
        0,
        0,
        u16::from(SHARED_SLOT_029_TOP_TILES[variant]) + 0x100,
    )?;
    for major_offset in 1..=usize::from(parameter >> 4) {
        let remainder = u16::from(SHARED_SLOT_029_REMAINDER_TILES[variant]);
        let tile = if variant < 2 {
            remainder + 0x100
        } else {
            let existing = get_placement_cell(cache, layout, placement, major_offset, 0)?;
            existing & 0xff00 | remainder
        };
        set_placement_cell(cache, layout, placement, major_offset, 0, tile)?;
    }
    Ok(())
}

fn render_shared_slot_014(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let variant = usize::from(parameter & 0x0f);
    let existing_top = get_placement_cell(cache, layout, placement, 0, 0)?;
    let top = match existing_top.to_le_bytes()[0] {
        0x08 => SHARED_SLOT_014_TOP_ON_08[variant],
        0x0e => SHARED_SLOT_014_TOP_ON_0E[variant],
        _ => SHARED_SLOT_014_BASE_TILES[variant],
    };
    set_placement_cell(cache, layout, placement, 0, 0, u16::from(top))?;
    let encoded_height = usize::from(parameter >> 4);
    let height = if encoded_height == 0 {
        0x100
    } else {
        encoded_height
    };
    let base = SHARED_SLOT_014_BASE_TILES[variant];
    for major_offset in 1..height {
        set_placement_cell(cache, layout, placement, major_offset, 0, u16::from(base))?;
    }
    let existing_bottom = get_placement_cell(cache, layout, placement, height, 0)?;
    let bottom = match existing_bottom.to_le_bytes()[0] {
        0x0e => SHARED_SLOT_014_BOTTOM_ON_0E[variant],
        0x08 => SHARED_SLOT_014_BOTTOM_ON_08[variant],
        _ => base,
    };
    set_placement_cell(cache, layout, placement, height, 0, u16::from(bottom))
}

fn render_shared_slot_011(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    command: u8,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let variant = usize::from(
        command
            .checked_sub(24)
            .ok_or(StandardObjectRenderError::InvalidCommand(command))?,
    );
    let (&top, &remainder) = SHARED_SLOT_011_TOP_TILES
        .get(variant)
        .zip(SHARED_SLOT_011_REMAINDER_TILES.get(variant))
        .ok_or(StandardObjectRenderError::InvalidCommand(command))?;
    let major_span = usize::from(parameter >> 4) + 1;
    let minor_span = usize::from(parameter & 0x0f) + 1;
    for major_offset in 0..major_span {
        for minor_offset in 0..minor_span {
            set_placement_cell(
                cache,
                layout,
                placement,
                major_offset,
                minor_offset,
                u16::from(if major_offset == 0 { top } else { remainder }),
            )?;
        }
    }
    Ok(())
}

fn render_shared_slot_008(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let group = usize::from((parameter & 0x0f) != 0);
    let encoded_height = usize::from(parameter >> 4);
    let height = if encoded_height == 0 {
        0x100
    } else {
        encoded_height
    };
    for (minor_offset, &top_tile) in SHARED_SLOT_008_TILES[group][0].iter().enumerate() {
        set_placement_cell(
            cache,
            layout,
            placement,
            0,
            minor_offset,
            u16::from(top_tile),
        )?;
        for major_offset in 1..height {
            set_placement_cell(
                cache,
                layout,
                placement,
                major_offset,
                minor_offset,
                u16::from(SHARED_SLOT_008_TILES[group][1][minor_offset]),
            )?;
        }
        set_placement_cell(
            cache,
            layout,
            placement,
            height,
            minor_offset,
            u16::from(SHARED_SLOT_008_TILES[group][2][minor_offset]),
        )?;
    }
    Ok(())
}

fn render_shared_slot_010(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let tile = u16::from(SHARED_SLOT_010_TILES[usize::from(parameter >> 4)]) + 0x100;
    for minor_offset in 0..=usize::from(parameter & 0x0f) {
        set_placement_cell(cache, layout, placement, 0, minor_offset, tile)?;
    }
    Ok(())
}

fn render_shared_slot_001(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let shape = parameter & 0x0f;
    let height = usize::from(parameter >> 4);
    let mut major_offset = 0;
    if shape < 3 {
        let top = match shape {
            0 => [0x133, 0x134],
            1 => [0x137, 0x138],
            _ => [0x139, 0x13a],
        };
        set_placement_pair(cache, layout, placement, major_offset, top)?;
        major_offset += 1;
    }
    if shape == 5 {
        for _ in 0..=height {
            set_placement_pair(cache, layout, placement, major_offset, [0x168, 0x169])?;
            major_offset += 1;
        }
        return Ok(());
    }
    let middle_rows = if shape == 2 {
        if height == 0 { 0xff } else { height - 1 }
    } else if height == 0 && shape > 2 {
        0x100
    } else {
        height
    };
    for _ in 0..middle_rows {
        set_placement_pair(cache, layout, placement, major_offset, [0x135, 0x136])?;
        major_offset += 1;
    }
    let ending = match shape {
        2 => Some([0x139, 0x13a]),
        3 => Some([0x133, 0x134]),
        4 => Some([0x137, 0x138]),
        _ => None,
    };
    if let Some(ending) = ending {
        set_placement_pair(cache, layout, placement, major_offset, ending)?;
    }
    Ok(())
}

fn set_placement_pair(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    major_offset: usize,
    tiles: [u16; 2],
) -> Result<(), StandardObjectRenderError> {
    for (minor_offset, tile) in tiles.into_iter().enumerate() {
        set_placement_cell(cache, layout, placement, major_offset, minor_offset, tile)?;
    }
    Ok(())
}

fn set_placement_cell(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    major_offset: usize,
    minor_offset: usize,
    tile: u16,
) -> Result<(), StandardObjectRenderError> {
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
    cache.set(layout, x, y, tile)?;
    Ok(())
}

fn set_placement_cell_signed(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    major_offset: isize,
    minor_offset: isize,
    tile: u16,
) -> Result<(), StandardObjectRenderError> {
    let major = usize::from(placement.major)
        .checked_add_signed(major_offset)
        .ok_or(StandardObjectRenderError::CoordinateOverflow)?;
    let minor = usize::from(placement.minor)
        .checked_add_signed(minor_offset)
        .ok_or(StandardObjectRenderError::CoordinateOverflow)?;
    let (x, y) = if layout.vertical {
        (minor, major)
    } else {
        (major, minor)
    };
    cache.set(layout, x, y, tile)?;
    Ok(())
}

fn get_placement_cell_signed(
    cache: &NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    major_offset: isize,
    minor_offset: isize,
) -> Result<u16, StandardObjectRenderError> {
    let major = usize::from(placement.major)
        .checked_add_signed(major_offset)
        .ok_or(StandardObjectRenderError::CoordinateOverflow)?;
    let minor = usize::from(placement.minor)
        .checked_add_signed(minor_offset)
        .ok_or(StandardObjectRenderError::CoordinateOverflow)?;
    let (x, y) = if layout.vertical {
        (minor, major)
    } else {
        (major, minor)
    };
    Ok(cache.get(layout, x, y)?)
}

fn get_placement_cell(
    cache: &NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    major_offset: usize,
    minor_offset: usize,
) -> Result<u16, StandardObjectRenderError> {
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
    Ok(cache.get(layout, x, y)?)
}

fn render_shared_slot_002(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    parameter: u8,
) -> Result<(), StandardObjectRenderError> {
    let variant_base = usize::from(parameter >> 4) * 2;
    let encoded_run = usize::from(parameter & 0x0f);
    for major_offset in 0..2 {
        let variant = variant_base + major_offset;
        let table_index = variant % SHARED_SLOT_002_END_TILES.len();
        let mut minor_offset = 0;
        if variant < 4 {
            set_placement_cell(
                cache,
                layout,
                placement,
                major_offset,
                minor_offset,
                u16::from(SHARED_SLOT_002_END_TILES[table_index]) + 0x100,
            )?;
            minor_offset += 1;
        }
        let run = if encoded_run == 0 && variant > 3 {
            0x100
        } else {
            encoded_run
        };
        for _ in 0..run {
            set_placement_cell(
                cache,
                layout,
                placement,
                major_offset,
                minor_offset,
                u16::from(SHARED_SLOT_002_FILL_TILES[table_index]) + 0x100,
            )?;
            minor_offset += 1;
        }
        if variant > 3 {
            set_placement_cell(
                cache,
                layout,
                placement,
                major_offset,
                minor_offset,
                u16::from(SHARED_SLOT_002_END_TILES[table_index]) + 0x100,
            )?;
        }
    }
    Ok(())
}

fn render_pattern(
    cache: &mut NativeLevelMap16Cache,
    layout: NativeLevelMap16Layout,
    placement: lm_level::NativeObjectPlacement,
    major_span: usize,
    minor_span: usize,
    definition: &StandardObjectDefinition,
) -> Result<(), StandardObjectRenderError> {
    for major_offset in 0..major_span {
        for minor_offset in 0..minor_span {
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
            cache.set(
                layout,
                x,
                y,
                definition.pattern.tile_for(
                    major_offset,
                    minor_offset,
                    major_span,
                    minor_span,
                    definition.major_expansion,
                    definition.minor_expansion,
                ),
            )?;
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
    for selector in 0x10_u8..=0x50 {
        let Some(tile) = lunar_magic_shared_extended_object_tile(selector) else {
            continue;
        };
        definitions.set_extended(
            selector,
            StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![tile],
            },
        )?;
    }
    Ok(())
}

/// Installs shared standard-object renderers recovered from Lunar Magic's native dispatch table.
///
/// This set grows as each command renderer is authenticated. Unknown commands remain deliberately
/// absent so callers can distinguish recovered output from guessed tiles.
///
/// # Errors
///
/// Returns a definition error if a recovered pattern cannot be installed.
pub fn install_lunar_magic_shared_standard_objects(
    definitions: &mut StandardObjectDefinitionSet,
) -> Result<(), StandardObjectRenderError> {
    // Dispatch slot 001 (command 15): parameter-selected two-column ledges and posts.
    definitions.set_native(
        15,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![0x135],
            },
            extent: ObjectExtent::FixedOne,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::SharedSlot001,
        },
    )?;
    // Dispatch slot 002 (command 16): two row variants selected by the high nibble.
    definitions.set_native(
        16,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![0x13d],
            },
            extent: ObjectExtent::FixedOne,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::SharedSlot002,
        },
    )?;
    // Dispatch slot 003 (command 17): 0x141 once, 0x142 once, then 0x143 for every
    // remaining high-nibble cell. The low parameter nibble is ignored.
    definitions.set_native(
        17,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 3,
                height: 1,
                tiles: vec![0x141, 0x142, 0x143],
            },
            extent: ObjectExtent::HighNibbleByOne,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::Pattern,
        },
    )?;
    // Dispatch slot 007 (command 20): tile 0x100 across the first row, then 0x03f.
    definitions.set_native(
        20,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 2,
                height: 1,
                tiles: vec![0x100, 0x03f],
            },
            extent: ObjectExtent::ParameterNibbles,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::Pattern,
        },
    )?;
    // Dispatch slot 008 (command 21): three columns with selected top/middle/bottom caps.
    definitions.set_native(
        21,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![0x25],
            },
            extent: ObjectExtent::FixedOne,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::SharedSlot008,
        },
    )?;
    // Dispatch slot 009 (command 22): a parameter-sized rectangle of tile 0x02c.
    definitions.set(
        22,
        StandardObjectPattern {
            width: 1,
            height: 1,
            tiles: vec![0x02c],
        },
    )?;
    // Dispatch slot 010 (command 23): high nibble selects one tile and low nibble its run.
    definitions.set_native(
        23,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![0x105],
            },
            extent: ObjectExtent::FixedOne,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::SharedSlot010,
        },
    )?;
    install_lunar_magic_shared_standard_objects_high(definitions)?;
    install_shared_handler_aliases(definitions)
}

fn install_lunar_magic_shared_standard_objects_high(
    definitions: &mut StandardObjectDefinitionSet,
) -> Result<(), StandardObjectRenderError> {
    for (variant, command) in (24_u8..=27).enumerate() {
        let top = [0x00, 0x01, 0x04, 0x08][variant];
        let remainder = [0x02, 0x03, 0x05, 0x0b][variant];
        definitions.set_native(
            command,
            StandardObjectDefinition {
                pattern: StandardObjectPattern {
                    width: 2,
                    height: 1,
                    tiles: vec![top, remainder],
                },
                extent: ObjectExtent::ParameterNibbles,
                major_expansion: AxisExpansion::Clamp,
                minor_expansion: AxisExpansion::Clamp,
                renderer: NativeRenderer::Pattern,
            },
        )?;
    }
    // Dispatch slot 012 (command 28): two rows selected from packed table word 0x4426.
    definitions.set_native(
        28,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 2,
                height: 1,
                tiles: vec![0x026, 0x144],
            },
            extent: ObjectExtent::TwoByLowNibble,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::Pattern,
        },
    )?;
    // Dispatch slot 013 (command 29): zero or more 0x00b rows and a final 0x00e row.
    definitions.set_native(
        29,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 2,
                height: 1,
                tiles: vec![0x00b, 0x00e],
            },
            extent: ObjectExtent::ParameterNibbles,
            major_expansion: AxisExpansion::FinalEdge,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::Pattern,
        },
    )?;
    install_handler14_command(definitions)?;
    // Dispatch slots 015/016 (commands 31/32): capped sequences on opposite axes.
    definitions.set_native(
        31,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 3,
                height: 1,
                tiles: vec![0x153, 0x154, 0x155],
            },
            extent: ObjectExtent::HighNibbleByOne,
            major_expansion: AxisExpansion::PreserveEdges,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::Pattern,
        },
    )?;
    definitions.set_native(
        32,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 1,
                height: 3,
                tiles: vec![0x156, 0x157, 0x158],
            },
            extent: ObjectExtent::OneByLowNibble,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::PreserveEdges,
            renderer: NativeRenderer::Pattern,
        },
    )?;
    // Dispatch slot 006 (command 33): three rows and a full-byte run length.
    definitions.set_native(
        33,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 2,
                height: 1,
                tiles: vec![0x100, 0x03f],
            },
            extent: ObjectExtent::ThreeByParameterByte,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::Pattern,
        },
    )?;
    Ok(())
}

fn install_handler14_command(
    definitions: &mut StandardObjectDefinitionSet,
) -> Result<(), StandardObjectRenderError> {
    definitions.set_native(
        30,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![0x0a],
            },
            extent: ObjectExtent::FixedOne,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::SharedSlot014,
        },
    )
}

fn install_shared_handler_aliases(
    definitions: &mut StandardObjectDefinitionSet,
) -> Result<(), StandardObjectRenderError> {
    definitions.set_handler(
        0,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![0],
            },
            extent: ObjectExtent::FixedOne,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::NoOp,
        },
    )?;
    definitions.set_handler(
        11,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![0],
            },
            extent: ObjectExtent::FixedOne,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::SharedSlot011,
        },
    )?;
    for (handler, renderer) in [
        (17, NativeRenderer::SharedSlot017),
        (18, NativeRenderer::SharedSlot018),
        (19, NativeRenderer::SharedSlot019),
        (20, NativeRenderer::SharedSlot020),
        (21, NativeRenderer::SharedSlot021),
        (22, NativeRenderer::SharedSlot022),
        (23, NativeRenderer::SharedSlot023),
        (26, NativeRenderer::SharedSlot026),
        (27, NativeRenderer::SharedSlot027),
        (29, NativeRenderer::SharedSlot029),
        (30, NativeRenderer::SharedSlot030),
        (31, NativeRenderer::SharedSlot031),
        (33, NativeRenderer::SharedSlot033),
        (34, NativeRenderer::SharedSlot034),
        (35, NativeRenderer::SharedSlot035),
        (36, NativeRenderer::SharedSlot036),
        (40, NativeRenderer::SharedSlot040),
        (41, NativeRenderer::SharedSlot041),
        (42, NativeRenderer::SharedSlot042),
        (43, NativeRenderer::SharedSlot043),
        (44, NativeRenderer::SharedSlot044),
        (45, NativeRenderer::SharedSlot045),
        (46, NativeRenderer::SharedSlot046),
        (47, NativeRenderer::SharedSlot047),
        (48, NativeRenderer::SharedSlot048),
        (49, NativeRenderer::SharedSlot049),
        (51, NativeRenderer::SharedSlot051),
        (52, NativeRenderer::SharedSlot052),
        (56, NativeRenderer::SharedSlot056),
    ] {
        definitions.set_handler(
            handler,
            StandardObjectDefinition {
                pattern: StandardObjectPattern {
                    width: 1,
                    height: 1,
                    tiles: vec![0],
                },
                extent: ObjectExtent::FixedOne,
                major_expansion: AxisExpansion::Clamp,
                minor_expansion: AxisExpansion::Clamp,
                renderer,
            },
        )?;
    }
    install_simple_mapped_handlers(definitions)?;
    for (command, handler) in [
        (15, 1),
        (16, 2),
        (17, 3),
        (20, 7),
        (21, 8),
        (22, 9),
        (23, 10),
        (28, 12),
        (29, 13),
        (30, 14),
        (31, 15),
        (32, 16),
        (33, 6),
    ] {
        definitions.alias(command, handler)?;
    }
    Ok(())
}

fn install_simple_mapped_handlers(
    definitions: &mut StandardObjectDefinitionSet,
) -> Result<(), StandardObjectRenderError> {
    definitions.set_handler(
        28,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 1,
                height: 2,
                tiles: vec![0x10e, 0x0b8],
            },
            extent: ObjectExtent::ParameterNibbles,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::Pattern,
        },
    )?;
    definitions.set_handler(
        32,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![0x065],
            },
            extent: ObjectExtent::ParameterNibbles,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::Pattern,
        },
    )?;
    definitions.set_handler(
        37,
        StandardObjectDefinition {
            pattern: StandardObjectPattern {
                width: 2,
                height: 1,
                tiles: vec![0x109, 0x086],
            },
            extent: ObjectExtent::TwoByLowNibble,
            major_expansion: AxisExpansion::Clamp,
            minor_expansion: AxisExpansion::Clamp,
            renderer: NativeRenderer::Pattern,
        },
    )?;
    for (handler, pattern, major_expansion) in [
        (
            54,
            StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![0x1ff],
            },
            AxisExpansion::Clamp,
        ),
        (
            55,
            StandardObjectPattern {
                width: 2,
                height: 1,
                tiles: vec![0x165, 0x14e],
            },
            AxisExpansion::FinalEdge,
        ),
        (
            57,
            StandardObjectPattern {
                width: 1,
                height: 1,
                tiles: vec![0x165],
            },
            AxisExpansion::Clamp,
        ),
    ] {
        definitions.set_handler(
            handler,
            StandardObjectDefinition {
                pattern,
                extent: ObjectExtent::ParameterNibbles,
                major_expansion,
                minor_expansion: AxisExpansion::Clamp,
                renderer: NativeRenderer::Pattern,
            },
        )?;
    }
    for (handler, tile) in [(24, 0x16c), (25, 0x16d)] {
        definitions.set_handler(
            handler,
            StandardObjectDefinition {
                pattern: StandardObjectPattern {
                    width: 1,
                    height: 1,
                    tiles: vec![tile],
                },
                extent: ObjectExtent::ParameterNibbles,
                major_expansion: AxisExpansion::Clamp,
                minor_expansion: AxisExpansion::Clamp,
                renderer: NativeRenderer::Pattern,
            },
        )?;
    }
    Ok(())
}

/// Returns the recovered Map16 tile for a shared command-zero extended object.
#[must_use]
pub fn lunar_magic_shared_extended_object_tile(selector: u8) -> Option<u16> {
    let index = usize::from(selector.checked_sub(0x10)?);
    let tile = u16::from(*SHARED_EXTENDED_TILES.get(index)?);
    Some(tile + if selector < 0x23 { 0 } else { 0x100 })
}

impl StandardObjectPattern {
    fn tile_for(
        &self,
        x: usize,
        y: usize,
        target_width: usize,
        target_height: usize,
        x_expansion: AxisExpansion,
        y_expansion: AxisExpansion,
    ) -> u16 {
        let source_x = expandable_axis_index(x, target_width, self.width, x_expansion);
        let source_y = expandable_axis_index(y, target_height, self.height, y_expansion);
        self.tiles[source_y * self.width + source_x]
    }
}

fn expandable_axis_index(
    position: usize,
    target_len: usize,
    pattern_len: usize,
    expansion: AxisExpansion,
) -> usize {
    match expansion {
        AxisExpansion::Clamp => position.min(pattern_len - 1),
        AxisExpansion::FinalEdge => {
            if position + 1 == target_len {
                pattern_len - 1
            } else {
                position.min(pattern_len.saturating_sub(2))
            }
        }
        AxisExpansion::PreserveEdges => match pattern_len {
            1 => 0,
            2 => position.min(1),
            _ if target_len <= pattern_len => position.min(pattern_len - 1),
            _ if position == 0 => 0,
            _ if position + 1 == target_len => pattern_len - 1,
            _ => 1 + (position - 1) % (pattern_len - 2),
        },
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
        assert_eq!(
            pattern.tile_for(
                0,
                0,
                5,
                4,
                AxisExpansion::PreserveEdges,
                AxisExpansion::PreserveEdges
            ),
            1
        );
        assert_eq!(
            pattern.tile_for(
                1,
                1,
                5,
                4,
                AxisExpansion::PreserveEdges,
                AxisExpansion::PreserveEdges
            ),
            5
        );
        assert_eq!(
            pattern.tile_for(
                4,
                3,
                5,
                4,
                AxisExpansion::PreserveEdges,
                AxisExpansion::PreserveEdges
            ),
            9
        );
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
    fn family_map_resolves_object_id_to_handler_definition() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[15] = 1;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0xf0, 0]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x25,
        )
        .unwrap();
        assert_eq!(report.rendered_objects, 1);
        assert_eq!(report.cache.cells()[0], 0x133);
        assert_eq!(report.cache.cells()[1], 0x134);
    }

    #[test]
    fn mapped_null_handler_is_an_explicit_rendered_no_op() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 0;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0x10, 0xff]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x25,
        )
        .unwrap();
        assert_eq!(report.rendered_objects, 1);
        assert!(report.missing_commands.is_empty());
        assert!(report.cache.cells().iter().all(|tile| *tile == 0x25));
    }

    #[test]
    fn mapped_handler_11_uses_the_original_object_id_as_variant() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[51] = 11;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x60, 0x30, 0x11]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x25,
        )
        .unwrap();
        for minor in 0..2 {
            assert_eq!(
                report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 0, minor)],
                0x38
            );
            assert_eq!(
                report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 1, minor)],
                0x20
            );
        }
    }

    #[test]
    fn mapped_handlers_17_19_and_20_follow_live_vanilla_tables() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let cases = [
            (
                17,
                0x23,
                vec![(0, 0, 0x85), (0, 1, 0x86), (0, 2, 0x86), (0, 3, 0x87)],
            ),
            (
                19,
                0x13,
                vec![(0, 0, 0x9c), (0, 1, 0x9c), (0, 2, 0x9c), (0, 3, 0x9c)],
            ),
            (20, 0x23, vec![(0, 0, 0x98), (1, 0, 0x98), (2, 0, 0x98)]),
        ];
        for (handler, parameter, expected) in cases {
            let mut handler_map = [0xff; 64];
            handler_map[1] = handler;
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x10, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            assert!(report.missing_commands.is_empty());
            for (major, minor, tile) in expected {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handler_18_repeats_four_authenticated_motif_rows() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 18;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0x10, 0x03]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x25,
        )
        .unwrap();
        let expected = [
            [
                0x107, 0x10a, 0x10a, 0x108, 0x10a, 0x10a, 0x108, 0x10a, 0x10a, 0x109,
            ],
            [
                0x081, 0x082, 0x083, 0x081, 0x082, 0x083, 0x081, 0x082, 0x083, 0x081,
            ],
            [
                0x081, 0x025, 0x084, 0x081, 0x025, 0x084, 0x081, 0x025, 0x084, 0x081,
            ],
            [
                0x081, 0x025, 0x084, 0x081, 0x025, 0x084, 0x081, 0x025, 0x084, 0x081,
            ],
        ];
        for (major, row) in expected.into_iter().enumerate() {
            for (minor, tile) in row.into_iter().enumerate() {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handlers_24_and_25_fill_vanilla_page_one_tiles() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        for (handler, tile) in [(24, 0x16c), (25, 0x16d)] {
            let mut handler_map = [0xff; 64];
            handler_map[1] = handler;
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x10, 0x12]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            for major in 0..2 {
                for minor in 0..3 {
                    assert_eq!(
                        report.cache.cells()
                            [NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                        tile
                    );
                }
            }
        }
    }

    #[test]
    fn mapped_handlers_28_and_29_preserve_their_distinct_rows() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        for (handler, parameter, expected) in [
            (
                28,
                0x22,
                vec![(0, 0, 0x10e), (2, 0, 0x10e), (0, 1, 0x0b8), (2, 2, 0x0b8)],
            ),
            (29, 0x21, vec![(0, 0, 0x15e), (1, 0, 0x15d), (2, 0, 0x15d)]),
        ] {
            let mut handler_map = [0xff; 64];
            handler_map[1] = handler;
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x10, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            for (major, minor, tile) in expected {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handler_27_adapts_even_rows_and_alternates_odd_rows() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 27;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0x10, 0x20]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x10e,
        )
        .unwrap();
        for (major, pair) in [[0x10b, 0x10c], [0x0bb, 0x0bc], [0x10b, 0x10c]]
            .into_iter()
            .enumerate()
        {
            for (minor, tile) in pair.into_iter().enumerate() {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handler_26_uses_adaptive_even_and_table_odd_rows() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 26;
        for (parameter, blank, expected) in [
            (0x20, 0x10e, [0x10d, 0x0be, 0x10d]),
            (0x21, 0x0b6, [0x0b7, 0x0c0, 0x0b7]),
        ] {
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x10, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                blank,
            )
            .unwrap();
            for (major, tile) in expected.into_iter().enumerate() {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, 0)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handler_21_expands_its_diagonal_widening_rows() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 21;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0x18, 0x20]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x25,
        )
        .unwrap();
        for (major, start_minor, row) in [
            (0, 8, vec![0x1c4, 0x1c5]),
            (1, 7, vec![0x1c7, 0x1ec, 0x1ed, 0x1c6]),
            (2, 6, vec![0x1c7, 0x1ee, 0x159, 0x15a, 0x1ef]),
        ] {
            for (column, tile) in row.into_iter().enumerate() {
                assert_eq!(
                    report.cache.cells()
                        [NativeLevelMap16Cache::cell_index(layout(), major, start_minor + column)],
                    tile
                );
            }
        }
        assert_eq!(
            report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 3, 6)],
            0x1eb
        );
    }

    #[test]
    fn mapped_handler_22_builds_adaptive_widening_and_body_rows() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 22;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0x18, 0x12]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x25,
        )
        .unwrap();
        for (major, start_minor, row) in [
            (0, 8, vec![0x1aa, 0x0a1]),
            (1, 7, vec![0x1aa, 0x1e2, 0x03f, 0x0a6]),
            (2, 6, vec![0x1aa, 0x1e2, 0x03f, 0x03f, 0x03f, 0x0a6]),
            (3, 6, vec![0x1f7, 0x03f, 0x03f, 0x03f, 0x03f, 0x03f, 0x0a6]),
            (4, 7, vec![0x0a3, 0x03f, 0x03f, 0x03f, 0x03f, 0x03f, 0x0a6]),
        ] {
            for (column, tile) in row.into_iter().enumerate() {
                assert_eq!(
                    report.cache.cells()
                        [NativeLevelMap16Cache::cell_index(layout(), major, start_minor + column)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handler_23_builds_bounded_widening_and_body_rows() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 23;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0x18, 0x12]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x25,
        )
        .unwrap();
        for (major, start_minor, row) in [
            (0, 8, vec![0x0af, 0x1af]),
            (1, 7, vec![0x0a9, 0x03f, 0x1e4, 0x1af]),
            (2, 6, vec![0x0a9, 0x03f, 0x03f, 0x03f, 0x1e4, 0x1af]),
            (3, 6, vec![0x0a9, 0x03f, 0x03f, 0x03f, 0x03f, 0x03f, 0x1f9]),
            (4, 7, vec![0x0a9, 0x03f, 0x03f, 0x03f, 0x03f, 0x03f, 0x0ac]),
        ] {
            for (column, tile) in row.into_iter().enumerate() {
                assert_eq!(
                    report.cache.cells()
                        [NativeLevelMap16Cache::cell_index(layout(), major, start_minor + column)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handler_30_writes_six_by_sixteen_low_byte_strips() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 30;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0x10, 0]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x125,
        )
        .unwrap();
        assert_eq!(report.cache.cells()[0], 0x1b4);
        assert_eq!(
            report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 3, 4)],
            0x1b5
        );
        assert_eq!(
            report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 5, 0)],
            0x1b5
        );
        assert_eq!(
            report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 5, 15)],
            0x1b1
        );
    }

    #[test]
    fn mapped_handler_31_renders_cap_and_body_pairs() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 31;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0x10, 0x20]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x25,
        )
        .unwrap();
        for (major, pair) in [[0x161, 0x162], [0x163, 0x164], [0x163, 0x164]]
            .into_iter()
            .enumerate()
        {
            for (minor, tile) in pair.into_iter().enumerate() {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handler_32_fills_its_lookup_tile_rectangle() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 32;
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0x10, 0x12]).unwrap()],
        };
        let report = render_mapped_standard_object_stream(
            &stream,
            &definitions,
            &handler_map,
            layout(),
            0x25,
        )
        .unwrap();
        for major in 0..2 {
            for minor in 0..3 {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    0x065
                );
            }
        }
    }

    #[test]
    fn mapped_handlers_34_35_and_37_follow_native_axes() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        for (handler, parameter, expected) in [
            (34, 0x12, vec![(0, 0, 0x159), (0, 1, 0x159), (0, 2, 0x159)]),
            (35, 0x21, vec![(0, 0, 0x15c), (1, 0, 0x15c), (2, 0, 0x15c)]),
            (
                37,
                0x02,
                vec![
                    (0, 0, 0x109),
                    (0, 1, 0x109),
                    (0, 2, 0x109),
                    (1, 0, 0x086),
                    (1, 1, 0x086),
                    (1, 2, 0x086),
                ],
            ),
        ] {
            let mut handler_map = [0xff; 64];
            handler_map[1] = handler;
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x10, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            for (major, minor, tile) in expected {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handler_33_mirrors_expanding_page_one_wedges() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 33;
        for (parameter, start_minor, start, end, direction) in [
            (0x22, 4_usize, 0x1cf, 0x1f4, 1_isize),
            (0x20, 8_usize, 0x1ce, 0x1f3, -1_isize),
        ] {
            let stream = ObjectStream {
                records: vec![
                    ObjectRecord::new(vec![
                        0,
                        0x10 | u8::try_from(start_minor).unwrap(),
                        parameter,
                    ])
                    .unwrap(),
                ],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            assert_eq!(
                report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 0, start_minor)],
                start
            );
            for row in 1_usize..=3 {
                for fill in 0..row.saturating_sub(1) {
                    let minor = start_minor
                        .checked_add_signed(direction * isize::try_from(fill).unwrap())
                        .unwrap();
                    assert_eq!(
                        report.cache.cells()
                            [NativeLevelMap16Cache::cell_index(layout(), row, minor)],
                        0x03f
                    );
                }
                let end_minor = start_minor
                    .checked_add_signed(direction * isize::try_from(row.saturating_sub(1)).unwrap())
                    .unwrap();
                assert_eq!(
                    report.cache.cells()
                        [NativeLevelMap16Cache::cell_index(layout(), row, end_minor)],
                    end
                );
                if row < 3 {
                    let start_minor = start_minor
                        .checked_add_signed(direction * isize::try_from(row).unwrap())
                        .unwrap();
                    assert_eq!(
                        report.cache.cells()
                            [NativeLevelMap16Cache::cell_index(layout(), row, start_minor)],
                        start
                    );
                }
            }
        }
    }

    #[test]
    fn mapped_handlers_36_and_40_render_bordered_transitions() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        for (handler, parameter, expected) in [
            (
                36,
                0x22,
                vec![
                    vec![0x15d, 0x15e, 0x15f],
                    vec![0x160, 0x161, 0x162],
                    vec![0x163, 0x164, 0x165],
                ],
            ),
            (
                40,
                0x20,
                vec![vec![0x133, 0x134], vec![0x09d, 0x09e], vec![0x133, 0x134]],
            ),
        ] {
            let mut handler_map = [0xff; 64];
            handler_map[1] = handler;
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x10, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            for (major, row) in expected.into_iter().enumerate() {
                for (minor, tile) in row.into_iter().enumerate() {
                    assert_eq!(
                        report.cache.cells()
                            [NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                        tile
                    );
                }
            }
        }
    }

    #[test]
    fn mapped_handlers_41_42_and_43_follow_lookup_selected_axes() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        for (handler, parameter, expected) in [
            (41, 0x12, vec![(0, 0, 0x10d), (0, 1, 0x10d), (0, 2, 0x10d)]),
            (42, 0x12, vec![(0, 0, 0x093), (0, 1, 0x093), (0, 2, 0x093)]),
            (43, 0x21, vec![(0, 0, 0x091), (1, 0, 0x091), (2, 0, 0x091)]),
        ] {
            let mut handler_map = [0xff; 64];
            handler_map[1] = handler;
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x10, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            for (major, minor, tile) in expected {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handlers_46_through_48_render_adaptive_and_capped_runs() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        for (handler, parameter, blank, expected) in [
            (46, 0x03, 0x073, vec![vec![0x10a, 0x108, 0x108, 0x10b]]),
            (
                47,
                0x12,
                0x025,
                vec![vec![0x073, 0x074, 0x075], vec![0x073, 0x074, 0x075]],
            ),
            (48, 0x02, 0x025, vec![vec![0x159, 0x15a, 0x15b]]),
        ] {
            let mut handler_map = [0xff; 64];
            handler_map[1] = handler;
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x10, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                blank,
            )
            .unwrap();
            for (major, row) in expected.into_iter().enumerate() {
                for (minor, tile) in row.into_iter().enumerate() {
                    assert_eq!(
                        report.cache.cells()
                            [NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                        tile
                    );
                }
            }
        }
    }

    #[test]
    fn mapped_handler_44_dispatches_all_diagonal_variants() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 44;
        for (parameter, expected) in [
            (
                0x20,
                vec![
                    (0, 8, 0x08c),
                    (0, 9, 0x08d),
                    (1, 6, 0x08c),
                    (1, 7, 0x08d),
                    (2, 4, 0x08c),
                    (2, 5, 0x08d),
                ],
            ),
            (
                0x22,
                vec![
                    (0, 8, 0x08e),
                    (0, 9, 0x08f),
                    (1, 10, 0x08e),
                    (1, 11, 0x08f),
                    (2, 12, 0x08e),
                    (2, 13, 0x08f),
                ],
            ),
            (0x24, vec![(0, 8, 0x094), (1, 7, 0x094), (2, 6, 0x094)]),
            (0x25, vec![(0, 8, 0x095), (1, 9, 0x095), (2, 10, 0x095)]),
        ] {
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x18, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            for (major, minor, tile) in expected {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handler_45_mirrors_diagonal_edge_pairs() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let mut handler_map = [0xff; 64];
        handler_map[1] = 45;
        for (parameter, expected) in [
            (
                0x02,
                vec![
                    (0, 8, 0x088),
                    (1, 8, 0x08a),
                    (2, 7, 0x088),
                    (3, 7, 0x08a),
                    (4, 6, 0x088),
                    (5, 6, 0x08a),
                ],
            ),
            (
                0x12,
                vec![
                    (0, 8, 0x089),
                    (1, 8, 0x08b),
                    (2, 9, 0x089),
                    (3, 9, 0x08b),
                    (4, 10, 0x089),
                    (5, 10, 0x08b),
                ],
            ),
        ] {
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x18, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            for (major, minor, tile) in expected {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn mapped_handlers_49_51_and_52_follow_recovered_shapes() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        for (handler, parameter, expected) in [
            (49, 0x20, vec![vec![0x15c], vec![0x15d], vec![0x15e]]),
            (
                51,
                0x02,
                vec![vec![0x0a3, 0x0a3, 0x0a3], vec![0x10e, 0x10e, 0x10e]],
            ),
            (52, 0x20, vec![vec![0x15a], vec![0x15b], vec![0x15b]]),
        ] {
            let mut handler_map = [0xff; 64];
            handler_map[1] = handler;
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x10, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            for (major, row) in expected.into_iter().enumerate() {
                for (minor, tile) in row.into_iter().enumerate() {
                    assert_eq!(
                        report.cache.cells()
                            [NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                        tile
                    );
                }
            }
        }
    }

    #[test]
    fn mapped_handlers_54_through_57_render_fill_cap_and_lookup_rows() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        for (handler, parameter, expected) in [
            (54, 0x11, vec![vec![0x1ff, 0x1ff], vec![0x1ff, 0x1ff]]),
            (
                55,
                0x12,
                vec![vec![0x165, 0x165, 0x165], vec![0x14e, 0x14e, 0x14e]],
            ),
            (56, 0x22, vec![vec![0x151], vec![0x151], vec![0x14f]]),
            (57, 0x11, vec![vec![0x165, 0x165], vec![0x165, 0x165]]),
        ] {
            let mut handler_map = [0xff; 64];
            handler_map[1] = handler;
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![0, 0x10, parameter]).unwrap()],
            };
            let report = render_mapped_standard_object_stream(
                &stream,
                &definitions,
                &handler_map,
                layout(),
                0x25,
            )
            .unwrap();
            for (major, row) in expected.into_iter().enumerate() {
                for (minor, tile) in row.into_iter().enumerate() {
                    assert_eq!(
                        report.cache.cells()
                            [NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                        tile
                    );
                }
            }
        }
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

    #[test]
    fn recovered_command_17_ignores_low_nibble_and_clamps_trailing_tile() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x20, 0x10, 0x35]).unwrap()],
        };
        let report = render_standard_object_stream(&stream, &definitions, layout(), 0x25).unwrap();
        assert_eq!(report.rendered_objects, 1);
        assert!(report.missing_commands.is_empty());
        for (x, tile) in [0x141, 0x142, 0x143, 0x143].into_iter().enumerate() {
            assert_eq!(
                report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), x, 0)],
                tile
            );
        }
        assert_eq!(
            report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 0, 1)],
            0x25
        );
    }

    #[test]
    fn recovered_command_16_uses_two_live_lookup_table_rows() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x20, 0, 0x02]).unwrap()],
        };
        let report = render_standard_object_stream(&stream, &definitions, layout(), 0x25).unwrap();
        for (major, expected) in [[0x13b, 0x13d, 0x13d], [0x13c, 0x13e, 0x13e]]
            .into_iter()
            .enumerate()
        {
            for (minor, tile) in expected.into_iter().enumerate() {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn recovered_command_15_selects_native_top_middle_and_end_pairs() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0, 0xf0, 0x32]).unwrap()],
        };
        let report = render_standard_object_stream(&stream, &definitions, layout(), 0x25).unwrap();
        let expected = [
            [0x139, 0x13a],
            [0x135, 0x136],
            [0x135, 0x136],
            [0x139, 0x13a],
        ];
        for (major, row) in expected.into_iter().enumerate() {
            for (minor, tile) in row.into_iter().enumerate() {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn recovered_commands_22_and_23_follow_native_extent_rules() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let stream = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0x20, 0x60, 0x11]).unwrap(),
                ObjectRecord::new(vec![0x24, 0x70, 0x23]).unwrap(),
            ],
        };
        let report = render_standard_object_stream(&stream, &definitions, layout(), 0x25).unwrap();
        for major in 0..2 {
            for minor in 0..2 {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    0x02c
                );
            }
        }
        for minor in 0..4 {
            assert_eq!(
                report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 4, minor)],
                0x1a4
            );
        }
    }

    #[test]
    fn recovered_command_20_uses_a_distinct_top_row() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x20, 0x40, 0x22]).unwrap()],
        };
        let report = render_standard_object_stream(&stream, &definitions, layout(), 0x25).unwrap();
        for major in 0..3 {
            for minor in 0..3 {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    if major == 0 { 0x100 } else { 0x03f }
                );
            }
        }
    }

    #[test]
    fn recovered_command_21_uses_selected_three_column_caps() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x20, 0x50, 0x21]).unwrap()],
        };
        let report = render_standard_object_stream(&stream, &definitions, layout(), 0x25).unwrap();
        for (major, expected) in [[0x39, 0x25, 0x3c], [0x3a, 0x25, 0x3d], [0x3b, 0x25, 0x3e]]
            .into_iter()
            .enumerate()
        {
            for (minor, tile) in expected.into_iter().enumerate() {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }
    }

    #[test]
    fn recovered_commands_28_and_29_use_native_row_extents() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let command_28 = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x20, 0xc0, 0xf2]).unwrap()],
        };
        let report =
            render_standard_object_stream(&command_28, &definitions, layout(), 0x25).unwrap();
        for major in 0..2 {
            for minor in 0..3 {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    if major == 0 { 0x026 } else { 0x144 }
                );
            }
        }

        let command_29 = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x20, 0xd0, 0x21]).unwrap()],
        };
        let report =
            render_standard_object_stream(&command_29, &definitions, layout(), 0x25).unwrap();
        for major in 0..3 {
            for minor in 0..2 {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    if major == 2 { 0x00e } else { 0x00b }
                );
            }
        }
    }

    #[test]
    fn recovered_handler_14_adapts_both_ends_to_existing_tiles() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        definitions
            .set(
                1,
                StandardObjectPattern {
                    width: 1,
                    height: 1,
                    tiles: vec![0x08],
                },
            )
            .unwrap();
        definitions
            .set(
                2,
                StandardObjectPattern {
                    width: 1,
                    height: 1,
                    tiles: vec![0x0e],
                },
            )
            .unwrap();
        let stream = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0, 0x10, 0]).unwrap(),
                ObjectRecord::new(vec![2, 0x20, 0]).unwrap(),
                ObjectRecord::new(vec![0x20, 0xe0, 0x20]).unwrap(),
            ],
        };
        let report = render_standard_object_stream(&stream, &definitions, layout(), 0x25).unwrap();
        assert_eq!(report.cache.cells()[0], 0x07);
        assert_eq!(
            report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 1, 0)],
            0x0a
        );
        assert_eq!(
            report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), 2, 0)],
            0x0d
        );
    }

    #[test]
    fn recovered_commands_24_through_27_select_two_phase_tiles() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        for (command, expected) in [
            (24, [0x00, 0x02]),
            (25, [0x01, 0x03]),
            (26, [0x04, 0x05]),
            (27, [0x08, 0x0b]),
        ] {
            let first = (command & 0x30) << 1;
            let second = (command & 0x0f) << 4;
            let stream = ObjectStream {
                records: vec![ObjectRecord::new(vec![first, second, 0x12]).unwrap()],
            };
            let report =
                render_standard_object_stream(&stream, &definitions, layout(), 0x25).unwrap();
            for (major, &expected_tile) in expected.iter().enumerate() {
                for minor in 0..3 {
                    assert_eq!(
                        report.cache.cells()
                            [NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                        expected_tile
                    );
                }
            }
        }
    }

    #[test]
    fn recovered_commands_31_through_33_use_command_specific_axes() {
        let mut definitions = StandardObjectDefinitionSet::empty();
        install_lunar_magic_shared_standard_objects(&mut definitions).unwrap();
        let cases = [
            (
                ObjectRecord::new(vec![0x20, 0xf0, 0x3f]).unwrap(),
                vec![(0, 0x153), (1, 0x154), (2, 0x154), (3, 0x155)],
            ),
            (
                ObjectRecord::new(vec![0x40, 0, 0xf3]).unwrap(),
                vec![(0, 0x156), (1, 0x157), (2, 0x157), (3, 0x158)],
            ),
        ];
        for (record, expected) in cases {
            let command = record.command_id();
            let report = render_standard_object_stream(
                &ObjectStream {
                    records: vec![record],
                },
                &definitions,
                layout(),
                0x25,
            )
            .unwrap();
            for (offset, tile) in expected {
                let (major, minor) = if command == 31 {
                    (offset, 0)
                } else {
                    (0, offset)
                };
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    tile
                );
            }
        }

        let command_33 = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x40, 0x10, 3]).unwrap()],
        };
        let report =
            render_standard_object_stream(&command_33, &definitions, layout(), 0x25).unwrap();
        for major in 0..3 {
            for minor in 0..4 {
                assert_eq!(
                    report.cache.cells()[NativeLevelMap16Cache::cell_index(layout(), major, minor)],
                    if major == 0 { 0x100 } else { 0x03f }
                );
            }
        }
    }
}
