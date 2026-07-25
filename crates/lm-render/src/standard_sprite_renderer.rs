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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StandardSpritePreviewMode {
    pub alternate_display: bool,
    pub alternate_graphics: bool,
    /// First native sprite-record byte used by placement-dependent handlers.
    pub placement_first: u8,
}

/// Renders the first authenticated family of Lunar Magic standard-sprite previews.
///
/// Sprite IDs `$00`–`$08` use authenticated handler shapes and preview definitions.
/// Lunar Magic substitutes tile `$115` when its alternate sprite-number display mode is active.
/// Other IDs remain unresolved and return `None` instead of fabricating artwork.
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
            placement_first: 0,
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
        0x12 => parts(&[(0x01, 0, 0)]),
        0x13 => parts(&[(0x0e, 0, 1)]),
        0x14 => parts(&[(0x0f, 0, 1)]),
        0x15 | 0x17 | 0x18 => parts(&[(0x14, 0, 1)]),
        0x16 => parts(&[(0x15, 0, 1)]),
        0x1a => parts(&[(0x16, 8, -31), (0x26, 8, -15)]),
        0x1b => parts(&[(0x24, 0, 1)]),
        0x1c => parts(&[(0x25, 0, 1)]),
        0x1d => parts(&[(0x17, 0, 1)]),
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
        0x37 => parts(&[(0x38, 0, 1)]),
        0x38 => parts(&[(0x48, 0, 1)]),
        0x39..=0x3b => render_square_handler(
            0x39 + u16::from(sprite_number - 0x39) * 2,
            mode.placement_first,
        ),
        0x3c => parts(&[(0x54, 8, -15), (0x64, 0, 1)]),
        0x3d => parts(&[(0x55 + u16::from(mode.placement_first & 1) * 0x10, 0, 1)]),
        0x3e => parts(&[(0x56, -6, -14), (0x66, -6, 2)]),
        0x3f => parts(&[(0x56, -6, -14), (0x67, -6, 2)]),
        0x40 => parts(&[(0x74, 0, 1), (0x75, 8, 1), (0x76, 24, 1)]),
        0x41 => parts(&[(0x154, 0, 1), (0x155, 8, 1), (0x156, 24, 1)]),
        0x42 => parts(&[(0x77, 0, 1), (0x87, 0, 17)]),
        0x43 => parts(&[(0x94, 0, 1), (0x57, -12, 1), (0x95, 16, 1)]),
        0x44 => parts(&[(0x96, 4, 1)]),
        0x45 => parts(&[(0x97, -16, 1), (0x98, -4, 1), (0x99, 12, 1), (0x88, -2, -7)]),
        0x46 => parts(&[(0x14, -2, 1)]),
        0x47 => parts(&[(0x89, 0, -3)]),
        0x48 => parts(&[(0xa4, 0, 0), (0xa5, 16, 0)]),
        0x49 => parts(&[(0xa6, 0, 1)]),
        0x4a => parts(&[(0x58, 8, -16), (0x68, 8, 0)]),
        0x4b => render_handler_4b(mode.placement_first),
        0x4c => parts(&[(0xa7, 0, 1), (0xa8, 0, 1)]),
        0x4d => {
            let mut values = vec![(0xa7, 0, 1)];
            values.extend(shared_marker_parts());
            parts(&values)
        }
        0x4e => parts(&[(0x16, 8, -8), (0xb4, 8, 8)]),
        0x4f => parts(&[(0x16, 8, -8), (0xb4, 8, 8), (0xb5, 6, -16), (0xb6, 18, -16)]),
        0x50 => parts(&[(0x78, 0, 1)]),
        0x51 => parts(&[(0x59, -4, 0), (0x69, 4, 0), (0x69, 20, 0), (0x79, 28, 0)]),
        0x52 => parts(&[(0x21b, 0, 1)]),
        0x53 => render_handler_53(),
        0x54 | 0x56 => parts(&[
            (0x6a, 0, 1),
            (0x5b, 16, 1),
            (0x5b, 32, 1),
            (0x5b, 48, 1),
            (0x5a, 64, 1),
        ]),
        0x55 | 0x57 => parts(&[
            (0x5d, 0, 1),
            (0x5e, 16, 1),
            (0x5f, 32, 1),
            (0x6e, 8, 17),
            (0x6f, 24, 17),
        ]),
        0x58 => parts(&[
            (0x5c, 0, 1),
            (0x5c, 0, 17),
            (0x6c, 0, 33),
            (0x5c, 0, -15),
            (0x5c, 0, -31),
        ]),
        0x59 => parts(&[
            (0x5c, 0, 1),
            (0x5c, 16, 1),
            (0x6c, 32, 1),
            (0x5c, -16, 1),
            (0x5c, -32, 1),
        ]),
        0x5a => parts(&[(0x142, 0, 0), (0x143, 16, 0), (0x144, 32, 0)]),
        0x5b => parts(&[
            (0x7b, 0, 1),
            (0x6b, 16, 1),
            (0x6b, 32, 1),
            (0x6b, 48, 1),
            (0x7a, 64, 1),
        ]),
        0x5c => parts(&[
            (0x8a, 0, 0),
            (0x8b, 16, 0),
            (0xff, 32, 0),
            (0x9a, 8, 16),
            (0x13b, 24, 16),
        ]),
        0x5d => parts(&[
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
        0x5e => {
            let mut values = Vec::with_capacity(9);
            for row in 1_i16..=5 {
                values.push((0x6d, 32, row * 16));
            }
            values.extend([(0x7c, 8, 9), (0x7d, 24, 9), (0x7d, 40, 9), (0x7e, 56, 9)]);
            parts(&values)
        }
        0x5f => parts(&[(0x8c, 0, 1), (0x8d, 16, 1)]),
        0x60 => parts(&[(0x7f, 0, 4), (0x7f, 16, 4), (0x7f, 32, 4), (0x7f, 48, 4)]),
        0x61 => render_left_chain(false),
        0x62 => render_left_chain(true),
        0x63 => render_left_chain(mode.placement_first & 1 == 0),
        0x69 => parts(&[(0x27, 3, 0), (0x9c, 7, 4)]),
        0x6a => parts(&[(0xb7, 0, 1), (0xb7, 16, 1), (0xb7, 24, 1)]),
        0x6b => parts(&[(0xb7, -16, 1), (0xb7, -32, 1), (0xb7, -40, 1)]),
        0x6d => parts(&[(0xd5, -8, 1), (0xd6, 8, 1), (0xc5, -8, -15), (0xc6, 8, -15)]),
        0x6e => parts(&[(0xb9, -2, 1)]),
        0x6f => parts(&[
            (0xc7, 1, 1),
            (0xd7, 0, 17),
            (0xc7, 1, 33),
            (0xd7, 0, 49),
            (0xc7, 1, 65),
        ]),
        0x70 => parts(&[(0xe5, -3, 8), (0xe6, 13, 8), (0xe7, 5, 11)]),
        0x71 => parts(&[(0xf5, -3, 8), (0xf6, 13, 8), (0xf7, 5, 11)]),
        0x72 => parts(&[
            (0x42, 0, 1),
            (0xe8 + u16::from(mode.placement_first & 1) * 0x10, 8, 1),
        ]),
        0x73 => parts(&[(0x101, 0, 0)]),
        0x74 => parts(&[(0x104, 0, 0)]),
        0x75 => parts(&[(0x105, 0, 0)]),
        0x76 => parts(&[(0x106, 0, 0)]),
        0x77 => parts(&[(0x100, 0, 0)]),
        0x78 => parts(&[(0xc8, 0, 1)]),
        0x79 => parts(&[(0xd8, 4, 4)]),
        0x7a => parts(&[(0xca, 0, 0), (0xc9, -16, 0)]),
        0x7b => parts(&[(0xcc, 0, 1), (0xcd, 16, 1), (0xce, 0, 17), (0xcf, 16, 17)]),
        0x7c => parts(&[(0xba, 4, -1)]),
        0x7d => parts(&[(0x06, -8, -9), (0x07, 16, -9), (0xcb, -5, -1)]),
        0x7e => parts(&[(0x06, -8, -9), (0x07, 16, -9), (0x103, -5, -1)]),
        0x7f => parts(&[(0x0b, 0, 1)]),
        0x81 => render_flagged_variant_handler(mode.placement_first, false),
        0x82 => render_flagged_variant_handler(mode.placement_first, true),
        _ => None,
    }
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

#[allow(clippy::too_many_lines)] // Sparse authenticated indices are clearer as one lookup table.
fn preview_definition(index: u16) -> Option<[u16; 4]> {
    Some(match index & 0x7fff {
        0x001 => [0x0400, 0x0410, 0x0401, 0x0411],
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
        0x020 => [0x14a2, 0x14b2, 0x14a3, 0x14b3],
        0x021 => [0x10a2, 0x10b2, 0x10a3, 0x10b3],
        0x022 => [0x0ca2, 0x0cb2, 0x0ca3, 0x0cb3],
        0x023 => [0x08a2, 0x08b2, 0x08a3, 0x08b3],
        0x040 => [0x14ca, 0x14da, 0x14cb, 0x14db],
        0x041 => [0x10ca, 0x10da, 0x10cb, 0x10db],
        0x042 => [0x0ce2, 0x0cf2, 0x0ce3, 0x0cf3],
        0x043 => [0x08ca, 0x08da, 0x08cb, 0x08db],
        0x009 | 0x04f => [0x0dcc, 0x0ddc, 0x0dcd, 0x0ddd],
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
        0x01f | 0x06e => [0x0588, 0x0598, 0x0589, 0x0599],
        0x024 => [0x018a, 0x019a, 0x018b, 0x019b],
        0x025 => [0x04a6, 0x04b6, 0x04a7, 0x04b7],
        0x026 => [0x50cf, 0x50df, 0x50ce, 0x50de],
        0x027 => [0x0060, 0x0070, 0x0061, 0x0071],
        0x028 | 0x039 => [0x0dc4, 0x0dd4, 0x0dc5, 0x0dd5],
        0x02a => [0x05a2, 0x05b2, 0x45a2, 0x45b2],
        0x02b => [0xd0bf, 0xd0af, 0xd0be, 0xd0ae],
        0x02d => [0x11ec, 0x11fc, 0x11ed, 0x11fd],
        0x02e => [0xc95d, 0xc94d, 0xc95c, 0xc94c],
        0x02f => [0xc95b, 0xc94b, 0xc95a, 0xc94a],
        0x034 => [0x058e, 0x059e, 0x058f, 0x059f],
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
        0x044 => [0x05ae, 0x05be, 0x05af, 0x05bf],
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
        0x05a => [0x05ec, 0x05fc, 0x05ed, 0x05fd],
        0x05b => [0x05eb, 0x05fb, 0x05ec, 0x05fc],
        0x05c => [0x0040, 0x0050, 0x0041, 0x0051],
        0x05d => [0x0585, 0x0595, 0x0586, 0x0596],
        0x05e => [0x4587, 0x4597, 0x4586, 0x4596],
        0x05f => [0x4586, 0x4596, 0x4585, 0x4595],
        0x063 => [0x512a, 0x513a, 0x5129, 0x5139],
        0x064 => [0x4d8d, 0x4d9d, 0x4d8c, 0x4d9c],
        0x065 => [0x0442, 0x0452, 0x0443, 0x0453],
        0x066 => [0x09a3, 0x09b3, 0x49a3, 0x49b3],
        0x067 => [0x09a2, 0x09b2, 0x49a2, 0x49b2],
        0x068 => [0x15ee, 0x15fe, 0x15ef, 0x15ff],
        0x069 => [0x21ea, 0x21fa, 0x21eb, 0x21fb],
        0x06a => [0x05ea, 0x05fa, 0x05eb, 0x05fb],
        0x06b => [0x01eb, 0x01fb, 0x01ec, 0x01fc],
        0x06c => [0x4041, 0x4051, 0x4040, 0x4050],
        0x06d => [0x01a2, 0x01b2, 0x01a3, 0x01b3],
        0x06f => [0x4589, 0x4599, 0x4588, 0x4598],
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
        0x087 => [0x4def, 0x4dff, 0x4dee, 0x4dfe],
        0x088 => [0x150a, 0x151a, 0x150b, 0x151b],
        0x089 => [0x59c5, 0x59d5, 0x59c4, 0x59d4],
        0x08a => [0x15cb, 0x15db, 0x15cc, 0x15dc],
        0x08b => [0x55cd, 0x55dd, 0x55cc, 0x55dc],
        0x08c => [0x1500, 0x0110, 0x1501, 0x0111],
        0x08d => [0x5501, 0x4111, 0x5500, 0x4110],
        0x090 => [0x1640, 0x1650, 0x1641, 0x1651],
        0x094 => [0x4583, 0x4593, 0x4582, 0x4592],
        0x095 => [0x4581, 0x4591, 0x4580, 0x4590],
        0x096 => [0x00ea, 0x80ea, 0x0019, 0x0019],
        0x098 => [0x1528, 0x1538, 0x1529, 0x1539],
        0x099 => [0x152a, 0x153a, 0x1419, 0x1419],
        0x09a => [0x15e4, 0x15f4, 0x15e5, 0x15f5],
        0x09b => [0x55e6, 0x55f6, 0x55e5, 0x55f5],
        0x09c => [0x114d, 0x1019, 0x1019, 0x1019],
        0x0a0 => [0x1642, 0x1652, 0x1643, 0x1653],
        0x0a4 => [0x15a4, 0x15b4, 0x15a5, 0x15b5],
        0x0a5 => [0x15a6, 0x15b6, 0x15a7, 0x15b7],
        0x0a6 => [0x558d, 0x559d, 0x558c, 0x559c],
        0x0a7 => [0x0186, 0x0196, 0x0187, 0x0197],
        0x0a8 => [0x01ce, 0x0188, 0x01ce, 0x0189],
        0x0b0 => [0x003d, 0x1019, 0x1019, 0x1019],
        0x0b1 => [0xc03d, 0x1019, 0x1019, 0x1019],
        0x0b2 => [0x003c, 0x1019, 0x1019, 0x1019],
        0x0b3 => [0xc03c, 0x1019, 0x1019, 0x1019],
        0x0b4 => [0x14c4, 0x1419, 0x54c4, 0x1419],
        0x0b5 => [0x082c, 0x0819, 0x0819, 0x0819],
        0x0b6 => [0x482c, 0x0819, 0x0819, 0x0819],
        0x0b7 => [0x143d, 0x0019, 0x143d, 0x0019],
        0x0b9 => [0x1daa, 0x1dba, 0x1dab, 0x1dbb],
        0x0ba => [0x01e4, 0x01f4, 0x01e5, 0x01f5],
        0x0c0 => [0x1980, 0x1990, 0x1981, 0x1991],
        0x0c2 => [0x1984, 0x1994, 0x1985, 0x1995],
        0x0c3 => [0x1986, 0x1996, 0x1987, 0x1997],
        0x0c4 => [0x19c0, 0x19d0, 0x19c1, 0x19d1],
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
        0x0d3 => [0x19a6, 0x19b6, 0x19a7, 0x19b7],
        0x0d4 => [0x19e0, 0x19f0, 0x19e1, 0x19f1],
        0x0d5 => [0x1de4, 0x1df4, 0x1de5, 0x1df5],
        0x0d6 => [0x1de6, 0x1df6, 0x1de7, 0x1df7],
        0x0d7 => [0x09e8, 0x09f8, 0x09e9, 0x09f9],
        0x0d8 => [0x31ec, 0x0019, 0x0019, 0x0019],
        0x0e0 => [0x99b0, 0x99a0, 0x99b1, 0x99b1],
        0x0e1 => [0x99b2, 0x99a2, 0x99b3, 0x99a3],
        0x0e2 => [0x19c4, 0x19d4, 0x19c5, 0x19d5],
        0x0e3 => [0x19c6, 0x19d6, 0x19c7, 0x19d7],
        0x0e4 => [0x19e8, 0x19f8, 0x19e9, 0x19f9],
        0x0e5 => [0x15e0, 0x15f0, 0x15e1, 0x15f1],
        0x0e6 => [0x1419, 0x15f2, 0x1419, 0x1419],
        0x0e7 => [0x11f4, 0x1019, 0x11f5, 0x1019],
        0x0e8 => [0x11c8, 0x11d8, 0x1019, 0x11d0],
        0x0f0 => [0x9990, 0x9980, 0x9991, 0x9981],
        0x0f1 => [0x9992, 0x9982, 0x9993, 0x9983],
        0x0f2 => [0x19e4, 0x19f4, 0x19e5, 0x19f5],
        0x0f3 => [0x19e6, 0x19f6, 0x19e7, 0x19f7],
        0x0f4 => [0x59e9, 0x59f9, 0x59e8, 0x59f8],
        0x0f5 => [0x11e0, 0x11f0, 0x11e1, 0x11f1],
        0x0f6 => [0x1019, 0x11f2, 0x1019, 0x1019],
        0x0f7 => [0x09f4, 0x0819, 0x09f5, 0x0819],
        0x0f8 => [0x09c8, 0x09d8, 0x0819, 0x09d0],
        0x0ff => [0x55cc, 0x55dc, 0x55cb, 0x55db],
        0x100 => [0x5425, 0x5435, 0x5424, 0x5434],
        0x101 => [0x5025, 0x5035, 0x5024, 0x5034],
        0x103 => [0x4025, 0x4035, 0x4024, 0x4034],
        0x104 => [0x5427, 0x5437, 0x5426, 0x5436],
        0x105 => [0x4849, 0x4859, 0x4848, 0x4858],
        0x106 => [0x480f, 0x481f, 0x480e, 0x481e],
        0x108 => [0x002a, 0x003a, 0x002b, 0x003b],
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
        0x13b => [0x55e5, 0x55f5, 0x55e4, 0x55f4],
        0x115 => [0x04e8, 0x04f8, 0x04e9, 0x04f9],
        0x119 => [0x0019, 0x0019, 0x09d6, 0x0019],
        0x13c => [0x0a48, 0x0a58, 0x4a48, 0x4a58],
        0x140 => [0x14a0, 0x14b0, 0x14a1, 0x14b1],
        0x145 => [0x0019, 0x09c7, 0x0019, 0x0019],
        0x148 => [0x9110, 0x9100, 0x9111, 0x9101],
        0x149 => [0x9112, 0x9102, 0x9112, 0x9102],
        0x14a => [0xd111, 0xd101, 0xd110, 0xd100],
        0x154 => [0x0d88, 0x0d98, 0x0d89, 0x0d99],
        0x155 => [0x0c19, 0x0c19, 0x0da8, 0x0db8],
        0x156 => [0x0da9, 0x0db9, 0x0daa, 0x0dba],
        0x1ce => [0x09f3, 0xc9f3, 0x09ce, 0x09ce],
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
        assert_eq!(geometry(0x19, false), None, "handler 25 draws text");
        assert_eq!(geometry(0x1e, false), None, "handler 30 is input-dependent");
        for sprite in [0x15, 0x16, 0x17, 0x18, 0x1a, 0x1b, 0x1c, 0x1d, 0x1f] {
            assert_eq!(geometry(sprite, true).unwrap(), [(0x115, 0, 1)]);
        }
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
        assert_eq!(geometry(0x37, 0), [(0x38, 0, 1)]);
        assert_eq!(geometry(0x38, 0), [(0x48, 0, 1)]);
        assert_eq!(
            geometry(0x39, 0),
            [
                (0x39, 0, 0),
                (0x3a, 16, 0),
                (0x49, 0, 16),
                (0x4a, 16, 16),
                (0x3f, 8, 9)
            ]
        );
        assert_eq!(geometry(0x39, 1)[4], (0x4f, 8, 9));
        assert_eq!(geometry(0x3a, 0)[0], (0x3b, 0, 0));
        assert_eq!(geometry(0x3b, 0)[0], (0x3d, 0, 0));
        assert_eq!(geometry(0x3c, 0), [(0x54, 8, -15), (0x64, 0, 1)]);
        assert_eq!(geometry(0x3d, 0), [(0x55, 0, 1)]);
        assert_eq!(geometry(0x3d, 1), [(0x65, 0, 1)]);
        assert_eq!(geometry(0x3e, 0), [(0x56, -6, -14), (0x66, -6, 2)]);
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
        assert_eq!(geometry(0x42), [(0x77, 0, 1), (0x87, 0, 17)]);
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
        assert_eq!(geometry(0x48), [(0xa4, 0, 0), (0xa5, 16, 0)]);
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
        assert_eq!(geometry(0x4d, 0).len(), 5);
        assert_eq!(geometry(0x4e, 0), [(0x16, 8, -8), (0xb4, 8, 8)]);
        assert_eq!(
            geometry(0x4f, 0),
            [(0x16, 8, -8), (0xb4, 8, 8), (0xb5, 6, -16), (0xb6, 18, -16)]
        );
        assert_eq!(geometry(0x50, 0), [(0x78, 0, 1)]);
        assert_eq!(
            geometry(0x51, 0),
            [(0x59, -4, 0), (0x69, 4, 0), (0x69, 20, 0), (0x79, 28, 0)]
        );
        assert_eq!(geometry(0x52, 0), [(0x21b, 0, 1)]);
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
                (0x6a, 0, 1),
                (0x5b, 16, 1),
                (0x5b, 32, 1),
                (0x5b, 48, 1),
                (0x5a, 64, 1)
            ]
        );
        assert_eq!(geometry(0x54), geometry(0x56));
        assert_eq!(
            geometry(0x55),
            [
                (0x5d, 0, 1),
                (0x5e, 16, 1),
                (0x5f, 32, 1),
                (0x6e, 8, 17),
                (0x6f, 24, 17)
            ]
        );
        assert_eq!(geometry(0x55), geometry(0x57));
        assert_eq!(
            geometry(0x58),
            [
                (0x5c, 0, 1),
                (0x5c, 0, 17),
                (0x6c, 0, 33),
                (0x5c, 0, -15),
                (0x5c, 0, -31)
            ]
        );
        assert_eq!(
            geometry(0x59),
            [
                (0x5c, 0, 1),
                (0x5c, 16, 1),
                (0x6c, 32, 1),
                (0x5c, -16, 1),
                (0x5c, -32, 1)
            ]
        );
        assert_eq!(
            geometry(0x5a),
            [(0x142, 0, 0), (0x143, 16, 0), (0x144, 32, 0)]
        );
        assert_eq!(
            geometry(0x5b),
            [
                (0x7b, 0, 1),
                (0x6b, 16, 1),
                (0x6b, 32, 1),
                (0x6b, 48, 1),
                (0x7a, 64, 1)
            ]
        );
        assert_eq!(
            geometry(0x5c),
            [
                (0x8a, 0, 0),
                (0x8b, 16, 0),
                (0xff, 32, 0),
                (0x9a, 8, 16),
                (0x13b, 24, 16)
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
            geometry(0x5e),
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
        assert_eq!(geometry(0x5f), [(0x8c, 0, 1), (0x8d, 16, 1)]);
        assert_eq!(
            geometry(0x60),
            [(0x7f, 0, 4), (0x7f, 16, 4), (0x7f, 32, 4), (0x7f, 48, 4)]
        );
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
        assert_eq!(geometry(0x61, 0), short);
        assert_eq!(geometry(0x62, 0), long);
        assert_eq!(geometry(0x63, 0), long);
        assert_eq!(geometry(0x63, 1), short);
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
            [(0xb7, -16, 1), (0xb7, -32, 1), (0xb7, -40, 1)]
        );
        assert_eq!(
            geometry(0x6d, 0),
            [(0xd5, -8, 1), (0xd6, 8, 1), (0xc5, -8, -15), (0xc6, 8, -15)]
        );
        assert_eq!(geometry(0x6e, 0), [(0xb9, -2, 1)]);
        assert_eq!(
            geometry(0x6f, 0),
            [
                (0xc7, 1, 1),
                (0xd7, 0, 17),
                (0xc7, 1, 33),
                (0xd7, 0, 49),
                (0xc7, 1, 65)
            ]
        );
        assert_eq!(
            geometry(0x70, 0),
            [(0xe5, -3, 8), (0xe6, 13, 8), (0xe7, 5, 11)]
        );
        assert_eq!(
            geometry(0x71, 0),
            [(0xf5, -3, 8), (0xf6, 13, 8), (0xf7, 5, 11)]
        );
        assert_eq!(geometry(0x72, 0), [(0x42, 0, 1), (0xe8, 8, 1)]);
        assert_eq!(geometry(0x72, 1), [(0x42, 0, 1), (0xf8, 8, 1)]);
        assert_eq!(geometry(0x73, 0), [(0x101, 0, 0)]);
        assert_eq!(geometry(0x74, 0), [(0x104, 0, 0)]);
        assert_eq!(geometry(0x75, 0), [(0x105, 0, 0)]);
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
        assert_eq!(geometry(0x76), [(0x106, 0, 0)]);
        assert_eq!(geometry(0x77), [(0x100, 0, 0)]);
        assert_eq!(geometry(0x78), [(0xc8, 0, 1)]);
        assert_eq!(geometry(0x79), [(0xd8, 4, 4)]);
        assert_eq!(geometry(0x7a), [(0xca, 0, 0), (0xc9, -16, 0)]);
        assert_eq!(
            geometry(0x7b),
            [(0xcc, 0, 1), (0xcd, 16, 1), (0xce, 0, 17), (0xcf, 16, 17)]
        );
        assert_eq!(geometry(0x7c), [(0xba, 4, -1)]);
        assert_eq!(
            geometry(0x7d),
            [(0x06, -8, -9), (0x07, 16, -9), (0xcb, -5, -1)]
        );
        assert_eq!(
            geometry(0x7e),
            [(0x06, -8, -9), (0x07, 16, -9), (0x103, -5, -1)]
        );
        assert_eq!(geometry(0x7f), [(0x0b, 0, 1)]);
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
        assert_eq!(
            geometry(0x81, 0),
            [
                (0x06, -12, -9),
                (0x07, 8, -9),
                (0x801a, -1, -9),
                (0x108, -3, -1)
            ]
        );
        assert_eq!(geometry(0x81, 1)[2].0, 0x8104);
        assert_eq!(geometry(0x81, 2)[2].0, 0x8106);
        assert_eq!(geometry(0x81, 3)[2].0, 0x8100);
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
            0x81,
            StandardSpritePreviewMode::default(),
        )
        .unwrap();
        assert_eq!(flagged[2].subtiles, preview_definition(0x1a).unwrap());
    }
}
