//! Bounded ownership-aware scripts for native 4bpp graphics edits.

use lm_app::GraphicsControllerEdit;
use lm_graphics::{GraphicsOwnership, GraphicsTileChange, GraphicsTileOwner, IndexedTile};
use std::collections::BTreeSet;
use std::fmt;

const MAGIC: &str = "LMGFXED1";
pub const MAX_SCRIPT_LEN: usize = 128 * 1024;
const MAX_LINE_LEN: usize = 8192;
const MAX_LINES: usize = 8192;
const MAX_COMMANDS: usize = 1024;
const MAX_TILES: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsEditScript {
    pub ownership: GraphicsOwnership,
    pub edits: Vec<GraphicsControllerEdit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsEditScriptError {
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
    MissingOwnership,
    DuplicateOwnership {
        line: usize,
    },
    OwnershipAfterEdit {
        line: usize,
    },
    TileLimit {
        line: usize,
        actual: usize,
        maximum: usize,
    },
    DuplicateOwnerOverride {
        line: usize,
        index: usize,
    },
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
    InvalidOwner {
        line: usize,
        value: String,
    },
    InvalidTilePixels {
        line: usize,
        actual: usize,
    },
}

impl fmt::Display for GraphicsEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native graphics edit script: {self:?}")
    }
}

impl std::error::Error for GraphicsEditScriptError {}

pub fn parse(input: &str) -> Result<GraphicsEditScript, GraphicsEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(GraphicsEditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines.next().ok_or(GraphicsEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(GraphicsEditScriptError::UnsupportedVersion(magic.into()));
    }
    let mut state = ParseState::default();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        validate_line(line, raw)?;
        let content = raw.split('#').next().unwrap_or_default().trim();
        if !content.is_empty() {
            parse_line(&mut state, line, content)?;
        }
    }
    Ok(GraphicsEditScript {
        ownership: state
            .ownership
            .ok_or(GraphicsEditScriptError::MissingOwnership)?,
        edits: state.edits,
    })
}

#[derive(Default)]
struct ParseState {
    ownership: Option<GraphicsOwnership>,
    owner_overrides: BTreeSet<usize>,
    edits: Vec<GraphicsControllerEdit>,
}

fn validate_line(line: usize, raw: &str) -> Result<(), GraphicsEditScriptError> {
    if line > MAX_LINES {
        return Err(GraphicsEditScriptError::TooManyLines { maximum: MAX_LINES });
    }
    if raw.len() > MAX_LINE_LEN {
        return Err(GraphicsEditScriptError::LineTooLong {
            line,
            actual: raw.len(),
            maximum: MAX_LINE_LEN,
        });
    }
    Ok(())
}

fn parse_line(
    state: &mut ParseState,
    line: usize,
    content: &str,
) -> Result<(), GraphicsEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["owners", count, owner] => parse_owners(state, line, count, owner),
        ["owner", index, owner @ ("editable" | "fixed")] => {
            parse_owner_override(state, line, index, owner, None)
        }
        ["owner", index, "exanimation", record] => {
            parse_owner_override(state, line, index, "exanimation", Some(record))
        }
        ["set", index, tile] => push_edit(
            state,
            GraphicsControllerEdit::ApplyChanges(vec![GraphicsTileChange {
                index: hex_usize(line, index)?,
                tile: parse_tile(line, tile)?,
            }]),
        ),
        ["changes", pairs @ ..] if !pairs.is_empty() && pairs.len() % 2 == 0 => {
            let changes = pairs
                .chunks_exact(2)
                .map(|pair| {
                    Ok(GraphicsTileChange {
                        index: hex_usize(line, pair[0])?,
                        tile: parse_tile(line, pair[1])?,
                    })
                })
                .collect::<Result<_, GraphicsEditScriptError>>()?;
            push_edit(state, GraphicsControllerEdit::ApplyChanges(changes))
        }
        ["range", start, tiles @ ..] if !tiles.is_empty() => push_edit(
            state,
            GraphicsControllerEdit::ReplaceRange {
                start: hex_usize(line, start)?,
                tiles: tiles
                    .iter()
                    .map(|tile| parse_tile(line, tile))
                    .collect::<Result<_, _>>()?,
            },
        ),
        [command, ..] if !matches!(*command, "owners" | "owner" | "set" | "changes" | "range") => {
            Err(GraphicsEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(GraphicsEditScriptError::WrongArity { line }),
    }
}

fn parse_owners(
    state: &mut ParseState,
    line: usize,
    count: &str,
    owner: &str,
) -> Result<(), GraphicsEditScriptError> {
    if state.ownership.is_some() {
        return Err(GraphicsEditScriptError::DuplicateOwnership { line });
    }
    if !state.edits.is_empty() {
        return Err(GraphicsEditScriptError::OwnershipAfterEdit { line });
    }
    let count = hex_usize(line, count)?;
    if count > MAX_TILES {
        return Err(GraphicsEditScriptError::TileLimit {
            line,
            actual: count,
            maximum: MAX_TILES,
        });
    }
    state.ownership = Some(GraphicsOwnership::from_owners(vec![
        simple_owner(
            line, owner
        )?;
        count
    ]));
    Ok(())
}

fn parse_owner_override(
    state: &mut ParseState,
    line: usize,
    index: &str,
    owner: &str,
    record: Option<&str>,
) -> Result<(), GraphicsEditScriptError> {
    if !state.edits.is_empty() {
        return Err(GraphicsEditScriptError::OwnershipAfterEdit { line });
    }
    let index = hex_usize(line, index)?;
    if !state.owner_overrides.insert(index) {
        return Err(GraphicsEditScriptError::DuplicateOwnerOverride { line, index });
    }
    let owner = match (owner, record) {
        ("exanimation", Some(record)) => GraphicsTileOwner::ExAnimation {
            record: hex_u16(line, record)?,
        },
        (_, None) => simple_owner(line, owner)?,
        _ => {
            return Err(GraphicsEditScriptError::InvalidOwner {
                line,
                value: owner.into(),
            });
        }
    };
    state
        .ownership
        .as_mut()
        .ok_or(GraphicsEditScriptError::MissingOwnership)?
        .set_owner(index, owner)
        .map_err(|_| GraphicsEditScriptError::InvalidNumber {
            line,
            value: index.to_string(),
        })
}

fn simple_owner(line: usize, value: &str) -> Result<GraphicsTileOwner, GraphicsEditScriptError> {
    match value {
        "editable" => Ok(GraphicsTileOwner::Editable),
        "fixed" => Ok(GraphicsTileOwner::Fixed),
        _ => Err(GraphicsEditScriptError::InvalidOwner {
            line,
            value: value.into(),
        }),
    }
}

fn parse_tile(line: usize, value: &str) -> Result<IndexedTile, GraphicsEditScriptError> {
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    if value.len() != IndexedTile::PIXEL_COUNT {
        return Err(GraphicsEditScriptError::InvalidTilePixels {
            line,
            actual: value.len(),
        });
    }
    let mut pixels = [0; IndexedTile::PIXEL_COUNT];
    for (index, character) in value.bytes().enumerate() {
        pixels[index] = match character {
            b'0'..=b'9' => character - b'0',
            b'a'..=b'f' => character - b'a' + 10,
            b'A'..=b'F' => character - b'A' + 10,
            _ => {
                return Err(GraphicsEditScriptError::InvalidTilePixels {
                    line,
                    actual: value.len(),
                });
            }
        };
    }
    Ok(IndexedTile::new(pixels))
}

fn push_edit(
    state: &mut ParseState,
    edit: GraphicsControllerEdit,
) -> Result<(), GraphicsEditScriptError> {
    if state.ownership.is_none() {
        return Err(GraphicsEditScriptError::MissingOwnership);
    }
    if state.edits.len() == MAX_COMMANDS {
        return Err(GraphicsEditScriptError::TooManyCommands {
            maximum: MAX_COMMANDS,
        });
    }
    state.edits.push(edit);
    Ok(())
}

fn hex_u16(line: usize, value: &str) -> Result<u16, GraphicsEditScriptError> {
    u16::from_str_radix(strip_prefix(value), 16).map_err(|_| invalid_number(line, value))
}

fn hex_usize(line: usize, value: &str) -> Result<usize, GraphicsEditScriptError> {
    usize::from_str_radix(strip_prefix(value), 16).map_err(|_| invalid_number(line, value))
}

fn invalid_number(line: usize, value: &str) -> GraphicsEditScriptError {
    GraphicsEditScriptError::InvalidNumber {
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

    fn tile(value: char) -> String {
        std::iter::repeat_n(value, 64).collect()
    }

    #[test]
    fn parses_ownership_and_both_graphics_edit_shapes() {
        let input = format!(
            "LMGFXED1\nowners 3 editable\nowner 0 fixed\nowner 2 exanimation 9\nchanges 1 {}\nrange 1 {}\n",
            tile('a'),
            tile('f')
        );
        let script = parse(&input).unwrap();
        assert_eq!(script.ownership.len(), 3);
        assert_eq!(script.ownership.owner(0), Some(GraphicsTileOwner::Fixed));
        assert_eq!(
            script.ownership.owner(2),
            Some(GraphicsTileOwner::ExAnimation { record: 9 })
        );
        assert_eq!(script.edits.len(), 2);
    }

    #[test]
    fn rejects_bad_ownership_pixels_framing_and_limits() {
        for script in [
            "wrong\n".into(),
            "LMGFXED1\nset 0 0000\n".into(),
            "LMGFXED1\nowners 1 editable\nset 0 0000\n".into(),
            format!("LMGFXED1\nowners 1 editable\nset 0 {}\n", tile('g')),
            "LMGFXED1\nowners 1 editable\nowner 2 fixed\n".into(),
            "LMGFXED1\nowners 1 unknown\n".into(),
            "LMGFXED1\nowners 1 editable\nowners 1 fixed\n".into(),
            format!(
                "LMGFXED1\nowners 1 editable\nset 0 {}\nowner 0 fixed\n",
                tile('0')
            ),
        ] {
            assert!(parse(&script).is_err(), "accepted {script:?}");
        }
        assert!(matches!(
            parse(&"x".repeat(MAX_SCRIPT_LEN + 1)),
            Err(GraphicsEditScriptError::TooLarge { .. })
        ));
        let commands = format!(
            "LMGFXED1\nowners 1 editable\n{}",
            format!("set 0 {}\n", tile('0')).repeat(MAX_COMMANDS + 1)
        );
        assert!(matches!(
            parse(&commands),
            Err(GraphicsEditScriptError::TooManyCommands { .. })
        ));
        let long_line = format!("LMGFXED1\n{}", "x".repeat(MAX_LINE_LEN + 1));
        assert!(matches!(
            parse(&long_line),
            Err(GraphicsEditScriptError::LineTooLong { .. })
        ));
    }
}
