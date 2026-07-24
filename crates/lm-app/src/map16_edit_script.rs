//! Strict bounded scripts for native Map16 controller edits.

use lm_app::{Map16ControllerEdit, Map16DocumentEdit};
use lm_level::{Map16Address, Map16Page, Map16Quadrant, Map16Tile, Subtile};
use std::fmt;

const MAGIC: &str = "LMM16ED1";
pub const MAX_SCRIPT_LEN: usize = 64 * 1024;
const MAX_LINE_LEN: usize = 4096;
const MAX_LINES: usize = 8192;
const MAX_COMMANDS: usize = 2048;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16EditScriptError {
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
    PortableOnlyCommand {
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
    InvalidQuadrant {
        line: usize,
        value: String,
    },
}

impl fmt::Display for Map16EditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native Map16 edit script: {self:?}")
    }
}

impl std::error::Error for Map16EditScriptError {}

pub fn parse(input: &str) -> Result<Vec<Map16ControllerEdit>, Map16EditScriptError> {
    parse_commands(input)?
        .into_iter()
        .map(|(line, edit)| match edit {
            ParsedEdit::Native(edit) => Ok(edit),
            ParsedEdit::AppendBlankPage { .. } => Err(Map16EditScriptError::PortableOnlyCommand {
                line,
                command: "append-blank-page".into(),
            }),
            ParsedEdit::RemoveLastPage { .. } => Err(Map16EditScriptError::PortableOnlyCommand {
                line,
                command: "remove-last-page".into(),
            }),
        })
        .collect()
}

pub fn parse_document(input: &str) -> Result<Vec<Map16DocumentEdit>, Map16EditScriptError> {
    parse_commands(input)?
        .into_iter()
        .map(|(_, edit)| match edit {
            ParsedEdit::Native(edit) => document_edit(edit),
            ParsedEdit::AppendBlankPage { resolution_limit } => Map16DocumentEdit::AppendPage {
                page: Map16Page {
                    tiles: vec![Map16Tile::default(); Map16Page::TILE_COUNT],
                },
                resolution_limit,
            },
            ParsedEdit::RemoveLastPage { resolution_limit } => {
                Map16DocumentEdit::RemoveLastPage { resolution_limit }
            }
        })
        .map(Ok)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParsedEdit {
    Native(Map16ControllerEdit),
    AppendBlankPage { resolution_limit: usize },
    RemoveLastPage { resolution_limit: usize },
}

fn parse_commands(input: &str) -> Result<Vec<(usize, ParsedEdit)>, Map16EditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(Map16EditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines.next().ok_or(Map16EditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(Map16EditScriptError::UnsupportedVersion(magic.into()));
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if line > MAX_LINES {
            return Err(Map16EditScriptError::TooManyLines { maximum: MAX_LINES });
        }
        if raw.len() > MAX_LINE_LEN {
            return Err(Map16EditScriptError::LineTooLong {
                line,
                actual: raw.len(),
                maximum: MAX_LINE_LEN,
            });
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        if edits.len() == MAX_COMMANDS {
            return Err(Map16EditScriptError::TooManyCommands {
                maximum: MAX_COMMANDS,
            });
        }
        edits.push((line, parse_command(line, content)?));
    }
    Ok(edits)
}

fn parse_command(line: usize, content: &str) -> Result<ParsedEdit, Map16EditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["subtile", page, tile, quadrant, subtile, limit] => {
            Ok(ParsedEdit::Native(Map16ControllerEdit::SetSubtile {
                address: address(line, page, tile)?,
                quadrant: quadrant_value(line, quadrant)?,
                subtile: Subtile(hex_u16(line, subtile)?),
                resolution_limit: hex_usize(line, limit)?,
            }))
        }
        ["acts-like", page, tile, acts_like, limit] => {
            Ok(ParsedEdit::Native(Map16ControllerEdit::SetActsLike {
                address: address(line, page, tile)?,
                acts_like: hex_u16(line, acts_like)?,
                resolution_limit: hex_usize(line, limit)?,
            }))
        }
        [
            "tile",
            page,
            tile,
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            acts_like,
            limit,
        ] => Ok(ParsedEdit::Native(Map16ControllerEdit::ReplaceTiles {
            replacements: vec![(
                address(line, page, tile)?,
                Map16Tile {
                    top_left: Subtile(hex_u16(line, top_left)?),
                    top_right: Subtile(hex_u16(line, top_right)?),
                    bottom_left: Subtile(hex_u16(line, bottom_left)?),
                    bottom_right: Subtile(hex_u16(line, bottom_right)?),
                    acts_like: hex_u16(line, acts_like)?,
                },
            )],
            resolution_limit: hex_usize(line, limit)?,
        })),
        ["append-blank-page", limit] => Ok(ParsedEdit::AppendBlankPage {
            resolution_limit: hex_usize(line, limit)?,
        }),
        ["remove-last-page", limit] => Ok(ParsedEdit::RemoveLastPage {
            resolution_limit: hex_usize(line, limit)?,
        }),
        [command, ..]
            if !matches!(
                *command,
                "subtile" | "acts-like" | "tile" | "append-blank-page" | "remove-last-page"
            ) =>
        {
            Err(Map16EditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(Map16EditScriptError::WrongArity { line }),
    }
}

fn document_edit(edit: Map16ControllerEdit) -> Map16DocumentEdit {
    match edit {
        Map16ControllerEdit::ReplaceTiles {
            replacements,
            resolution_limit,
        } => Map16DocumentEdit::ReplaceTiles {
            replacements,
            resolution_limit,
        },
        Map16ControllerEdit::SetSubtile {
            address,
            quadrant,
            subtile,
            resolution_limit,
        } => Map16DocumentEdit::SetSubtile {
            address,
            quadrant,
            subtile,
            resolution_limit,
        },
        Map16ControllerEdit::SetActsLike {
            address,
            acts_like,
            resolution_limit,
        } => Map16DocumentEdit::SetActsLike {
            address,
            acts_like,
            resolution_limit,
        },
    }
}

fn address(line: usize, page: &str, tile: &str) -> Result<Map16Address, Map16EditScriptError> {
    Ok(Map16Address {
        page: hex_usize(line, page)?,
        tile: hex_usize(line, tile)?,
    })
}

fn quadrant_value(line: usize, value: &str) -> Result<Map16Quadrant, Map16EditScriptError> {
    match value {
        "tl" => Ok(Map16Quadrant::TopLeft),
        "tr" => Ok(Map16Quadrant::TopRight),
        "bl" => Ok(Map16Quadrant::BottomLeft),
        "br" => Ok(Map16Quadrant::BottomRight),
        _ => Err(Map16EditScriptError::InvalidQuadrant {
            line,
            value: value.into(),
        }),
    }
}

fn hex_u16(line: usize, value: &str) -> Result<u16, Map16EditScriptError> {
    u16::from_str_radix(strip_prefix(value), 16).map_err(|_| invalid_number(line, value))
}

fn hex_usize(line: usize, value: &str) -> Result<usize, Map16EditScriptError> {
    usize::from_str_radix(strip_prefix(value), 16).map_err(|_| invalid_number(line, value))
}

fn invalid_number(line: usize, value: &str) -> Map16EditScriptError {
    Map16EditScriptError::InvalidNumber {
        line,
        value: value.into(),
    }
}

fn strip_prefix(value: &str) -> &str {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_map16_controller_edit_shape() {
        let edits = parse(
            "LMM16ED1\n\
             subtile 01 02 br 4321 200\n\
             acts-like 01 02 0003 200\n\
             tile 02 03 0001 0002 0003 0004 0005 400\n",
        )
        .unwrap();
        assert_eq!(edits.len(), 3);
        assert!(matches!(
            edits[0],
            Map16ControllerEdit::SetSubtile {
                quadrant: Map16Quadrant::BottomRight,
                subtile: Subtile(0x4321),
                ..
            }
        ));
        assert!(matches!(edits[1], Map16ControllerEdit::SetActsLike { .. }));
        assert!(matches!(edits[2], Map16ControllerEdit::ReplaceTiles { .. }));
    }

    #[test]
    fn rejects_bad_framing_arity_values_and_limits() {
        for script in [
            "wrong\n",
            "LMM16ED1\nsubtile 0 0 xx 1 10\n",
            "LMM16ED1\nacts-like 0 0 nope 10\n",
            "LMM16ED1\ntile 0 0 1 2\n",
            "LMM16ED1\nunknown 0\n",
        ] {
            assert!(parse(script).is_err(), "accepted {script:?}");
        }
        assert!(matches!(
            parse(&"x".repeat(MAX_SCRIPT_LEN + 1)),
            Err(Map16EditScriptError::TooLarge { .. })
        ));
        let commands = format!(
            "LMM16ED1\n{}",
            "acts-like 0 0 0 10\n".repeat(MAX_COMMANDS + 1)
        );
        assert!(matches!(
            parse(&commands),
            Err(Map16EditScriptError::TooManyCommands { .. })
        ));
    }

    #[test]
    fn portable_page_commands_are_document_only() {
        let script = "LMM16ED1\nappend-blank-page 200\nremove-last-page 100\n";
        let edits = parse_document(script).unwrap();
        assert!(matches!(edits[0], Map16DocumentEdit::AppendPage { .. }));
        assert!(matches!(
            edits[1],
            Map16DocumentEdit::RemoveLastPage {
                resolution_limit: 0x100
            }
        ));
        assert!(matches!(
            parse(script),
            Err(Map16EditScriptError::PortableOnlyCommand { line: 2, .. })
        ));
    }
}
