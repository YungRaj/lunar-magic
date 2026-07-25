/// One recovered 8×8 source tile used by Lunar Magic's standard-sprite preview renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StandardSpritePreviewTile {
    /// Index in the four consecutive 128-tile SP graphics slots.
    pub tile_index: u16,
    /// Signed preview displacement in pixels from the sprite placement origin.
    pub x: i16,
    pub y: i16,
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
    let tile_index = match (sprite_number, alternate_display) {
        (0x00..=0x03, true) => 0x115,
        (0x00..=0x03, false) => 0x40 + u16::from(sprite_number),
        _ => return None,
    };
    Some(vec![StandardSpritePreviewTile {
        tile_index,
        x: 0,
        y: 0,
    }])
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
                    tile_index: 0x40 + u16::from(sprite),
                    x: 0,
                    y: 0,
                }])
            );
            assert_eq!(
                render_lunar_magic_standard_sprite(sprite, true),
                Some(vec![StandardSpritePreviewTile {
                    tile_index: 0x115,
                    x: 0,
                    y: 0,
                }])
            );
        }
        assert_eq!(render_lunar_magic_standard_sprite(4, false), None);
    }
}
