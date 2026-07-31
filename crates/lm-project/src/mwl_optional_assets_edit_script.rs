use crate::MwlOptionalAssetsEdit;
use lm_graphics::{
    Bgr555, ExAnimationFeature, ExAnimationFeatureOptions, ExAnimationFrame, ExAnimationRecord,
};
use std::fmt;

pub const MAGIC: &str = "LMMWLOE1";
pub const MAX_SCRIPT_LEN: usize = 1024 * 1024;
const MAX_COMMANDS: usize = 1024;
const MAX_LINE_LEN: usize = ExAnimationRecord::ENCODED_LEN * 2 + 128;

/// Parses one bounded semantic MWL optional-assets edit script.
///
/// # Errors
///
/// Rejects wrong magic, excessive input, malformed commands, invalid numeric/hexadecimal fields,
/// and record payloads that are not exactly one lossless `ExAnimation` workspace record.
pub fn parse(text: &str) -> Result<Vec<MwlOptionalAssetsEdit>, EditScriptError> {
    if text.len() > MAX_SCRIPT_LEN {
        return Err(EditScriptError::TooLarge(text.len()));
    }
    let mut lines = text.lines();
    if lines.next() != Some(MAGIC) {
        return Err(EditScriptError::MissingMagic);
    }
    let mut edits = Vec::new();
    for (offset, raw) in lines.enumerate() {
        let line = offset + 2;
        if raw.len() > MAX_LINE_LEN {
            return Err(EditScriptError::LineTooLong { line });
        }
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        if edits.len() >= MAX_COMMANDS {
            return Err(EditScriptError::TooManyCommands(edits.len() + 1));
        }
        edits.push(parse_line(raw, line)?);
    }
    Ok(edits)
}

fn parse_line(raw: &str, line: usize) -> Result<MwlOptionalAssetsEdit, EditScriptError> {
    let fields = raw.split_whitespace().collect::<Vec<_>>();
    match fields.as_slice() {
        ["palette-metadata", first, second] => Ok(MwlOptionalAssetsEdit::SetPaletteMetadata([
            hex_u32(first, line)?,
            hex_u32(second, line)?,
        ])),
        ["palette-color", index, color] => Ok(MwlOptionalAssetsEdit::SetPaletteColor {
            index: decimal(index, line)?,
            color: Bgr555(hex_u16(color, line)?),
        }),
        ["exanimation-metadata", first, second] => {
            Ok(MwlOptionalAssetsEdit::SetExAnimationMetadata([
                hex_u32(first, line)?,
                hex_u32(second, line)?,
            ]))
        }
        ["exanimation-features", palette, vanilla, global, level] => {
            let mut options = ExAnimationFeatureOptions::decode(0);
            for (feature, value) in [
                (ExAnimationFeature::PaletteAnimation, palette),
                (ExAnimationFeature::VanillaAnimation, vanilla),
                (ExAnimationFeature::GlobalExAnimation, global),
                (ExAnimationFeature::LevelExAnimation, level),
            ] {
                options.set_enabled(feature, switch(value, line)?);
            }
            Ok(MwlOptionalAssetsEdit::SetExAnimationFeatures(options))
        }
        ["exanimation-create"] => Ok(MwlOptionalAssetsEdit::CreateExAnimation),
        ["exanimation-globals", setting, header] => {
            Ok(MwlOptionalAssetsEdit::SetExAnimationGlobals {
                setting: hex_u8(setting, line)?,
                header_value: hex_u32(header, line)?,
            })
        }
        ["trigger", index, "off"] => Ok(MwlOptionalAssetsEdit::SetTrigger {
            index: decimal(index, line)?,
            value: None,
        }),
        ["trigger", index, value] => Ok(MwlOptionalAssetsEdit::SetTrigger {
            index: decimal(index, line)?,
            value: Some(hex_u8(value, line)?),
        }),
        ["record-insert", index, record] => Ok(MwlOptionalAssetsEdit::InsertRecord {
            index: decimal(index, line)?,
            record: decode_record(record, line)?,
        }),
        ["record-replace", index, record] => Ok(MwlOptionalAssetsEdit::ReplaceRecord {
            index: decimal(index, line)?,
            record: decode_record(record, line)?,
        }),
        ["record-remove", index] => Ok(MwlOptionalAssetsEdit::RemoveRecord {
            index: decimal(index, line)?,
        }),
        ["frame-insert", record, index, words @ ..] if (1..=2).contains(&words.len()) => {
            Ok(MwlOptionalAssetsEdit::InsertFrame {
                record: decimal(record, line)?,
                index: decimal(index, line)?,
                frame: frame(words, line)?,
            })
        }
        ["frame-replace", record, index, words @ ..] if (1..=2).contains(&words.len()) => {
            Ok(MwlOptionalAssetsEdit::ReplaceFrame {
                record: decimal(record, line)?,
                index: decimal(index, line)?,
                frame: frame(words, line)?,
            })
        }
        ["frame-remove", record, index] => Ok(MwlOptionalAssetsEdit::RemoveFrame {
            record: decimal(record, line)?,
            index: decimal(index, line)?,
        }),
        ["frame-move", record, from, before] => Ok(MwlOptionalAssetsEdit::MoveFrameBefore {
            record: decimal(record, line)?,
            from: decimal(from, line)?,
            before: decimal(before, line)?,
        }),
        [command, ..] => Err(EditScriptError::UnknownOrMalformed {
            line,
            command: (*command).to_owned(),
        }),
        [] => unreachable!("empty lines are filtered"),
    }
}

fn switch(value: &str, line: usize) -> Result<bool, EditScriptError> {
    match value {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(EditScriptError::UnknownOrMalformed {
            line,
            command: "exanimation-features".into(),
        }),
    }
}

fn frame(words: &[&str], line: usize) -> Result<ExAnimationFrame, EditScriptError> {
    Ok(ExAnimationFrame {
        source_words: words
            .iter()
            .map(|word| hex_u16(word, line))
            .collect::<Result<_, _>>()?,
    })
}

fn decode_record(text: &str, line: usize) -> Result<ExAnimationRecord, EditScriptError> {
    let bytes = decode_hex(text, line)?;
    ExAnimationRecord::decode(&bytes).map_err(|_| EditScriptError::RecordLength {
        line,
        actual: bytes.len(),
    })
}

fn decode_hex(text: &str, line: usize) -> Result<Vec<u8>, EditScriptError> {
    if text.len() % 2 != 0 {
        return Err(EditScriptError::InvalidHex { line });
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|value| u8::from_str_radix(value, 16).ok())
                .ok_or(EditScriptError::InvalidHex { line })
        })
        .collect()
}

fn decimal(text: &str, line: usize) -> Result<usize, EditScriptError> {
    text.parse()
        .map_err(|_| EditScriptError::InvalidNumber { line })
}

fn hex_u8(text: &str, line: usize) -> Result<u8, EditScriptError> {
    u8::from_str_radix(text, 16).map_err(|_| EditScriptError::InvalidNumber { line })
}

fn hex_u16(text: &str, line: usize) -> Result<u16, EditScriptError> {
    u16::from_str_radix(text, 16).map_err(|_| EditScriptError::InvalidNumber { line })
}

fn hex_u32(text: &str, line: usize) -> Result<u32, EditScriptError> {
    u32::from_str_radix(text, 16).map_err(|_| EditScriptError::InvalidNumber { line })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditScriptError {
    TooLarge(usize),
    MissingMagic,
    LineTooLong { line: usize },
    TooManyCommands(usize),
    UnknownOrMalformed { line: usize, command: String },
    InvalidNumber { line: usize },
    InvalidHex { line: usize },
    RecordLength { line: usize, actual: usize },
}

impl fmt::Display for EditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "MWL optional-assets edit script failed: {self:?}"
        )
    }
}

impl std::error::Error for EditScriptError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    fn record_hex() -> String {
        ExAnimationRecord::new(1, 0, 0, 0x100, false, &[0, 6], false)
            .unwrap()
            .encoded()
            .iter()
            .fold(String::new(), |mut text, byte| {
                write!(text, "{byte:02x}").expect("string writes cannot fail");
                text
            })
    }

    #[test]
    fn parses_every_semantic_edit_kind() {
        let text = format!(
            "{MAGIC}\npalette-metadata 1 2\npalette-color 256 1234\nexanimation-metadata 3 4\nexanimation-features on off on off\nexanimation-create\nexanimation-globals 05 00000006\ntrigger 3 07\ntrigger 4 off\nrecord-insert 0 {}\nrecord-replace 0 {}\nrecord-remove 0\nframe-insert 0 1 1111\nframe-replace 0 0 2222 3333\nframe-remove 0 1\nframe-move 0 1 0\n",
            record_hex(),
            record_hex()
        );
        let edits = parse(&text).unwrap();
        assert_eq!(edits.len(), 15);
        assert!(matches!(
            edits[1],
            MwlOptionalAssetsEdit::SetPaletteColor { index: 256, .. }
        ));
        assert!(matches!(
            edits[7],
            MwlOptionalAssetsEdit::SetTrigger { value: None, .. }
        ));
        assert!(matches!(
            &edits[12],
            MwlOptionalAssetsEdit::ReplaceFrame { frame, .. }
                if frame.source_words == [0x2222, 0x3333]
        ));
        assert!(matches!(
            edits[3],
            MwlOptionalAssetsEdit::SetExAnimationFeatures(options)
                if options.enabled(ExAnimationFeature::PaletteAnimation)
                    && !options.enabled(ExAnimationFeature::VanillaAnimation)
                    && options.enabled(ExAnimationFeature::GlobalExAnimation)
                    && !options.enabled(ExAnimationFeature::LevelExAnimation)
        ));
    }

    #[test]
    fn rejects_bad_magic_numbers_records_and_commands() {
        assert_eq!(parse("bad\n"), Err(EditScriptError::MissingMagic));
        assert!(parse(&format!("{MAGIC}\npalette-color no 1\n")).is_err());
        assert!(parse(&format!("{MAGIC}\nrecord-insert 0 00\n")).is_err());
        assert!(parse(&format!("{MAGIC}\nunknown 1\n")).is_err());
        assert!(parse(&format!("{MAGIC}\nexanimation-features yes off on off\n")).is_err());
    }
}
