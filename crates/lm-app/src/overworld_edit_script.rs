//! Bounded ownership-aware scripts for the complete native overworld aggregate.

use crate::exanimation_edit_script;
use lm_app::OverworldControllerEdit;
use lm_graphics::{Bgr555, PaletteChange, PaletteEntryOwner, PaletteOwnership};
use lm_overworld::{EventReveal, OverworldEndpoint, OverworldSprite};
use std::collections::BTreeSet;
use std::fmt;

mod values;

use values::{hex, parse_bytes, parse_layer, parse_owner, parse_submap};

const MAGIC: &str = "LMOWEDT1";
pub const MAX_SCRIPT_LEN: usize = 256 * 1024;
const MAX_LINE_LEN: usize = 16 * 1024;
const MAX_LINES: usize = 16_384;
const MAX_COMMANDS: usize = 4096;
const MAX_PALETTE_COLORS: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldEditScript {
    pub slot: usize,
    pub palette_ownership: PaletteOwnership,
    pub edits: Vec<OverworldControllerEdit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldEditScriptError {
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
    MissingSlot,
    DuplicateSlot {
        line: usize,
    },
    MissingOwnership,
    DuplicateOwnership {
        line: usize,
    },
    HeaderAfterEdit {
        line: usize,
    },
    DuplicateOwnerOverride {
        line: usize,
        index: usize,
    },
    OwnershipIndex {
        line: usize,
        index: usize,
        len: usize,
    },
    PaletteLimit {
        line: usize,
        actual: usize,
        maximum: usize,
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
    InvalidLayer {
        line: usize,
        value: String,
    },
    InvalidSubmap {
        line: usize,
        value: String,
    },
    InvalidExtra {
        line: usize,
        value: String,
    },
    Animation {
        line: usize,
        message: String,
    },
}

impl fmt::Display for OverworldEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native overworld edit script: {self:?}")
    }
}

impl std::error::Error for OverworldEditScriptError {}

#[derive(Default)]
struct ParseState {
    slot: Option<usize>,
    ownership: Option<Vec<PaletteEntryOwner>>,
    owner_overrides: BTreeSet<usize>,
    edits: Vec<OverworldControllerEdit>,
}

pub fn parse(input: &str) -> Result<OverworldEditScript, OverworldEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(OverworldEditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines.next().ok_or(OverworldEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(OverworldEditScriptError::UnsupportedVersion(magic.into()));
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
    Ok(OverworldEditScript {
        slot: state.slot.ok_or(OverworldEditScriptError::MissingSlot)?,
        palette_ownership: PaletteOwnership::from_owners(
            state
                .ownership
                .ok_or(OverworldEditScriptError::MissingOwnership)?,
        ),
        edits: state.edits,
    })
}

fn validate_line(line: usize, raw: &str) -> Result<(), OverworldEditScriptError> {
    if line > MAX_LINES {
        return Err(OverworldEditScriptError::TooManyLines { maximum: MAX_LINES });
    }
    if raw.len() > MAX_LINE_LEN {
        return Err(OverworldEditScriptError::LineTooLong {
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
) -> Result<(), OverworldEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["slot", value] => parse_slot(state, line, value),
        ["palette-owners", count, owner] => parse_owners(state, line, count, owner),
        ["palette-owner", index, owner] => parse_owner_override(state, line, index, owner),
        ["layer", layer, x, y, tile] => push_edit(
            state,
            OverworldControllerEdit::SetLayerTile {
                layer: parse_layer(line, layer)?,
                x: hex(line, x)?,
                y: hex(line, y)?,
                tile: hex(line, tile)?,
            },
        ),
        ["event", index, source, destination] => push_edit(
            state,
            OverworldControllerEdit::ReplaceEventReveal {
                index: hex(line, index)?,
                reveal: EventReveal {
                    source_tile: hex(line, source)?,
                    destination_tile: hex(line, destination)?,
                },
            },
        ),
        ["endpoint", index, x, y, submap] => push_edit(
            state,
            OverworldControllerEdit::ReplaceEndpoint {
                index: hex(line, index)?,
                endpoint: OverworldEndpoint {
                    x: hex(line, x)?,
                    y: hex(line, y)?,
                    submap: hex(line, submap)?,
                },
            },
        ),
        ["message", message, column, row, tile] => push_edit(
            state,
            OverworldControllerEdit::SetMessageTile {
                message: hex(line, message)?,
                column: hex(line, column)?,
                row: hex(line, row)?,
                tile: hex(line, tile)?,
            },
        ),
        ["sprite", index, id, x, y, submap, extra] => push_edit(
            state,
            OverworldControllerEdit::ReplaceSprite {
                index: hex(line, index)?,
                sprite: OverworldSprite {
                    id: hex(line, id)?,
                    x: hex(line, x)?,
                    y: hex(line, y)?,
                    submap: parse_submap(line, submap)?,
                    extra: parse_bytes(line, extra)?,
                },
            },
        ),
        ["palette", index, color] => push_edit(
            state,
            OverworldControllerEdit::PaletteChanges(vec![PaletteChange {
                index: hex(line, index)?,
                color: Bgr555(hex(line, color)?),
            }]),
        ),
        ["animation", rest @ ..] if !rest.is_empty() => {
            let command = rest.join(" ");
            let edit = exanimation_edit_script::parse_command(line, &command).map_err(|error| {
                OverworldEditScriptError::Animation {
                    line,
                    message: error.to_string(),
                }
            })?;
            push_edit(state, OverworldControllerEdit::Animation(vec![edit]))
        }
        [command, ..]
            if !matches!(
                *command,
                "slot"
                    | "palette-owners"
                    | "palette-owner"
                    | "layer"
                    | "event"
                    | "endpoint"
                    | "message"
                    | "sprite"
                    | "palette"
                    | "animation"
            ) =>
        {
            Err(OverworldEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(OverworldEditScriptError::WrongArity { line }),
    }
}

fn parse_slot(
    state: &mut ParseState,
    line: usize,
    value: &str,
) -> Result<(), OverworldEditScriptError> {
    if !state.edits.is_empty() {
        return Err(OverworldEditScriptError::HeaderAfterEdit { line });
    }
    if state.slot.replace(hex(line, value)?).is_some() {
        return Err(OverworldEditScriptError::DuplicateSlot { line });
    }
    Ok(())
}

fn parse_owners(
    state: &mut ParseState,
    line: usize,
    count: &str,
    owner: &str,
) -> Result<(), OverworldEditScriptError> {
    if !state.edits.is_empty() {
        return Err(OverworldEditScriptError::HeaderAfterEdit { line });
    }
    if state.ownership.is_some() {
        return Err(OverworldEditScriptError::DuplicateOwnership { line });
    }
    let count = hex::<usize>(line, count)?;
    if count > MAX_PALETTE_COLORS {
        return Err(OverworldEditScriptError::PaletteLimit {
            line,
            actual: count,
            maximum: MAX_PALETTE_COLORS,
        });
    }
    state.ownership = Some(vec![parse_owner(line, owner)?; count]);
    Ok(())
}

fn parse_owner_override(
    state: &mut ParseState,
    line: usize,
    index: &str,
    owner: &str,
) -> Result<(), OverworldEditScriptError> {
    if !state.edits.is_empty() {
        return Err(OverworldEditScriptError::HeaderAfterEdit { line });
    }
    let index = hex(line, index)?;
    if !state.owner_overrides.insert(index) {
        return Err(OverworldEditScriptError::DuplicateOwnerOverride { line, index });
    }
    let owners = state
        .ownership
        .as_mut()
        .ok_or(OverworldEditScriptError::MissingOwnership)?;
    let len = owners.len();
    *owners
        .get_mut(index)
        .ok_or(OverworldEditScriptError::OwnershipIndex { line, index, len })? =
        parse_owner(line, owner)?;
    Ok(())
}

fn push_edit(
    state: &mut ParseState,
    edit: OverworldControllerEdit,
) -> Result<(), OverworldEditScriptError> {
    state.edits.push(edit);
    if state.edits.len() > MAX_COMMANDS {
        return Err(OverworldEditScriptError::TooManyCommands {
            maximum: MAX_COMMANDS,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_complete_overworld_domain() {
        let script = parse("LMOWEDT1\nslot 0\npalette-owners 10 editable\npalette-owner 2 fixed\nlayer 2 1 2 1234\nevent 0 1 2\nendpoint 0 3 4 5\nmessage 0 1 2 aa\nsprite 0 7 8 9 6 aabb\npalette 3 9234\nanimation trigger 4 aa\n").unwrap();
        assert_eq!(script.edits.len(), 7);
        assert_eq!(script.slot, 0);
    }

    #[test]
    fn rejects_late_headers_bad_shapes_and_bad_nested_commands() {
        assert!(
            parse("LMOWEDT1\nslot 0\npalette-owners 1 editable\nlayer 1 0 0 1\nslot 1\n").is_err()
        );
        assert!(
            parse("LMOWEDT1\nslot 0\npalette-owners 1 editable\npalette-owner 2 fixed\n").is_err()
        );
        assert!(parse("LMOWEDT1\nslot 0\npalette-owners 1 editable\nanimation nope\n").is_err());
    }
}
