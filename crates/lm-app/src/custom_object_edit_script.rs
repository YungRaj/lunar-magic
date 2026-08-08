//! Bounded edit scripts for paired native custom-object sidecars.

use lm_app::CustomObjectLibraryEdit;
use lm_level::{CustomObjectEntry, DescriptionFormat, LineEnding, ObjectRecord};
use std::fmt;

const MAGIC: &str = "LMCUSED1";
pub const MAX_SCRIPT_LEN: usize = 128 * 1024;
const MAX_LINE_LEN: usize = 8192;
const MAX_LINES: usize = 8192;
const MAX_COMMANDS: usize = 2048;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomObjectEditScriptError {
    TooLarge {
        actual: usize,
        maximum: usize,
    },
    TooManyLines {
        maximum: usize,
    },
    LineTooLong {
        line: usize,
        actual: usize,
        maximum: usize,
    },
    MissingMagic,
    UnsupportedVersion(String),
    TooManyCommands {
        maximum: usize,
    },
    UnknownCommand {
        line: usize,
        command: String,
    },
    WrongArity {
        line: usize,
    },
    InvalidNumber {
        line: usize,
        value: String,
    },
    InvalidHex {
        line: usize,
        value: String,
    },
    InvalidUtf8 {
        line: usize,
    },
    InvalidObject {
        line: usize,
        message: String,
    },
    InvalidEntry {
        line: usize,
        message: String,
    },
    InvalidFormat {
        line: usize,
    },
}

impl fmt::Display for CustomObjectEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid custom-object edit script: {self:?}")
    }
}

impl std::error::Error for CustomObjectEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<CustomObjectLibraryEdit>, CustomObjectEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(CustomObjectEditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(CustomObjectEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(CustomObjectEditScriptError::UnsupportedVersion(
            magic.into(),
        ));
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if line > MAX_LINES {
            return Err(CustomObjectEditScriptError::TooManyLines { maximum: MAX_LINES });
        }
        if raw.len() > MAX_LINE_LEN {
            return Err(CustomObjectEditScriptError::LineTooLong {
                line,
                actual: raw.len(),
                maximum: MAX_LINE_LEN,
            });
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if !content.is_empty() {
            edits.push(parse_line(line, content)?);
            if edits.len() > MAX_COMMANDS {
                return Err(CustomObjectEditScriptError::TooManyCommands {
                    maximum: MAX_COMMANDS,
                });
            }
        }
    }
    Ok(edits)
}

fn parse_line(
    line: usize,
    content: &str,
) -> Result<CustomObjectLibraryEdit, CustomObjectEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["insert", index, object, description] => Ok(CustomObjectLibraryEdit::Insert {
            index: hex_usize(line, index)?,
            entry: entry(line, object, description)?,
        }),
        ["replace", index, object, description] => Ok(CustomObjectLibraryEdit::Replace {
            index: hex_usize(line, index)?,
            entry: entry(line, object, description)?,
        }),
        ["remove", index] => Ok(CustomObjectLibraryEdit::Remove {
            index: hex_usize(line, index)?,
        }),
        ["move", from, to] => Ok(CustomObjectLibraryEdit::Move {
            from: hex_usize(line, from)?,
            to: hex_usize(line, to)?,
        }),
        ["format", bom, ending, trailing] => Ok(CustomObjectLibraryEdit::SetDescriptionFormat(
            DescriptionFormat {
                utf8_bom: parse_toggle(line, bom, "bom", "no-bom")?,
                line_ending: match *ending {
                    "lf" => LineEnding::Lf,
                    "crlf" => LineEnding::CrLf,
                    _ => return Err(CustomObjectEditScriptError::InvalidFormat { line }),
                },
                trailing_line_ending: parse_toggle(line, trailing, "trailing", "no-trailing")?,
            },
        )),
        [command, ..]
            if !matches!(
                *command,
                "insert" | "replace" | "remove" | "move" | "format"
            ) =>
        {
            Err(CustomObjectEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(CustomObjectEditScriptError::WrongArity { line }),
    }
}

fn entry(
    line: usize,
    object: &str,
    description: &str,
) -> Result<CustomObjectEntry, CustomObjectEditScriptError> {
    let objects = object
        .split(',')
        .map(|object| {
            ObjectRecord::new(hex_bytes(line, object)?).map_err(|error| {
                CustomObjectEditScriptError::InvalidObject {
                    line,
                    message: error.to_string(),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let description = String::from_utf8(hex_bytes(line, description)?)
        .map_err(|_| CustomObjectEditScriptError::InvalidUtf8 { line })?;
    CustomObjectEntry::new_group(objects, description).map_err(|error| {
        CustomObjectEditScriptError::InvalidEntry {
            line,
            message: error.to_string(),
        }
    })
}

fn parse_toggle(
    line: usize,
    value: &str,
    enabled: &str,
    disabled: &str,
) -> Result<bool, CustomObjectEditScriptError> {
    if value == enabled {
        Ok(true)
    } else if value == disabled {
        Ok(false)
    } else {
        Err(CustomObjectEditScriptError::InvalidFormat { line })
    }
}

fn hex_bytes(line: usize, value: &str) -> Result<Vec<u8>, CustomObjectEditScriptError> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CustomObjectEditScriptError::InvalidHex {
            line,
            value: value.into(),
        });
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| {
                CustomObjectEditScriptError::InvalidHex {
                    line,
                    value: value.into(),
                }
            })
        })
        .collect()
}

fn hex_usize(line: usize, value: &str) -> Result<usize, CustomObjectEditScriptError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    usize::from_str_radix(value, 16).map_err(|_| CustomObjectEditScriptError::InvalidNumber {
        line,
        value: value.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries_ordering_and_text_framing() {
        let edits = parse("LMCUSED1\ninsert 0 010003,020804 5365636f6e6420e29883\nreplace 0 020004 4368616e676564\nmove 0 0\nremove 0\nformat no-bom lf no-trailing\n").unwrap();
        assert_eq!(edits.len(), 5);
        let CustomObjectLibraryEdit::Insert { entry, .. } = &edits[0] else {
            panic!()
        };
        assert_eq!(entry.description, "Second ☃");
        assert_eq!(entry.objects().count(), 2);
    }

    #[test]
    fn rejects_bad_versions_objects_utf8_and_format() {
        assert!(parse("OLD\n").is_err());
        assert!(parse("LMCUSED1\ninsert 0 01 41\n").is_err());
        assert!(parse("LMCUSED1\ninsert 0 010003 ff\n").is_err());
        assert!(parse("LMCUSED1\nformat bom native trailing\n").is_err());
    }
}
