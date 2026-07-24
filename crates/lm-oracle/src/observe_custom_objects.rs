use crate::Observation;
use lm_level::{CustomObjectLibrary, LineEnding};

/// Produces a canonical record-addressable snapshot of synchronized `.mw0`/`.mw0t` state.
#[must_use]
pub fn observe_custom_object_library(library: &CustomObjectLibrary) -> Observation {
    let mut result = Observation::new();
    let format = library.description_format();
    put(
        &mut result,
        "custom-objects/count",
        &library.entries().len(),
    );
    put(
        &mut result,
        "custom-objects/text/utf8-bom",
        &format.utf8_bom,
    );
    put(
        &mut result,
        "custom-objects/text/line-ending",
        &match format.line_ending {
            LineEnding::Lf => "lf",
            LineEnding::CrLf => "crlf",
        },
    );
    put(
        &mut result,
        "custom-objects/text/trailing-line-ending",
        &format.trailing_line_ending,
    );
    for (index, entry) in library.entries().iter().enumerate() {
        let base = format!("custom-objects/entries/{index:04x}");
        put(
            &mut result,
            &format!("{base}/object"),
            &hex(entry.object.encoded()),
        );
        put(
            &mut result,
            &format!("{base}/description"),
            &entry.description,
        );
    }
    result
}

fn put<T: ToString + ?Sized>(result: &mut Observation, path: &str, value: &T) {
    result
        .insert(path, value.to_string())
        .expect("custom-object observation paths are unique");
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("String writes cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_objects_and_unicode_descriptions_are_observed() {
        let library = CustomObjectLibrary::decode(
            &[1, 0, 3, 0xff],
            b"\xef\xbb\xbfObject \xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e\r\n",
        )
        .unwrap();
        let observed = observe_custom_object_library(&library);
        assert_eq!(observed.get("custom-objects/count"), Some("1"));
        assert_eq!(observed.get("custom-objects/text/utf8-bom"), Some("true"));
        assert_eq!(
            observed.get("custom-objects/text/line-ending"),
            Some("crlf")
        );
        assert_eq!(
            observed.get("custom-objects/entries/0000/object"),
            Some("010003")
        );
        assert_eq!(
            observed.get("custom-objects/entries/0000/description"),
            Some("Object 日本語")
        );
    }
}
