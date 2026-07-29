/// One recovered 16×16 definition used by Lunar Magic's standard-sprite preview renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StandardSpritePreviewTile {
    /// Lunar Magic's preview-definition index.
    pub definition_index: u16,
    /// Top-left, top-right, bottom-left, and bottom-right SNES tile words.
    pub subtiles: [u16; 4],
    /// Signed preview displacement in pixels from the sprite placement origin.
    pub x: i16,
    pub y: i16,
}

/// The recovered source of a standard-sprite preview dispatch entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardSpritePreviewSource {
    /// Lunar Magic has authenticated built-in artwork for this ID.
    BuiltIn,
    /// Lunar Magic deliberately routes this ID to its empty/default preview handler.
    NativeEmpty,
    /// Lunar Magic reserves this ID for custom-display bookkeeping supplied by SSC data.
    CustomDisplay,
}

/// Classifies every byte-sized sprite ID by its recovered Lunar Magic preview source.
#[must_use]
pub const fn lunar_magic_standard_sprite_preview_source(
    sprite_number: u8,
) -> StandardSpritePreviewSource {
    match sprite_number {
        0x29 | 0x30 | 0xee | 0xf0 | 0xf1 => StandardSpritePreviewSource::NativeEmpty,
        0xf6..=0xff => StandardSpritePreviewSource::CustomDisplay,
        _ => StandardSpritePreviewSource::BuiltIn,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StandardLevelOrientation {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StandardSpriteWideContext {
    /// The native edge validator accepts the placement and `$64` uses its
    /// ordinary three-middle-segment stem.
    #[default]
    ValidShort,
    /// The active native context tables select `$64`'s seven-middle-segment stem.
    ValidLong64,
    /// The native wide-object edge validator rejects the placement.
    Invalid,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StandardSpritePreviewMode {
    pub alternate_display: bool,
    pub alternate_graphics: bool,
    /// Low nibble of Lunar Magic's active sprite-graphics mode selector.
    pub sprite_graphics_mode: u8,
    /// Frame phase used by animated standard-sprite previews.
    pub animation_phase: u8,
    /// Enables Lunar Magic's context-specific definition overrides.
    pub special_display_mode: bool,
    /// First native sprite-record byte used by placement-dependent handlers.
    pub placement_first: u8,
    /// Native major-axis tile coordinate. Some handlers receive its within-screen
    /// coordinate as their first argument and derive direction from its parity.
    pub placement_major: u16,
    /// Lunar Magic's active level-mode selector used by text-based generator previews.
    pub level_mode: u8,
    /// Determines which native position nibble text-based generator previews inspect.
    pub level_orientation: StandardLevelOrientation,
    /// Context selected by Lunar Magic's two sprite-selector tables and
    /// wide-object edge validator.
    pub wide_context: StandardSpriteWideContext,
    /// Zero-based count of earlier standard `$8A` handlers in the current
    /// full sprite-list render.
    pub sprite_8a_sequence_index: u8,
}

/// Renders Lunar Magic's authenticated standard-sprite preview handlers.
///
/// Lunar Magic substitutes tile `$115` when its alternate sprite-number display mode is active.
/// IDs routed to the native empty/default handler return `None`.
#[must_use]
pub fn render_lunar_magic_standard_sprite(
    sprite_number: u8,
    alternate_display: bool,
) -> Option<Vec<StandardSpritePreviewTile>> {
    render_lunar_magic_standard_sprite_with_mode(
        sprite_number,
        StandardSpritePreviewMode {
            alternate_display,
            alternate_graphics: false,
            sprite_graphics_mode: 0,
            animation_phase: 0,
            special_display_mode: false,
            placement_first: 0,
            placement_major: 0,
            level_mode: 0,
            level_orientation: StandardLevelOrientation::Horizontal,
            wide_context: StandardSpriteWideContext::ValidShort,
            sprite_8a_sequence_index: 0,
        },
    )
}

/// Renders authenticated sprite handlers with both recovered global display switches.
#[must_use]
#[allow(clippy::too_many_lines)] // Mirrors Lunar Magic's recovered sprite dispatch table.
pub fn render_lunar_magic_standard_sprite_with_mode(
    sprite_number: u8,
    mode: StandardSpritePreviewMode,
) -> Option<Vec<StandardSpritePreviewTile>> {
    if mode.alternate_display && sprite_number <= 10 {
        return parts(&[(0x115, 0, 1)]);
    }
    match sprite_number {
        0x00..=0x03 => parts(&[(0x40 + u16::from(sprite_number), 0, 1)]),
        0x04 if mode.alternate_graphics => parts(&[(0x13, 0, -14), (0x23, 0, 2)]),
        0x04 => parts(&[(0x10, 0, -14), (0x20, 0, 2)]),
        0x05 if mode.alternate_graphics => parts(&[(0x12, 0, -14), (0x22, 0, 2)]),
        0x05 => parts(&[(0x11, 0, -14), (0x21, 0, 2)]),
        0x06 => parts(&[(0x12, 0, -14), (0x22, 0, 2)]),
        0x07 => parts(&[(0x13, 0, -14), (0x23, 0, 2)]),
        0x08 => parts(&[(0x10, 0, -14), (0x20, 0, 2), (0x08, 1, -10)]),
        0x09 if mode.placement_first & 0x10 != 0 => {
            parts(&[(0x10, 0, -15), (0x140, 0, 1), (0x08, 1, -11)])
        }
        0x09 => parts(&[(0x10, 0, -14), (0x20, 0, 2), (0x07, 9, -11)]),
        0x0a => parts(&[(0x11, 0, -14), (0x21, 0, 2), (0x07, 9, -11)]),
        0x0b
        | 0x0c
        | 0x0d
        | 0x0f
        | 0x11
        | 0x12
        | 0x13..=0x18
        | 0x1a
        | 0x1b..=0x1d
        | 0x1f
        | 0x20..=0x28
        | 0x2a
        | 0x2b
        | 0x2e
        | 0x31..=0x34
        | 0x36..=0x3c
        | 0x3e
        | 0x3f..=0x41
        | 0x42
        | 0x43
        | 0x46
        | 0x47
        | 0x4b..=0x50
        | 0x6d..=0x72
        | 0x84
        | 0x86
        | 0x8e
        | 0x9d
        | 0x9f
        | 0xa1
        | 0xa2
        | 0xa3
        | 0xa5
        | 0xa7
        | 0xa8
        | 0xab
        | 0xac
        | 0xad
        | 0xaf..=0xb2
        | 0xb3
        | 0xb9
        | 0xba
        | 0xbb
        | 0xbc
        | 0xbf
        | 0xc0
            if mode.alternate_display =>
        {
            parts(&[(0x115, 0, 1)])
        }
        0x0b => parts(&[(0x11, 0, -14), (0x21, 0, 2), (0x08, 1, -10)]),
        0x0c => parts(&[(0x13, 0, -14), (0x23, 0, 2), (0x07, 9, -11)]),
        0x0d => parts(&[(0x09, 0, 1)]),
        0x0e => parts(&[(0x0a, 0, 1)]),
        0x0f => parts(&[(0x0c, 0, 1)]),
        0x10 => parts(&[
            (0x06, -10, -7),
            (if mode.alternate_display { 0x115 } else { 0x0c }, 0, 1),
            (0x07, 13, -7),
        ]),
        0x11 => parts(&[(0x0d, 0, 1)]),
        0x8a if mode.sprite_8a_sequence_index <= 3 => {
            parts(&[(0x110 + u16::from(mode.sprite_8a_sequence_index), 0, 1)])
        }
        0x12 | 0x8a => parts(&[(0x01, 0, 0)]),
        0x13 => parts(&[(0x0e, 0, 1)]),
        0x14 => parts(&[(0x0f, 0, 1)]),
        0x15 | 0x17 | 0x18 => parts(&[(0x14, 0, 1)]),
        0x16 => parts(&[(0x15, 0, 1)]),
        0x1a => parts(&[(0x16, 8, -31), (0x26, 8, -15)]),
        0x19 => render_text_lines(&[("Display Level", 0), ("  Message 1  ", 8)]),
        0x1b => parts(&[(0x24, 0, 1)]),
        0x1c => parts(&[(0x25, 0, 1)]),
        0x1d => parts(&[(0x17, 0, 1)]),
        // Dispatch $1E @ $004C45B0 branches on the low bit of its packed
        // placement coordinate, not the first encoded sprite-record byte.
        0x1e => render_handler_1e(mode.placement_major),
        0x1f => parts(&[(0x18, 0, -15), (0x28, 0, 1)]),
        0x20 => parts(&[(0x03, 4, 0), (0x04, -4, 8), (0x05, 12, 8)]),
        0x21 => parts(&[(0x1a, 0, 1)]),
        0x22..=0x24 => {
            let variant = u16::from(mode.placement_first & 1) * 0xd4;
            let base = 0x50 + u16::from(sprite_number - 0x22);
            parts(&[(base + variant, 0, -4), (base + 0x10 + variant, 0, 12)])
        }
        0x25 => {
            let variant = u16::from(mode.placement_first & 1) * 0xd4;
            parts(&[(0x53 + variant, 0, -4), (0x63 + variant, 0, 12)])
        }
        0x26 => parts(&[(0x34, 4, 1), (0x35, 20, 1), (0x44, 4, 17), (0x45, 20, 17)]),
        0x27 => parts(&[(0x2a, 0, 1)]),
        0x28 => render_handler_28(),
        0x2a => parts(&[(0x1b, 8, 15), (0x2b, 8, 31)]),
        0x2b => parts(&[(0x1ce, 0, 0)]),
        0x2c => {
            let definition = match mode.placement_first & 3 {
                0 => 0x71,
                1 | 3 => 0x72,
                2 => 0x73,
                _ => unreachable!(),
            };
            parts(&[(definition, 0, 1)])
        }
        0x2d => parts(&[(0x80, 0, 1)]),
        0x2e => parts(&[(0x2d, 0, 1)]),
        0x2f => parts(&[(0x1e, 0, 1)]),
        0x31 => parts(&[(0x37, 0, 1)]),
        0x32 => parts(&[(0x36, -8, -14), (0x46, 0, 1)]),
        0x33 => parts(&[(0x13c, 0, 0), (0x119, -1, 20), (0x145, 2, 28)]),
        0x34 => parts(&[(0x2e, -16, 3), (0x2f, 0, 3)]),
        0x35 => parts(&[(0x90, -10, 1), (0xa0, 0, 17)]),
        0x36 => parts(&[(0x1f, 0, 1)]),
        // Dispatch $37 @ $004C5230 emits definition $1F; $38 is a separate
        // handler despite the two adjacent sprite numbers.
        0x37 if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0x37 => parts(&[(0x1f, 0, 1)]),
        // Dispatch entry $38 points at $004C5260 and emits definition $38.
        0x38 => parts(&[(0x38, 0, 1)]),
        // Dispatch $39 @ $004C5290 emits the same single Boo definition as $DE's
        // repeated pattern, with Lunar Magic's ordinary one-pixel baseline offset.
        0x39 => parts(&[(if mode.alternate_display { 0x115 } else { 0x48 }, 0, 1)]),
        0x3a..=0x3b => render_square_handler(
            0x39 + u16::from(sprite_number - 0x39) * 2,
            mode.placement_first,
        ),
        0x3c => parts(&[(0x54, 8, -15), (0x64, 0, 1)]),
        // Dispatch entry $3D points at $004C55C0. The native handler first emits
        // definition $54 one row above at x+8, then definition $64 at the placement.
        0x3d if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0x3d => parts(&[(0x54, 8, -15), (0x64, 0, 1)]),
        // Dispatch entry $3E points at $004C5630. The low bit of the packed
        // major coordinate passed to the native handler selects $55 or $65.
        0x3e => parts(&[(0x55 + u16::from(mode.placement_major & 1) * 0x10, 0, 1)]),
        0x3f => parts(&[(0x56, -6, -14), (0x67, -6, 2)]),
        0x40 => parts(&[(0x74, 0, 1), (0x75, 8, 1), (0x76, 24, 1)]),
        0x41 => parts(&[(0x154, 0, 1), (0x155, 8, 1), (0x156, 24, 1)]),
        // Dispatch slot $42 points at $004C57E0 and emits Lunar Magic's horizontal
        // three-definition dolphin preview.
        0x42 => parts(&[(0x154, 0, 1), (0x155, 8, 1), (0x156, 24, 1)]),
        0x43 => parts(&[(0x94, 0, 1), (0x57, -12, 1), (0x95, 16, 1)]),
        0x44 => parts(&[(0x96, 4, 1)]),
        0x45 => parts(&[(0x97, -16, 1), (0x98, -4, 1), (0x99, 12, 1), (0x88, -2, -7)]),
        0x46 => parts(&[(0x14, -2, 1)]),
        0x47 => parts(&[(0x89, 0, -3)]),
        // Dispatch $48 @ $004C5AA0 emits definition $89 three pixels above
        // the placement, or the shared $115 marker in alternate display mode.
        0x48 if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0x48 => parts(&[(0x89, 0, -3)]),
        0x49 => parts(&[(0xa6, 0, 1)]),
        0x4a => parts(&[(0x58, 8, -16), (0x68, 8, 0)]),
        0x4b => render_handler_4b(mode.placement_first),
        0x4c => parts(&[(0xa7, 0, 1), (0xa8, 0, 1)]),
        // $004C5C60 emits both overlapping parts of the small platform sprite.
        0x4d => parts(&[(0xa7, 0, 1), (0xa8, 0, 1)]),
        0x4e => parts(&[(0x16, 8, -8), (0xb4, 8, 8)]),
        // RenderConditionalTiles16AndB4 @ $004C5D10 emits only this pair.
        0x4f => parts(&[(0x16, 8, -8), (0xb4, 8, 8)]),
        // Dispatch $50 @ $004C5D70 first emits the shared $16/$B4 pair, then
        // adds the two plant-head definitions one cell above it.
        0x50 => parts(&[(0x16, 8, -8), (0xb4, 8, 8), (0xb5, 6, -16), (0xb6, 18, -16)]),
        0x51 => parts(&[(0x59, -4, 0), (0x69, 4, 0), (0x69, 20, 0), (0x79, 28, 0)]),
        // Dispatch $52 @ $004C5E50 is the same four-definition platform
        // geometry as $51, including its packed-coordinate edge transition.
        0x52 => parts(&[(0x59, -4, 0), (0x69, 4, 0), (0x69, 20, 0), (0x79, 28, 0)]),
        0x53 => render_handler_53(),
        // Dispatch $54 @ $004C5EE0 walks a three-by-three packed-cell grid,
        // advancing the definition index across $128-$12A, $138-$13A, and
        // $148-$14A. $56 is a distinct neighboring handler.
        0x54 => parts(&[
            (0x128, 8, 8),
            (0x129, 24, 8),
            (0x12a, 40, 8),
            (0x138, 8, 24),
            (0x139, 24, 24),
            (0x13a, 40, 24),
            (0x148, 8, 40),
            (0x149, 24, 40),
            (0x14a, 40, 40),
        ]),
        0x55 | 0x57 => parts(&[
            (0x6a, 0, 1),
            (0x5b, 16, 1),
            (0x5b, 32, 1),
            (0x5b, 48, 1),
            (0x5a, 64, 1),
        ]),
        0x56 | 0x58 => parts(&[
            (0x5d, 0, 1),
            (0x5e, 16, 1),
            (0x5f, 32, 1),
            (0x6e, 8, 17),
            (0x6f, 24, 17),
        ]),
        0x59 => parts(&[
            (0x5c, 0, 1),
            (0x5c, 0, 17),
            (0x6c, 0, 33),
            (0x5c, 0, -15),
            (0x5c, 0, -31),
        ]),
        // Dispatch $5A @ $004C6230 is the horizontal five-cell $5C/$6C cross.
        0x5a => parts(&[
            (0x5c, 0, 1),
            (0x5c, 16, 1),
            (0x6c, 32, 1),
            (0x5c, -16, 1),
            (0x5c, -32, 1),
        ]),
        // Dispatch $5B @ $004C6300 emits the adjacent $142-$144 trio.
        0x5b => parts(&[(0x142, 0, 0), (0x143, 16, 0), (0x144, 32, 0)]),
        // Dispatch $5C @ $004C6380 is the five-cell floating platform.
        0x5c => parts(&[
            (0x7b, 0, 1),
            (0x6b, 16, 1),
            (0x6b, 32, 1),
            (0x6b, 48, 1),
            (0x7a, 64, 1),
        ]),
        // Dispatch $5D @ $004C6450 emits the short two-row platform.
        0x5d => parts(&[
            (0x8a, 0, 0),
            (0x8b, 16, 0),
            (0xff, 32, 0),
            (0x9a, 8, 16),
            (0x13b, 24, 16),
        ]),
        // Dispatch $5E @ $004C64C0 widens that platform to five upper and
        // four lower definitions.
        0x5e => parts(&[
            (0x8a, 0, 1),
            (0x8b, 16, 1),
            (0x8b, 32, 1),
            (0x8b, 48, 1),
            (0xff, 64, 1),
            (0x9a, 8, 17),
            (0x9b, 24, 17),
            (0x9b, 40, 17),
            (0x13b, 56, 17),
        ]),
        // Dispatch $5F @ $004C6600 advances through Lunar Magic's packed
        // coordinates to emit five vertical chain links and a four-part platform.
        0x5f => {
            let mut values = Vec::with_capacity(9);
            for row in 1_i16..=5 {
                values.push((0x6d, 32, row * 16));
            }
            values.extend([(0x7c, 8, 9), (0x7d, 24, 9), (0x7d, 40, 9), (0x7e, 56, 9)]);
            parts(&values)
        }
        // Dispatch $60 @ $004C6720 is the adjacent $8C/$8D pair.
        0x60 => parts(&[(0x8c, 0, 1), (0x8d, 16, 1)]),
        // Dispatch $61 @ $004C6780 advances one packed cell after each call
        // and emits four adjacent skull-raft definitions at y+4.
        0x61 => parts(&[(0x7f, 0, 4), (0x7f, 16, 4), (0x7f, 32, 4), (0x7f, 48, 4)]),
        // $004C67D0 emits only definitions $7C/$7D/$7E.
        0x62 => render_left_chain(false),
        // Dispatch $63 @ $004C6980 tests the low bit of Lunar Magic's packed
        // major coordinate argument. Level $12C proves this is not byte 0 of
        // the serialized sprite record: its record byte is odd at even major
        // coordinate $12 and must take the five-part platform path.
        0x63 => render_left_chain(mode.placement_major & 1 == 0),
        0x64 => render_handler_64(mode),
        0x65 => render_handler_65_66(mode, false),
        0x66 => render_handler_65_66(mode, true),
        0x67 => render_handler_67(mode),
        0x68 => render_handler_68(mode),
        0x69 => parts(&[(0x27, 3, 0), (0x9c, 7, 4)]),
        0x6a => parts(&[(0xb7, 0, 1), (0xb7, 16, 1), (0xb7, 24, 1)]),
        // Dispatch $6B @ $004C6FC0 advances one packed cell before its second
        // call, then offsets the third tile eight pixels farther right.
        0x6b => parts(&[(0xb7, 0, 1), (0xb7, 16, 1), (0xb7, 24, 1)]),
        // Dispatch $6C @ $004C7030 is the left-extending counterpart.
        0x6c => parts(&[(0xb7, -16, 1), (0xb7, -32, 1), (0xb7, -40, 1)]),
        // The installed dispatch targets from $6D through $73 are consecutive,
        // but the previous table was shifted backward by one slot.
        0x6d => parts(&[(0x80b8, 0, 0)]),
        0x6e => parts(&[(0xd5, -8, 1), (0xd6, 8, 1), (0xc5, -8, -15), (0xc6, 8, -15)]),
        0x6f => parts(&[(0xb9, -2, 1)]),
        0x70 => parts(&[
            (0xc7, 1, 1),
            (0xd7, 0, 17),
            (0xc7, 1, 33),
            (0xd7, 0, 49),
            (0xc7, 1, 65),
        ]),
        0x71 => parts(&[(0xe5, -3, 8), (0xe6, 13, 8), (0xe7, 5, 11)]),
        0x72 => parts(&[(0xf5, -3, 8), (0xf6, 13, 8), (0xf7, 5, 11)]),
        // $004C7330 draws the shared Koopa body ($42) followed by its
        // coordinate-direction head ($E8/$F8) eight pixels to the right.
        0x73 if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0x73 => parts(&[
            (0x42, 0, 1),
            (0xe8 + u16::from(mode.placement_major & 1) * 0x10, 8, 1),
        ]),
        // Dispatches $74–$78 @ $004C73A0–$004C7420 are five consecutive
        // single-definition handlers. $81 aliases the first one.
        0x74 => parts(&[(0x101, 0, 0)]),
        0x75 => parts(&[(0x104, 0, 0)]),
        0x76 => parts(&[(0x105, 0, 0)]),
        0x77 => parts(&[(0x106, 0, 0)]),
        0x78 => parts(&[(0x100, 0, 0)]),
        0x79 => parts(&[(0xc8, 0, 1)]),
        0x7a => parts(&[(0xd8, 4, 4)]),
        0x7b => parts(&[(0xca, 0, 0), (0xc9, -16, 0)]),
        0x7c => parts(&[(0xcc, 0, 1), (0xcd, 16, 1), (0xce, 0, 17), (0xcf, 16, 17)]),
        0x7d => parts(&[(0xba, 4, -1)]),
        0x7e => parts(&[(0x06, -8, -9), (0x07, 16, -9), (0xcb, -5, -1)]),
        0x7f => parts(&[(0x06, -8, -9), (0x07, 16, -9), (0x103, -5, -1)]),
        0x80 => parts(&[(0x0b, 0, 1)]),
        0x81 => parts(&[(0x101, 0, 0)]),
        0x82 => render_flagged_variant_handler(mode.placement_first, true),
        0x83 => render_handler_83(mode.placement_first),
        0x84 => parts(&[
            (0xdd, 0, -15),
            (0xdc, 24, -1),
            (0xdb, 16, 0),
            (0xda, 8, 1),
            (0xdb, 32, 0),
            (0xd9, 0, 1),
        ]),
        // Dispatch $86 @ $004C7A00 draws Wiggler as a six-part composite.
        0x86 => parts(&[
            (0xdd, 0, -15),
            (0xdc, 24, -1),
            (0xdb, 16, 0),
            (0xda, 8, 1),
            (0xdb, 32, 0),
            (0xd9, 0, 1),
        ]),
        // The native $87 entry reuses the complete $85 geometry.
        0x85 | 0x87 => parts(&[
            (0x27, -5, 0),
            (0x27, 5, 1),
            (0x27, -3, 4),
            (0x27, 3, 4),
            (0x9c, 4, 8),
        ]),
        0x88 => parts(&[(0x06, 0, 1)]),
        // Handler $8B is the native two-tile DE/DF renderer.  $89 has an
        // independently installed handler but reaches the same tile geometry.
        0x89 | 0x8b => parts(&[(0xde, 0, 0), (0xdf, 16, 0)]),
        0x8c => {
            let mut values = Vec::with_capacity(8);
            for row in 0_i16..4 {
                values.extend([(0x109, 4, row * 16 + 4), (0x10a, 2, row * 16 + 4)]);
            }
            parts(&values)
        }
        // $8F dispatches to $004C7F20. Like $8D, it draws two E9/EA platform
        // pairs; bit 0 of the first placement byte selects a four- or eight-cell
        // gap between the pairs.
        0x8d | 0x8f => {
            let spacing = if mode.placement_first & 1 == 0 {
                128
            } else {
                64
            };
            parts(&[
                (0xe9, -8, 0),
                (0xea, 8, 0),
                (0xe9, spacing - 8, 0),
                (0xea, spacing + 8, 0),
            ])
        }
        0x8e => render_handler_8e(),
        // Dispatch $90 @ $004C7FA0 calls the same 4×4 definition-grid helper
        // used by $8E, with base $100 producing definitions $1C0..$1F3.
        0x90 if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0x90 => render_handler_8e(),
        // Dispatch $91 @ $004C7FE0 is the four-part Chargin' Chuck preview.
        // It is also reused verbatim by dispatch $96.
        0x91 | 0x96 => parts(&[
            (0x1ee, -6, -11),
            (0x1fe, -6, 1),
            (0x1ff, 10, 1),
            (0x1ef, 4, -15),
        ]),
        0x92 => {
            let mut values = vec![
                (0x1fa, -4, 1),
                (0x1fb, 12, 1),
                (0x1ea, -21, 1),
                (0x1eb, -3, 1),
            ];
            if mode.placement_first & 1 == 0 {
                values.push((0x54, -2, -8));
            }
            values.push((
                if mode.placement_first & 1 == 0 {
                    0x169
                } else {
                    0x1ed
                },
                0,
                5,
            ));
            parts(&values)
        }
        0x93 => parts(&[
            (0x1f8, -8, -1),
            (0x1f9, 8, -1),
            (0x1e8, -24, -1),
            (0x1e9, -8, -1),
        ]),
        0x94 => parts(&[
            (0x1ee, -5, -9),
            (0x1f6, -16, 4),
            (0x1f7, 0, 1),
            (0x1ec, -24, -8),
        ]),
        // Dispatch-table entry $95 points at $004C81E0 and always emits this four-part
        // two-row composite. The placement-dependent tongue-like handler is entry $98.
        0x95 => parts(&[
            (0x1f8, -8, -1),
            (0x1f9, 8, -1),
            (0x1e8, -8, -17),
            (0x1e9, 8, -17),
        ]),
        0x97 => parts(&[
            (0x1de, -12, 1),
            (0x1df, 4, 1),
            (0x1cf, -14, 1),
            (0x1ce, 20, 0),
        ]),
        0x98 => parts(&[
            (0x1dc, -16, 1),
            (0x1dd, 0, 1),
            (0x1db, -32, 1),
            (0x1cc, -32, 1),
            (0x1cd, -16, 1),
        ]),
        0x99 => parts(&[
            (0x1cb, 0, 0),
            (0x1cb, 16, 0),
            (0x1d9, -14, -10),
            (0x1da, 30, -10),
        ]),
        0x9a => render_handler_9a(mode.level_orientation),
        // Dispatch $9D @ $004C8750 receives Lunar Magic's packed major
        // coordinate byte. Its low two bits choose the bubble payload.
        0x9d => render_handler_9a_legacy(mode.placement_major as u8, mode.level_orientation),
        // Dispatch $9B @ $004C85D0 shifts the packed placement upward for
        // two rows of the compact five-part preview.
        0x9b => parts(&[
            (0x1dc, 0, -15),
            (0x1dd, 16, -15),
            (0x1db, -16, -15),
            (0x1cc, 0, -31),
            (0x1cd, 16, -31),
        ]),
        // Dispatch $9C @ $004C86C0 emits two body cells and two overlays.
        0x9c => parts(&[
            (0x1cb, 0, 0),
            (0x1cb, 16, 0),
            (0x1d9, -14, -10),
            (0x1da, 30, -10),
        ]),
        0x9e => render_handler_9e(mode.placement_major),
        // Sprite $9F is Banzai Bill. The game draws it as a 4×4 grid of 16×16 OAM
        // tiles; using the unrelated five-part preview made the level-$105 obstacle
        // appear at roughly one quarter of its native 64×64 size.
        0x9f => render_banzai_bill(),
        0xa0 => render_handler_a0(mode.placement_first),
        0xa1 => {
            let base = 0x120 + u16::from(mode.placement_first & 1) * 2;
            parts(&[
                (base, -8, -8),
                (base + 1, 8, -8),
                (base + 0x10, -8, 8),
                (base + 0x11, 8, 8),
            ])
        }
        0xa2 => parts(&[(
            if mode.sprite_graphics_mode & 0x0f == 2 {
                0xa9
            } else {
                0xf9
            } + u16::from(mode.placement_first & 1),
            0,
            1,
        )]),
        0xa3 => render_handler_a3(mode.placement_first),
        0xa4 => parts(&[(0xfb, 0, 0)]),
        // Dispatch $A5 @ $004C8D30 selects $A9/$AA only in sprite graphics
        // mode 2; every other mode uses $F9/$FA. Placement bit 0 selects the pair.
        0xa5 => parts(&[(
            if mode.sprite_graphics_mode & 0x0f == 2 {
                0xa9
            } else {
                0xf9
            } + u16::from(mode.placement_first & 1),
            0,
            1,
        )]),
        // Dispatch $A6 @ $004C8DB0 draws the four 32x32 corner definitions and
        // a placement-bit-selected center overlay.
        0xa6 if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0xa6 => parts(&[
            (0x1d7, -8, 8),
            (0x1d8, 8, 8),
            (0x1c7, -8, -8),
            (0x1c8, 8, -8),
            (0x1c6, i16::from(mode.placement_first & 1) * 8, 0),
        ]),
        0xa7 => parts(&[(0x1c9, 0, 1), (0x1ca, 16, 1)]),
        // Dispatch $A8 @ $004C8F10 emits definition $FC eight pixels above
        // the placement in the ordinary display mode. The alternate-display
        // $115 branch is handled by the shared guard above.
        0xa8 => parts(&[(0xfc, 0, -8)]),
        0xa9 => {
            let mut values = vec![(0x16d, 0, 1)];
            for column in 1_i16..=4 {
                values.push((0x208, -column * 16, 1));
            }
            parts(&values)
        }
        // Dispatch entry $AA points at $004C9040. It emits definitions $1C9 and
        // $1CA in two horizontally adjacent cells.
        0xaa if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0xaa => parts(&[(0x1c9, 0, 1), (0x1ca, 16, 1)]),
        // Sprite $AB is Rex. Its ordinary standing frame uses the recovered two-part Rex
        // definitions; the former long Blargg-like composite belonged to a different dispatch
        // entry and visibly stretched each Rex across five tiles in pristine level $105.
        0xab => parts(&[(0x18d, -4, -15), (0x20b, 0, 0)]),
        // Dispatch $AC @ $004C9120 emits definition $16D at the placement,
        // then four $208 definitions one cell upward each iteration.
        0xac => parts(&[
            (0x16d, 0, 1),
            (0x208, 0, -15),
            (0x208, 0, -31),
            (0x208, 0, -47),
            (0x208, 0, -63),
        ]),
        // Dispatch $AD @ $004C9180 is the downward counterpart: $17D at
        // the placement followed by four $16E definitions one cell apart.
        0xad => parts(&[
            (0x17d, 0, 1),
            (0x16e, 0, 17),
            (0x16e, 0, 33),
            (0x16e, 0, 49),
            (0x16e, 0, 65),
        ]),
        0xae => parts(&[(0xb8, 0, 0)]),
        0xaf => parts(&[(0x15d, 0, 0)]),
        0xb0 => parts(&[(0x14d, 0, 1)]),
        // Dispatch $B1 @ $004C9400 emits only definition $B8 at the placement.
        0xb1 => parts(&[(0xb8, 0, 0)]),
        0xb2 => parts(&[(0x13d, 0, 0)]),
        0xb3 => parts(&[(
            if mode.special_display_mode && mode.sprite_graphics_mode & 0x0f == 0x0d {
                0x116
            } else {
                0x12d
            },
            0,
            0,
        )]),
        // Dispatch $B4 @ $004C94A0 emits a four-definition 2x2 composite,
        // unless Lunar Magic's alternate-number display is active.
        0xb4 if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0xb4 => parts(&[
            (0x0ab, 0, 1),
            (0x0ac, 16, 1),
            (0x0bb, 0, 17),
            (0x0bc, 16, 17),
        ]),
        // Dispatch $B5 @ $004C9530 is a single definition with the same
        // alternate-number override.
        0xb5 if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0xb5 => parts(&[(0x13d, 0, 0)]),
        // Dispatch $B6 @ $004C9570 uses the generic alternate marker first,
        // then selects the castle-specific definition only for graphics mode $D.
        0xb6 if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0xb6 => parts(&[(
            if mode.special_display_mode && mode.sprite_graphics_mode & 0x0f == 0x0d {
                0x116
            } else {
                0x12d
            },
            0,
            0,
        )]),
        // Dispatch $B7 @ $004C95E0 is the three-definition platform
        // composite; it has no placement-coordinate branch.
        0xb7 => parts(&[(0x185, 16, 1), (0x194, 16, 1), (0x195, 32, 1)]),
        0xb8 => parts(&[
            (0x18b, 0, 1),
            (0x18c, 16, 1),
            (0x19b, 16, 1),
            (0x19c, 32, 1),
        ]),
        // Dispatch entry $B9 points at $004C9710. It emits the three recovered
        // line-guided-sprite tiles; bit 0 of the placement byte selects $10C/$10D.
        0xb9 => parts(&[
            (0x10b, 0, 1),
            (0x10c + u16::from(mode.placement_first & 1), 0, 1),
            (0x10a, 3, 1),
        ]),
        // Dispatch $BA @ $004C9770 emits the horizontal flying-platform
        // composite. Lunar Magic branches on the low bit of the packed major
        // coordinate passed to the handler, rather than the first stream byte.
        0xba => parts(&[
            (0x198, 0, 1),
            (0x199, 16, 1),
            (
                if mode.placement_major & 1 == 0 {
                    0x184
                } else {
                    0x187
                },
                12,
                5,
            ),
        ]),
        // Dispatch $BB @ $004C97F0 is a 2x2 composite; it is not the unrelated
        // single definition $17B.
        0xbb => parts(&[
            (0x18b, 0, 1),
            (0x18c, 16, 1),
            (0x19b, 0, 17),
            (0x19c, 16, 17),
        ]),
        0xbc => parts(&[
            (0x174, 0, 1),
            (0x175, 16, 1),
            (0x164, -16, 1),
            (0x165, 0, 1),
        ]),
        // Dispatch entry $BD points at $004C9990 and emits definition $02.
        0xbd if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0xbd => parts(&[(0x02, 0, 1)]),
        // Dispatch entry $BE points at $004C99C0. The ordinary native path emits
        // only definition $17B; the former five-part winged-block composite came
        // from a different handler.
        0xbe if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0xbe => parts(&[(0x17b, 0, 1)]),
        // Dispatch $BF @ $004C9A00 draws Mega Mole as four adjacent quadrant
        // definitions. The former single $166 definition belongs to a different
        // native handler and reduced the preview to a small fragment.
        0xbf => parts(&[
            (0x174, 0, 1),
            (0x175, 16, 1),
            (0x164, 0, -15),
            (0x165, 16, -15),
        ]),
        // Dispatch $C0 @ $004C9AD0 advances one packed cell after each call
        // and emits the three adjacent floating-platform definitions.
        0xc0 => parts(&[(0x176, 0, 3), (0x177, 16, 3), (0x178, 32, 3)]),
        0xc1 => {
            // Native handler at 004C9B20. Placement bit 0 shifts the platform two
            // pixels down instead of two pixels up; the wing definitions sit ten
            // pixels above that three-block platform.
            let platform_y = if mode.placement_first & 1 == 0 { -2 } else { 2 };
            parts(&[
                (0x1cb, 0, platform_y),
                (0x1cb, 16, platform_y),
                (0x1cb, 32, platform_y),
                (0x1da, 46, platform_y - 10),
                (0x1d9, -14, platform_y - 10),
            ])
        }
        // Dispatch-table entry $C2 points at $004C9BE0.  Its ordinary path emits only
        // definition $166; the former 20-part composite belongs to another native entry.
        0xc2 if mode.alternate_display => parts(&[(0x115, 0, 1)]),
        0xc2 => parts(&[(0x166, 0, 0)]),
        0xc3 => parts(&[(0x179, 0, 0)]),
        // Dispatch $C4 @ $004C9CC0 emits the four-cell $EC/$ED/$ED/$EE
        // platform. Treating it as external-definition sentinel $8101 made
        // every platform in level $136 appear as a mushroom.
        0xc4 => parts(&[(0xec, 0, 1), (0xed, 16, 1), (0xed, 32, 1), (0xee, 48, 1)]),
        0xc5 => parts(&[(0x167, 0, 1)]),
        // Dispatch $C6 @ $004C9E70 emits only definition $179.
        0xc6 => parts(&[(0x179, 0, 0)]),
        // Dispatch $C7 @ $004C9E90 emits the external-definition sentinel
        // $8101 at the placement; the dolphin composite belongs to $CA.
        0xc7 => parts(&[(0x8101, 0, 0)]),
        // Dispatch $C8 @ $004C9EB0 emits only definition $167 at y+1.
        0xc8 => parts(&[(0x167, 0, 1)]),
        0xc9 => parts(&[
            (if mode.alternate_display { 0x115 } else { 0x38 }, 0, 1),
            (0x114, 0, -8),
        ]),
        // $004C9F10 advances one row and emits the recovered three-part dolphin preview.
        0xca => parts(&[(0x158, 0, 26), (0x159, 16, 26), (0x168, 8, 16)]),
        0xcb => render_handler_ca_cb(true, mode.alternate_display),
        0xcc if mode.alternate_display => parts(&[(0x115, 0, 1), (0x115, 16, 1), (0x114, 5, -16)]),
        0xcc => parts(&[
            (0x56, -6, -14),
            (0x66, -6, 2),
            (0x56, 10, -14),
            (0x67, 10, 2),
            (0x114, 5, -16),
        ]),
        0xcd => parts(&[(0x154, 0, 0), (0x114, 0, -8), (0x155, 8, 0), (0x156, 24, 0)]),
        0xce => parts(&[(0x84, 0, 0), (0x114, 0, -8), (0x85, 16, 0), (0x86, 24, 0)]),
        0xcf => parts(&[
            (if mode.alternate_display { 0x115 } else { 0x14 }, 0, 1),
            (0x114, 0, -8),
        ]),
        0xd0 => render_text_lines(&[(" Turn Off ", 0), ("Generator2", 8)]),
        0xd1 if mode.alternate_display => parts(&[(0x115, 0, 1), (0x114, 0, 0)]),
        0xd1 => parts(&[(0xe5, -3, 8), (0xe6, 13, 8), (0xe7, 5, 11), (0x114, 0, 0)]),
        0xd2 => render_handler_d2(),
        0xd3 if mode.alternate_display => parts(&[(0x115, 0, 1), (0x114, 0, 0)]),
        0xd3 => parts(&[(0xe5, -3, 8), (0xe6, 13, 8), (0xe7, 5, 11), (0x114, 0, 0)]),
        0xd4 => render_handler_d2(),
        0xd5 => parts(&[
            (if mode.alternate_display { 0x115 } else { 0x25 }, 0, 1),
            (0x25, 8, 1),
            (0x114, 0, -8),
        ]),
        0xd6 => parts(&[(0x157, 0, 0), (0x114, 0, -8)]),
        0xd7 => parts(&[(0x11d, 0, 0), (0x114, 0, -8)]),
        0xd8 => parts(&[
            (if mode.alternate_display { 0x115 } else { 0x14d }, 0, 1),
            (0x114, 0, -8),
        ]),
        0xd9 => render_text_lines(&[(" Turn Off ", 0), ("Generators", 8)]),
        0xda => parts(&[(if mode.alternate_display { 0x115 } else { 0x32 }, 0, 1)]),
        // Dispatch $DB @ $004CA590 selects the normal or alternate-graphics
        // shell definition; $DC @ $004CA5E0 always uses the latter.
        0xdb => parts(&[(
            if mode.alternate_display {
                0x115
            } else if mode.alternate_graphics {
                0x32
            } else {
                0x31
            },
            0,
            1,
        )]),
        0xdc => parts(&[(if mode.alternate_display { 0x115 } else { 0x32 }, 0, 1)]),
        0xdd => parts(&[(if mode.alternate_display { 0x115 } else { 0x33 }, 0, 1)]),
        // Dispatch $DE @ $004CA640 emits definition $48 at the placement and at
        // four packed-cell neighbors. The signed offsets below are the materialized
        // horizontal/vertical editor-grid coordinates. The old three-arm composite
        // belongs to the distinct $E0 dispatch at $004CA790.
        0xde => {
            let definition = if mode.alternate_display { 0x115 } else { 0x48 };
            let offsets = match mode.level_orientation {
                StandardLevelOrientation::Horizontal => {
                    [(0, 16), (-32, 16), (-16, -16), (32, 16), (16, -16)]
                }
                StandardLevelOrientation::Vertical => {
                    [(16, 0), (16, -32), (-16, -16), (16, 32), (-16, 16)]
                }
            };
            parts(&offsets.map(|(x, y)| (definition, x, y)))
        }
        0xdf => parts(&[(0x1b8, 0, 0), (0x114, 0, 0)]),
        // Dispatch entry $E0 points at $004CA790. Its direction bit comes from the
        // dispatch identity ($E0 is even), not the placement byte; the two recovered
        // loops plus the $004C8BC0 stem form the three-arm platform composite.
        0xe0 => render_handler_de(0),
        0xe1 => parts(&[(0x1b8, 0, 0), (0x114, 0, 0)]),
        0xe2 => parts(&[(0x14c, 0, 0), (0x114, 0, 0)]),
        // RenderTenTileSegmentedMarker emits its ten-element definition and
        // signed-offset tables from index 9 down to index 0.
        0xe3 => parts(&[
            (0x1a6, -62, -49),
            (0x1ab, -30, -72),
            (0x1a8, 7, -79),
            (0x1a6, 43, -66),
            (0x1a6, 70, -37),
            (0x1ab, 81, 0),
            (0x1a8, 71, 38),
            (0x1a6, 45, 67),
            (0x1a6, 9, 80),
            (0x1ab, -29, 75),
        ]),
        0xe4 => parts(&[(0x14b, 0, 0), (0x114, 0, 0)]),
        0xe5 => render_handler_e5(mode),
        // Dispatch $E6 @ $004CAC20 emits definitions $14B and $114 at the
        // placement. The Layer 2 / Smash text belongs to another handler.
        0xe6 => parts(&[(0x14b, 0, 0), (0x114, 0, 0)]),
        0xe7 | 0xef => render_handler_e7(mode),
        // $E8 aliases $E7's native dispatch target at $004CAC50. Its strings at
        // $005C467C begin with "Auto-Scroll", followed by the Special 1..4 labels.
        0xe8 => render_handler_e5(mode),
        0xe9 => render_handler_e9(mode),
        // Dispatch $EA @ $004CAE50 is the Layer 2 scroll-range label.
        0xea => render_handler_e7(mode),
        0xf2 => render_text_lines(&[("   Layer 2   ", 0), ("On/Off Switch", 8)]),
        0xeb | 0xf3 => render_handler_eb(mode),
        0xec | 0xf4 => render_text_lines(&[("Fast BG Scroll", 0)]),
        0xed | 0xf5 => render_handler_ed(mode),
        // $EE/$F0/$F1 retain the native empty/default dispatch entry.
        // $F6-$FF share Lunar Magic's custom-display bookkeeping fallback:
        // it records the placement but has no built-in preview definition.
        _ => None,
    }
}

fn render_handler_83(placement_first: u8) -> Option<Vec<StandardSpritePreviewTile>> {
    // 004c7850: the low two placement bits choose the central definition.  The
    // high bit in these words is part of Lunar Magic's preview-definition
    // selector and must not be mistaken for a Map16 flip flag.
    let center = match placement_first & 3 {
        0 => 0x801a,
        1 => 0x8104,
        2 => 0x8106,
        3 => 0x8100,
        _ => unreachable!(),
    };
    parts(&[
        (0x06, -14, -9),
        (0x07, -8, -9),
        (center, -3, -9),
        (0x108, -3, -1),
    ])
}

fn render_handler_64(mode: StandardSpritePreviewMode) -> Option<Vec<StandardSpritePreviewTile>> {
    let middle_count = if mode.placement_first & 1 != 0
        && mode.wide_context == StandardSpriteWideContext::ValidLong64
    {
        7
    } else {
        3
    };
    let mut values = Vec::with_capacity(middle_count + 2);
    values.push((0x8e, -8, -7));
    for middle in 0..middle_count {
        values.push((0x8f, -8, 9 + i16::try_from(middle).ok()? * 16));
    }
    values.push((0x9f, -8, 9 + i16::try_from(middle_count).ok()? * 16));
    parts(&values)
}

fn render_handler_65_66(
    mode: StandardSpritePreviewMode,
    lower_arm: bool,
) -> Option<Vec<StandardSpritePreviewTile>> {
    let odd = mode.placement_first & 1 != 0;
    if !odd && mode.wide_context == StandardSpriteWideContext::Invalid {
        return None;
    }
    let variant_delta = if odd { 2 } else { 0 };
    let mut values = Vec::with_capacity(6);
    if lower_arm {
        values.extend([
            (0x12c, 8, 23 - variant_delta),
            (0x11c, 8, 9 - variant_delta),
        ]);
    } else {
        values.extend([
            (0xaf, 8, -33 - variant_delta),
            (0xbf, 8, -19 - variant_delta),
        ]);
    }
    values.push((if odd { 0x9e } else { 0x8e }, 8, -7));
    values.push((0x201, if odd { 16 } else { 8 }, -13));
    if odd {
        values.extend([(0x204, 13, -13), (0x205, 13, -13)]);
    } else {
        values.extend([(0x202, 3, -13), (0x203, 3, -13)]);
    }
    parts(&values)
}

fn render_handler_67(mode: StandardSpritePreviewMode) -> Option<Vec<StandardSpritePreviewTile>> {
    if mode.alternate_display {
        return parts(&[(0x115, 0, 1)]);
    }
    if mode.placement_first & 1 == 0 && mode.wide_context == StandardSpriteWideContext::Invalid {
        return None;
    }
    let base = if mode.placement_first & 1 == 0 {
        0xad
    } else {
        0xab
    };
    parts(&[
        (base + 0x10, 0, 1),
        (base + 0x11, 16, 1),
        (base, 0, -15),
        (base + 1, 16, -15),
    ])
}

fn render_handler_68(mode: StandardSpritePreviewMode) -> Option<Vec<StandardSpritePreviewTile>> {
    if mode.alternate_display {
        return parts(&[(0x115, 0, 1)]);
    }
    if mode.placement_first & 1 == 0 && mode.wide_context == StandardSpriteWideContext::Invalid {
        return None;
    }
    parts(&[(
        if mode.placement_first & 1 == 0 {
            0xaa
        } else {
            0xa9
        },
        8,
        -7,
    )])
}

fn render_handler_1e(placement_major: u16) -> Option<Vec<StandardSpritePreviewTile>> {
    let mut values = vec![(0x27, -5, 0), (0x27, 5, 1)];
    if placement_major & 1 != 0 {
        values.push((0xfd, -12, -14));
    }
    values.extend([
        (0x19, 1, -16),
        (0x29, 1, 0),
        (0x27, -3, 4),
        (0x27, 3, 4),
        (0x9c, 4, 8),
    ]);
    if placement_major & 1 != 0 {
        values.extend([
            (0xfe, -28, -14),
            (0xfe, -28, 2),
            (0xfe, -28, 18),
            (0xfe, -28, 34),
            (0x210, -20, 50),
        ]);
    }
    parts(&values)
}

fn render_handler_9a(
    orientation: StandardLevelOrientation,
) -> Option<Vec<StandardSpritePreviewTile>> {
    // $004C8520 addresses one cell along each packed coordinate axis. Which
    // physical axis that represents swaps in vertical levels.
    let (major_x, major_y, minor_x, minor_y) = match orientation {
        StandardLevelOrientation::Horizontal => (-16, 0, 0, -16),
        StandardLevelOrientation::Vertical => (0, -16, -16, 0),
    };
    parts(&[
        (0x1de, major_x + 4, major_y + 1),
        (0x1df, 4, 1),
        (0x1cf, minor_x + 2, minor_y + 1),
        (0x1ce, -minor_x + 4, -minor_y),
    ])
}

fn render_handler_9a_legacy(
    placement_first: u8,
    orientation: StandardLevelOrientation,
) -> Option<Vec<StandardSpritePreviewTile>> {
    let head = match placement_first & 3 {
        0 => 0x1bd,
        1 => 0x1ad,
        2 => 0x14,
        3 => 0x211,
        _ => unreachable!(),
    };
    // RenderFixedFiveTileObjectMarker @ $004C3B30 moves one packed-coordinate
    // axis between the lower and upper pairs. In a horizontal level that is a
    // vertical cell, not another horizontal cell; treating both packed nibbles
    // as X spread the bubble into the "popping" shape seen in level $123.
    let marker = match orientation {
        StandardLevelOrientation::Horizontal => [
            (0x1bf, -1, 3),
            (0x1be, -15, 3),
            (0x1af, -1, -11),
            (0x1ae, -15, -11),
            (0x20c, -16, -12),
        ],
        // Preserve the previously recovered vertical presentation until a
        // vertical vanilla instance supplies framebuffer evidence for the
        // packed-axis swap.
        StandardLevelOrientation::Vertical => [
            (0x1bf, -1, 3),
            (0x1be, -15, 3),
            (0x1af, -17, 5),
            (0x1ae, -31, 5),
            (0x20c, -32, 4),
        ],
    };
    let mut values = vec![(head, -8, 0)];
    values.extend(marker);
    parts(&values)
}

fn render_handler_9e(placement_major: u16) -> Option<Vec<StandardSpritePreviewTile>> {
    let direction = if placement_major & 1 == 0 { -1 } else { 1 };
    let grinder_x = if direction < 0 { -16 } else { 0 };
    parts(&[
        // $004C87D0 derives direction from bit 0 of its coordinate argument,
        // advances the packed vertical coordinate for the chain, and offsets the
        // 2×2 grinder one cell left only for the negative direction.
        (0x1d6, direction, 16),
        (0x1d6, direction * 3, 32),
        (0x1c4, grinder_x, 48),
        (0x1c5, grinder_x + 16, 48),
        (0x1d4, grinder_x, 64),
        (0x1d5, grinder_x + 16, 64),
    ])
}

fn render_handler_a0(placement_first: u8) -> Option<Vec<StandardSpritePreviewTile>> {
    let direction = if placement_first & 1 == 0 { -1 } else { 1 };
    parts(&[
        (0xeb, 16 + direction, 0),
        (0xeb, 32 + direction * 3, 0),
        (0xeb, 48 + direction * 5, 0),
        (0xed, 48 + direction * 6, 0),
        (0xec, 32 + direction * 6, 0),
        (0xee, 64 + direction * 6, 0),
    ])
}

fn render_handler_a3(placement_first: u8) -> Option<Vec<StandardSpritePreviewTile>> {
    // RenderThreeEbAndEdEcEeTiles @ $004C8BC0 derives a two-pixel lean from
    // the low placement bit. It advances one packed row for each of three
    // $EB links, then builds the three-cell $EC/$ED/$EE platform around the
    // final coordinate.
    let direction = if placement_first & 1 == 0 { -1 } else { 1 };
    let first_x = direction * 2;
    let second_x = direction * 4;
    let final_x = direction * 6;
    parts(&[
        (0xeb, first_x, 0),
        (0xeb, second_x, 16),
        (0xeb, final_x, 32),
        (0xed, final_x, 48),
        (0xec, final_x - 16, 48),
        (0xee, final_x + 16, 48),
    ])
}

fn render_handler_ca_cb(
    right_definition: bool,
    alternate_display: bool,
) -> Option<Vec<StandardSpritePreviewTile>> {
    if alternate_display {
        parts(&[(0x115, 0, 1), (0x114, -8, -16)])
    } else {
        parts(&[
            (0x56, -6, -14),
            (if right_definition { 0x67 } else { 0x66 }, -6, 2),
            (0x114, -8, -16),
        ])
    }
}

fn render_handler_d2() -> Option<Vec<StandardSpritePreviewTile>> {
    let mut values = vec![(0x1bd, -8, 0)];
    append_handler_3b30(&mut values, 0);
    values.push((0x114, 0, -16));
    values.push((0x1ad, 8, 0));
    append_handler_3b30(&mut values, 16);
    values.push((0x14, 24, 0));
    append_handler_3b30(&mut values, 32);
    parts(&values)
}

fn append_handler_3b30(values: &mut Vec<(u16, i16, i16)>, x_offset: i16) {
    values.extend([
        (0x1bf, x_offset - 1, 3),
        (0x1be, x_offset - 15, 3),
        (0x1af, x_offset - 17, 5),
        (0x1ae, x_offset - 31, 5),
        (0x20c, x_offset - 32, 4),
    ]);
}

fn render_text_lines(lines: &[(&str, i16)]) -> Option<Vec<StandardSpritePreviewTile>> {
    let capacity = lines.iter().map(|(text, _)| text.len() * 2).sum();
    let mut values = Vec::with_capacity(capacity);
    for &(text, y) in lines {
        for (column, character) in text.bytes().enumerate() {
            let x = i16::try_from(column).expect("sprite preview text fits i16") * 8;
            values.push((0x3c7c, x, y));
            values.push((0x3c00 + u16::from(character), x, y));
        }
    }
    parts(&values)
}

fn preview_position_nibble(mode: StandardSpritePreviewMode) -> u8 {
    if mode.level_orientation == StandardLevelOrientation::Vertical {
        mode.placement_first & 0x0f
    } else {
        mode.placement_first >> 4
    }
}

fn render_handler_e5(mode: StandardSpritePreviewMode) -> Option<Vec<StandardSpritePreviewTile>> {
    let position = preview_position_nibble(mode);
    let label = match (mode.level_mode & 3, position) {
        (0, 0) => " Special 1 ",
        (0, 1) => "Special 1-A",
        (1, 0) => " Special 2 ",
        (1, 1) => "Special 2-A",
        (2, 0) => " Special 3 ",
        (3, 0) => " Special 4 ",
        _ => "MAY GLITCH!",
    };
    render_text_lines(&[("Auto-Scroll", 0), (label, 8)])
}

fn render_handler_e7(mode: StandardSpritePreviewMode) -> Option<Vec<StandardSpritePreviewTile>> {
    let position = preview_position_nibble(mode);
    let label = match (mode.level_mode & 3, position) {
        (0, 0) => "   Range 12   ",
        (0 | 2, 1) => "   Range 05   ",
        (1, 0) => "   Range 08   ",
        (3, 0) => "   Range 06   ",
        (3, 1) => "Smash Range 11",
        _ => " MAY GLITCH!! ",
    };
    render_text_lines(&[("Layer 2 Scroll", 0), (label, 8)])
}

fn render_handler_e9(mode: StandardSpritePreviewMode) -> Option<Vec<StandardSpritePreviewTile>> {
    let position = preview_position_nibble(mode);
    let label = match (mode.level_mode & 3, position) {
        (0, 0) => "Sideways Short",
        (1, 0) => "Sideways  Long",
        _ => " MAY GLITCH!! ",
    };
    render_text_lines(&[("Layer 2 Scroll", 0), (label, 8)])
}

fn render_handler_eb(mode: StandardSpritePreviewMode) -> Option<Vec<StandardSpritePreviewTile>> {
    let position = preview_position_nibble(mode);
    let label = if mode.level_mode & 3 == 1 {
        match position {
            0 | 3 => "  Medium   ",
            5 => "  Medium 2 ",
            _ => "   Fast    ",
        }
    } else {
        match position {
            0 | 4 => "   Slow    ",
            1 => "  Medium 2 ",
            _ => "   Fast    ",
        }
    };
    render_text_lines(&[(label, 0), ("Auto-Scroll", 8)])
}

fn render_handler_ed(mode: StandardSpritePreviewMode) -> Option<Vec<StandardSpritePreviewTile>> {
    let label = if preview_position_nibble(mode) == 0 {
        match mode.level_mode & 3 {
            0 => "Sink  Short",
            1 => " Sink Long ",
            2 => "  Rise Up  ",
            3 => " Give Some ",
            _ => unreachable!(),
        }
    } else {
        "MAY GLITCH!"
    };
    render_text_lines(&[("  Layer 2  ", 0), (label, 8)])
}

fn render_handler_de(placement_first: u8) -> Option<Vec<StandardSpritePreviewTile>> {
    let values = if placement_first & 1 == 0 {
        [
            (0xeb, -2, 16),
            (0xeb, -4, 32),
            (0xeb, -6, 48),
            (0xed, -6, 48),
            (0xec, -22, 48),
            (0xee, 10, 48),
            (0xeb, -12, -10),
            (0xeb, -24, -20),
            (0xeb, -36, -30),
            (0xed, -36, -30),
            (0xec, -52, -30),
            (0xee, -20, -30),
            (0xeb, 15, -6),
            (0xeb, 30, -12),
            (0xeb, 45, -18),
            (0xed, 45, -18),
            (0xec, 29, -18),
            (0xee, 61, -18),
        ]
    } else {
        [
            (0xeb, 2, 16),
            (0xeb, 4, 32),
            (0xeb, 6, 48),
            (0xed, 6, 48),
            (0xec, -10, 48),
            (0xee, 22, 48),
            (0xeb, -15, -6),
            (0xeb, -30, -12),
            (0xeb, -45, -18),
            (0xed, -45, -18),
            (0xec, -61, -18),
            (0xee, -29, -18),
            (0xeb, 12, -10),
            (0xeb, 24, -20),
            (0xeb, 36, -30),
            (0xed, 36, -30),
            (0xec, 20, -30),
            (0xee, 52, -30),
        ]
    };
    parts(&values)
}

fn render_handler_8e() -> Option<Vec<StandardSpritePreviewTile>> {
    render_definition_grid(0x1c0, -4, 0)
}

fn render_definition_grid(
    first_definition: u16,
    origin_x: i16,
    origin_y: i16,
) -> Option<Vec<StandardSpritePreviewTile>> {
    let mut values = Vec::with_capacity(16);
    for row in 0_u16..4 {
        for column in 0_u16..4 {
            values.push((
                first_definition + row * 0x10 + column,
                i16::try_from(column).expect("four-column preview") * 16 + origin_x,
                i16::try_from(row).expect("four-row preview") * 16 + origin_y,
            ));
        }
    }
    parts(&values)
}

fn render_flagged_variant_handler(
    placement_first: u8,
    shifted_right: bool,
) -> Option<Vec<StandardSpritePreviewTile>> {
    let definition = match placement_first & 3 {
        0 => 0x801a,
        1 => 0x8104,
        2 => 0x8106,
        3 => 0x8100,
        _ => unreachable!(),
    };
    if shifted_right {
        parts(&[
            (0x06, -8, -9),
            (0x07, 14, -9),
            (definition, 5, -9),
            (0x108, 3, -1),
        ])
    } else {
        parts(&[
            (0x06, -12, -9),
            (0x07, 8, -9),
            (definition, -1, -9),
            (0x108, -3, -1),
        ])
    }
}

fn render_left_chain(long: bool) -> Option<Vec<StandardSpritePreviewTile>> {
    if long {
        parts(&[
            (0x7b, -40, -7),
            (0x6b, -24, -7),
            (0x6b, -8, -7),
            (0x6b, 8, -7),
            (0x7a, 24, -7),
        ])
    } else {
        parts(&[(0x7c, -24, -7), (0x7d, -8, -7), (0x7e, 8, -7)])
    }
}

fn render_handler_4b(placement_first: u8) -> Option<Vec<StandardSpritePreviewTile>> {
    let mut values = Vec::with_capacity(6);
    match placement_first & 3 {
        0 => values.push((0x14, 0, 0)),
        1 => values.push((0x0c, 0, 0)),
        2 => values.push((0x40, 0, 0)),
        3 => {
            values.push((0x10, 0, -16));
            values.push((0x20, 0, 0));
        }
        _ => unreachable!(),
    }
    values.extend(shared_marker_parts());
    parts(&values)
}

fn shared_marker_parts() -> [(u16, i16, i16); 4] {
    [(0xb0, -4, -4), (0xb1, 12, -4), (0xb2, -4, 8), (0xb3, 12, 8)]
}

fn render_handler_53() -> Option<Vec<StandardSpritePreviewTile>> {
    let mut values = Vec::with_capacity(9);
    for row in 0_u16..3 {
        for column in 0_u16..3 {
            values.push((
                0x128 + row * 0x10 + column,
                8 + i16::try_from(column).ok()? * 16,
                i16::try_from(row).ok()? * 16,
            ));
        }
    }
    parts(&values)
}

fn render_square_handler(
    definition: u16,
    placement_first: u8,
) -> Option<Vec<StandardSpritePreviewTile>> {
    parts(&[
        (definition, 0, 0),
        (definition + 1, 16, 0),
        (definition + 0x10, 0, 16),
        (definition + 0x11, 16, 16),
        (0x3f + u16::from(placement_first & 1) * 0x10, 8, 9),
    ])
}

fn render_handler_28() -> Option<Vec<StandardSpritePreviewTile>> {
    let mut values = Vec::with_capacity(20);
    values.push((0xe4, -7, 24));
    for row in 0_u16..4 {
        for column in 0_u16..4 {
            values.push((
                0xc0 + row * 0x10 + column,
                -4 + i16::try_from(column).ok()? * 16,
                i16::try_from(row).ok()? * 16,
            ));
        }
    }
    values.extend([(0xf4, 29, 24), (0xc4, 4, 18), (0xd4, 4, 34)]);
    parts(&values)
}

fn parts(values: &[(u16, i16, i16)]) -> Option<Vec<StandardSpritePreviewTile>> {
    values
        .iter()
        .map(|&(definition_index, x, y)| {
            Some(StandardSpritePreviewTile {
                definition_index,
                subtiles: preview_definition(definition_index)?,
                x,
                y,
            })
        })
        .collect()
}

fn render_banzai_bill() -> Option<Vec<StandardSpritePreviewTile>> {
    let mut values = Vec::with_capacity(16);
    for index in 0_u16..16 {
        let x = i16::try_from(index % 4).ok()?.saturating_mul(16);
        let y = i16::try_from(index / 4).ok()?.saturating_mul(16);
        values.push(StandardSpritePreviewTile {
            definition_index: 0x220 + index,
            subtiles: preview_definition(0x220 + index)?,
            x,
            y,
        });
    }
    Some(values)
}

#[allow(clippy::too_many_lines)] // Sparse authenticated indices are clearer as one lookup table.
pub(crate) fn preview_definition(index: u16) -> Option<[u16; 4]> {
    if (0x3c00..=0x3cff).contains(&index) {
        return Some([index, 0x0019, 0x0019, 0x0019]);
    }
    if (0x220..=0x22f).contains(&index) {
        const TILES: [u16; 16] = [
            0x80, 0x82, 0x84, 0x86, 0xa0, 0x88, 0xce, 0xee, 0xc0, 0xc2, 0xce, 0xee, 0x8e, 0xae,
            0x84, 0x86,
        ];
        let entry = usize::from(index - 0x220);
        let tile = TILES[entry] | 0x0100;
        let palette_and_page = 0x0400;
        if entry >= 14 {
            return Some([
                palette_and_page | 0x8000 | tile.saturating_add(0x10),
                palette_and_page | 0x8000 | tile,
                palette_and_page | 0x8000 | tile.saturating_add(0x11),
                palette_and_page | 0x8000 | tile.saturating_add(1),
            ]);
        }
        return Some([
            palette_and_page | tile,
            palette_and_page | tile.saturating_add(0x10),
            palette_and_page | tile.saturating_add(1),
            palette_and_page | tile.saturating_add(0x11),
        ]);
    }
    Some(match index & 0x7fff {
        0x001 | 0x116 | 0x11b | 0x12b => [0x0400, 0x0410, 0x0401, 0x0411],
        0x002 => [0x4c87, 0x4c97, 0x4c86, 0x4c96],
        0x003 => [0x1188, 0x1019, 0x1019, 0x1019],
        0x004 => [0x0d89, 0x0c19, 0x0c19, 0x0c19],
        0x005 => [0x0998, 0x0819, 0x0819, 0x0819],
        0x008 => [0x0c19, 0x0c19, 0x0c19, 0x0c5d],
        0x007 => [0x0cc6, 0x0cd6, 0x0cc7, 0x0cd7],
        0x006 => [0x4cc7, 0x4cd7, 0x4cc6, 0x4cd6],
        0x010 => [0x1482, 0x1492, 0x1483, 0x1493],
        0x011 => [0x1082, 0x1092, 0x1083, 0x1093],
        0x012 => [0x0c82, 0x0c92, 0x0c83, 0x0c93],
        0x013 => [0x0882, 0x0892, 0x0883, 0x0893],
        0x019 | 0x02d => [0x11ec, 0x11fc, 0x11ed, 0x11fd],
        0x020 => [0x14a2, 0x14b2, 0x14a3, 0x14b3],
        0x021 => [0x10a2, 0x10b2, 0x10a3, 0x10b3],
        0x022 => [0x0ca2, 0x0cb2, 0x0ca3, 0x0cb3],
        0x023 => [0x08a2, 0x08b2, 0x08a3, 0x08b3],
        0x029 => [0x11ee, 0x11fe, 0x11ef, 0x11ff],
        0x030 => [0x548d, 0x549d, 0x548c, 0x549c],
        0x031 => [0x508d, 0x509d, 0x508c, 0x509c],
        0x032 => [0x4c8d, 0x4c9d, 0x4c8c, 0x4c9c],
        0x033 => [0x488d, 0x489d, 0x488c, 0x489c],
        0x040 => [0x14ca, 0x14da, 0x14cb, 0x14db],
        0x041 => [0x10ca, 0x10da, 0x10cb, 0x10db],
        0x042 => [0x0ce2, 0x0cf2, 0x0ce3, 0x0cf3],
        0x043 => [0x08ca, 0x08da, 0x08cb, 0x08db],
        0x009 | 0x04f | 0x1b5 => [0x0dcc, 0x0ddc, 0x0dcd, 0x0ddd],
        0x00a => [0x0019, 0x0019, 0x00eb, 0x00fb],
        0x00b => [0x40ed, 0x40fd, 0x40ec, 0x40fc],
        0x00c => [0x08a8, 0x08b8, 0x08a9, 0x08b9],
        0x00d | 0x0c1 => [0x1982, 0x1992, 0x1983, 0x1993],
        0x00e => [0x1182, 0x1192, 0x1183, 0x1193],
        0x00f => [0x1184, 0x9184, 0x5184, 0xd184],
        0x014 => [0x0967, 0x0977, 0x0968, 0x0978],
        0x015 => [0x0969, 0x0979, 0x096a, 0x097a],
        0x016 => [0x50af, 0x50bf, 0x50ae, 0x50be],
        0x017 => [0x098e, 0x099e, 0x098f, 0x099f],
        0x018 => [0x0da0, 0x0db4, 0x0da5, 0x0db5],
        0x01a => [0x08e8, 0x08f8, 0x08e9, 0x08f9],
        0x01b => [0xd5df, 0xd5cf, 0xd5de, 0xd5ce],
        0x01e => [0x1428, 0x9428, 0x5428, 0xd428],
        0x01f | 0x06e | 0x191 => [0x0588, 0x0598, 0x0589, 0x0599],
        0x024 => [0x018a, 0x019a, 0x018b, 0x019b],
        0x025 => [0x04a6, 0x04b6, 0x04a7, 0x04b7],
        0x026 => [0x50cf, 0x50df, 0x50ce, 0x50de],
        0x027 => [0x0060, 0x0070, 0x0061, 0x0071],
        0x028 | 0x039 => [0x0dc4, 0x0dd4, 0x0dc5, 0x0dd5],
        0x02a => [0x05a2, 0x05b2, 0x45a2, 0x45b2],
        0x02b => [0xd0bf, 0xd0af, 0xd0be, 0xd0ae],
        0x02e => [0xc95d, 0xc94d, 0xc95c, 0xc94c],
        0x02f => [0xc95b, 0xc94b, 0xc95a, 0xc94a],
        0x034 | 0x0af | 0x1b0 => [0x058e, 0x059e, 0x058f, 0x059f],
        0x035 => [0x458e, 0x459e, 0x0419, 0x0419],
        0x036 => [0x0564, 0x0574, 0x0565, 0x0575],
        0x037 => [0x058c, 0x059c, 0x058d, 0x059d],
        0x038 => [0x19ed, 0x19fd, 0x19ee, 0x19fe],
        0x03a => [0x4dc5, 0x4dd5, 0x4dc4, 0x4dd4],
        0x03b => [0x0dc6, 0x0dd6, 0x0dc7, 0x0dd7],
        0x03c => [0x4dc7, 0x4dd7, 0x4dc6, 0x4dd6],
        0x03d => [0x0dc8, 0x0dd8, 0x0dc9, 0x0dd9],
        0x03e => [0x4dc9, 0x4dd9, 0x4dc8, 0x4dd8],
        0x03f => [0x0dca, 0x0dda, 0x0dcb, 0x0ddb],
        0x044 | 0x0bf | 0x1b1 => [0x05ae, 0x05be, 0x05af, 0x05bf],
        0x045 => [0x45ae, 0x45be, 0x0419, 0x0419],
        0x046 => [0x0568, 0x0578, 0x0569, 0x0579],
        0x048 => [0x196a, 0x197a, 0x196b, 0x197b],
        0x049 => [0x8dd4, 0x8dc4, 0x8dd5, 0x8dc5],
        0x04a => [0xcdd5, 0xcdc5, 0xcdd4, 0xcdc4],
        0x04b => [0x8dd6, 0x8dc6, 0x8dd7, 0x8dc7],
        0x04c => [0xcdd7, 0xcdc7, 0xcdd6, 0xcdc6],
        0x04d => [0x8dd8, 0x8dc8, 0x8dd9, 0x8dc9],
        0x04e => [0xcdd9, 0xcdc9, 0xcdd8, 0xcdc8],
        0x050 => [0x154c, 0x155c, 0x154d, 0x155d],
        0x051 => [0x114c, 0x115c, 0x114d, 0x115d],
        0x052 => [0x554d, 0x555d, 0x554c, 0x555c],
        0x060 => [0x1529, 0x1539, 0x152a, 0x153a],
        0x061 => [0x1129, 0x1139, 0x112a, 0x113a],
        0x062 => [0x552a, 0x553a, 0x5529, 0x5539],
        0x053 => [0x514d, 0x515d, 0x514c, 0x515c],
        0x054 => [0x0c19, 0x0de1, 0x0df0, 0x0c19],
        0x055 => [0x0c42, 0x0c52, 0x0c43, 0x0c53],
        0x056 | 0x074 | 0x097 => [0x0de2, 0x0df2, 0x0de3, 0x0df3],
        0x057 => [0x0062, 0x0072, 0x0063, 0x0073],
        0x058 => [0x15ec, 0x15fc, 0x15ed, 0x15fd],
        0x059 => [0x61ec, 0x61fc, 0x61eb, 0x61fb],
        0x05a | 0x19b => [0x05ec, 0x05fc, 0x05ed, 0x05fd],
        0x05b => [0x05eb, 0x05fb, 0x05ec, 0x05fc],
        0x05c => [0x0040, 0x0050, 0x0041, 0x0051],
        0x05d | 0x176 => [0x0585, 0x0595, 0x0586, 0x0596],
        0x05e => [0x4587, 0x4597, 0x4586, 0x4596],
        0x05f | 0x178 => [0x4586, 0x4596, 0x4585, 0x4595],
        0x063 => [0x512a, 0x513a, 0x5129, 0x5139],
        0x064 => [0x4d8d, 0x4d9d, 0x4d8c, 0x4d9c],
        0x065 => [0x0442, 0x0452, 0x0443, 0x0453],
        0x066 => [0x09a3, 0x09b3, 0x49a3, 0x49b3],
        0x067 => [0x09a2, 0x09b2, 0x49a2, 0x49b2],
        0x068 => [0x15ee, 0x15fe, 0x15ef, 0x15ff],
        0x069 => [0x21ea, 0x21fa, 0x21eb, 0x21fb],
        0x06a | 0x1c4 => [0x05ea, 0x05fa, 0x05eb, 0x05fb],
        0x06b => [0x01eb, 0x01fb, 0x01ec, 0x01fc],
        0x06c => [0x4041, 0x4051, 0x4040, 0x4050],
        0x06d => [0x01a2, 0x01b2, 0x01a3, 0x01b3],
        0x06f | 0x1a6 => [0x4589, 0x4599, 0x4588, 0x4598],
        0x071 | 0x12a => [0x5101, 0x5111, 0x5100, 0x5110],
        0x072 => [0x4d01, 0x4d11, 0x4d00, 0x4d10],
        0x073 => [0x4901, 0x4911, 0x4900, 0x4910],
        0x075 => [0x0c19, 0x0c19, 0x0de7, 0x0df7],
        0x076 => [0x0de8, 0x0df8, 0x0de9, 0x0df9],
        0x077 => [0x4dcf, 0x4ddf, 0x4dce, 0x4dde],
        0x078 => [0x11a7, 0x11b7, 0x11a8, 0x11b8],
        0x079 => [0x21eb, 0x21fb, 0x21ec, 0x21fc],
        0x07a => [0x01ec, 0x01fc, 0x01ed, 0x01fd],
        0x07b => [0x01ea, 0x01fa, 0x01eb, 0x01fb],
        0x07c | 0x142 => [0x0160, 0x0170, 0x0161, 0x0171],
        0x07d | 0x143 => [0x0161, 0x0171, 0x0162, 0x0172],
        0x07e | 0x144 => [0x0162, 0x0172, 0x0163, 0x0173],
        0x07f => [0x45e3, 0x45f3, 0x45e2, 0x45f2],
        0x080 => [0x5681, 0x5691, 0x5680, 0x5690],
        0x084 => [0x4daa, 0x4dba, 0x4da9, 0x4db9],
        0x085 => [0x4da8, 0x4db8, 0x4c19, 0x4c19],
        0x086 => [0x4d89, 0x4d99, 0x4d88, 0x4d98],
        0x087 => [0x4def, 0x4dff, 0x4dee, 0x4dfe],
        0x088 => [0x150a, 0x151a, 0x150b, 0x151b],
        0x089 => [0x59c5, 0x59d5, 0x59c4, 0x59d4],
        0x08a => [0x15cb, 0x15db, 0x15cc, 0x15dc],
        0x08b => [0x55cd, 0x55dd, 0x55cc, 0x55dc],
        0x08c => [0x1500, 0x0110, 0x1501, 0x0111],
        0x08d => [0x5501, 0x4111, 0x5500, 0x4110],
        0x08e => [0x0dc0, 0x0dd0, 0x0dc1, 0x0dd1],
        0x08f => [0x01ce, 0x01de, 0x01cf, 0x01df],
        0x090 => [0x1640, 0x1650, 0x1641, 0x1651],
        0x094 => [0x4583, 0x4593, 0x4582, 0x4592],
        0x095 => [0x4581, 0x4591, 0x4580, 0x4590],
        0x096 => [0x00ea, 0x80ea, 0x0019, 0x0019],
        0x098 => [0x1528, 0x1538, 0x1529, 0x1539],
        0x099 => [0x152a, 0x153a, 0x1419, 0x1419],
        0x09a | 0x185 => [0x15e4, 0x15f4, 0x15e5, 0x15f5],
        0x09b => [0x55e6, 0x55f6, 0x55e5, 0x55f5],
        0x09c => [0x114d, 0x1019, 0x1019, 0x1019],
        0x09e => [0x0de0, 0x0df0, 0x0de1, 0x0df1],
        0x09f => [0x01ce, 0x01ee, 0x01cf, 0x01ef],
        0x0a0 => [0x1642, 0x1652, 0x1643, 0x1653],
        0x0a4 | 0x1d2 | 0x1e7 => [0x15a4, 0x15b4, 0x15a5, 0x15b5],
        0x0a5 | 0x1d3 => [0x15a6, 0x15b6, 0x15a7, 0x15b7],
        0x0a6 => [0x558d, 0x559d, 0x558c, 0x559c],
        0x0a7 => [0x0186, 0x0196, 0x0187, 0x0197],
        0x0a8 => [0x01ce, 0x0188, 0x01ce, 0x0189],
        0x0a9 | 0x0dc => [0x09c8, 0x09d8, 0x09c9, 0x09d9],
        0x0aa => [0x49c9, 0x49d9, 0x49c8, 0x49d8],
        0x0ab => [0x056c, 0x057c, 0x056d, 0x057d],
        0x0ac => [0x456d, 0x457d, 0x456c, 0x457c],
        0x0ad => [0x056e, 0x057e, 0x056f, 0x057f],
        0x0ae => [0x456f, 0x457f, 0x456e, 0x457e],
        0x0b0 => [0x003d, 0x1019, 0x1019, 0x1019],
        0x0b1 => [0xc03d, 0x1019, 0x1019, 0x1019],
        0x0b2 => [0x003c, 0x1019, 0x1019, 0x1019],
        0x0b3 => [0xc03c, 0x1019, 0x1019, 0x1019],
        0x0b4 => [0x14c4, 0x1419, 0x54c4, 0x1419],
        0x0b5 => [0x082c, 0x0819, 0x0819, 0x0819],
        0x0b6 => [0x482c, 0x0819, 0x0819, 0x0819],
        0x0b7 => [0x143d, 0x0019, 0x143d, 0x0019],
        0x0b8 => [0x002e, 0x003e, 0x002f, 0x003f],
        0x0b9 | 0x1bb => [0x1daa, 0x1dba, 0x1dab, 0x1dbb],
        0x0bb => [0x857c, 0x856c, 0x857d, 0x856d],
        0x0bc => [0xc57d, 0xc56d, 0xc57c, 0xc56c],
        0x0bd => [0x857e, 0x856e, 0x857f, 0x856f],
        0x0be => [0xc57f, 0xc56f, 0xc57e, 0xc56e],
        0x0ba => [0x01e4, 0x01f4, 0x01e5, 0x01f5],
        0x0c0 | 0x179 => [0x1980, 0x1990, 0x1981, 0x1991],
        0x0c2 => [0x1984, 0x1994, 0x1985, 0x1995],
        0x0c3 | 0x15b => [0x1986, 0x1996, 0x1987, 0x1997],
        0x0c4 | 0x15c => [0x19c0, 0x19d0, 0x19c1, 0x19d1],
        0x0c5 => [0x1dc0, 0x1dd0, 0x1dc1, 0x1dd1],
        0x0c6 => [0x1dc2, 0x1dd2, 0x1dc3, 0x1dd3],
        0x0c7 => [0x098a, 0x099a, 0x098b, 0x099b],
        0x0c8 => [0x54af, 0x54bf, 0x54ae, 0x54be],
        0x0c9 => [0x0419, 0x0419, 0x0419, 0x04d4],
        0x0ca => [0x0419, 0x04d5, 0x0419, 0x04d5],
        0x0cb => [0x50e9, 0x50f9, 0x50e8, 0x50f8],
        0x0cc => [0x418c, 0x419c, 0x418b, 0x419b],
        0x0cd => [0x418a, 0x419a, 0x4019, 0x4019],
        0x0ce => [0x4182, 0x4192, 0x4181, 0x4191],
        0x0cf => [0x4180, 0x4190, 0x4019, 0x4019],
        0x0d0 => [0x19a0, 0x19b0, 0x19a1, 0x19b1],
        0x0d1 => [0x19a2, 0x19b2, 0x19a3, 0x19b3],
        0x0d2 => [0x19a4, 0x19b4, 0x19a5, 0x19b5],
        0x0d3 | 0x16b | 0x1c9 => [0x19a6, 0x19b6, 0x19a7, 0x19b7],
        0x0d4 => [0x19e0, 0x19f0, 0x19e1, 0x19f1],
        0x0d5 => [0x1de4, 0x1df4, 0x1de5, 0x1df5],
        0x0d6 => [0x1de6, 0x1df6, 0x1de7, 0x1df7],
        0x0d7 => [0x09e8, 0x09f8, 0x09e9, 0x09f9],
        0x0d8 => [0x31ec, 0x0019, 0x0019, 0x0019],
        0x0d9 => [0x098c, 0x099c, 0x098d, 0x099d],
        0x0da => [0x09c4, 0x09d4, 0x09c5, 0x09d5],
        0x0db => [0x09c6, 0x09d6, 0x09c7, 0x09d7],
        0x0dd => [0x0819, 0x0819, 0x0819, 0x1598],
        0x0de => [0x09c5, 0x09d5, 0x09c6, 0x09d6],
        0x0df => [0x49c6, 0x49d6, 0x49c5, 0x49d5],
        0x0e0 => [0x99b0, 0x99a0, 0x99b1, 0x99b1],
        0x0e1 => [0x99b2, 0x99a2, 0x99b3, 0x99a3],
        0x0e2 => [0x19c4, 0x19d4, 0x19c5, 0x19d5],
        0x0e3 => [0x19c6, 0x19d6, 0x19c7, 0x19d7],
        0x0e4 => [0x19e8, 0x19f8, 0x19e9, 0x19f9],
        0x0e5 | 0x194 => [0x15e0, 0x15f0, 0x15e1, 0x15f1],
        0x0e6 => [0x1419, 0x15f2, 0x1419, 0x1419],
        0x0e7 => [0x11f4, 0x1019, 0x11f5, 0x1019],
        0x0e8 => [0x11c8, 0x11d8, 0x1019, 0x11d0],
        0x0e9 | 0x1c0 => [0x1580, 0x1590, 0x1581, 0x1591],
        0x0ea => [0x5581, 0x5591, 0x5580, 0x5590],
        0x0eb => [0x05a2, 0x05b2, 0x05a3, 0x05b3],
        0x0ec => [0x0560, 0x0570, 0x0561, 0x0571],
        0x0ed => [0x0561, 0x0571, 0x0562, 0x0572],
        0x0ee => [0x0562, 0x0572, 0x0563, 0x0573],
        0x0fb => [0x094a, 0x095a, 0x094b, 0x095b],
        0x0fc => [0x09a0, 0x09b0, 0x09a1, 0x09b1],
        0x0f0 => [0x9990, 0x9980, 0x9991, 0x9981],
        0x0f1 => [0x9992, 0x9982, 0x9993, 0x9983],
        0x0f2 => [0x19e4, 0x19f4, 0x19e5, 0x19f5],
        0x0f3 => [0x19e6, 0x19f6, 0x19e7, 0x19f7],
        0x0f4 => [0x59e9, 0x59f9, 0x59e8, 0x59f8],
        0x0f5 => [0x11e0, 0x11f0, 0x11e1, 0x11f1],
        0x0f6 => [0x1019, 0x11f2, 0x1019, 0x1019],
        0x0f7 => [0x09f4, 0x0819, 0x09f5, 0x0819],
        0x0f8 => [0x09c8, 0x09d8, 0x0819, 0x09d0],
        0x0f9 => [0x090a, 0x091a, 0x090b, 0x091b],
        0x0fa => [0x490b, 0x491b, 0x490a, 0x491a],
        0x0fd => [0x09aa, 0x09ba, 0x09ab, 0x09bb],
        0x0fe => [0x4819, 0x4819, 0x0989, 0x0989],
        0x0ff => [0x55cc, 0x55dc, 0x55cb, 0x55db],
        0x100 => [0x5425, 0x5435, 0x5424, 0x5434],
        0x101 => [0x5025, 0x5035, 0x5024, 0x5034],
        0x103 => [0x4025, 0x4035, 0x4024, 0x4034],
        0x104 => [0x5427, 0x5437, 0x5426, 0x5436],
        0x105 => [0x4849, 0x4859, 0x4848, 0x4858],
        0x106 => [0x480f, 0x481f, 0x480e, 0x481e],
        0x107 => [0xc871, 0xc861, 0xc870, 0xc860],
        0x108 => [0x002a, 0x003a, 0x002b, 0x003b],
        0x109 => [0x047f, 0x0019, 0x0019, 0x0019],
        0x10a => [0x081d, 0x0019, 0x0019, 0x0019],
        0x10b => [0x4cc1, 0x4cd1, 0x4cc0, 0x4cd0],
        0x10c => [0x0056, 0x0019, 0x0019, 0x0019],
        0x10d => [0x0029, 0x0019, 0x0019, 0x0019],
        0x110 => [0x4819, 0x49d2, 0x4819, 0x4819],
        0x111 => [0x5019, 0x51d2, 0x5019, 0x5019],
        0x112 => [0x1419, 0x55d2, 0x1419, 0x1419],
        0x113 => [0x4c19, 0x4dd2, 0x4c19, 0x4c19],
        0x120 => [0x01aa, 0x01ba, 0x01ab, 0x01bb],
        0x121 => [0x41ab, 0x41bb, 0x41aa, 0x41ba],
        0x122 => [0x01ac, 0x01bc, 0x01ad, 0x01bd],
        0x123 => [0x41ad, 0x41bd, 0x41ac, 0x41bc],
        0x11c => [0x85be, 0x85ae, 0x85bf, 0x85af],
        0x12c => [0x859e, 0x858e, 0x859f, 0x858f],
        0x12d => [0x49ad, 0x49bd, 0x49ac, 0x49bc],
        0x130 => [0x81ba, 0x81aa, 0x81bb, 0x81ab],
        0x131 => [0xc1bb, 0xc1ab, 0xc1ba, 0xc1aa],
        0x132 => [0x81bc, 0x81ac, 0x81bd, 0x81ad],
        0x133 => [0xc1bd, 0xc1ad, 0xc1bc, 0xc1ac],
        0x13d => [0x9486, 0x1486, 0xd44e, 0x5486],
        0x141 => [0x0860, 0x0870, 0x0861, 0x0871],
        0x14b => [0x11e2, 0x11f2, 0x11e3, 0x11f3],
        0x14c => [0x0dae, 0x0dbe, 0x0daf, 0x0dbf],
        0x14d => [0x1133, 0x1019, 0x1134, 0x1019],
        0x14e | 0x1fa => [0x1540, 0x1550, 0x1541, 0x1551],
        0x14f => [0x1542, 0x1552, 0x1419, 0x1419],
        0x150 => [0x1945, 0x1955, 0x1946, 0x1956],
        0x151 => [0x1947, 0x1957, 0x1948, 0x1958],
        0x152 => [0x5946, 0x5956, 0x5945, 0x5955],
        0x153 => [0x1963, 0x1973, 0x1964, 0x1974],
        0x15d => [0x41e1, 0x41f1, 0x41e0, 0x41f0],
        0x15e => [0x1560, 0x1419, 0x1561, 0x1419],
        0x15f => [0x1562, 0x1419, 0x1419, 0x1419],
        0x160 => [0x1965, 0x1975, 0x1966, 0x1976],
        0x161 => [0x1967, 0x1977, 0x1967, 0x1977],
        0x162 => [0x5966, 0x5976, 0x5965, 0x5975],
        0x163 => [0x1938, 0x1819, 0x1819, 0x1819],
        0x16f => [0x0019, 0x0019, 0x0070, 0x0019],
        0x170 => [0x9955, 0x9945, 0x9956, 0x9946],
        0x171 => [0x9957, 0x9947, 0x9958, 0x9948],
        0x172 => [0xd956, 0xd946, 0xd955, 0xd945],
        0x173 => [0x1939, 0x1819, 0x1819, 0x1819],
        0x17b => [0x55af, 0x55bf, 0x55ae, 0x55be],
        0x17a => [0x0130, 0x0140, 0x0131, 0x0019],
        0x184 => [0x15b3, 0x0019, 0x0019, 0x0019],
        0x186 | 0x13b => [0x55e5, 0x55f5, 0x55e4, 0x55f4],
        0x187 => [0x15b6, 0x0019, 0x0019, 0x0019],
        0x188 => [0x0530, 0x0540, 0x0531, 0x0419],
        0x189 => [0x0541, 0x0551, 0x0542, 0x0552],
        0x18a => [0x0019, 0x0135, 0x0019, 0x0136],
        0x18b => [0x05cc, 0x05dc, 0x05cd, 0x05dd],
        0x195 => [0x15e2, 0x15f2, 0x15e3, 0x15f3],
        0x196 => [0x55e3, 0x55f3, 0x55e2, 0x55f2],
        0x197 => [0x55e1, 0x55f1, 0x55e0, 0x55f0],
        0x198 => [0x15c4, 0x15d4, 0x15c5, 0x15d5],
        0x199 => [0x55c5, 0x55d5, 0x55c4, 0x55d4],
        0x19a => [0x0145, 0x0019, 0x0146, 0x0156],
        0x16d => [0x815a, 0x814a, 0x815b, 0x814b],
        0x15a | 0x16c => [0x19c2, 0x19d2, 0x19c3, 0x19d3],
        0x164 => [0x01c6, 0x01d6, 0x01c7, 0x01d7],
        0x165 => [0x01c8, 0x01d8, 0x01c9, 0x01d9],
        0x158 | 0x180 => [0x0580, 0x0590, 0x0581, 0x0591],
        0x159 | 0x181 => [0x0582, 0x0592, 0x0583, 0x0593],
        0x16a => [0x19e2, 0x19f2, 0x19e3, 0x19f3],
        0x167 => [0x102a, 0x103a, 0x102b, 0x103b],
        0x168 | 0x182 => [0x0584, 0x0594, 0x0585, 0x0595],
        0x16e => [0x016a, 0x017a, 0x016b, 0x017b],
        0x174 => [0x01e6, 0x01f6, 0x01e7, 0x01f7],
        0x175 => [0x01e8, 0x01f8, 0x01e9, 0x01f9],
        0x177 | 0x183 => [0x0586, 0x0596, 0x0587, 0x0597],
        0x17d => [0x014a, 0x015a, 0x014b, 0x015b],
        0x17e => [0x1d40, 0x1d50, 0x1d41, 0x1d51],
        0x17f => [0x1d42, 0x1d52, 0x1d43, 0x1d53],
        0x18d => [0x0d8a, 0x0d9a, 0x0d8b, 0x0d9b],
        0x18e => [0x1d60, 0x1d70, 0x1d61, 0x1d71],
        0x18f => [0x1d62, 0x1d72, 0x1d63, 0x1d73],
        0x19d => [0x0dac, 0x0dbc, 0x0dad, 0x0dbd],
        0x19e => [0x054e, 0x055e, 0x054f, 0x055f],
        0x19f => [0x454f, 0x455f, 0x454e, 0x455e],
        0x1ac => [0x05c8, 0x05d8, 0x05c9, 0x05d9],
        0x1a4 => [0x5965, 0x5975, 0x5964, 0x5974],
        0x1a5 => [0x518b, 0x519b, 0x518a, 0x519a],
        0x1a8 => [0x45a9, 0x45b9, 0x45a8, 0x45b8],
        0x1aa => [0x45af, 0x45bf, 0x45ae, 0x45be],
        0x1ab => [0x45ab, 0x45bb, 0x45aa, 0x45ba],
        0x1c1 => [0x1582, 0x1592, 0x1583, 0x1593],
        0x1c2 => [0x1584, 0x1594, 0x1585, 0x1595],
        0x1c3 => [0x1586, 0x1596, 0x1587, 0x1597],
        0x1c5 => [0x45eb, 0x45fb, 0x45ea, 0x45fa],
        0x1c6 => [0x0909, 0x0019, 0x0019, 0x0019],
        0x1c7 => [0x090c, 0x091c, 0x090d, 0x091d],
        0x1c8 => [0x090e, 0x091e, 0x090f, 0x091f],
        0x1ca => [0x19a3, 0x99a3, 0x1819, 0x1819],
        0x1d0 => [0x15a0, 0x15b0, 0x15a1, 0x15b1],
        0x166 | 0x1d1 => [0x15a2, 0x15b2, 0x15a3, 0x15b3],
        0x1d4 => [0x85fa, 0x85ea, 0x85fb, 0x85eb],
        0x1d5 => [0xc5fb, 0xc5eb, 0xc5fa, 0xc5ea],
        0x1d6 => [0x05e8, 0x05f8, 0x05e9, 0x05f9],
        0x1d7 => [0x891c, 0x890c, 0x891d, 0x890d],
        0x1d8 => [0x891e, 0x890e, 0x891f, 0x890f],
        0x1de => [0x09c1, 0x09d1, 0x09c2, 0x09d2],
        0x1df => [0x09c3, 0x09d3, 0x09c4, 0x09d4],
        0x1e0 => [0x95b0, 0x95a0, 0x95b1, 0x95b1],
        0x1e1 => [0x95b2, 0x95a2, 0x95b3, 0x95a3],
        0x1e2 => [0x95b4, 0x95a4, 0x95b5, 0x95a5],
        0x1e3 => [0x95b6, 0x95a6, 0x95b7, 0x95a7],
        0x1e4 => [0x11b6, 0x1019, 0x1019, 0x1019],
        0x1e5 => [0x11ad, 0x1019, 0x1019, 0x1019],
        0x1e6 => [0x1419, 0x1419, 0x15bd, 0x1419],
        0x1e8 => [0x1419, 0x1419, 0x1544, 0x1554],
        0x1e9 => [0x1545, 0x1555, 0x1419, 0x1419],
        0x1ea => [0x1419, 0x150c, 0x1419, 0x1419],
        0x1eb => [0x1419, 0x550c, 0x1419, 0x1419],
        0x1ec => [0x418b, 0x419b, 0x418a, 0x419a],
        0x1ed => [0x154b, 0x155b, 0x154c, 0x155c],
        0x1ee => [0x1506, 0x1516, 0x1507, 0x1517],
        0x1ef => [0x0c19, 0x0d1c, 0x0c19, 0x0d1d],
        0x1f0 => [0x9590, 0x9580, 0x9591, 0x9581],
        0x1f1 => [0x9592, 0x9582, 0x9593, 0x9583],
        0x1f2 => [0x9594, 0x9584, 0x9595, 0x9585],
        0x1f3 => [0x9596, 0x9586, 0x9597, 0x9587],
        0x1f4 => [0x15ce, 0x15de, 0x15cf, 0x15df],
        0x1f5 => [0x55cf, 0x55df, 0x55ce, 0x55de],
        0x1f6 => [0x1419, 0x1419, 0x15cb, 0x1419],
        0x1f7 => [0x15cc, 0x15dc, 0x15cd, 0x15dd],
        0x1f8 => [0x1542, 0x1552, 0x1543, 0x1553],
        0x1f9 => [0x5543, 0x5553, 0x5542, 0x5552],
        0x1fb => [0x5540, 0x5550, 0x1419, 0x1419],
        0x1fc => [0x152d, 0x153d, 0x152e, 0x153e],
        0x1fd => [0x552d, 0x553d, 0x1419, 0x1419],
        0x1fe => [0x1523, 0x1533, 0x1524, 0x1534],
        0x1ff => [0x1525, 0x1535, 0x1419, 0x1419],
        0x200 => [0x11e2, 0x1419, 0x31e3, 0x1419],
        0x201 => [0x0062, 0x1019, 0x1019, 0x1019],
        0x202 => [0x0064, 0x1019, 0x1019, 0x1019],
        0x203 => [0x0066, 0x1019, 0x1019, 0x1019],
        0x204 => [0x1019, 0x1019, 0x0064, 0x1019],
        0x205 => [0x1019, 0x1019, 0x0066, 0x1019],
        0x208 => [0x817a, 0x816a, 0x817b, 0x816b],
        0x20c => [0x2c19, 0x2c19, 0x2c19, 0x0599],
        0x20d => [0x1419, 0x1419, 0x1419, 0x350d],
        0x20e => [0x354e, 0x355e, 0x354f, 0x355f],
        0x20f => [0x1419, 0x355d, 0x1419, 0x1419],
        0x210 => [0x1424, 0x1434, 0x1425, 0x1435],
        0x211 => [0x1024, 0x1034, 0x1025, 0x1035],
        0x21f => [0x35ae, 0x35be, 0x35af, 0x35bf],
        0x124 => [0x5508, 0x5518, 0x5507, 0x5517],
        0x125 => [0x5108, 0x5118, 0x5107, 0x5117],
        0x126 => [0x1507, 0x1517, 0x1508, 0x1518],
        0x127 => [0x1107, 0x1117, 0x1108, 0x1118],
        0x128 => [0x1100, 0x1110, 0x1101, 0x1111],
        0x129 => [0x1102, 0x1112, 0x1102, 0x1112],
        0x134 => [0x5528, 0x5538, 0x5527, 0x5537],
        0x135 => [0x5128, 0x5138, 0x5127, 0x5137],
        0x136 => [0x1527, 0x1537, 0x1528, 0x1538],
        0x137 => [0x1127, 0x1137, 0x1128, 0x1138],
        0x138 => [0x1120, 0x1120, 0x1121, 0x1121],
        0x139 => [0x9122, 0x9122, 0x9122, 0x9122],
        0x13a => [0x5121, 0x5121, 0x5120, 0x5120],
        0x114 => [0x08ef, 0x0019, 0x0019, 0x0019],
        0x115 => [0x04e8, 0x04f8, 0x04e9, 0x04f9],
        0x11d => [0x05a6, 0x05b6, 0x05a7, 0x05b7],
        0x119 => [0x0019, 0x0019, 0x09d6, 0x0019],
        0x13c => [0x0648, 0x0658, 0x4648, 0x4658],
        0x140 => [0x14a0, 0x14b0, 0x14a1, 0x14b1],
        0x145 => [0x0019, 0x09c7, 0x0019, 0x0019],
        0x148 => [0x9110, 0x9100, 0x9111, 0x9101],
        0x149 => [0x9112, 0x9102, 0x9112, 0x9102],
        0x14a => [0xd111, 0xd101, 0xd110, 0xd100],
        0x154 => [0x0d88, 0x0d98, 0x0d89, 0x0d99],
        0x155 => [0x0c19, 0x0c19, 0x0da8, 0x0db8],
        0x156 => [0x0da9, 0x0db9, 0x0daa, 0x0dba],
        0x157 => [0x05a4, 0x05b4, 0x05a5, 0x05b5],
        0x169 => [0x554c, 0x555c, 0x554b, 0x555b],
        0x1ad => [0x89dc, 0x89cc, 0x89dd, 0x89cd],
        0x1ae => [0x0da0, 0x0db0, 0x0da1, 0x2db1],
        0x1af => [0x4da1, 0x6db1, 0x4da0, 0x4db0],
        0x1bd => [0x88b8, 0x88a8, 0x88b9, 0x88a9],
        0x1be => [0x8db0, 0x8da0, 0xadb1, 0x8da1],
        0x1bf => [0xedb1, 0xcda1, 0xcdb0, 0xcda0],
        0x190 => [0x05a0, 0x05b0, 0x05a1, 0x05b1],
        0x18c | 0x192 | 0x1a2 => [0x05ce, 0x05de, 0x05cf, 0x05df],
        0x193 | 0x19c | 0x1a3 => [0x05ee, 0x05fe, 0x05ef, 0x05ff],
        0x1a0 => [0x05c0, 0x05d0, 0x05c1, 0x05d1],
        0x1a1 => [0x05c2, 0x05d2, 0x05c3, 0x05d3],
        0x1b2 => [0x8594, 0x8584, 0x8595, 0x8585],
        0x20a => [0x0c86, 0x0c96, 0x0c87, 0x0c97],
        0x20b => [0x0daa, 0x0dba, 0x0dab, 0x0dbb],
        0x1b3 => [0x8596, 0x8586, 0x8597, 0x8587],
        0x1b4 => [0x59ad, 0x59bd, 0x59ac, 0x59bc],
        0x1b6 => [0x1d88, 0x1d98, 0x1d89, 0x1d99],
        0x1b7 => [0x1d8c, 0x1d9c, 0x1d8d, 0x1d9d],
        0x1b8 => [0x1da8, 0x1db8, 0x1da9, 0x1db9],
        0x1b9 => [0x1d8e, 0x1d9e, 0x1d8f, 0x1d9f],
        0x1ba => [0x1dae, 0x1dbe, 0x1daf, 0x1dbf],
        0x1ce => [0x09f3, 0xc9f3, 0x09ce, 0x09ce],
        0x1cf => [0x0819, 0x0998, 0x0819, 0x0999],
        0x1cb => [0x0440, 0x0450, 0x0441, 0x0451],
        0x1cc => [0x0c19, 0x0c19, 0x0c19, 0x0d5a],
        0x1cd => [0x0c19, 0x0d4a, 0x0c19, 0x0c19],
        0x1d9 => [0x44c7, 0x44d7, 0x44c6, 0x44d6],
        0x1da => [0x04c6, 0x04d6, 0x04c7, 0x04d7],
        0x1db => [0x0d6d, 0x0d7d, 0x0d6e, 0x0d7e],
        0x1dc => [0x0d46, 0x0d56, 0x0d47, 0x0d57],
        0x1dd => [0x0d48, 0x0d58, 0x0d49, 0x0d59],
        0x21b => [0x4c41, 0x4c51, 0x4c40, 0x4c50],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_four_dispatch_entries_select_the_recovered_source_tiles() {
        for sprite in 0_u8..=3 {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, false),
                Some(vec![StandardSpritePreviewTile {
                    definition_index: 0x40 + u16::from(sprite),
                    subtiles: preview_definition(0x40 + u16::from(sprite)).unwrap(),
                    x: 0,
                    y: 1,
                }])
            );
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true),
                Some(vec![StandardSpritePreviewTile {
                    definition_index: 0x115,
                    subtiles: [0x04e8, 0x04f8, 0x04e9, 0x04f9],
                    x: 0,
                    y: 1,
                }])
            );
        }
    }

    #[test]
    fn handlers_four_through_seven_follow_both_recovered_switches() {
        let indices = |sprite, mode| {
            render_lunar_magic_standard_sprite_with_mode(sprite, mode)
                .unwrap()
                .iter()
                .map(|part| part.definition_index)
                .collect::<Vec<_>>()
        };
        let ordinary = StandardSpritePreviewMode::default();
        assert_eq!(indices(4, ordinary), [0x10, 0x20]);
        assert_eq!(indices(5, ordinary), [0x11, 0x21]);
        assert_eq!(indices(6, ordinary), [0x12, 0x22]);
        assert_eq!(indices(7, ordinary), [0x13, 0x23]);
        assert_eq!(
            indices(
                4,
                StandardSpritePreviewMode {
                    alternate_display: false,
                    alternate_graphics: true,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [0x13, 0x23]
        );
        assert_eq!(
            indices(
                5,
                StandardSpritePreviewMode {
                    alternate_display: false,
                    alternate_graphics: true,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [0x12, 0x22]
        );
        assert_eq!(
            indices(
                7,
                StandardSpritePreviewMode {
                    alternate_display: true,
                    alternate_graphics: false,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [0x115]
        );
        assert_eq!(
            render_lunar_magic_standard_sprite(8, false)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>(),
            [(0x10, 0, -14), (0x20, 0, 2), (0x08, 1, -10)]
        );
    }

    #[test]
    fn handlers_nine_and_ten_preserve_the_native_placement_variant() {
        let geometry = |sprite, placement_first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(9, 0),
            [(0x10, 0, -14), (0x20, 0, 2), (0x07, 9, -11)]
        );
        assert_eq!(
            geometry(9, 0x10),
            [(0x10, 0, -15), (0x140, 0, 1), (0x08, 1, -11)]
        );
        assert_eq!(
            geometry(10, 0),
            [(0x11, 0, -14), (0x21, 0, 2), (0x07, 9, -11)]
        );
    }

    #[test]
    fn handlers_eleven_through_twenty_preserve_dispatch_specific_shapes() {
        let geometry = |sprite, alternate_display| {
            render_lunar_magic_standard_sprite(sprite, alternate_display)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0x0b, false),
            [(0x11, 0, -14), (0x21, 0, 2), (0x08, 1, -10)]
        );
        assert_eq!(
            geometry(0x0c, false),
            [(0x13, 0, -14), (0x23, 0, 2), (0x07, 9, -11)]
        );
        assert_eq!(geometry(0x0d, false), [(0x09, 0, 1)]);
        assert_eq!(geometry(0x0e, true), [(0x0a, 0, 1)]);
        assert_eq!(geometry(0x0f, false), [(0x0c, 0, 1)]);
        assert_eq!(
            geometry(0x10, false),
            [(0x06, -10, -7), (0x0c, 0, 1), (0x07, 13, -7)]
        );
        assert_eq!(
            geometry(0x10, true),
            [(0x06, -10, -7), (0x115, 0, 1), (0x07, 13, -7)]
        );
        assert_eq!(geometry(0x11, false), [(0x0d, 0, 1)]);
        assert_eq!(geometry(0x12, false), [(0x01, 0, 0)]);
        assert_eq!(geometry(0x13, false), [(0x0e, 0, 1)]);
        assert_eq!(geometry(0x14, false), [(0x0f, 0, 1)]);
        for sprite in [0x0b, 0x0c, 0x0d, 0x0f, 0x11, 0x12, 0x13, 0x14] {
            assert_eq!(geometry(sprite, true), [(0x115, 0, 1)]);
        }
    }

    #[test]
    fn handlers_twenty_one_through_thirty_one_preserve_proven_tile_geometry() {
        let geometry = |sprite, alternate_display| {
            render_lunar_magic_standard_sprite(sprite, alternate_display).map(|parts| {
                parts
                    .iter()
                    .map(|part| (part.definition_index, part.x, part.y))
                    .collect::<Vec<_>>()
            })
        };
        for sprite in [0x15, 0x17, 0x18] {
            assert_eq!(geometry(sprite, false).unwrap(), [(0x14, 0, 1)]);
        }
        assert_eq!(geometry(0x16, false).unwrap(), [(0x15, 0, 1)]);
        assert_eq!(
            geometry(0x1a, false).unwrap(),
            [(0x16, 8, -31), (0x26, 8, -15)]
        );
        assert_eq!(geometry(0x1b, false).unwrap(), [(0x24, 0, 1)]);
        assert_eq!(geometry(0x1c, false).unwrap(), [(0x25, 0, 1)]);
        assert_eq!(geometry(0x1d, false).unwrap(), [(0x17, 0, 1)]);
        assert_eq!(
            geometry(0x1f, false).unwrap(),
            [(0x18, 0, -15), (0x28, 0, 1)]
        );
        assert_eq!(
            geometry(0x1e, false).unwrap(),
            [
                (0x27, -5, 0),
                (0x27, 5, 1),
                (0x19, 1, -16),
                (0x29, 1, 0),
                (0x27, -3, 4),
                (0x27, 3, 4),
                (0x9c, 4, 8)
            ]
        );
        let text = geometry(0x19, false).unwrap();
        assert_eq!(text[1], (0x3c44, 0, 0));
        assert_eq!(text["Display Level".len() * 2 + 1], (0x3c20, 0, 8));
        for sprite in [0x15, 0x16, 0x17, 0x18, 0x1a, 0x1b, 0x1c, 0x1d, 0x1f] {
            assert_eq!(geometry(sprite, true).unwrap(), [(0x115, 0, 1)]);
        }
        let extended = render_lunar_magic_standard_sprite_with_mode(
            0x1e,
            StandardSpritePreviewMode {
                placement_major: 1,
                ..StandardSpritePreviewMode::default()
            },
        )
        .unwrap();
        assert_eq!(extended.len(), 13);
        assert_eq!(
            extended
                .last()
                .map(|part| (part.definition_index, part.x, part.y)),
            Some((0x210, -20, 50))
        );
    }

    #[test]
    fn handlers_thirty_two_through_thirty_six_preserve_placement_variants() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0x20, 0),
            [(0x03, 4, 0), (0x04, -4, 8), (0x05, 12, 8)]
        );
        assert_eq!(geometry(0x21, 0), [(0x1a, 0, 1)]);
        assert_eq!(geometry(0x22, 0), [(0x50, 0, -4), (0x60, 0, 12)]);
        assert_eq!(geometry(0x22, 1), [(0x124, 0, -4), (0x134, 0, 12)]);
        assert_eq!(geometry(0x23, 0), [(0x51, 0, -4), (0x61, 0, 12)]);
        assert_eq!(geometry(0x23, 1), [(0x125, 0, -4), (0x135, 0, 12)]);
        assert_eq!(geometry(0x24, 0), [(0x52, 0, -4), (0x62, 0, 12)]);
        assert_eq!(geometry(0x24, 1), [(0x126, 0, -4), (0x136, 0, 12)]);
        for sprite in 0x20..=0x24 {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true)
                    .unwrap()
                    .iter()
                    .map(|part| part.definition_index)
                    .collect::<Vec<_>>(),
                [0x115]
            );
        }
    }

    #[test]
    fn handlers_thirty_seven_through_thirty_nine_preserve_native_geometry() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0x25, 0), [(0x53, 0, -4), (0x63, 0, 12)]);
        assert_eq!(geometry(0x25, 1), [(0x127, 0, -4), (0x137, 0, 12)]);
        assert_eq!(
            geometry(0x26, 0),
            [(0x34, 4, 1), (0x35, 20, 1), (0x44, 4, 17), (0x45, 20, 17)]
        );
        assert_eq!(geometry(0x27, 0), [(0x2a, 0, 1)]);
        for sprite in 0x25..=0x27 {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true)
                    .unwrap()
                    .iter()
                    .map(|part| part.definition_index)
                    .collect::<Vec<_>>(),
                [0x115]
            );
        }
    }

    #[test]
    fn handler_forty_preserves_the_recovered_four_by_four_composite() {
        let geometry = render_lunar_magic_standard_sprite(0x28, false)
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>();
        assert_eq!(geometry.len(), 20);
        assert_eq!(geometry[0], (0xe4, -7, 24));
        assert_eq!(
            &geometry[1..=4],
            &[(0xc0, -4, 0), (0xc1, 12, 0), (0xc2, 28, 0), (0xc3, 44, 0)]
        );
        assert_eq!(
            &geometry[13..=16],
            &[
                (0xf0, -4, 48),
                (0xf1, 12, 48),
                (0xf2, 28, 48),
                (0xf3, 44, 48)
            ]
        );
        assert_eq!(
            &geometry[17..],
            &[(0xf4, 29, 24), (0xc4, 4, 18), (0xd4, 4, 34)]
        );
        assert_eq!(
            render_lunar_magic_standard_sprite(0x28, true)
                .unwrap()
                .iter()
                .map(|part| part.definition_index)
                .collect::<Vec<_>>(),
            [0x115]
        );
    }

    #[test]
    fn handlers_forty_two_through_forty_seven_preserve_recovered_variants() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0x2a, 0), [(0x1b, 8, 15), (0x2b, 8, 31)]);
        assert_eq!(geometry(0x2b, 0), [(0x1ce, 0, 0)]);
        assert_eq!(geometry(0x2c, 0), [(0x71, 0, 1)]);
        assert_eq!(geometry(0x2c, 1), [(0x72, 0, 1)]);
        assert_eq!(geometry(0x2c, 2), [(0x73, 0, 1)]);
        assert_eq!(geometry(0x2c, 3), [(0x72, 0, 1)]);
        assert_eq!(geometry(0x2d, 0), [(0x80, 0, 1)]);
        assert_eq!(geometry(0x2e, 0), [(0x2d, 0, 1)]);
        assert_eq!(geometry(0x2f, 0), [(0x1e, 0, 1)]);
        for sprite in [0x2a, 0x2b, 0x2e] {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true)
                    .unwrap()
                    .iter()
                    .map(|part| part.definition_index)
                    .collect::<Vec<_>>(),
                [0x115]
            );
        }
    }

    #[test]
    fn handlers_forty_nine_through_fifty_two_preserve_recovered_composites() {
        let geometry = |sprite| {
            render_lunar_magic_standard_sprite(sprite, false)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0x31), [(0x37, 0, 1)]);
        assert_eq!(geometry(0x32), [(0x36, -8, -14), (0x46, 0, 1)]);
        assert_eq!(
            geometry(0x33),
            [(0x13c, 0, 0), (0x119, -1, 20), (0x145, 2, 28)]
        );
        assert_eq!(
            preview_definition(0x13c).unwrap(),
            [0x0648, 0x0658, 0x4648, 0x4658]
        );
        assert_eq!(geometry(0x34), [(0x2e, -16, 3), (0x2f, 0, 3)]);
        for sprite in 0x31..=0x34 {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true)
                    .unwrap()
                    .iter()
                    .map(|part| part.definition_index)
                    .collect::<Vec<_>>(),
                [0x115]
            );
        }
    }

    #[test]
    fn handlers_fifty_three_through_sixty_two_preserve_placement_shapes() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0x35, 0), [(0x90, -10, 1), (0xa0, 0, 17)]);
        assert_eq!(geometry(0x36, 0), [(0x1f, 0, 1)]);
        assert_eq!(geometry(0x37, 0), [(0x1f, 0, 1)]);
        assert_eq!(geometry(0x38, 0), [(0x38, 0, 1)]);
        assert_eq!(geometry(0x39, 0), [(0x48, 0, 1)]);
        assert_eq!(geometry(0x39, 1), [(0x48, 0, 1)]);
        assert_eq!(geometry(0x3a, 0)[0], (0x3b, 0, 0));
        assert_eq!(geometry(0x3b, 0)[0], (0x3d, 0, 0));
        assert_eq!(geometry(0x3c, 0), [(0x54, 8, -15), (0x64, 0, 1)]);
        assert_eq!(geometry(0x3d, 0), [(0x54, 8, -15), (0x64, 0, 1)]);
        assert_eq!(geometry(0x3d, 1), [(0x54, 8, -15), (0x64, 0, 1)]);
        assert_eq!(geometry(0x3e, 0), [(0x55, 0, 1)]);
        assert_eq!(geometry(0x3e, 1), [(0x55, 0, 1)]);
        assert_eq!(
            render_lunar_magic_standard_sprite_with_mode(
                0x3e,
                StandardSpritePreviewMode {
                    placement_first: 0,
                    placement_major: 1,
                    ..StandardSpritePreviewMode::default()
                }
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>(),
            [(0x65, 0, 1)]
        );
        for sprite in [0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3e] {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true)
                    .unwrap()
                    .iter()
                    .map(|part| part.definition_index)
                    .collect::<Vec<_>>(),
                [0x115]
            );
        }
    }

    #[test]
    fn handlers_sixty_three_through_sixty_five_preserve_recovered_geometry() {
        let geometry = |sprite| {
            render_lunar_magic_standard_sprite(sprite, false)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0x3f), [(0x56, -6, -14), (0x67, -6, 2)]);
        assert_eq!(geometry(0x40), [(0x74, 0, 1), (0x75, 8, 1), (0x76, 24, 1)]);
        assert_eq!(
            geometry(0x41),
            [(0x154, 0, 1), (0x155, 8, 1), (0x156, 24, 1)]
        );
        for sprite in 0x3f..=0x41 {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true)
                    .unwrap()
                    .iter()
                    .map(|part| part.definition_index)
                    .collect::<Vec<_>>(),
                [0x115]
            );
        }
    }

    #[test]
    fn handlers_sixty_six_through_seventy_four_preserve_recovered_geometry() {
        let geometry = |sprite| {
            render_lunar_magic_standard_sprite(sprite, false)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0x42),
            [(0x154, 0, 1), (0x155, 8, 1), (0x156, 24, 1)]
        );
        assert_eq!(
            geometry(0x43),
            [(0x94, 0, 1), (0x57, -12, 1), (0x95, 16, 1)]
        );
        assert_eq!(geometry(0x44), [(0x96, 4, 1)]);
        assert_eq!(
            geometry(0x45),
            [(0x97, -16, 1), (0x98, -4, 1), (0x99, 12, 1), (0x88, -2, -7)]
        );
        assert_eq!(geometry(0x46), [(0x14, -2, 1)]);
        assert_eq!(geometry(0x47), [(0x89, 0, -3)]);
        assert_eq!(geometry(0x48), [(0x89, 0, -3)]);
        assert_eq!(
            render_lunar_magic_standard_sprite(0x48, true)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>(),
            [(0x115, 0, 1)]
        );
        assert_eq!(geometry(0x49), [(0xa6, 0, 1)]);
        assert_eq!(geometry(0x4a), [(0x58, 8, -16), (0x68, 8, 0)]);
        for sprite in [0x42, 0x43, 0x46, 0x47] {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true)
                    .unwrap()
                    .iter()
                    .map(|part| part.definition_index)
                    .collect::<Vec<_>>(),
                [0x115]
            );
        }
    }

    #[test]
    fn handlers_seventy_five_through_eighty_three_preserve_shared_composites() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0x4b, 0)[0], (0x14, 0, 0));
        assert_eq!(geometry(0x4b, 1)[0], (0x0c, 0, 0));
        assert_eq!(geometry(0x4b, 2)[0], (0x40, 0, 0));
        assert_eq!(&geometry(0x4b, 3)[..2], &[(0x10, 0, -16), (0x20, 0, 0)]);
        assert_eq!(
            &geometry(0x4b, 0)[1..],
            &[(0xb0, -4, -4), (0xb1, 12, -4), (0xb2, -4, 8), (0xb3, 12, 8)]
        );
        assert_eq!(geometry(0x4c, 0), [(0xa7, 0, 1), (0xa8, 0, 1)]);
        assert_eq!(geometry(0x4d, 0), [(0xa7, 0, 1), (0xa8, 0, 1)]);
        assert_eq!(geometry(0x4e, 0), [(0x16, 8, -8), (0xb4, 8, 8)]);
        assert_eq!(geometry(0x4f, 0), [(0x16, 8, -8), (0xb4, 8, 8)]);
        assert_eq!(
            geometry(0x50, 0),
            [(0x16, 8, -8), (0xb4, 8, 8), (0xb5, 6, -16), (0xb6, 18, -16)]
        );
        assert_eq!(
            geometry(0x51, 0),
            [(0x59, -4, 0), (0x69, 4, 0), (0x69, 20, 0), (0x79, 28, 0)]
        );
        assert_eq!(
            geometry(0x52, 0),
            [(0x59, -4, 0), (0x69, 4, 0), (0x69, 20, 0), (0x79, 28, 0)]
        );
        let matrix = geometry(0x53, 0);
        assert_eq!(matrix.len(), 9);
        assert_eq!(
            &matrix[..3],
            &[(0x128, 8, 0), (0x129, 24, 0), (0x12a, 40, 0)]
        );
        assert_eq!(
            &matrix[6..],
            &[(0x148, 8, 32), (0x149, 24, 32), (0x14a, 40, 32)]
        );
        for sprite in 0x4b..=0x50 {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true)
                    .unwrap()
                    .iter()
                    .map(|part| part.definition_index)
                    .collect::<Vec<_>>(),
                [0x115]
            );
        }
    }

    #[test]
    fn handlers_eighty_four_through_ninety_two_preserve_multi_cell_shapes() {
        let geometry = |sprite| {
            render_lunar_magic_standard_sprite(sprite, false)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0x54),
            [
                (0x128, 8, 8),
                (0x129, 24, 8),
                (0x12a, 40, 8),
                (0x138, 8, 24),
                (0x139, 24, 24),
                (0x13a, 40, 24),
                (0x148, 8, 40),
                (0x149, 24, 40),
                (0x14a, 40, 40)
            ]
        );
        assert_eq!(
            geometry(0x55),
            [
                (0x6a, 0, 1),
                (0x5b, 16, 1),
                (0x5b, 32, 1),
                (0x5b, 48, 1),
                (0x5a, 64, 1)
            ]
        );
        assert_eq!(geometry(0x55), geometry(0x57));
        assert_eq!(
            geometry(0x56),
            [
                (0x5d, 0, 1),
                (0x5e, 16, 1),
                (0x5f, 32, 1),
                (0x6e, 8, 17),
                (0x6f, 24, 17)
            ]
        );
        assert_eq!(geometry(0x56), geometry(0x58));
        assert_eq!(
            geometry(0x59),
            [
                (0x5c, 0, 1),
                (0x5c, 0, 17),
                (0x6c, 0, 33),
                (0x5c, 0, -15),
                (0x5c, 0, -31)
            ]
        );
        assert_eq!(
            geometry(0x5a),
            [
                (0x5c, 0, 1),
                (0x5c, 16, 1),
                (0x6c, 32, 1),
                (0x5c, -16, 1),
                (0x5c, -32, 1)
            ]
        );
        assert_eq!(
            geometry(0x5b),
            [(0x142, 0, 0), (0x143, 16, 0), (0x144, 32, 0)]
        );
        assert_eq!(
            geometry(0x5c),
            [
                (0x7b, 0, 1),
                (0x6b, 16, 1),
                (0x6b, 32, 1),
                (0x6b, 48, 1),
                (0x7a, 64, 1)
            ]
        );
    }

    #[test]
    fn handlers_ninety_three_through_ninety_six_preserve_large_shapes() {
        let geometry = |sprite| {
            render_lunar_magic_standard_sprite(sprite, false)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0x5d),
            [
                (0x8a, 0, 0),
                (0x8b, 16, 0),
                (0xff, 32, 0),
                (0x9a, 8, 16),
                (0x13b, 24, 16)
            ]
        );
        assert_eq!(
            geometry(0x5e),
            [
                (0x8a, 0, 1),
                (0x8b, 16, 1),
                (0x8b, 32, 1),
                (0x8b, 48, 1),
                (0xff, 64, 1),
                (0x9a, 8, 17),
                (0x9b, 24, 17),
                (0x9b, 40, 17),
                (0x13b, 56, 17)
            ]
        );
        assert_eq!(
            geometry(0x5f),
            [
                (0x6d, 32, 16),
                (0x6d, 32, 32),
                (0x6d, 32, 48),
                (0x6d, 32, 64),
                (0x6d, 32, 80),
                (0x7c, 8, 9),
                (0x7d, 24, 9),
                (0x7d, 40, 9),
                (0x7e, 56, 9)
            ]
        );
        assert_eq!(geometry(0x60), [(0x8c, 0, 1), (0x8d, 16, 1)]);
    }

    #[test]
    fn handlers_ninety_seven_through_ninety_nine_select_left_chains() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        let short = vec![(0x7c, -24, -7), (0x7d, -8, -7), (0x7e, 8, -7)];
        let long = vec![
            (0x7b, -40, -7),
            (0x6b, -24, -7),
            (0x6b, -8, -7),
            (0x6b, 8, -7),
            (0x7a, 24, -7),
        ];
        assert_eq!(
            geometry(0x61, 0),
            [(0x7f, 0, 4), (0x7f, 16, 4), (0x7f, 32, 4), (0x7f, 48, 4)]
        );
        assert_eq!(geometry(0x62, 0), short);
        let geometry_63 = |first, major| {
            render_lunar_magic_standard_sprite_with_mode(
                0x63,
                StandardSpritePreviewMode {
                    placement_first: first,
                    placement_major: major,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(geometry_63(1, 0x12), long);
        assert_eq!(geometry_63(0, 0x13), short);
    }

    #[test]
    fn handlers_one_oh_five_through_one_seventeen_preserve_proven_shapes() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0x69, 0), [(0x27, 3, 0), (0x9c, 7, 4)]);
        assert_eq!(
            geometry(0x6a, 0),
            [(0xb7, 0, 1), (0xb7, 16, 1), (0xb7, 24, 1)]
        );
        assert_eq!(
            geometry(0x6b, 0),
            [(0xb7, 0, 1), (0xb7, 16, 1), (0xb7, 24, 1)]
        );
        assert_eq!(
            geometry(0x6c, 0),
            [(0xb7, -16, 1), (0xb7, -32, 1), (0xb7, -40, 1)]
        );
        assert_eq!(geometry(0x6d, 0), [(0x80b8, 0, 0)]);
        assert_eq!(
            geometry(0x6e, 0),
            [(0xd5, -8, 1), (0xd6, 8, 1), (0xc5, -8, -15), (0xc6, 8, -15)]
        );
        assert_eq!(geometry(0x6f, 0), [(0xb9, -2, 1)]);
        assert_eq!(
            geometry(0x70, 0),
            [
                (0xc7, 1, 1),
                (0xd7, 0, 17),
                (0xc7, 1, 33),
                (0xd7, 0, 49),
                (0xc7, 1, 65)
            ]
        );
        assert_eq!(
            geometry(0x71, 0),
            [(0xe5, -3, 8), (0xe6, 13, 8), (0xe7, 5, 11)]
        );
        assert_eq!(
            geometry(0x72, 0),
            [(0xf5, -3, 8), (0xf6, 13, 8), (0xf7, 5, 11)]
        );
        assert_eq!(geometry(0x73, 0), [(0x42, 0, 1), (0xe8, 8, 1)]);
        assert_eq!(
            render_lunar_magic_standard_sprite_with_mode(
                0x73,
                StandardSpritePreviewMode {
                    placement_major: 1,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>(),
            [(0x42, 0, 1), (0xf8, 8, 1)]
        );
        assert_eq!(
            render_lunar_magic_standard_sprite(0x73, true)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>(),
            [(0x115, 0, 1)]
        );
        assert_eq!(geometry(0x74, 0), [(0x101, 0, 0)]);
        assert_eq!(geometry(0x75, 0), [(0x104, 0, 0)]);
        for sprite in 0x6d..=0x72 {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true)
                    .unwrap()
                    .iter()
                    .map(|part| part.definition_index)
                    .collect::<Vec<_>>(),
                [0x115]
            );
        }
    }

    #[test]
    fn handlers_one_eighteen_through_one_twenty_seven_preserve_fixed_shapes() {
        let geometry = |sprite| {
            render_lunar_magic_standard_sprite(sprite, false)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0x76), [(0x105, 0, 0)]);
        assert_eq!(geometry(0x77), [(0x106, 0, 0)]);
        assert_eq!(geometry(0x78), [(0x100, 0, 0)]);
        assert_eq!(geometry(0x79), [(0xc8, 0, 1)]);
        assert_eq!(geometry(0x7a), [(0xd8, 4, 4)]);
        assert_eq!(geometry(0x7b), [(0xca, 0, 0), (0xc9, -16, 0)]);
        assert_eq!(
            geometry(0x7c),
            [(0xcc, 0, 1), (0xcd, 16, 1), (0xce, 0, 17), (0xcf, 16, 17)]
        );
        assert_eq!(geometry(0x7d), [(0xba, 4, -1)]);
        assert_eq!(
            geometry(0x7e),
            [(0x06, -8, -9), (0x07, 16, -9), (0xcb, -5, -1)]
        );
        assert_eq!(
            geometry(0x7f),
            [(0x06, -8, -9), (0x07, 16, -9), (0x103, -5, -1)]
        );
        assert_eq!(geometry(0x80), [(0x0b, 0, 1)]);
    }

    #[test]
    fn handlers_one_twenty_nine_and_one_thirty_preserve_flagged_variants() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        for first in 0..=3 {
            assert_eq!(geometry(0x81, first), [(0x101, 0, 0)]);
        }
        assert_eq!(
            geometry(0x82, 0),
            [
                (0x06, -8, -9),
                (0x07, 14, -9),
                (0x801a, 5, -9),
                (0x108, 3, -1)
            ]
        );
        let flagged = render_lunar_magic_standard_sprite_with_mode(
            0x82,
            StandardSpritePreviewMode::default(),
        )
        .unwrap();
        assert_eq!(flagged[2].subtiles, preview_definition(0x1a).unwrap());
    }

    #[test]
    fn later_handlers_preserve_fixed_and_placement_spaced_shapes() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0x84, 0),
            [
                (0xdd, 0, -15),
                (0xdc, 24, -1),
                (0xdb, 16, 0),
                (0xda, 8, 1),
                (0xdb, 32, 0),
                (0xd9, 0, 1)
            ]
        );
        assert_eq!(
            geometry(0x85, 0),
            [
                (0x27, -5, 0),
                (0x27, 5, 1),
                (0x27, -3, 4),
                (0x27, 3, 4),
                (0x9c, 4, 8)
            ]
        );
        assert_eq!(geometry(0x86, 0), geometry(0x84, 0));
        assert_eq!(geometry(0x89, 0), [(0xde, 0, 0), (0xdf, 16, 0)]);
        assert_eq!(
            geometry(0x8c, 0),
            [
                (0x109, 4, 4),
                (0x10a, 2, 4),
                (0x109, 4, 20),
                (0x10a, 2, 20),
                (0x109, 4, 36),
                (0x10a, 2, 36),
                (0x109, 4, 52),
                (0x10a, 2, 52)
            ]
        );
        assert_eq!(
            geometry(0x8d, 0),
            [(0xe9, -8, 0), (0xea, 8, 0), (0xe9, 120, 0), (0xea, 136, 0)]
        );
        assert_eq!(
            geometry(0x8d, 1),
            [(0xe9, -8, 0), (0xea, 8, 0), (0xe9, 56, 0), (0xea, 72, 0)]
        );
        assert_eq!(
            render_lunar_magic_standard_sprite(0x84, true)
                .unwrap()
                .iter()
                .map(|part| part.definition_index)
                .collect::<Vec<_>>(),
            [0x115]
        );
    }

    #[test]
    fn handlers_8e_through_90_preserve_recovered_composites() {
        let geometry = |sprite, alternate_display| {
            render_lunar_magic_standard_sprite(sprite, alternate_display)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        let grid = geometry(0x8e, false);
        assert_eq!(grid.len(), 16);
        assert_eq!(
            &grid[..4],
            [
                (0x1c0, -4, 0),
                (0x1c1, 12, 0),
                (0x1c2, 28, 0),
                (0x1c3, 44, 0)
            ]
        );
        assert_eq!(
            &grid[12..],
            [
                (0x1f0, -4, 48),
                (0x1f1, 12, 48),
                (0x1f2, 28, 48),
                (0x1f3, 44, 48)
            ]
        );
        assert_eq!(geometry(0x8e, true), [(0x115, 0, 1)]);
        assert_eq!(
            geometry(0x8f, false),
            [(0xe9, -8, 0), (0xea, 8, 0), (0xe9, 120, 0), (0xea, 136, 0)]
        );
        assert_eq!(geometry(0x90, false), grid);
        assert_eq!(geometry(0x90, true), [(0x115, 0, 1)]);
    }

    #[test]
    fn handlers_96_and_97_preserve_recovered_dispatch_aliases() {
        let geometry = |sprite, alternate_display| {
            render_lunar_magic_standard_sprite(sprite, alternate_display)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0x96, false),
            [
                (0x1ee, -6, -11),
                (0x1fe, -6, 1),
                (0x1ff, 10, 1),
                (0x1ef, 4, -15)
            ]
        );
        assert_eq!(geometry(0x96, true), geometry(0x96, false));
        assert_eq!(
            geometry(0x97, false),
            [
                (0x1de, -12, 1),
                (0x1df, 4, 1),
                (0x1cf, -14, 1),
                (0x1ce, 20, 0)
            ]
        );
    }

    #[test]
    fn handlers_91_through_94_normalize_native_render_cell_wrapping() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0x91, 0),
            [
                (0x1ee, -6, -11),
                (0x1fe, -6, 1),
                (0x1ff, 10, 1),
                (0x1ef, 4, -15)
            ]
        );
        assert_eq!(geometry(0x96, 0), geometry(0x91, 0));
        assert_eq!(
            geometry(0x92, 0),
            [
                (0x1fa, -4, 1),
                (0x1fb, 12, 1),
                (0x1ea, -21, 1),
                (0x1eb, -3, 1),
                (0x54, -2, -8),
                (0x169, 0, 5)
            ]
        );
        assert_eq!(
            geometry(0x92, 1),
            [
                (0x1fa, -4, 1),
                (0x1fb, 12, 1),
                (0x1ea, -21, 1),
                (0x1eb, -3, 1),
                (0x1ed, 0, 5)
            ]
        );
        assert_eq!(
            geometry(0x93, 0),
            [
                (0x1f8, -8, -1),
                (0x1f9, 8, -1),
                (0x1e8, -24, -1),
                (0x1e9, -8, -1)
            ]
        );
        assert_eq!(
            geometry(0x94, 0),
            [
                (0x1ee, -5, -9),
                (0x1f6, -16, 4),
                (0x1f7, 0, 1),
                (0x1ec, -24, -8)
            ]
        );
    }

    #[test]
    fn handlers_98_99_and_9c_preserve_wrapped_and_grid_geometry() {
        let geometry = |sprite, alternate_display| {
            render_lunar_magic_standard_sprite(sprite, alternate_display)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0x98, false),
            [
                (0x1dc, -16, 1),
                (0x1dd, 0, 1),
                (0x1db, -32, 1),
                (0x1cc, -32, 1),
                (0x1cd, -16, 1)
            ]
        );
        assert_eq!(
            geometry(0x99, false),
            [
                (0x1cb, 0, 0),
                (0x1cb, 16, 0),
                (0x1d9, -14, -10),
                (0x1da, 30, -10)
            ]
        );
        assert_eq!(
            geometry(0x9c, false),
            [
                (0x1cb, 0, 0),
                (0x1cb, 16, 0),
                (0x1d9, -14, -10),
                (0x1da, 30, -10)
            ]
        );
        assert_eq!(geometry(0x9c, true), geometry(0x9c, false));
    }

    #[test]
    fn handler_95_uses_the_fixed_native_two_row_composite() {
        let geometry = |first| {
            render_lunar_magic_standard_sprite_with_mode(
                0x95,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        let expected = [
            (0x1f8, -8, -1),
            (0x1f9, 8, -1),
            (0x1e8, -8, -17),
            (0x1e9, 8, -17),
        ];
        for first in 0..4 {
            assert_eq!(geometry(first), expected);
        }
    }

    #[test]
    fn handler_9a_preserves_native_packed_coordinate_axes() {
        let geometry = |orientation, alternate_display| {
            render_lunar_magic_standard_sprite_with_mode(
                0x9a,
                StandardSpritePreviewMode {
                    level_orientation: orientation,
                    alternate_display,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(StandardLevelOrientation::Horizontal, false),
            [
                (0x1de, -12, 1),
                (0x1df, 4, 1),
                (0x1cf, 2, -15),
                (0x1ce, 4, 16)
            ]
        );
        assert_eq!(
            geometry(StandardLevelOrientation::Vertical, false),
            [
                (0x1de, 4, -15),
                (0x1df, 4, 1),
                (0x1cf, -14, 1),
                (0x1ce, 20, 0)
            ]
        );
        assert_eq!(
            geometry(StandardLevelOrientation::Horizontal, true),
            geometry(StandardLevelOrientation::Horizontal, false)
        );
    }

    #[test]
    fn handler_9b_preserves_authenticated_two_row_geometry() {
        let geometry = |first, alternate_display| {
            render_lunar_magic_standard_sprite_with_mode(
                0x9b,
                StandardSpritePreviewMode {
                    placement_first: first,
                    alternate_display,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0, false),
            [
                (0x1dc, 0, -15),
                (0x1dd, 16, -15),
                (0x1db, -16, -15),
                (0x1cc, 0, -31),
                (0x1cd, 16, -31)
            ]
        );
        assert_eq!(geometry(1, false), geometry(0, false));
        assert_eq!(geometry(0, true), geometry(0, false));
    }

    #[test]
    fn handlers_9e_through_a1_preserve_composites_and_variants() {
        let geometry = |sprite, first, alternate_display| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    alternate_display,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0x9e, 0, false),
            [
                (0x1d6, -1, 16),
                (0x1d6, -3, 32),
                (0x1c4, -16, 48),
                (0x1c5, 0, 48),
                (0x1d4, -16, 64),
                (0x1d5, 0, 64)
            ]
        );
        let odd_9e = render_lunar_magic_standard_sprite_with_mode(
            0x9e,
            StandardSpritePreviewMode {
                placement_major: 1,
                ..StandardSpritePreviewMode::default()
            },
        )
        .unwrap()
        .iter()
        .map(|part| (part.definition_index, part.x, part.y))
        .collect::<Vec<_>>();
        assert_eq!(
            odd_9e,
            [
                (0x1d6, 1, 16),
                (0x1d6, 3, 32),
                (0x1c4, 0, 48),
                (0x1c5, 16, 48),
                (0x1d4, 0, 64),
                (0x1d5, 16, 64)
            ]
        );
        assert_eq!(
            geometry(0xa0, 0, false),
            [
                (0xeb, 15, 0),
                (0xeb, 29, 0),
                (0xeb, 43, 0),
                (0xed, 42, 0),
                (0xec, 26, 0),
                (0xee, 58, 0)
            ]
        );
        assert_eq!(
            geometry(0xa0, 1, false),
            [
                (0xeb, 17, 0),
                (0xeb, 35, 0),
                (0xeb, 53, 0),
                (0xed, 54, 0),
                (0xec, 38, 0),
                (0xee, 70, 0)
            ]
        );
        assert_eq!(
            geometry(0xa1, 0, false),
            [
                (0x120, -8, -8),
                (0x121, 8, -8),
                (0x130, -8, 8),
                (0x131, 8, 8)
            ]
        );
        assert_eq!(
            geometry(0xa1, 1, false),
            [
                (0x122, -8, -8),
                (0x123, 8, -8),
                (0x132, -8, 8),
                (0x133, 8, 8)
            ]
        );
        assert_eq!(geometry(0xa1, 0, true), [(0x115, 0, 1)]);
    }

    #[test]
    fn banzai_bill_uses_the_native_four_by_four_oam_composition() {
        let geometry = render_lunar_magic_standard_sprite(0x9f, false)
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>();
        let expected = (0_u16..16)
            .map(|index| {
                (
                    0x220 + index,
                    i16::try_from(index % 4).unwrap() * 16,
                    i16::try_from(index / 4).unwrap() * 16,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(geometry, expected);
        assert_eq!(
            render_lunar_magic_standard_sprite(0x9f, true)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>(),
            [(0x115, 0, 1)]
        );
        assert_eq!(
            preview_definition(0x220),
            Some([0x0580, 0x0590, 0x0581, 0x0591])
        );
        assert_eq!(
            preview_definition(0x22e),
            Some([0x8594, 0x8584, 0x8595, 0x8585])
        );
    }

    #[test]
    fn handlers_a2_through_a5_preserve_graphics_and_placement_variants() {
        let geometry = |sprite, mode| {
            render_lunar_magic_standard_sprite_with_mode(sprite, mode)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0xa2, StandardSpritePreviewMode::default()),
            [(0xf9, 0, 1)]
        );
        assert_eq!(
            geometry(
                0xa2,
                StandardSpritePreviewMode {
                    placement_first: 1,
                    sprite_graphics_mode: 2,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [(0xaa, 0, 1)]
        );
        assert_eq!(
            geometry(0xa3, StandardSpritePreviewMode::default()),
            [
                (0xeb, -2, 0),
                (0xeb, -4, 16),
                (0xeb, -6, 32),
                (0xed, -6, 48),
                (0xec, -22, 48),
                (0xee, 10, 48)
            ]
        );
        assert_eq!(
            geometry(
                0xa3,
                StandardSpritePreviewMode {
                    placement_first: 1,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [
                (0xeb, 2, 0),
                (0xeb, 4, 16),
                (0xeb, 6, 32),
                (0xed, 6, 48),
                (0xec, -10, 48),
                (0xee, 22, 48)
            ]
        );
        assert_eq!(
            geometry(0xa4, StandardSpritePreviewMode::default()),
            [(0xfb, 0, 0)]
        );
        assert_eq!(
            geometry(0xa5, StandardSpritePreviewMode::default()),
            [(0xf9, 0, 1)]
        );
        assert_eq!(
            geometry(
                0xa5,
                StandardSpritePreviewMode {
                    sprite_graphics_mode: 2,
                    placement_first: 1,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [(0xaa, 0, 1)]
        );
        for sprite in [0xa2, 0xa3, 0xa5] {
            assert_eq!(
                geometry(
                    sprite,
                    StandardSpritePreviewMode {
                        alternate_display: true,
                        ..StandardSpritePreviewMode::default()
                    }
                ),
                [(0x115, 0, 1)]
            );
        }
    }

    #[test]
    fn handlers_a7_through_ac_preserve_adjacent_cell_chains() {
        let geometry = |sprite, alternate_display| {
            render_lunar_magic_standard_sprite(sprite, alternate_display)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0xa7, false), [(0x1c9, 0, 1), (0x1ca, 16, 1)]);
        assert_eq!(geometry(0xa8, false), [(0xfc, 0, -8)]);
        assert_eq!(
            geometry(0xa9, false),
            [
                (0x16d, 0, 1),
                (0x208, -16, 1),
                (0x208, -32, 1),
                (0x208, -48, 1),
                (0x208, -64, 1)
            ]
        );
        assert_eq!(geometry(0xaa, false), [(0x1c9, 0, 1), (0x1ca, 16, 1)]);
        assert_eq!(geometry(0xaa, true), [(0x115, 0, 1)]);
        assert_eq!(
            geometry(0xac, false),
            [
                (0x16d, 0, 1),
                (0x208, 0, -15),
                (0x208, 0, -31),
                (0x208, 0, -47),
                (0x208, 0, -63)
            ]
        );
        for sprite in [0xa7, 0xa8, 0xac] {
            assert_eq!(geometry(sprite, true), [(0x115, 0, 1)]);
        }
    }

    #[test]
    fn handlers_a6_and_ab_preserve_recovered_composites() {
        let geometry = |sprite, mode| {
            render_lunar_magic_standard_sprite_with_mode(sprite, mode)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0xa6, StandardSpritePreviewMode::default()),
            [
                (0x1d7, -8, 8),
                (0x1d8, 8, 8),
                (0x1c7, -8, -8),
                (0x1c8, 8, -8),
                (0x1c6, 0, 0)
            ]
        );
        assert_eq!(
            geometry(
                0xa6,
                StandardSpritePreviewMode {
                    placement_first: 1,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [
                (0x1d7, -8, 8),
                (0x1d8, 8, 8),
                (0x1c7, -8, -8),
                (0x1c8, 8, -8),
                (0x1c6, 8, 0)
            ]
        );
        assert_eq!(
            geometry(
                0xa6,
                StandardSpritePreviewMode {
                    alternate_display: true,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [(0x115, 0, 1)]
        );
        assert_eq!(
            geometry(0xab, StandardSpritePreviewMode::default()),
            [(0x18d, -4, -15), (0x20b, 0, 0)]
        );
        assert_eq!(
            geometry(
                0xab,
                StandardSpritePreviewMode {
                    alternate_display: true,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [(0x115, 0, 1)]
        );
    }

    #[test]
    fn handlers_ad_through_b2_preserve_stepped_and_single_shapes() {
        let geometry = |sprite, alternate_display| {
            render_lunar_magic_standard_sprite(sprite, alternate_display)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0xad, false),
            [
                (0x17d, 0, 1),
                (0x16e, 0, 17),
                (0x16e, 0, 33),
                (0x16e, 0, 49),
                (0x16e, 0, 65)
            ]
        );
        assert_eq!(geometry(0xae, false), [(0xb8, 0, 0)]);
        assert_eq!(geometry(0xaf, false), [(0x15d, 0, 0)]);
        assert_eq!(geometry(0xb0, false), [(0x14d, 0, 1)]);
        assert_eq!(geometry(0xb1, false), [(0xb8, 0, 0)]);
        assert_eq!(geometry(0xb2, false), [(0x13d, 0, 0)]);
        for sprite in [0xad, 0xaf, 0xb0, 0xb1, 0xb2] {
            assert_eq!(geometry(sprite, true), [(0x115, 0, 1)]);
        }
    }

    #[test]
    fn handler_b3_selects_the_context_specific_graphics_definition() {
        let definition = |mode| {
            render_lunar_magic_standard_sprite_with_mode(0xb3, mode).unwrap()[0].definition_index
        };
        assert_eq!(definition(StandardSpritePreviewMode::default()), 0x12d);
        assert_eq!(
            definition(StandardSpritePreviewMode {
                special_display_mode: true,
                sprite_graphics_mode: 0x0d,
                ..StandardSpritePreviewMode::default()
            }),
            0x116
        );
        assert_eq!(
            definition(StandardSpritePreviewMode {
                special_display_mode: true,
                sprite_graphics_mode: 0x0c,
                ..StandardSpritePreviewMode::default()
            }),
            0x12d
        );
        assert_eq!(
            definition(StandardSpritePreviewMode {
                alternate_display: true,
                ..StandardSpritePreviewMode::default()
            }),
            0x115
        );
    }

    #[test]
    fn handlers_b4_through_bb_preserve_adjacent_and_placement_shapes() {
        let geometry = |sprite, first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0xb4, 0),
            [
                (0x0ab, 0, 1),
                (0x0ac, 16, 1),
                (0x0bb, 0, 17),
                (0x0bc, 16, 17)
            ]
        );
        assert_eq!(geometry(0xb5, 0), [(0x13d, 0, 0)]);
        assert_eq!(geometry(0xb6, 0), [(0x12d, 0, 0)]);
        assert_eq!(
            render_lunar_magic_standard_sprite_with_mode(
                0xb6,
                StandardSpritePreviewMode {
                    special_display_mode: true,
                    sprite_graphics_mode: 0x0d,
                    ..StandardSpritePreviewMode::default()
                }
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>(),
            [(0x116, 0, 0)]
        );
        assert_eq!(
            render_lunar_magic_standard_sprite(0xb6, true)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>(),
            [(0x115, 0, 1)]
        );
        assert_eq!(
            geometry(0xb7, 0),
            [(0x185, 16, 1), (0x194, 16, 1), (0x195, 32, 1)]
        );
        assert_eq!(
            geometry(0xb8, 0),
            [
                (0x18b, 0, 1),
                (0x18c, 16, 1),
                (0x19b, 16, 1),
                (0x19c, 32, 1)
            ]
        );
        assert_eq!(
            geometry(0xba, 0),
            [(0x198, 0, 1), (0x199, 16, 1), (0x184, 12, 5)]
        );
        assert_eq!(
            render_lunar_magic_standard_sprite_with_mode(
                0xba,
                StandardSpritePreviewMode {
                    placement_first: 0,
                    placement_major: 1,
                    ..StandardSpritePreviewMode::default()
                }
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>(),
            [(0x198, 0, 1), (0x199, 16, 1), (0x187, 12, 5)]
        );
        assert_eq!(
            geometry(0xbb, 0),
            [
                (0x18b, 0, 1),
                (0x18c, 16, 1),
                (0x19b, 0, 17),
                (0x19c, 16, 17)
            ]
        );
        for sprite in [0xba, 0xbb] {
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true)
                    .unwrap()
                    .iter()
                    .map(|part| (part.definition_index, part.x, part.y))
                    .collect::<Vec<_>>(),
                [(0x115, 0, 1)]
            );
        }
    }

    #[test]
    fn handler_b9_uses_authenticated_004c9710_geometry() {
        let geometry = |first, alternate_display| {
            render_lunar_magic_standard_sprite_with_mode(
                0xb9,
                StandardSpritePreviewMode {
                    placement_first: first,
                    alternate_display,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0, false),
            [(0x10b, 0, 1), (0x10c, 0, 1), (0x10a, 3, 1)]
        );
        assert_eq!(
            geometry(1, false),
            [(0x10b, 0, 1), (0x10d, 0, 1), (0x10a, 3, 1)]
        );
        assert_eq!(geometry(2, false), geometry(0, false));
        assert_eq!(geometry(3, false), geometry(1, false));
        assert_eq!(geometry(0, true), [(0x115, 0, 1)]);
    }

    #[test]
    fn handlers_bc_through_c4_preserve_recovered_composites() {
        let geometry = |sprite, first, alternate_display| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    placement_first: first,
                    alternate_display,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(0xbc, 0, false),
            [
                (0x174, 0, 1),
                (0x175, 16, 1),
                (0x164, -16, 1),
                (0x165, 0, 1)
            ]
        );
        assert_eq!(geometry(0xbd, 0, false), [(0x02, 0, 1)]);
        assert_eq!(geometry(0xbd, 0, true), [(0x115, 0, 1)]);
        assert_eq!(geometry(0xbe, 0, false), [(0x17b, 0, 1)]);
        assert_eq!(geometry(0xbe, 1, false), [(0x17b, 0, 1)]);
        assert_eq!(geometry(0xbe, 0, true), [(0x115, 0, 1)]);
        assert_eq!(
            geometry(0xbf, 0, false),
            [
                (0x174, 0, 1),
                (0x175, 16, 1),
                (0x164, 0, -15),
                (0x165, 16, -15)
            ]
        );
        assert_eq!(
            geometry(0xc0, 0, false),
            [(0x176, 0, 3), (0x177, 16, 3), (0x178, 32, 3)]
        );
        assert_eq!(
            geometry(0xc1, 0, false),
            [
                (0x1cb, 0, -2),
                (0x1cb, 16, -2),
                (0x1cb, 32, -2),
                (0x1da, 46, -12),
                (0x1d9, -14, -12)
            ]
        );
        assert_eq!(
            geometry(0xc1, 1, false),
            [
                (0x1cb, 0, 2),
                (0x1cb, 16, 2),
                (0x1cb, 32, 2),
                (0x1da, 46, -8),
                (0x1d9, -14, -8)
            ]
        );
        assert_eq!(geometry(0xc2, 0, false), [(0x166, 0, 0)]);
        assert_eq!(geometry(0xc3, 0, false), [(0x179, 0, 0)]);
        assert_eq!(
            geometry(0xc4, 0, false),
            [(0xec, 0, 1), (0xed, 16, 1), (0xed, 32, 1), (0xee, 48, 1)]
        );
        for sprite in [0xbc, 0xbf, 0xc0] {
            assert_eq!(geometry(sprite, 0, true), [(0x115, 0, 1)]);
        }
    }

    #[test]
    fn handlers_c5_through_cf_preserve_recovered_composites() {
        let geometry = |sprite, alternate_display| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    alternate_display,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0xc5, false), [(0x167, 0, 1)]);
        assert_eq!(geometry(0xc6, false), [(0x179, 0, 0)]);
        assert_eq!(geometry(0xc7, false), [(0x8101, 0, 0)]);
        assert_eq!(geometry(0xc8, false), [(0x167, 0, 1)]);
        assert_eq!(geometry(0xc9, false), [(0x38, 0, 1), (0x114, 0, -8)]);
        assert_eq!(geometry(0xc9, true), [(0x115, 0, 1), (0x114, 0, -8)]);
        assert_eq!(
            geometry(0xca, false),
            [(0x158, 0, 26), (0x159, 16, 26), (0x168, 8, 16)]
        );
        assert_eq!(
            geometry(0xcb, false),
            [(0x56, -6, -14), (0x67, -6, 2), (0x114, -8, -16)]
        );
        assert_eq!(
            geometry(0xca, true),
            [(0x158, 0, 26), (0x159, 16, 26), (0x168, 8, 16)]
        );
        assert_eq!(geometry(0xcb, true), [(0x115, 0, 1), (0x114, -8, -16)]);
        assert_eq!(
            geometry(0xcc, false),
            [
                (0x56, -6, -14),
                (0x66, -6, 2),
                (0x56, 10, -14),
                (0x67, 10, 2),
                (0x114, 5, -16)
            ]
        );
        assert_eq!(
            geometry(0xcc, true),
            [(0x115, 0, 1), (0x115, 16, 1), (0x114, 5, -16)]
        );
        assert_eq!(
            geometry(0xcd, false),
            [(0x154, 0, 0), (0x114, 0, -8), (0x155, 8, 0), (0x156, 24, 0)]
        );
        assert_eq!(
            geometry(0xce, false),
            [(0x84, 0, 0), (0x114, 0, -8), (0x85, 16, 0), (0x86, 24, 0)]
        );
        assert_eq!(geometry(0xcf, false), [(0x14, 0, 1), (0x114, 0, -8)]);
        assert_eq!(geometry(0xcf, true), [(0x115, 0, 1), (0x114, 0, -8)]);
    }

    #[test]
    fn handlers_d2_through_d9_preserve_recovered_composites() {
        let geometry = |sprite, alternate_display| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite,
                StandardSpritePreviewMode {
                    alternate_display,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        let d2 = geometry(0xd2, false);
        assert_eq!(d2.len(), 19);
        assert_eq!(d2[0], (0x1bd, -8, 0));
        assert_eq!(
            &d2[1..6],
            &[
                (0x1bf, -1, 3),
                (0x1be, -15, 3),
                (0x1af, -17, 5),
                (0x1ae, -31, 5),
                (0x20c, -32, 4)
            ]
        );
        assert_eq!(d2[6], (0x114, 0, -16));
        assert_eq!(d2[7], (0x1ad, 8, 0));
        assert_eq!(d2[13], (0x14, 24, 0));
        assert_eq!(
            geometry(0xd3, false),
            [(0xe5, -3, 8), (0xe6, 13, 8), (0xe7, 5, 11), (0x114, 0, 0)]
        );
        assert_eq!(geometry(0xd3, true), [(0x115, 0, 1), (0x114, 0, 0)]);
        assert_eq!(geometry(0xd4, false), d2);
        assert_eq!(
            geometry(0xd5, false),
            [(0x25, 0, 1), (0x25, 8, 1), (0x114, 0, -8)]
        );
        assert_eq!(
            geometry(0xd5, true),
            [(0x115, 0, 1), (0x25, 8, 1), (0x114, 0, -8)]
        );
        assert_eq!(geometry(0xd6, false), [(0x157, 0, 0), (0x114, 0, -8)]);
        assert_eq!(geometry(0xd7, false), [(0x11d, 0, 0), (0x114, 0, -8)]);
        assert_eq!(geometry(0xd8, false), [(0x14d, 0, 1), (0x114, 0, -8)]);
        assert_eq!(geometry(0xd8, true), [(0x115, 0, 1), (0x114, 0, -8)]);
        assert_eq!(geometry(0xdc, false), [(0x32, 0, 1)]);
        assert_eq!(geometry(0xdc, true), [(0x115, 0, 1)]);
        let db_geometry = |alternate_graphics| {
            render_lunar_magic_standard_sprite_with_mode(
                0xdb,
                StandardSpritePreviewMode {
                    alternate_graphics,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        assert_eq!(db_geometry(false), [(0x31, 0, 1)]);
        assert_eq!(db_geometry(true), [(0x32, 0, 1)]);
        assert_eq!(geometry(0xdd, false), [(0x33, 0, 1)]);
        assert_eq!(geometry(0xdd, true), [(0x115, 0, 1)]);
    }

    #[test]
    fn handlers_df_through_e1_preserve_recovered_geometry() {
        let geometry = |sprite| {
            render_lunar_magic_standard_sprite(sprite, false)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(geometry(0xdf), [(0x1b8, 0, 0), (0x114, 0, 0)]);
        assert_eq!(
            geometry(0xde),
            [
                (0x48, 0, 16),
                (0x48, -32, 16),
                (0x48, -16, -16),
                (0x48, 32, 16),
                (0x48, 16, -16)
            ]
        );
        let vertical_de = render_lunar_magic_standard_sprite_with_mode(
            0xde,
            StandardSpritePreviewMode {
                level_orientation: StandardLevelOrientation::Vertical,
                ..StandardSpritePreviewMode::default()
            },
        )
        .unwrap()
        .into_iter()
        .map(|part| (part.definition_index, part.x, part.y))
        .collect::<Vec<_>>();
        assert_eq!(
            vertical_de,
            [
                (0x48, 16, 0),
                (0x48, 16, -32),
                (0x48, -16, -16),
                (0x48, 16, 32),
                (0x48, -16, 16)
            ]
        );
        assert_eq!(geometry(0xe0).len(), 18);
        assert_eq!(geometry(0xe1), [(0x1b8, 0, 0), (0x114, 0, 0)]);
        assert_eq!(geometry(0xe2), [(0x14c, 0, 0), (0x114, 0, 0)]);
        assert_eq!(
            geometry(0xe3),
            [
                (0x1a6, -62, -49),
                (0x1ab, -30, -72),
                (0x1a8, 7, -79),
                (0x1a6, 43, -66),
                (0x1a6, 70, -37),
                (0x1ab, 81, 0),
                (0x1a8, 71, 38),
                (0x1a6, 45, 67),
                (0x1a6, 9, 80),
                (0x1ab, -29, 75)
            ]
        );
        assert_eq!(geometry(0xe4), [(0x14b, 0, 0), (0x114, 0, 0)]);
    }

    #[test]
    fn fixed_text_handlers_emit_native_background_and_ascii_words() {
        let geometry = |sprite| {
            render_lunar_magic_standard_sprite(sprite, false)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        let d0 = geometry(0xd0);
        assert_eq!(d0.len(), (" Turn Off ".len() + "Generator2".len()) * 2);
        assert_eq!(
            &d0[..4],
            &[
                (0x3c7c, 0, 0),
                (0x3c20, 0, 0),
                (0x3c7c, 8, 0),
                (0x3c54, 8, 0)
            ]
        );
        assert_eq!(&d0[d0.len() - 2..], &[(0x3c7c, 72, 8), (0x3c32, 72, 8)]);

        let d9 = geometry(0xd9);
        assert_eq!(d9[d9.len() - 1], (0x3c73, 72, 8));
        let e8 = geometry(0xe8);
        assert_eq!(e8[1], (0x3c41, 0, 0));
        assert_eq!(e8["Auto-Scroll".len() * 2 + 1], (0x3c20, 0, 8));
        let ea = geometry(0xea);
        assert_eq!(ea["Layer 2 Scroll".len() * 2 + 1], (0x3c20, 0, 8));
        let f2 = geometry(0xf2);
        assert_eq!(f2["   Layer 2   ".len() * 2 + 1], (0x3c4f, 0, 8));
        let ec = geometry(0xec);
        assert_eq!(ec.len(), "Fast BG Scroll".len() * 2);
        assert_eq!(ec[1], (0x3c46, 0, 0));

        let direct = render_lunar_magic_standard_sprite(0xec, false).unwrap();
        assert_eq!(direct[1].subtiles, [0x3c46, 0x0019, 0x0019, 0x0019]);
    }

    #[test]
    fn configured_text_handlers_follow_level_mode_and_position_nibbles() {
        let line = |sprite, mode: StandardSpritePreviewMode, y| {
            render_lunar_magic_standard_sprite_with_mode(sprite, mode)
                .unwrap()
                .iter()
                .filter(|part| part.y == y && part.definition_index != 0x3c7c)
                .map(|part| {
                    char::from_u32(u32::from(part.definition_index - 0x3c00))
                        .expect("native preview glyph is ASCII")
                })
                .collect::<String>()
        };
        assert_eq!(
            line(0xe5, StandardSpritePreviewMode::default(), 8),
            " Special 1 "
        );
        assert_eq!(
            line(
                0xe5,
                StandardSpritePreviewMode {
                    placement_first: 0x10,
                    ..StandardSpritePreviewMode::default()
                },
                8
            ),
            "Special 1-A"
        );
        assert_eq!(
            line(
                0xe5,
                StandardSpritePreviewMode {
                    level_mode: 3,
                    ..StandardSpritePreviewMode::default()
                },
                8
            ),
            " Special 4 "
        );
        assert_eq!(
            render_lunar_magic_standard_sprite_with_mode(
                0xe6,
                StandardSpritePreviewMode {
                    level_mode: 2,
                    ..StandardSpritePreviewMode::default()
                }
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>(),
            [(0x14b, 0, 0), (0x114, 0, 0)]
        );
        assert_eq!(
            line(
                0xe7,
                StandardSpritePreviewMode {
                    level_mode: 3,
                    placement_first: 0x10,
                    ..StandardSpritePreviewMode::default()
                },
                8
            ),
            "Smash Range 11"
        );
        assert_eq!(
            line(
                0xe9,
                StandardSpritePreviewMode {
                    level_mode: 1,
                    ..StandardSpritePreviewMode::default()
                },
                8
            ),
            "Sideways  Long"
        );
        assert_eq!(
            line(
                0xeb,
                StandardSpritePreviewMode {
                    level_orientation: StandardLevelOrientation::Vertical,
                    placement_first: 5,
                    level_mode: 1,
                    ..StandardSpritePreviewMode::default()
                },
                0
            ),
            "  Medium 2 "
        );
    }

    #[test]
    fn handler_d1_preserves_native_helper_and_overlay_geometry() {
        let geometry = |alternate_display| {
            render_lunar_magic_standard_sprite(0xd1, alternate_display)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            geometry(false),
            [(0xe5, -3, 8), (0xe6, 13, 8), (0xe7, 5, 11), (0x114, 0, 0)]
        );
        assert_eq!(geometry(true), [(0x115, 0, 1), (0x114, 0, 0)]);

        let label = |mode| {
            render_lunar_magic_standard_sprite_with_mode(0xed, mode)
                .unwrap()
                .iter()
                .filter(|part| part.y == 8 && part.definition_index != 0x3c7c)
                .map(|part| {
                    char::from_u32(u32::from(part.definition_index - 0x3c00))
                        .expect("native preview glyph is ASCII")
                })
                .collect::<String>()
        };
        assert_eq!(
            label(StandardSpritePreviewMode {
                level_mode: 2,
                ..StandardSpritePreviewMode::default()
            }),
            "  Rise Up  "
        );
        assert_eq!(
            label(StandardSpritePreviewMode {
                placement_first: 0x10,
                ..StandardSpritePreviewMode::default()
            }),
            "MAY GLITCH!"
        );
    }

    #[test]
    fn handler_e0_preserves_its_three_arm_composite() {
        let geometry = |first| {
            render_lunar_magic_standard_sprite_with_mode(
                0xe0,
                StandardSpritePreviewMode {
                    placement_first: first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };
        let left = geometry(0);
        assert_eq!(left.len(), 18);
        assert_eq!(
            &left[..6],
            &[
                (0xeb, -2, 16),
                (0xeb, -4, 32),
                (0xeb, -6, 48),
                (0xed, -6, 48),
                (0xec, -22, 48),
                (0xee, 10, 48)
            ]
        );
        assert_eq!(
            &left[6..12],
            &[
                (0xeb, -12, -10),
                (0xeb, -24, -20),
                (0xeb, -36, -30),
                (0xed, -36, -30),
                (0xec, -52, -30),
                (0xee, -20, -30)
            ]
        );
        // $E0's direction comes from its even dispatch identity, not the packed
        // placement byte.
        assert_eq!(geometry(1), left);
    }

    #[test]
    fn recovered_80_through_8b_aliases_and_variant_geometry_are_not_fallbacks() {
        let geometry = |sprite_number, placement_first| {
            render_lunar_magic_standard_sprite_with_mode(
                sprite_number,
                StandardSpritePreviewMode {
                    placement_first,
                    ..StandardSpritePreviewMode::default()
                },
            )
            .unwrap()
            .iter()
            .map(|part| (part.definition_index, part.x, part.y))
            .collect::<Vec<_>>()
        };

        assert_eq!(geometry(0x80, 0), [(0x0b, 0, 1)]);
        assert_eq!(
            geometry(0x83, 0),
            [
                (0x06, -14, -9),
                (0x07, -8, -9),
                (0x801a, -3, -9),
                (0x108, -3, -1)
            ]
        );
        assert_eq!(geometry(0x83, 1)[2], (0x8104, -3, -9));
        assert_eq!(geometry(0x83, 2)[2], (0x8106, -3, -9));
        assert_eq!(geometry(0x83, 3)[2], (0x8100, -3, -9));
        assert_eq!(geometry(0x87, 0), geometry(0x85, 0));
        assert_eq!(geometry(0x88, 0), [(0x06, 0, 1)]);
        assert_eq!(geometry(0x8b, 0), geometry(0x89, 0));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Enumerates every native branch in one contiguous cluster.
    fn handlers_64_67_and_68_preserve_context_and_edge_branches() {
        let geometry = |sprite_number, mode| {
            render_lunar_magic_standard_sprite_with_mode(sprite_number, mode).map(|parts| {
                parts
                    .iter()
                    .map(|part| (part.definition_index, part.x, part.y))
                    .collect::<Vec<_>>()
            })
        };

        let short = geometry(0x64, StandardSpritePreviewMode::default()).unwrap();
        assert_eq!(
            short,
            [
                (0x8e, -8, -7),
                (0x8f, -8, 9),
                (0x8f, -8, 25),
                (0x8f, -8, 41),
                (0x9f, -8, 57)
            ]
        );
        let long = geometry(
            0x64,
            StandardSpritePreviewMode {
                placement_first: 1,
                wide_context: StandardSpriteWideContext::ValidLong64,
                ..StandardSpritePreviewMode::default()
            },
        )
        .unwrap();
        assert_eq!(long.len(), 9);
        assert_eq!(long.last(), Some(&(0x9f, -8, 121)));

        assert_eq!(
            geometry(0x65, StandardSpritePreviewMode::default()).unwrap(),
            [
                (0xaf, 8, -33),
                (0xbf, 8, -19),
                (0x8e, 8, -7),
                (0x201, 8, -13),
                (0x202, 3, -13),
                (0x203, 3, -13)
            ]
        );
        assert_eq!(
            geometry(0x66, StandardSpritePreviewMode::default()).unwrap(),
            [
                (0x12c, 8, 23),
                (0x11c, 8, 9),
                (0x8e, 8, -7),
                (0x201, 8, -13),
                (0x202, 3, -13),
                (0x203, 3, -13)
            ]
        );
        assert_eq!(
            geometry(0x67, StandardSpritePreviewMode::default()).unwrap(),
            [(0xbd, 0, 1), (0xbe, 16, 1), (0xad, 0, -15), (0xae, 16, -15)]
        );
        let odd = StandardSpritePreviewMode {
            placement_first: 1,
            ..StandardSpritePreviewMode::default()
        };
        assert_eq!(
            geometry(0x65, odd).unwrap(),
            [
                (0xaf, 8, -35),
                (0xbf, 8, -21),
                (0x9e, 8, -7),
                (0x201, 16, -13),
                (0x204, 13, -13),
                (0x205, 13, -13)
            ]
        );
        assert_eq!(
            geometry(0x66, odd).unwrap(),
            [
                (0x12c, 8, 21),
                (0x11c, 8, 7),
                (0x9e, 8, -7),
                (0x201, 16, -13),
                (0x204, 13, -13),
                (0x205, 13, -13)
            ]
        );
        assert_eq!(
            geometry(0x67, odd).unwrap(),
            [(0xbb, 0, 1), (0xbc, 16, 1), (0xab, 0, -15), (0xac, 16, -15)]
        );
        assert_eq!(
            geometry(0x68, StandardSpritePreviewMode::default()).unwrap(),
            [(0xaa, 8, -7)]
        );
        assert_eq!(geometry(0x68, odd).unwrap(), [(0xa9, 8, -7)]);

        let invalid = StandardSpritePreviewMode {
            wide_context: StandardSpriteWideContext::Invalid,
            ..StandardSpritePreviewMode::default()
        };
        assert!(geometry(0x65, invalid).is_none());
        assert!(geometry(0x66, invalid).is_none());
        assert!(geometry(0x67, invalid).is_none());
        assert!(geometry(0x68, invalid).is_none());
        assert_eq!(
            geometry(
                0x67,
                StandardSpritePreviewMode {
                    alternate_display: true,
                    wide_context: StandardSpriteWideContext::Invalid,
                    ..StandardSpritePreviewMode::default()
                }
            )
            .unwrap(),
            [(0x115, 0, 1)]
        );
    }

    #[test]
    fn remaining_standard_handlers_preserve_alias_sequence_and_composite_state() {
        let geometry = |sprite_number, mode| {
            render_lunar_magic_standard_sprite_with_mode(sprite_number, mode)
                .unwrap()
                .iter()
                .map(|part| (part.definition_index, part.x, part.y))
                .collect::<Vec<_>>()
        };

        assert_eq!(
            geometry(0x6c, StandardSpritePreviewMode::default()),
            [(0xb7, -16, 1), (0xb7, -32, 1), (0xb7, -40, 1)]
        );
        for (sequence_index, definition) in [(0, 0x110), (1, 0x111), (2, 0x112), (3, 0x113)] {
            assert_eq!(
                geometry(
                    0x8a,
                    StandardSpritePreviewMode {
                        sprite_8a_sequence_index: sequence_index,
                        ..StandardSpritePreviewMode::default()
                    }
                ),
                [(definition, 0, 1)]
            );
        }
        for sequence_index in [4, 5, u8::MAX] {
            assert_eq!(
                geometry(
                    0x8a,
                    StandardSpritePreviewMode {
                        sprite_8a_sequence_index: sequence_index,
                        ..StandardSpritePreviewMode::default()
                    }
                ),
                [(0x01, 0, 0)]
            );
        }

        assert_ne!(
            geometry(0x9d, StandardSpritePreviewMode::default()),
            geometry(0x9a, StandardSpritePreviewMode::default())
        );
        assert_eq!(
            geometry(
                0x9d,
                StandardSpritePreviewMode {
                    placement_first: 1,
                    placement_major: 28,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [
                (0x1bd, -8, 0),
                (0x1bf, -1, 3),
                (0x1be, -15, 3),
                (0x1af, -1, -11),
                (0x1ae, -15, -11),
                (0x20c, -16, -12)
            ]
        );
        assert_eq!(
            geometry(
                0x9d,
                StandardSpritePreviewMode {
                    placement_first: 0,
                    placement_major: 17,
                    ..StandardSpritePreviewMode::default()
                }
            )[0],
            (0x1ad, -8, 0)
        );
        assert_eq!(
            geometry(
                0x9d,
                StandardSpritePreviewMode {
                    alternate_display: true,
                    ..StandardSpritePreviewMode::default()
                }
            ),
            [(0x115, 0, 1)]
        );
    }

    #[test]
    fn late_compatibility_dispatch_entries_match_their_native_aliases() {
        let modes = [
            StandardSpritePreviewMode::default(),
            StandardSpritePreviewMode {
                placement_first: 0x10,
                level_mode: 1,
                ..StandardSpritePreviewMode::default()
            },
            StandardSpritePreviewMode {
                placement_first: 0x03,
                level_mode: 3,
                level_orientation: StandardLevelOrientation::Vertical,
                ..StandardSpritePreviewMode::default()
            },
        ];

        for mode in modes {
            for (compatibility_id, ordinary_id) in
                [(0xef, 0xe7), (0xf3, 0xeb), (0xf4, 0xec), (0xf5, 0xed)]
            {
                assert_eq!(
                    render_lunar_magic_standard_sprite_with_mode(compatibility_id, mode),
                    render_lunar_magic_standard_sprite_with_mode(ordinary_id, mode),
                    "${compatibility_id:02X} must alias ${ordinary_id:02X}"
                );
            }
        }
    }

    #[test]
    fn late_default_and_custom_fallback_entries_have_no_builtin_artwork() {
        for sprite_number in [0x29, 0x30, 0xee, 0xf0, 0xf1] {
            assert_eq!(
                render_lunar_magic_standard_sprite_with_mode(
                    sprite_number,
                    StandardSpritePreviewMode::default()
                ),
                None
            );
        }
        for sprite_number in 0xf6..=0xff {
            assert_eq!(
                render_lunar_magic_standard_sprite_with_mode(
                    sprite_number,
                    StandardSpritePreviewMode::default()
                ),
                None
            );
        }
    }

    #[test]
    fn every_sprite_id_has_exactly_its_recovered_preview_source() {
        let mut missing_built_in = Vec::new();
        for sprite_number in u8::MIN..=u8::MAX {
            let preview = render_lunar_magic_standard_sprite_with_mode(
                sprite_number,
                StandardSpritePreviewMode::default(),
            );
            match lunar_magic_standard_sprite_preview_source(sprite_number) {
                StandardSpritePreviewSource::BuiltIn => {
                    if preview.is_none() {
                        missing_built_in.push(sprite_number);
                    }
                }
                StandardSpritePreviewSource::NativeEmpty
                | StandardSpritePreviewSource::CustomDisplay => {
                    assert_eq!(
                        preview, None,
                        "${sprite_number:02X} fabricated native artwork"
                    );
                }
            }
        }
        assert!(
            missing_built_in.is_empty(),
            "IDs classified as built-in but empty in the default context: {missing_built_in:02X?}"
        );
    }
}
