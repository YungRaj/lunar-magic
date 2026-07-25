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
    Pattern,
    SharedSlot001,
    SharedSlot002,
    SharedSlot008,
    SharedSlot010,
    SharedSlot011,
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
    if definition.renderer == NativeRenderer::SharedSlot002 {
        return render_shared_slot_002(cache, layout, placement, parameter);
    }
    if definition.renderer == NativeRenderer::SharedSlot001 {
        return render_shared_slot_001(cache, layout, placement, parameter);
    }
    if definition.renderer == NativeRenderer::SharedSlot010 {
        return render_shared_slot_010(cache, layout, placement, parameter);
    }
    if definition.renderer == NativeRenderer::SharedSlot008 {
        return render_shared_slot_008(cache, layout, placement, parameter);
    }
    if definition.renderer == NativeRenderer::SharedSlot011 {
        return render_shared_slot_011(cache, layout, placement, command, parameter);
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

fn install_shared_handler_aliases(
    definitions: &mut StandardObjectDefinitionSet,
) -> Result<(), StandardObjectRenderError> {
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
        (31, 15),
        (32, 16),
        (33, 6),
    ] {
        definitions.alias(command, handler)?;
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
