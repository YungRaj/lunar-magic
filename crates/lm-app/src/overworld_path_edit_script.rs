//! Bounded stable-key scripts for portable overworld path graphs.

use lm_overworld::{PathDirection, PathEdge, PathGraphEdit, PathNode, Submap};
use std::fmt;

const MAGIC: &str = "LMOPEDT1";
pub const MAX_SCRIPT_LEN: usize = 256 * 1024;
const MAX_LINE_LEN: usize = 4096;
const MAX_LINES: usize = 16_384;
const MAX_COMMANDS: usize = 8192;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldPathEditScriptError {
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
    InvalidSubmap {
        line: usize,
        value: String,
    },
    InvalidDirection {
        line: usize,
        value: String,
    },
}

impl fmt::Display for OverworldPathEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid overworld path edit script: {self:?}")
    }
}

impl std::error::Error for OverworldPathEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<PathGraphEdit>, OverworldPathEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(OverworldPathEditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(OverworldPathEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(OverworldPathEditScriptError::UnsupportedVersion(
            magic.into(),
        ));
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if line > MAX_LINES {
            return Err(OverworldPathEditScriptError::TooManyLines { maximum: MAX_LINES });
        }
        if raw.len() > MAX_LINE_LEN {
            return Err(OverworldPathEditScriptError::LineTooLong {
                line,
                actual: raw.len(),
                maximum: MAX_LINE_LEN,
            });
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if !content.is_empty() {
            edits.push(parse_line(line, content)?);
            if edits.len() > MAX_COMMANDS {
                return Err(OverworldPathEditScriptError::TooManyCommands {
                    maximum: MAX_COMMANDS,
                });
            }
        }
    }
    Ok(edits)
}

fn parse_line(line: usize, content: &str) -> Result<PathGraphEdit, OverworldPathEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["node", "upsert", id, x, y, submap, level, flags] => {
            Ok(PathGraphEdit::UpsertNode(PathNode {
                id: hex(line, id)?,
                x: hex(line, x)?,
                y: hex(line, y)?,
                submap: parse_submap(line, submap)?,
                level: optional_hex(line, level)?,
                raw_flags: hex(line, flags)?,
            }))
        }
        ["node", "remove", id] => Ok(PathGraphEdit::RemoveNode(hex(line, id)?)),
        ["edge", "upsert", from, to, direction, exit, flags] => {
            Ok(PathGraphEdit::UpsertEdge(PathEdge {
                from: hex(line, from)?,
                to: hex(line, to)?,
                direction: parse_direction(line, direction)?,
                exit_index: optional_hex(line, exit)?,
                raw_flags: hex(line, flags)?,
            }))
        }
        ["edge", "remove", from, direction] => Ok(PathGraphEdit::RemoveEdge {
            from: hex(line, from)?,
            direction: parse_direction(line, direction)?,
        }),
        [command, ..] if !matches!(*command, "node" | "edge") => {
            Err(OverworldPathEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(OverworldPathEditScriptError::WrongArity { line }),
    }
}

fn parse_submap(line: usize, value: &str) -> Result<Submap, OverworldPathEditScriptError> {
    let encoded = hex::<u8>(line, value)?;
    Submap::decode(encoded).ok_or_else(|| OverworldPathEditScriptError::InvalidSubmap {
        line,
        value: value.into(),
    })
}

fn parse_direction(
    line: usize,
    value: &str,
) -> Result<PathDirection, OverworldPathEditScriptError> {
    match value {
        "up" => Ok(PathDirection::Up),
        "right" => Ok(PathDirection::Right),
        "down" => Ok(PathDirection::Down),
        "left" => Ok(PathDirection::Left),
        _ => Err(OverworldPathEditScriptError::InvalidDirection {
            line,
            value: value.into(),
        }),
    }
}

fn optional_hex<T>(line: usize, value: &str) -> Result<Option<T>, OverworldPathEditScriptError>
where
    T: TryFrom<u64>,
{
    if value == "none" {
        Ok(None)
    } else {
        hex(line, value).map(Some)
    }
}

fn hex<T>(line: usize, value: &str) -> Result<T, OverworldPathEditScriptError>
where
    T: TryFrom<u64>,
{
    let normalized = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(normalized, 16)
        .ok()
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| OverworldPathEditScriptError::InvalidNumber {
            line,
            value: value.into(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_complete_node_and_edge_surface() {
        let edits = parse("LMOPEDT1\nnode upsert 1 123 456 6 105 a0\nnode upsert 2 7 8 0 none 40\nedge upsert 1 2 right fe 81\nedge remove 1 right\nnode remove 2\n").unwrap();
        assert_eq!(edits.len(), 5);
        let PathGraphEdit::UpsertNode(node) = edits[0] else {
            panic!()
        };
        assert_eq!(node.level, Some(0x105));
    }

    #[test]
    fn rejects_bad_versions_submaps_directions_and_numbers() {
        assert!(parse("OLD\n").is_err());
        assert!(parse("LMOPEDT1\nnode upsert 1 0 0 7 none 0\n").is_err());
        assert!(parse("LMOPEDT1\nedge remove 1 diagonal\n").is_err());
        assert!(parse("LMOPEDT1\nnode remove nope\n").is_err());
    }
}
