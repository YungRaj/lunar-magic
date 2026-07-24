//! Bounded scripts for lossless portable Layer 3 document edits.

use lm_level::Layer3Edit;
use std::fmt;

const MAGIC: &str = "LML3EDT1";
pub const MAX_SCRIPT_LEN: usize = 256 * 1024;
const MAX_LINE_LEN: usize = 0x2_0001;
const MAX_LINES: usize = 4096;
const MAX_COMMANDS: usize = 2048;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer3EditScriptError {
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
}

impl fmt::Display for Layer3EditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Layer 3 edit script: {self:?}")
    }
}

impl std::error::Error for Layer3EditScriptError {}

pub fn parse(input: &str) -> Result<Vec<Layer3Edit>, Layer3EditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(Layer3EditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines.next().ok_or(Layer3EditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(Layer3EditScriptError::UnsupportedVersion(magic.into()));
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if line > MAX_LINES {
            return Err(Layer3EditScriptError::TooManyLines { maximum: MAX_LINES });
        }
        if raw.len() > MAX_LINE_LEN {
            return Err(Layer3EditScriptError::LineTooLong {
                line,
                actual: raw.len(),
                maximum: MAX_LINE_LEN,
            });
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if !content.is_empty() {
            edits.push(parse_line(line, content)?);
            if edits.len() > MAX_COMMANDS {
                return Err(Layer3EditScriptError::TooManyCommands {
                    maximum: MAX_COMMANDS,
                });
            }
        }
    }
    Ok(edits)
}

fn parse_line(line: usize, content: &str) -> Result<Layer3Edit, Layer3EditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["start", value] => Ok(Layer3Edit::SetStartPosition(hex(line, value)?)),
        ["size", value] => Ok(Layer3Edit::SetTilemapSize(hex(line, value)?)),
        ["liquid", value] => Ok(Layer3Edit::SetLiquidType(hex(line, value)?)),
        ["flags", value] => Ok(Layer3Edit::SetFlags(hex(line, value)?)),
        ["graphics", slot, file] => Ok(Layer3Edit::SetGraphicsFile {
            slot: hex(line, slot)?,
            file: hex(line, file)?,
        }),
        ["reserved", bytes] => Ok(Layer3Edit::SetReserved(fixed_bytes(line, bytes)?)),
        ["tilemap", bytes] => Ok(Layer3Edit::ReplaceTilemap(variable_bytes(line, bytes)?)),
        ["tilemap-range", offset, bytes] => Ok(Layer3Edit::ReplaceTilemapRange {
            offset: hex(line, offset)?,
            bytes: variable_bytes(line, bytes)?,
        }),
        ["remap", bytes] => Ok(Layer3Edit::ReplaceRemapCommands(variable_bytes(
            line, bytes,
        )?)),
        [command, ..]
            if !matches!(
                *command,
                "start"
                    | "size"
                    | "liquid"
                    | "flags"
                    | "graphics"
                    | "reserved"
                    | "tilemap"
                    | "tilemap-range"
                    | "remap"
            ) =>
        {
            Err(Layer3EditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(Layer3EditScriptError::WrongArity { line }),
    }
}

fn fixed_bytes<const N: usize>(line: usize, value: &str) -> Result<[u8; N], Layer3EditScriptError> {
    variable_bytes(line, value)?
        .try_into()
        .map_err(|bytes: Vec<u8>| Layer3EditScriptError::InvalidHex {
            line,
            value: format!("{} bytes", bytes.len()),
        })
}

fn variable_bytes(line: usize, value: &str) -> Result<Vec<u8>, Layer3EditScriptError> {
    if value == "-" {
        return Ok(Vec::new());
    }
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Layer3EditScriptError::InvalidHex {
            line,
            value: value.into(),
        });
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| {
                Layer3EditScriptError::InvalidHex {
                    line,
                    value: value.into(),
                }
            })
        })
        .collect()
}

fn hex<T>(line: usize, value: &str) -> Result<T, Layer3EditScriptError>
where
    T: TryFrom<u64>,
{
    let normalized = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(normalized, 16)
        .ok()
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| Layer3EditScriptError::InvalidNumber {
            line,
            value: value.into(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_standalone_edit_surface() {
        let edits = parse("LML3EDT1\nstart fe\nsize 3\nliquid 81\nflags a5\ngraphics 2 abc\nreserved 5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a\ntilemap 00010203\ntilemap-range 1 aabb\nremap fe07\n").unwrap();
        assert_eq!(edits.len(), 9);
    }

    #[test]
    fn parses_empty_buffers_and_rejects_bad_widths_versions_and_numbers() {
        assert_eq!(parse("LML3EDT1\ntilemap -\nremap -\n").unwrap().len(), 2);
        assert!(parse("OLD\n").is_err());
        assert!(parse("LML3EDT1\nreserved 00\n").is_err());
        assert!(parse("LML3EDT1\ngraphics nope 1\n").is_err());
    }
}
