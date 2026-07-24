//! Strict page-local edit scripts for standalone `LM16PAGE` documents.

use lm_app::Map16PageDocumentEdit;
use lm_level::{Map16Quadrant, Map16Tile, Subtile};
use std::fmt;

const MAGIC: &str = "LMPGEDT1";
pub const MAX_SCRIPT_LEN: usize = 64 * 1024;
const MAX_LINE_LEN: usize = 256;
const MAX_COMMANDS: usize = 2048;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16PageEditScriptError {
    TooLarge(usize),
    MissingMagic,
    LineTooLong { line: usize, bytes: usize },
    TooManyCommands,
    WrongArity { line: usize },
    UnknownCommand { line: usize, command: String },
    InvalidNumber { line: usize, value: String },
    InvalidQuadrant { line: usize, value: String },
}

impl fmt::Display for Map16PageEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Map16 page edit script: {self:?}")
    }
}

impl std::error::Error for Map16PageEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<Map16PageDocumentEdit>, Map16PageEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(Map16PageEditScriptError::TooLarge(input.len()));
    }
    let mut lines = input.lines();
    if lines.next() != Some(MAGIC) {
        return Err(Map16PageEditScriptError::MissingMagic);
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if raw.len() > MAX_LINE_LEN {
            return Err(Map16PageEditScriptError::LineTooLong {
                line,
                bytes: raw.len(),
            });
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        if edits.len() == MAX_COMMANDS {
            return Err(Map16PageEditScriptError::TooManyCommands);
        }
        edits.push(parse_command(line, content)?);
    }
    Ok(edits)
}

fn parse_command(
    line: usize,
    content: &str,
) -> Result<Map16PageDocumentEdit, Map16PageEditScriptError> {
    let fields: Vec<_> = content.split_ascii_whitespace().collect();
    match fields.as_slice() {
        ["subtile", tile, quadrant, value] => Ok(Map16PageDocumentEdit::SetSubtile {
            tile: hex_usize(line, tile)?,
            quadrant: parse_quadrant(line, quadrant)?,
            value: Subtile(hex_u16(line, value)?),
        }),
        ["acts-like", tile, value] => Ok(Map16PageDocumentEdit::SetActsLike {
            tile: hex_usize(line, tile)?,
            value: hex_u16(line, value)?,
        }),
        [
            "tile",
            tile,
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            acts_like,
        ] => Ok(Map16PageDocumentEdit::ReplaceTile {
            tile: hex_usize(line, tile)?,
            value: Map16Tile {
                top_left: Subtile(hex_u16(line, top_left)?),
                top_right: Subtile(hex_u16(line, top_right)?),
                bottom_left: Subtile(hex_u16(line, bottom_left)?),
                bottom_right: Subtile(hex_u16(line, bottom_right)?),
                acts_like: hex_u16(line, acts_like)?,
            },
        }),
        [command, ..] if !matches!(*command, "subtile" | "acts-like" | "tile") => {
            Err(Map16PageEditScriptError::UnknownCommand {
                line,
                command: (*command).to_owned(),
            })
        }
        _ => Err(Map16PageEditScriptError::WrongArity { line }),
    }
}

fn parse_quadrant(line: usize, value: &str) -> Result<Map16Quadrant, Map16PageEditScriptError> {
    match value {
        "tl" => Ok(Map16Quadrant::TopLeft),
        "tr" => Ok(Map16Quadrant::TopRight),
        "bl" => Ok(Map16Quadrant::BottomLeft),
        "br" => Ok(Map16Quadrant::BottomRight),
        _ => Err(Map16PageEditScriptError::InvalidQuadrant {
            line,
            value: value.to_owned(),
        }),
    }
}

fn hex_u16(line: usize, value: &str) -> Result<u16, Map16PageEditScriptError> {
    u16::from_str_radix(strip_prefix(value), 16).map_err(|_| invalid_number(line, value))
}

fn hex_usize(line: usize, value: &str) -> Result<usize, Map16PageEditScriptError> {
    usize::from_str_radix(strip_prefix(value), 16).map_err(|_| invalid_number(line, value))
}

fn strip_prefix(value: &str) -> &str {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value)
}

fn invalid_number(line: usize, value: &str) -> Map16PageEditScriptError {
    Map16PageEditScriptError::InvalidNumber {
        line,
        value: value.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_page_local_edit() {
        let edits =
            parse("LMPGEDT1\nsubtile 02 br 9234\nacts-like 03 ffff\ntile 04 1 2 3 4 5\n").unwrap();
        assert_eq!(edits.len(), 3);
        assert!(matches!(
            edits[0],
            Map16PageDocumentEdit::SetSubtile {
                tile: 2,
                quadrant: Map16Quadrant::BottomRight,
                value: Subtile(0x9234)
            }
        ));
        assert!(matches!(
            edits[1],
            Map16PageDocumentEdit::SetActsLike {
                tile: 3,
                value: 0xffff
            }
        ));
        assert!(matches!(
            edits[2],
            Map16PageDocumentEdit::ReplaceTile { tile: 4, .. }
        ));
    }

    #[test]
    fn malformed_framing_arity_numbers_quadrants_and_bounds_fail() {
        for text in [
            "bad\n",
            "LMPGEDT1\nunknown 1\n",
            "LMPGEDT1\nsubtile 1 xx 2\n",
            "LMPGEDT1\nacts-like nope 1\n",
            "LMPGEDT1\ntile 1 2\n",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
        assert!(matches!(
            parse(&"x".repeat(MAX_SCRIPT_LEN + 1)),
            Err(Map16PageEditScriptError::TooLarge(_))
        ));
    }
}
