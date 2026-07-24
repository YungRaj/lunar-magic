use crate::Observation;
use lm_level::{NativeLevelFile, ScreenJumpEncoding, SpriteToken};

/// Produces a native-stream-addressable snapshot of an `LMLVL1` transfer artifact.
#[must_use]
pub fn observe_native_level(file: &NativeLevelFile) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "native-level/source-level", file.source_level);
    put_hex(
        &mut result,
        "native-level/layer1/header",
        &file.layer1.header.encoded(),
    );
    put(
        &mut result,
        "native-level/layer1/object-count",
        file.layer1.objects.records.len(),
    );
    for (index, object) in file.layer1.objects.records.iter().enumerate() {
        let base = format!("native-level/layer1/objects/{index:04x}");
        put_hex(&mut result, &format!("{base}/encoded"), object.encoded());
        put(
            &mut result,
            &format!("{base}/command-id"),
            object.command_id(),
        );
        put(
            &mut result,
            &format!("{base}/parameter"),
            object.parameter(),
        );
        let coordinates = object.coordinate_nibbles();
        put(
            &mut result,
            &format!("{base}/coordinate-first"),
            coordinates.first,
        );
        put(
            &mut result,
            &format!("{base}/coordinate-second"),
            coordinates.second,
        );
        put(
            &mut result,
            &format!("{base}/screen-advance"),
            object.advances_screen(),
        );
        match object.screen_jump() {
            Some(jump) => {
                put(&mut result, &format!("{base}/kind"), "screen-jump");
                put(
                    &mut result,
                    &format!("{base}/screen-jump-encoding"),
                    match jump.encoding {
                        ScreenJumpEncoding::FirstLow => "first-low",
                        ScreenJumpEncoding::FirstHigh => "first-high",
                    },
                );
                put(
                    &mut result,
                    &format!("{base}/screen-jump-target"),
                    jump.packed_target,
                );
            }
            None => put(&mut result, &format!("{base}/kind"), "object"),
        }
    }
    put(
        &mut result,
        "native-level/sprites/header",
        file.sprites.header,
    );
    put(
        &mut result,
        "native-level/sprites/expanded",
        file.sprites.expanded,
    );
    put(
        &mut result,
        "native-level/sprites/token-count",
        file.sprites.tokens.len(),
    );
    for (index, token) in file.sprites.tokens.iter().enumerate() {
        let base = format!("native-level/sprites/tokens/{index:04x}");
        match token {
            SpriteToken::Record(record) => {
                put(&mut result, &format!("{base}/kind"), "record");
                put_hex(&mut result, &format!("{base}/encoded"), &record.encoded);
            }
            SpriteToken::Screen(value) => {
                put(&mut result, &format!("{base}/kind"), "screen");
                put(&mut result, &format!("{base}/value"), value);
            }
            SpriteToken::Control(value) => {
                put(&mut result, &format!("{base}/kind"), "control");
                put(&mut result, &format!("{base}/value"), value);
            }
        }
    }
    result
}

fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_value())
        .expect("observation paths are unique");
}

trait ObservationValue {
    fn into_value(self) -> String;
}
impl<T: ToString> ObservationValue for T {
    fn into_value(self) -> String {
        self.to_string()
    }
}

fn put_hex(result: &mut Observation, path: &str, bytes: &[u8]) {
    use std::fmt::Write;
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(value, "{byte:02x}").expect("String writes cannot fail");
    }
    put(result, path, value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{LevelObjectData, NativeSpriteStream, SpriteLengthTable};

    #[test]
    fn observes_native_objects_and_expanded_sprite_tokens() {
        let file = NativeLevelFile {
            source_level: 0x105,
            layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 0xa3, 0x04, 0, 0xff]).unwrap(),
            sprites: NativeSpriteStream::parse(
                &[0x10, 0xff, 3, 0x20, 1, 2, 0xff, 0xfe],
                true,
                &SpriteLengthTable::standard(),
            )
            .unwrap(),
        };
        let observed = observe_native_level(&file);
        assert_eq!(observed.get("native-level/source-level"), Some("261"));
        assert_eq!(
            observed.get("native-level/layer1/objects/0000/coordinate-first"),
            Some("3")
        );
        assert_eq!(
            observed.get("native-level/layer1/objects/0000/coordinate-second"),
            Some("4")
        );
        assert_eq!(
            observed.get("native-level/layer1/objects/0000/screen-advance"),
            Some("true")
        );
        assert_eq!(
            observed.get("native-level/layer1/objects/0000/kind"),
            Some("object")
        );
        assert_eq!(
            observed.get("native-level/sprites/tokens/0000/kind"),
            Some("screen")
        );
        assert_eq!(
            observed.get("native-level/sprites/tokens/0001/encoded"),
            Some("200102")
        );
    }

    #[test]
    fn observes_screen_jump_control_semantics() {
        let file = NativeLevelFile {
            source_level: 1,
            layer1: LevelObjectData::parse(
                &[0; 5]
                    .into_iter()
                    .chain([0x1a, 0x0b, 1, 0xff])
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
            sprites: NativeSpriteStream::parse(&[0, 0xff], false, &SpriteLengthTable::standard())
                .unwrap(),
        };
        let observed = observe_native_level(&file);
        assert_eq!(
            observed.get("native-level/layer1/objects/0000/kind"),
            Some("screen-jump")
        );
        assert_eq!(
            observed.get("native-level/layer1/objects/0000/screen-jump-encoding"),
            Some("first-low")
        );
        assert_eq!(
            observed.get("native-level/layer1/objects/0000/screen-jump-target"),
            Some("2842")
        );
    }
}
