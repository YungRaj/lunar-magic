//! Bounded ownership-aware scripts for exact native SNES palette edits.

use lm_app::PaletteControllerEdit;
use lm_graphics::{Bgr555, PaletteChange, PaletteEntryOwner, PaletteOwnership};
use std::collections::BTreeSet;
use std::fmt;

const MAGIC: &str = "LMPALED1";
pub const MAX_SCRIPT_LEN: usize = 64 * 1024;
const MAX_LINE_LEN: usize = 4096;
const MAX_LINES: usize = 8192;
const MAX_COMMANDS: usize = 4096;
const MAX_COLORS: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaletteEditScript {
    pub ownership: PaletteOwnership,
    pub edits: Vec<PaletteControllerEdit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaletteEditScriptError {
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
    ColorLimit {
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
}

impl fmt::Display for PaletteEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native palette edit script: {self:?}")
    }
}

impl std::error::Error for PaletteEditScriptError {}

pub fn parse(input: &str) -> Result<PaletteEditScript, PaletteEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(PaletteEditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines.next().ok_or(PaletteEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(PaletteEditScriptError::UnsupportedVersion(magic.into()));
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
    let ownership = state
        .ownership
        .ok_or(PaletteEditScriptError::MissingOwnership)?;
    Ok(PaletteEditScript {
        ownership,
        edits: state.edits,
    })
}

#[derive(Default)]
struct ParseState {
    ownership: Option<PaletteOwnership>,
    owner_overrides: BTreeSet<usize>,
    edits: Vec<PaletteControllerEdit>,
}

fn validate_line(line: usize, raw: &str) -> Result<(), PaletteEditScriptError> {
    if line > MAX_LINES {
        return Err(PaletteEditScriptError::TooManyLines { maximum: MAX_LINES });
    }
    if raw.len() > MAX_LINE_LEN {
        return Err(PaletteEditScriptError::LineTooLong {
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
) -> Result<(), PaletteEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["owners", count, owner] => parse_owners(state, line, count, owner),
        ["owner", index, owner @ ("editable" | "fixed")] => {
            parse_owner_override(state, line, index, owner, None)
        }
        ["owner", index, "exanimation", record] => {
            parse_owner_override(state, line, index, "exanimation", Some(record))
        }
        ["set", index, color] => push_edit(
            state,
            PaletteControllerEdit::ApplyChanges(vec![PaletteChange {
                index: hex_usize(line, index)?,
                color: Bgr555(hex_u16(line, color)?),
            }]),
        ),
        ["changes", pairs @ ..] if !pairs.is_empty() && pairs.len() % 2 == 0 => {
            let changes = pairs
                .chunks_exact(2)
                .map(|pair| {
                    Ok(PaletteChange {
                        index: hex_usize(line, pair[0])?,
                        color: Bgr555(hex_u16(line, pair[1])?),
                    })
                })
                .collect::<Result<Vec<_>, PaletteEditScriptError>>()?;
            push_edit(state, PaletteControllerEdit::ApplyChanges(changes))
        }
        ["range", start, colors @ ..] if !colors.is_empty() => push_edit(
            state,
            PaletteControllerEdit::ReplaceRange {
                start: hex_usize(line, start)?,
                colors: colors
                    .iter()
                    .map(|color| Ok(Bgr555(hex_u16(line, color)?)))
                    .collect::<Result<_, PaletteEditScriptError>>()?,
            },
        ),
        [command, ..] if !matches!(*command, "owners" | "owner" | "set" | "changes" | "range") => {
            Err(PaletteEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(PaletteEditScriptError::WrongArity { line }),
    }
}

fn parse_owners(
    state: &mut ParseState,
    line: usize,
    count: &str,
    owner: &str,
) -> Result<(), PaletteEditScriptError> {
    if state.ownership.is_some() {
        return Err(PaletteEditScriptError::DuplicateOwnership { line });
    }
    if !state.edits.is_empty() {
        return Err(PaletteEditScriptError::OwnershipAfterEdit { line });
    }
    let count = hex_usize(line, count)?;
    if count > MAX_COLORS {
        return Err(PaletteEditScriptError::ColorLimit {
            line,
            actual: count,
            maximum: MAX_COLORS,
        });
    }
    let owner = simple_owner(line, owner)?;
    state.ownership = Some(PaletteOwnership::from_owners(vec![owner; count]));
    Ok(())
}

fn parse_owner_override(
    state: &mut ParseState,
    line: usize,
    index: &str,
    owner: &str,
    record: Option<&str>,
) -> Result<(), PaletteEditScriptError> {
    if !state.edits.is_empty() {
        return Err(PaletteEditScriptError::OwnershipAfterEdit { line });
    }
    let index = hex_usize(line, index)?;
    if !state.owner_overrides.insert(index) {
        return Err(PaletteEditScriptError::DuplicateOwnerOverride { line, index });
    }
    let owner = match (owner, record) {
        ("exanimation", Some(record)) => PaletteEntryOwner::ExAnimation {
            record: hex_u16(line, record)?,
        },
        (_, None) => simple_owner(line, owner)?,
        _ => {
            return Err(PaletteEditScriptError::InvalidOwner {
                line,
                value: owner.into(),
            });
        }
    };
    state
        .ownership
        .as_mut()
        .ok_or(PaletteEditScriptError::MissingOwnership)?
        .set_owner(index, owner)
        .map_err(|_| PaletteEditScriptError::InvalidNumber {
            line,
            value: index.to_string(),
        })
}

fn simple_owner(line: usize, value: &str) -> Result<PaletteEntryOwner, PaletteEditScriptError> {
    match value {
        "editable" => Ok(PaletteEntryOwner::Editable),
        "fixed" => Ok(PaletteEntryOwner::Fixed),
        _ => Err(PaletteEditScriptError::InvalidOwner {
            line,
            value: value.into(),
        }),
    }
}

fn push_edit(
    state: &mut ParseState,
    edit: PaletteControllerEdit,
) -> Result<(), PaletteEditScriptError> {
    if state.ownership.is_none() {
        return Err(PaletteEditScriptError::MissingOwnership);
    }
    if state.edits.len() == MAX_COMMANDS {
        return Err(PaletteEditScriptError::TooManyCommands {
            maximum: MAX_COMMANDS,
        });
    }
    state.edits.push(edit);
    Ok(())
}

fn hex_u16(line: usize, value: &str) -> Result<u16, PaletteEditScriptError> {
    u16::from_str_radix(strip_prefix(value), 16).map_err(|_| invalid_number(line, value))
}

fn hex_usize(line: usize, value: &str) -> Result<usize, PaletteEditScriptError> {
    usize::from_str_radix(strip_prefix(value), 16).map_err(|_| invalid_number(line, value))
}

fn invalid_number(line: usize, value: &str) -> PaletteEditScriptError {
    PaletteEditScriptError::InvalidNumber {
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
    fn parses_ownership_overrides_and_both_edit_shapes() {
        let script = parse(
            "LMPALED1\n\
             owners 20 editable\n\
             owner 00 fixed\n\
             owner 11 exanimation 0009\n\
             changes 01 8001 02 7fff\n\
             range 03 1234 9234\n",
        )
        .unwrap();
        assert_eq!(script.ownership.len(), 0x20);
        assert_eq!(script.ownership.owner(0), Some(PaletteEntryOwner::Fixed));
        assert_eq!(
            script.ownership.owner(0x11),
            Some(PaletteEntryOwner::ExAnimation { record: 9 })
        );
        assert_eq!(script.edits.len(), 2);
    }

    #[test]
    fn rejects_missing_duplicate_late_or_malformed_ownership_and_limits() {
        for script in [
            "wrong\n",
            "LMPALED1\nset 0 1\n",
            "LMPALED1\nowners 20 editable\nowners 20 fixed\n",
            "LMPALED1\nowners 20 editable\nset 1 2\nowner 1 fixed\n",
            "LMPALED1\nowners 20 editable\nowner 1 fixed\nowner 1 editable\n",
            "LMPALED1\nowners 20 mystery\n",
            "LMPALED1\nowners 20 editable\nchanges 1\n",
            "LMPALED1\nowners 1 editable\nowner 2 fixed\n",
        ] {
            assert!(parse(script).is_err(), "accepted {script:?}");
        }
        assert!(matches!(
            parse(&"x".repeat(MAX_SCRIPT_LEN + 1)),
            Err(PaletteEditScriptError::TooLarge { .. })
        ));
        let commands = format!(
            "LMPALED1\nowners 1 editable\n{}",
            "set 0 0\n".repeat(MAX_COMMANDS + 1)
        );
        assert!(matches!(
            parse(&commands),
            Err(PaletteEditScriptError::TooManyCommands { .. })
        ));
        let long_line = format!("LMPALED1\n{}", "x".repeat(MAX_LINE_LEN + 1));
        assert!(matches!(
            parse(&long_line),
            Err(PaletteEditScriptError::LineTooLong { .. })
        ));
    }
}
