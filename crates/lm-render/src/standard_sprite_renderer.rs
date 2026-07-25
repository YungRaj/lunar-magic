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
        _ => None,
    }
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
    Some(match index {
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
        0x01f => [0x0588, 0x0598, 0x0589, 0x0599],
        0x024 => [0x018a, 0x019a, 0x018b, 0x019b],
        0x025 => [0x04a6, 0x04b6, 0x04a7, 0x04b7],
        0x026 => [0x50cf, 0x50df, 0x50ce, 0x50de],
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
        0x056 => [0x0de2, 0x0df2, 0x0de3, 0x0df3],
        0x063 => [0x512a, 0x513a, 0x5129, 0x5139],
        0x064 => [0x4d8d, 0x4d9d, 0x4d8c, 0x4d9c],
        0x065 => [0x0442, 0x0452, 0x0443, 0x0453],
        0x066 => [0x09a3, 0x09b3, 0x49a3, 0x49b3],
        0x071 => [0x5101, 0x5111, 0x5100, 0x5110],
        0x072 => [0x4d01, 0x4d11, 0x4d00, 0x4d10],
        0x073 => [0x4901, 0x4911, 0x4900, 0x4910],
        0x080 => [0x5681, 0x5691, 0x5680, 0x5690],
        0x090 => [0x1640, 0x1650, 0x1641, 0x1651],
        0x0a0 => [0x1642, 0x1652, 0x1643, 0x1653],
        0x0c0 => [0x1980, 0x1990, 0x1981, 0x1991],
        0x0c2 => [0x1984, 0x1994, 0x1985, 0x1995],
        0x0c3 => [0x1986, 0x1996, 0x1987, 0x1997],
        0x0c4 => [0x19c0, 0x19d0, 0x19c1, 0x19d1],
        0x0d0 => [0x19a0, 0x19b0, 0x19a1, 0x19b1],
        0x0d1 => [0x19a2, 0x19b2, 0x19a3, 0x19b3],
        0x0d2 => [0x19a4, 0x19b4, 0x19a5, 0x19b5],
        0x0d3 => [0x19a6, 0x19b6, 0x19a7, 0x19b7],
        0x0d4 => [0x19e0, 0x19f0, 0x19e1, 0x19f1],
        0x0e0 => [0x99b0, 0x99a0, 0x99b1, 0x99b1],
        0x0e1 => [0x99b2, 0x99a2, 0x99b3, 0x99a3],
        0x0e2 => [0x19c4, 0x19d4, 0x19c5, 0x19d5],
        0x0e3 => [0x19c6, 0x19d6, 0x19c7, 0x19d7],
        0x0e4 => [0x19e8, 0x19f8, 0x19e9, 0x19f9],
        0x0f0 => [0x9990, 0x9980, 0x9991, 0x9981],
        0x0f1 => [0x9992, 0x9982, 0x9993, 0x9983],
        0x0f2 => [0x19e4, 0x19f4, 0x19e5, 0x19f5],
        0x0f3 => [0x19e6, 0x19f6, 0x19e7, 0x19f7],
        0x0f4 => [0x59e9, 0x59f9, 0x59e8, 0x59f8],
        0x124 => [0x5508, 0x5518, 0x5507, 0x5517],
        0x125 => [0x5108, 0x5118, 0x5107, 0x5117],
        0x126 => [0x1507, 0x1517, 0x1508, 0x1518],
        0x127 => [0x1107, 0x1117, 0x1108, 0x1118],
        0x134 => [0x5528, 0x5538, 0x5527, 0x5537],
        0x135 => [0x5128, 0x5138, 0x5127, 0x5137],
        0x136 => [0x1527, 0x1537, 0x1528, 0x1538],
        0x137 => [0x1127, 0x1137, 0x1128, 0x1138],
        0x115 => [0x04e8, 0x04f8, 0x04e9, 0x04f9],
        0x119 => [0x0019, 0x0019, 0x09d6, 0x0019],
        0x13c => [0x0a48, 0x0a58, 0x4a48, 0x4a58],
        0x140 => [0x14a0, 0x14b0, 0x14a1, 0x14b1],
        0x145 => [0x0019, 0x09c7, 0x0019, 0x0019],
        0x1ce => [0x09f3, 0xc9f3, 0x09ce, 0x09ce],
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
}
