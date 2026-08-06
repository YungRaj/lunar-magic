use crate::{LegacyLevelHeader, NativeSpriteStream, ObjectStream};

/// Selects the native level-screen extent used by Lunar Magic's image exporters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelScreenExtentMode {
    /// Honor the highest serialized Layer 1 screen transition, including an otherwise empty tail.
    Stored,
    /// Recompute the tail from the last visible Layer 1 object or sprite.
    Auto,
}

/// Returns the number of major-axis screens to include in a native level image.
///
/// The result is always within SMW's one-to-32-screen namespace. `Stored` retains explicit Layer 1
/// screen jumps, while `Auto` ignores control-only tails and includes sprites exactly as Lunar
/// Magic's documented auto-set behavior does.
#[must_use]
pub fn native_level_screen_count(
    objects: &ObjectStream,
    sprites: &NativeSpriteStream,
    mode: LevelScreenExtentMode,
) -> u8 {
    let mut screen = 0_u16;
    let mut jump = None;
    let mut jump_advances = 0_u16;
    let mut stored_highest = 0_u16;
    for record in &objects.records {
        if let Some(next_jump) = record.screen_jump() {
            screen = next_jump.resolved_screen();
            jump = Some(next_jump);
            jump_advances = 0;
            stored_highest = stored_highest.max(screen);
            continue;
        }
        if record.advances_screen() {
            if let Some(jump) = jump {
                jump_advances = jump_advances.saturating_add(1);
                screen = jump.resolved_screen_after_advances(jump_advances);
            } else {
                screen = screen.saturating_add(1) & 0x1f;
            }
        }
        stored_highest = stored_highest.max(screen);
    }
    let highest = match mode {
        LevelScreenExtentMode::Stored => stored_highest,
        LevelScreenExtentMode::Auto => objects
            .native_placements()
            .into_iter()
            .map(|placement| placement.screen)
            .chain(
                sprites
                    .native_placements()
                    .into_iter()
                    .map(|placement| placement.screen),
            )
            .filter(|screen| *screen <= 31)
            .max()
            .unwrap_or(0),
    };
    u8::try_from(highest.min(31) + 1).unwrap_or(32)
}

/// Returns the native image extent while honoring the serialized five-bit header field.
///
/// Lunar Magic's stored-size exporter reads `LegacyLevelHeader::last_screen`; that value is
/// independent of object transition records and can deliberately retain an otherwise empty tail.
/// Auto mode continues to recompute the visible extent from objects and sprites.
#[must_use]
pub fn native_level_screen_count_with_header(
    header: LegacyLevelHeader,
    objects: &ObjectStream,
    sprites: &NativeSpriteStream,
    mode: LevelScreenExtentMode,
) -> u8 {
    match mode {
        LevelScreenExtentMode::Stored => header.last_screen().saturating_add(1),
        LevelScreenExtentMode::Auto => {
            native_level_screen_count(objects, sprites, LevelScreenExtentMode::Auto)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NativeSpriteStream, ObjectRecord, SpriteLengthTable};

    #[test]
    fn stored_extent_retains_control_tail_while_auto_uses_objects_and_sprites() {
        let objects = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0x01, 0x12, 0x10]).unwrap(),
                ObjectRecord::new(vec![0x08, 0x00, 0x01]).unwrap(),
            ],
        };
        let sprites = NativeSpriteStream::parse(
            &[0, 0, 0x05, 1, 0xff],
            false,
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        assert_eq!(
            native_level_screen_count(&objects, &sprites, LevelScreenExtentMode::Stored),
            9
        );
        assert_eq!(
            native_level_screen_count(&objects, &sprites, LevelScreenExtentMode::Auto),
            6
        );
    }

    #[test]
    fn stored_image_extent_uses_header_instead_of_inferred_transitions() {
        let header = LegacyLevelHeader::decode(&[0x13, 0, 0, 0, 0]).unwrap();
        let objects = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x01, 0x12, 0x10]).unwrap()],
        };
        let sprites =
            NativeSpriteStream::parse(&[0, 0xff], false, &SpriteLengthTable::standard()).unwrap();
        assert_eq!(
            native_level_screen_count_with_header(
                header,
                &objects,
                &sprites,
                LevelScreenExtentMode::Stored,
            ),
            20
        );
        assert_eq!(
            native_level_screen_count_with_header(
                header,
                &objects,
                &sprites,
                LevelScreenExtentMode::Auto,
            ),
            1
        );
    }

    #[test]
    fn auto_extent_ignores_trailing_custom_time_and_is_never_empty() {
        let objects = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x40, 0x80, 1]).unwrap()],
        };
        let sprites =
            NativeSpriteStream::parse(&[0, 0xff], false, &SpriteLengthTable::standard()).unwrap();
        assert_eq!(
            native_level_screen_count(&objects, &sprites, LevelScreenExtentMode::Auto),
            1
        );
    }

    #[test]
    fn extent_uses_the_layout_stride_screen_jump_target() {
        let objects = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![5, 3, 1]).unwrap(),
                ObjectRecord::new(vec![1, 0x10, 0]).unwrap(),
            ],
        };
        let sprites =
            NativeSpriteStream::parse(&[0, 0xff], false, &SpriteLengthTable::standard()).unwrap();
        assert_eq!(
            native_level_screen_count(&objects, &sprites, LevelScreenExtentMode::Auto),
            9
        );
    }

    #[test]
    fn automatic_extent_ignores_artwork_after_an_out_of_range_jump() {
        let objects = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0x1f, 0x0f, 1]).unwrap(),
                ObjectRecord::new(vec![1, 0x10, 0]).unwrap(),
            ],
        };
        let sprites =
            NativeSpriteStream::parse(&[0, 0xff], false, &SpriteLengthTable::standard()).unwrap();
        assert_eq!(
            native_level_screen_count(&objects, &sprites, LevelScreenExtentMode::Auto),
            1
        );
        assert_eq!(
            native_level_screen_count(&objects, &sprites, LevelScreenExtentMode::Stored),
            32
        );

        let advancing = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0x1f, 0x0f, 1]).unwrap(),
                ObjectRecord::new(vec![0x81, 0x10, 0]).unwrap(),
            ],
        };
        assert_eq!(
            native_level_screen_count(&advancing, &sprites, LevelScreenExtentMode::Auto),
            0x12
        );
    }

    #[test]
    fn screen_exit_marker_is_invisible_but_its_high_bit_advances_following_artwork() {
        let objects = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x9f, 0, 2, 0, 4]).unwrap()],
        };
        let sprites =
            NativeSpriteStream::parse(&[0, 0xff], false, &SpriteLengthTable::standard()).unwrap();
        assert_eq!(objects.records[0].screen_exit().unwrap().screen, 0x1f);
        assert!(objects.records[0].advances_screen());
        assert_eq!(
            native_level_screen_count(&objects, &sprites, LevelScreenExtentMode::Stored),
            2
        );
        assert_eq!(
            native_level_screen_count(&objects, &sprites, LevelScreenExtentMode::Auto),
            1
        );

        let with_visible_object = ObjectStream {
            records: vec![
                objects.records[0].clone(),
                ObjectRecord::new(vec![1, 0x10, 0]).unwrap(),
            ],
        };
        assert_eq!(
            native_level_screen_count(&with_visible_object, &sprites, LevelScreenExtentMode::Auto),
            2
        );
    }
}
