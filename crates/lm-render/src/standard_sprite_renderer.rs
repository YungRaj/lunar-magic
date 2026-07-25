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
        0x0b | 0x0c | 0x0d | 0x0f | 0x11 | 0x12 | 0x13..=0x18 | 0x1a | 0x1b..=0x1d | 0x1f
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
        0x20..=0x27 if mode.alternate_display => parts(&[(0x115, 0, 1)]),
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
        _ => None,
    }
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
        0x009 => [0x0dcc, 0x0ddc, 0x0dcd, 0x0ddd],
        0x00a => [0x0019, 0x0019, 0x00eb, 0x00fb],
        0x00c => [0x08a8, 0x08b8, 0x08a9, 0x08b9],
        0x00d => [0x1982, 0x1992, 0x1983, 0x1993],
        0x00e => [0x1182, 0x1192, 0x1183, 0x1193],
        0x00f => [0x1184, 0x9184, 0x5184, 0xd184],
        0x014 => [0x0967, 0x0977, 0x0968, 0x0978],
        0x015 => [0x0969, 0x0979, 0x096a, 0x097a],
        0x016 => [0x50af, 0x50bf, 0x50ae, 0x50be],
        0x017 => [0x098e, 0x099e, 0x098f, 0x099f],
        0x018 => [0x0da0, 0x0db4, 0x0da5, 0x0db5],
        0x01a => [0x08e8, 0x08f8, 0x08e9, 0x08f9],
        0x024 => [0x018a, 0x019a, 0x018b, 0x019b],
        0x025 => [0x04a6, 0x04b6, 0x04a7, 0x04b7],
        0x026 => [0x50cf, 0x50df, 0x50ce, 0x50de],
        0x028 => [0x0dc4, 0x0dd4, 0x0dc5, 0x0dd5],
        0x02a => [0x05a2, 0x05b2, 0x45a2, 0x45b2],
        0x034 => [0x058e, 0x059e, 0x058f, 0x059f],
        0x035 => [0x458e, 0x459e, 0x0419, 0x0419],
        0x044 => [0x05ae, 0x05be, 0x05af, 0x05bf],
        0x045 => [0x45ae, 0x45be, 0x0419, 0x0419],
        0x050 => [0x154c, 0x155c, 0x154d, 0x155d],
        0x051 => [0x114c, 0x115c, 0x114d, 0x115d],
        0x052 => [0x554d, 0x555d, 0x554c, 0x555c],
        0x060 => [0x1529, 0x1539, 0x152a, 0x153a],
        0x061 => [0x1129, 0x1139, 0x112a, 0x113a],
        0x062 => [0x552a, 0x553a, 0x5529, 0x5539],
        0x053 => [0x514d, 0x515d, 0x514c, 0x515c],
        0x063 => [0x512a, 0x513a, 0x5129, 0x5139],
        0x124 => [0x5508, 0x5518, 0x5507, 0x5517],
        0x125 => [0x5108, 0x5118, 0x5107, 0x5117],
        0x126 => [0x1507, 0x1517, 0x1508, 0x1518],
        0x127 => [0x1107, 0x1117, 0x1108, 0x1118],
        0x134 => [0x5528, 0x5538, 0x5527, 0x5537],
        0x135 => [0x5128, 0x5138, 0x5127, 0x5137],
        0x136 => [0x1527, 0x1537, 0x1528, 0x1538],
        0x137 => [0x1127, 0x1137, 0x1128, 0x1138],
        0x115 => [0x04e8, 0x04f8, 0x04e9, 0x04f9],
        0x140 => [0x14a0, 0x14b0, 0x14a1, 0x14b1],
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
}
