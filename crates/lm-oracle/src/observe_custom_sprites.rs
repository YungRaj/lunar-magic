use crate::Observation;
use lm_level::{CustomSpriteLibrary, LineEnding};
use std::fmt::Write;

/// Produces a canonical placement- and record-addressable `.mw2`/`.mwt` snapshot.
#[must_use]
pub fn observe_custom_sprite_library(library: &CustomSpriteLibrary) -> Observation {
    let mut result = Observation::new();
    let format = library.description_format();
    put(
        &mut result,
        "custom-sprites/header",
        &format!("{:02x}", library.header()),
    );
    put(
        &mut result,
        "custom-sprites/count",
        &library.entries().len(),
    );
    put(
        &mut result,
        "custom-sprites/text/utf8-bom",
        &format.utf8_bom,
    );
    put(
        &mut result,
        "custom-sprites/text/line-ending",
        &match format.line_ending {
            LineEnding::Lf => "lf",
            LineEnding::CrLf => "crlf",
        },
    );
    put(
        &mut result,
        "custom-sprites/text/trailing-line-ending",
        &format.trailing_line_ending,
    );
    for (entry_index, entry) in library.entries().iter().enumerate() {
        let base = format!("custom-sprites/entries/{entry_index:04x}");
        put(
            &mut result,
            &format!("{base}/description"),
            &entry.description,
        );
        put(
            &mut result,
            &format!("{base}/sprite-count"),
            &entry.sprites.len(),
        );
        for (sprite_index, sprite) in entry.sprites.iter().enumerate() {
            put(
                &mut result,
                &format!("{base}/sprites/{sprite_index:04x}"),
                &hex(&sprite.encoded),
            );
        }
    }
    result
}

fn put(result: &mut Observation, path: &str, value: &impl ToString) {
    result
        .insert(path, value.to_string())
        .expect("custom-sprite observation paths are unique");
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("String writes cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::SpriteLengthTable;

    #[test]
    fn grouped_records_and_framing_are_addressable() {
        let library = CustomSpriteLibrary::decode(
            &[0x5a, 1, 2, 3, 0, 4, 5, 5, 6, 7, 0xff],
            b"\xef\xbb\xbfPair\r\nSingle \xe2\x98\x83\r\n",
            &SpriteLengthTable::standard(),
        )
        .unwrap();
        let observed = observe_custom_sprite_library(&library);
        assert_eq!(observed.get("custom-sprites/header"), Some("5a"));
        assert_eq!(observed.get("custom-sprites/count"), Some("2"));
        assert_eq!(
            observed.get("custom-sprites/entries/0000/sprite-count"),
            Some("2")
        );
        assert_eq!(
            observed.get("custom-sprites/entries/0001/description"),
            Some("Single ☃")
        );
        assert_eq!(
            observed.get("custom-sprites/entries/0001/sprites/0000"),
            Some("050607")
        );
    }
}
