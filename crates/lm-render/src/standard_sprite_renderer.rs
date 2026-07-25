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
}

/// Renders the first authenticated family of Lunar Magic standard-sprite previews.
///
/// Sprite IDs `$00`–`$03` share one recovered handler shape and select source tiles `$40`–`$43`.
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
        },
    )
}

/// Renders authenticated sprite handlers with both recovered global display switches.
#[must_use]
pub fn render_lunar_magic_standard_sprite_with_mode(
    sprite_number: u8,
    mode: StandardSpritePreviewMode,
) -> Option<Vec<StandardSpritePreviewTile>> {
    if mode.alternate_display && sprite_number <= 7 {
        return parts(&[(0x115, 0, 0)]);
    }
    match sprite_number {
        0x00..=0x03 => parts(&[(0x40 + u16::from(sprite_number), 0, 0)]),
        0x04 if mode.alternate_graphics => parts(&[(0x13, 0, -16), (0x23, 0, 0)]),
        0x04 => parts(&[(0x10, 0, -16), (0x20, 0, 0)]),
        0x05 if mode.alternate_graphics => parts(&[(0x12, 0, -16), (0x22, 0, 0)]),
        0x05 => parts(&[(0x11, 0, -16), (0x21, 0, 0)]),
        0x06 => parts(&[(0x12, 0, -16), (0x22, 0, 0)]),
        0x07 => parts(&[(0x13, 0, -16), (0x23, 0, 0)]),
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
        0x115 => [0x04e8, 0x04f8, 0x04e9, 0x04f9],
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
                    y: 0,
                }])
            );
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true),
                Some(vec![StandardSpritePreviewTile {
                    definition_index: 0x115,
                    subtiles: [0x04e8, 0x04f8, 0x04e9, 0x04f9],
                    x: 0,
                    y: 0,
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
                }
            ),
            [0x115]
        );
        assert_eq!(render_lunar_magic_standard_sprite(8, false), None);
    }
}
