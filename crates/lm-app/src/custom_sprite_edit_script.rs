//! Bounded edit scripts for paired native custom-sprite sidecars.

use lm_app::CustomSpriteLibraryEdit;
use lm_level::{CustomSpriteEntry, DescriptionFormat, LineEnding, SpriteRecord};
use std::fmt;

const MAGIC: &str = "LMSPRED1";
pub const MAX_SCRIPT_LEN: usize = 128 * 1024;
const MAX_LINE_LEN: usize = 8192;
const MAX_LINES: usize = 8192;
const MAX_COMMANDS: usize = 2048;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomSpriteEditScriptError {
    TooLarge,
    TooManyLines,
    LineTooLong { line: usize },
    MissingMagic,
    UnsupportedVersion(String),
    TooManyCommands,
    UnknownCommand { line: usize, command: String },
    WrongArity { line: usize },
    InvalidNumber { line: usize, value: String },
    InvalidHex { line: usize, value: String },
    InvalidUtf8 { line: usize },
    InvalidEntry { line: usize, message: String },
    InvalidFormat { line: usize },
}

impl fmt::Display for CustomSpriteEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid custom-sprite edit script: {self:?}")
    }
}

impl std::error::Error for CustomSpriteEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<CustomSpriteLibraryEdit>, CustomSpriteEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(CustomSpriteEditScriptError::TooLarge);
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(CustomSpriteEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(CustomSpriteEditScriptError::UnsupportedVersion(
            magic.into(),
        ));
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if line > MAX_LINES {
            return Err(CustomSpriteEditScriptError::TooManyLines);
        }
        if raw.len() > MAX_LINE_LEN {
            return Err(CustomSpriteEditScriptError::LineTooLong { line });
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if !content.is_empty() {
            edits.push(parse_line(line, content)?);
            if edits.len() > MAX_COMMANDS {
                return Err(CustomSpriteEditScriptError::TooManyCommands);
            }
        }
    }
    Ok(edits)
}

fn parse_line(
    line: usize,
    content: &str,
) -> Result<CustomSpriteLibraryEdit, CustomSpriteEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["insert", index, sprites, description] => Ok(CustomSpriteLibraryEdit::Insert {
            index: hex_usize(line, index)?,
            entry: entry(line, sprites, description)?,
        }),
        ["replace", index, sprites, description] => Ok(CustomSpriteLibraryEdit::Replace {
            index: hex_usize(line, index)?,
            entry: entry(line, sprites, description)?,
        }),
        ["remove", index] => Ok(CustomSpriteLibraryEdit::Remove {
            index: hex_usize(line, index)?,
        }),
        ["move", from, to] => Ok(CustomSpriteLibraryEdit::Move {
            from: hex_usize(line, from)?,
            to: hex_usize(line, to)?,
        }),
        ["header", header] => Ok(CustomSpriteLibraryEdit::SetHeader(hex_u8(line, header)?)),
        ["format", bom, ending, trailing] => Ok(CustomSpriteLibraryEdit::SetDescriptionFormat(
            DescriptionFormat {
                utf8_bom: toggle(line, bom, "bom", "no-bom")?,
                line_ending: match *ending {
                    "lf" => LineEnding::Lf,
                    "crlf" => LineEnding::CrLf,
                    _ => return Err(CustomSpriteEditScriptError::InvalidFormat { line }),
                },
                trailing_line_ending: toggle(line, trailing, "trailing", "no-trailing")?,
            },
        )),
        [command, ..]
            if !matches!(
                *command,
                "insert" | "replace" | "remove" | "move" | "header" | "format"
            ) =>
        {
            Err(CustomSpriteEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(CustomSpriteEditScriptError::WrongArity { line }),
    }
}

fn entry(
    line: usize,
    records: &str,
    description: &str,
) -> Result<CustomSpriteEntry, CustomSpriteEditScriptError> {
    let sprites = records
        .split('+')
        .map(|record| {
            Ok(SpriteRecord {
                encoded: hex_bytes(line, record)?,
            })
        })
        .collect::<Result<_, CustomSpriteEditScriptError>>()?;
    let description = String::from_utf8(hex_bytes(line, description)?)
        .map_err(|_| CustomSpriteEditScriptError::InvalidUtf8 { line })?;
    CustomSpriteEntry::new(sprites, description).map_err(|error| {
        CustomSpriteEditScriptError::InvalidEntry {
            line,
            message: error.to_string(),
        }
    })
}

fn toggle(
    line: usize,
    value: &str,
    enabled: &str,
    disabled: &str,
) -> Result<bool, CustomSpriteEditScriptError> {
    match value {
        value if value == enabled => Ok(true),
        value if value == disabled => Ok(false),
        _ => Err(CustomSpriteEditScriptError::InvalidFormat { line }),
    }
}

fn hex_bytes(line: usize, value: &str) -> Result<Vec<u8>, CustomSpriteEditScriptError> {
    if value.is_empty()
        || value.len() % 2 != 0
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(CustomSpriteEditScriptError::InvalidHex {
            line,
            value: value.into(),
        });
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| {
                CustomSpriteEditScriptError::InvalidHex {
                    line,
                    value: value.into(),
                }
            })
        })
        .collect()
}

fn hex_usize(line: usize, value: &str) -> Result<usize, CustomSpriteEditScriptError> {
    usize::from_str_radix(value.strip_prefix("0x").unwrap_or(value), 16).map_err(|_| {
        CustomSpriteEditScriptError::InvalidNumber {
            line,
            value: value.into(),
        }
    })
}

fn hex_u8(line: usize, value: &str) -> Result<u8, CustomSpriteEditScriptError> {
    u8::try_from(hex_usize(line, value)?).map_err(|_| CustomSpriteEditScriptError::InvalidNumber {
        line,
        value: value.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grouped_records_header_and_format() {
        let edits = parse(
            "LMSPRED1\nreplace 0 010203+000405 50616972\nheader 5a\nformat no-bom lf trailing\n",
        )
        .unwrap();
        let CustomSpriteLibraryEdit::Replace { entry, .. } = &edits[0] else {
            panic!("wrong edit")
        };
        assert_eq!(entry.sprites.len(), 2);
        assert_eq!(entry.description, "Pair");
        assert_eq!(edits[1], CustomSpriteLibraryEdit::SetHeader(0x5a));
    }

    #[test]
    fn malformed_and_oversized_scripts_fail_before_edits_escape() {
        assert!(parse("wrong\n").is_err());
        assert!(parse("LMSPRED1\ninsert 0 01 xyz\n").is_err());
        assert!(parse("LMSPRED1\nunknown x\n").is_err());
        assert!(parse(&"x".repeat(MAX_SCRIPT_LEN + 1)).is_err());
    }
}
