//! Strict, bounded semantic scripts for pristine-table main and installed midway entrances.

use lm_app::VanillaEntranceEdit;
use lm_level::SeparateMidwayEntrance;
use lm_project::VanillaMainEntrance;
use std::fmt;

const MAGIC: &str = "LMENTR1";
pub const MAX_SCRIPT_LEN: usize = 64 * 1024;
const MAX_LINE_LEN: usize = 4096;
const MAX_LINES: usize = 8192;
const MAX_COMMANDS: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntranceEditScriptError {
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
}

impl fmt::Display for EntranceEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid entrance-edit script: {self:?}")
    }
}

impl std::error::Error for EntranceEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<VanillaEntranceEdit>, EntranceEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(EntranceEditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines.next().ok_or(EntranceEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(EntranceEditScriptError::UnsupportedVersion(magic.into()));
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if line > MAX_LINES {
            return Err(EntranceEditScriptError::TooManyLines { maximum: MAX_LINES });
        }
        if raw.len() > MAX_LINE_LEN {
            return Err(EntranceEditScriptError::LineTooLong {
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
            return Err(EntranceEditScriptError::TooManyCommands {
                maximum: MAX_COMMANDS,
            });
        }
        edits.push(parse_command(line, content)?);
    }
    Ok(edits)
}

fn parse_command(
    line: usize,
    content: &str,
) -> Result<VanillaEntranceEdit, EntranceEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["main", position, vertical, screen_method, mode_screen] => {
            Ok(VanillaEntranceEdit::SetMain(VanillaMainEntrance {
                position: hex_byte(line, position)?,
                vertical_settings: hex_byte(line, vertical)?,
                screen_and_method: hex_byte(line, screen_method)?,
                level_mode_and_screen: hex_byte(line, mode_screen)?,
            }))
        }
        ["layer2-scroll", table] => {
            let table = hex_byte(line, table)?;
            if table > 0x0f {
                return Err(EntranceEditScriptError::InvalidNumber {
                    line,
                    value: table.to_string(),
                });
            }
            Ok(VanillaEntranceEdit::SetLayer2ScrollTable(table))
        }
        ["midway", flags, position, additional_flags, high_position] => {
            Ok(VanillaEntranceEdit::SetMidway(SeparateMidwayEntrance {
                flags: hex_byte(line, flags)?,
                position: hex_byte(line, position)?,
                additional_flags: hex_byte(line, additional_flags)?,
                high_position: hex_byte(line, high_position)?,
            }))
        }
        [command, ..] if !matches!(*command, "main" | "layer2-scroll" | "midway") => {
            Err(EntranceEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(EntranceEditScriptError::WrongArity { line }),
    }
}

fn hex_byte(line: usize, value: &str) -> Result<u8, EntranceEditScriptError> {
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    u8::from_str_radix(value, 16).map_err(|_| EntranceEditScriptError::InvalidNumber {
        line,
        value: value.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_semantic_entrance_edit() {
        assert_eq!(
            parse("LMENTR1\nmain 12 34 56 78\nlayer2-scroll 0f\nmidway 9a bc de f0\n").unwrap(),
            [
                VanillaEntranceEdit::SetMain(VanillaMainEntrance {
                    position: 0x12,
                    vertical_settings: 0x34,
                    screen_and_method: 0x56,
                    level_mode_and_screen: 0x78,
                }),
                VanillaEntranceEdit::SetLayer2ScrollTable(0x0f),
                VanillaEntranceEdit::SetMidway(SeparateMidwayEntrance {
                    flags: 0x9a,
                    position: 0xbc,
                    additional_flags: 0xde,
                    high_position: 0xf0,
                }),
            ]
        );
    }

    #[test]
    fn rejects_bad_framing_values_and_bounds() {
        for script in [
            "wrong\n",
            "LMENTR1\nmain 00 01 02\n",
            "LMENTR1\nmain 00 01 02 xyz\n",
            "LMENTR1\nlayer2-scroll 10\n",
            "LMENTR1\nmidway 00 01 02\n",
            "LMENTR1\nunknown 00\n",
        ] {
            assert!(parse(script).is_err(), "accepted {script:?}");
        }
        assert!(matches!(
            parse(&"x".repeat(MAX_SCRIPT_LEN + 1)),
            Err(EntranceEditScriptError::TooLarge { .. })
        ));
        let too_many = format!("LMENTR1\n{}", "main 0 0 0 0\n".repeat(MAX_COMMANDS + 1));
        assert!(matches!(
            parse(&too_many),
            Err(EntranceEditScriptError::TooManyCommands { .. })
        ));
    }
}
