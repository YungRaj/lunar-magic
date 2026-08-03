//! Strict, bounded semantic edits for Lunar Magic's complete native secondary-exit table.

use lm_level::{SecondaryExit, SecondaryExitTable};
use std::fmt;

const MAGIC: &str = "LMSEXED1";
pub const MAX_SCRIPT_LEN: usize = 128 * 1024;
const MAX_LINE_LEN: usize = 4096;
const MAX_LINES: usize = 8192;
const MAX_COMMANDS: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecondaryExitEdit {
    Set { index: usize, value: SecondaryExit },
    Clear { index: usize },
    ClearAll,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecondaryExitEditScriptError {
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
    InvalidIndex {
        line: usize,
        index: usize,
    },
}

impl fmt::Display for SecondaryExitEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid secondary-exit edit script: {self:?}")
    }
}

impl std::error::Error for SecondaryExitEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<SecondaryExitEdit>, SecondaryExitEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(SecondaryExitEditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(SecondaryExitEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(SecondaryExitEditScriptError::UnsupportedVersion(
            magic.into(),
        ));
    }
    let mut edits = Vec::new();
    for (offset, raw) in lines.enumerate() {
        let line = offset + 2;
        if line > MAX_LINES {
            return Err(SecondaryExitEditScriptError::TooManyLines { maximum: MAX_LINES });
        }
        if raw.len() > MAX_LINE_LEN {
            return Err(SecondaryExitEditScriptError::LineTooLong {
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
            return Err(SecondaryExitEditScriptError::TooManyCommands {
                maximum: MAX_COMMANDS,
            });
        }
        edits.push(parse_command(line, content)?);
    }
    Ok(edits)
}

pub fn apply(
    table: &mut SecondaryExitTable,
    edits: &[SecondaryExitEdit],
) -> Result<(), SecondaryExitEditScriptError> {
    let mut staged = table.clone();
    for edit in edits {
        match edit {
            SecondaryExitEdit::Set { index, value } => {
                let entry = staged.entries.get_mut(*index).ok_or(
                    SecondaryExitEditScriptError::InvalidIndex {
                        line: 0,
                        index: *index,
                    },
                )?;
                *entry = *value;
            }
            SecondaryExitEdit::Clear { index } => {
                let entry = staged.entries.get_mut(*index).ok_or(
                    SecondaryExitEditScriptError::InvalidIndex {
                        line: 0,
                        index: *index,
                    },
                )?;
                *entry = SecondaryExit::default();
            }
            SecondaryExitEdit::ClearAll => staged.entries.fill(SecondaryExit::default()),
        }
    }
    *table = staged;
    Ok(())
}

fn parse_command(
    line: usize,
    content: &str,
) -> Result<SecondaryExitEdit, SecondaryExitEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        [
            "set",
            index,
            destination,
            position,
            screen,
            x,
            y,
            destination_flags,
            x_flags,
            additional,
        ] => Ok(SecondaryExitEdit::Set {
            index: index_value(line, index)?,
            value: SecondaryExit {
                destination_level: hex_u16(line, destination)?,
                position_and_method: hex_u8(line, position)?,
                screen: hex_u8(line, screen)?,
                x: hex_u8(line, x)?,
                y: hex_u8(line, y)?,
                destination_flags: hex_u8(line, destination_flags)?,
                x_and_overworld_flags: hex_u8(line, x_flags)?,
                additional_flags: hex_u8(line, additional)?,
            },
        }),
        ["clear", index] => Ok(SecondaryExitEdit::Clear {
            index: index_value(line, index)?,
        }),
        ["clear-all"] => Ok(SecondaryExitEdit::ClearAll),
        [command, ..] if !matches!(*command, "set" | "clear" | "clear-all") => {
            Err(SecondaryExitEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(SecondaryExitEditScriptError::WrongArity { line }),
    }
}

fn index_value(line: usize, value: &str) -> Result<usize, SecondaryExitEditScriptError> {
    let index = usize::from(hex_u16(line, value)?);
    if index >= SecondaryExitTable::ENTRY_COUNT {
        return Err(SecondaryExitEditScriptError::InvalidIndex { line, index });
    }
    Ok(index)
}

fn hex_u8(line: usize, value: &str) -> Result<u8, SecondaryExitEditScriptError> {
    let stripped = strip_hex_prefix(value);
    u8::from_str_radix(stripped, 16).map_err(|_| SecondaryExitEditScriptError::InvalidNumber {
        line,
        value: value.into(),
    })
}

fn hex_u16(line: usize, value: &str) -> Result<u16, SecondaryExitEditScriptError> {
    let stripped = strip_hex_prefix(value);
    u16::from_str_radix(stripped, 16).map_err(|_| SecondaryExitEditScriptError::InvalidNumber {
        line,
        value: value.into(),
    })
}

fn strip_hex_prefix(value: &str) -> &str {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_applies_set_clear_and_clear_all_in_order() {
        let edits = parse(
            "LMSEXED1\nclear-all\nset 0123 0105 02 03 04 05 20 80 07\nclear 0123\nset 0124 0106 01 02 03 04 20 90 08\n",
        )
        .unwrap();
        let mut table = SecondaryExitTable {
            entries: vec![SecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT],
        };
        table.entries[0] = SecondaryExit {
            destination_level: 1,
            ..SecondaryExit::default()
        };
        apply(&mut table, &edits).unwrap();
        assert_eq!(table.entries[0], SecondaryExit::default());
        assert_eq!(table.entries[0x123], SecondaryExit::default());
        assert_eq!(table.entries[0x124].destination_level, 0x106);
        assert_eq!(table.entries[0x124].x_and_overworld_flags, 0x90);
    }

    #[test]
    fn rejects_bad_framing_numbers_indexes_and_bounds() {
        for script in [
            "wrong\n",
            "LMSEXED1\nset 0000 0105 02 03 04 05 20 80\n",
            "LMSEXED1\nset 2000 0105 02 03 04 05 20 80 07\n",
            "LMSEXED1\nclear xyz\n",
            "LMSEXED1\nclear-all extra\n",
            "LMSEXED1\nunknown\n",
        ] {
            assert!(parse(script).is_err(), "accepted {script:?}");
        }
        assert!(matches!(
            parse(&"x".repeat(MAX_SCRIPT_LEN + 1)),
            Err(SecondaryExitEditScriptError::TooLarge { .. })
        ));
        let too_many = format!("LMSEXED1\n{}", "clear-all\n".repeat(MAX_COMMANDS + 1));
        assert!(matches!(
            parse(&too_many),
            Err(SecondaryExitEditScriptError::TooManyCommands { .. })
        ));
    }
}
