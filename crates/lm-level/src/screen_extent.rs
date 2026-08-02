use crate::{NativeSpriteStream, ObjectStream};

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
    let mut stored_highest = 0_u16;
    let mut visible_highest = 0_u16;
    for record in &objects.records {
        if let Some(jump) = record.screen_jump() {
            screen = jump.packed_target;
            stored_highest = stored_highest.max(screen);
            continue;
        }
        if record.advances_screen() {
            screen = screen.saturating_add(1).min(31);
        }
        stored_highest = stored_highest.max(screen);
        if record.command_id() != 0x28 {
            visible_highest = visible_highest.max(screen);
        }
    }
    let highest = match mode {
        LevelScreenExtentMode::Stored => stored_highest,
        LevelScreenExtentMode::Auto => sprites
            .native_placements()
            .iter()
            .fold(visible_highest, |highest, placement| {
                highest.max(placement.screen)
            }),
    };
    u8::try_from(highest.min(31) + 1).unwrap_or(32)
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
}
