//! Strict semantic scripts for Lunar Magic's installed per-level sprite-spawn controls.

use std::fmt;

pub const MAX_SCRIPT_LEN: usize = 1024;
const MAGIC: &str = "LMSPAWN1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpriteSpawnEdit {
    Properties {
        vertical_range: u8,
        smart_spawn: bool,
    },
    BoundaryInteractionAir(bool),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpriteSpawnEditScriptError {
    TooLarge,
    MissingMagic,
    UnsupportedVersion(String),
    TooManyLines,
    NoCommands,
    DuplicateCommand(usize, String),
    WrongArity(usize),
    UnknownCommand(usize, String),
    InvalidRange(usize, String),
    InvalidBoolean(usize, String),
}

impl fmt::Display for SpriteSpawnEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid sprite-spawn edit script: {self:?}")
    }
}

impl std::error::Error for SpriteSpawnEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<SpriteSpawnEdit>, SpriteSpawnEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(SpriteSpawnEditScriptError::TooLarge);
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(SpriteSpawnEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(SpriteSpawnEditScriptError::UnsupportedVersion(magic.into()));
    }
    let mut edits = Vec::new();
    let mut properties_seen = false;
    let mut boundary_seen = false;
    for (offset, raw) in lines.enumerate() {
        let line = offset + 2;
        if line > 16 {
            return Err(SpriteSpawnEditScriptError::TooManyLines);
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        let words: Vec<_> = content.split_whitespace().collect();
        let (command, edit) = match words.as_slice() {
            ["settings", vertical_range, smart_spawn] => (
                "settings",
                SpriteSpawnEdit::Properties {
                    vertical_range: vertical_range
                        .parse::<u8>()
                        .ok()
                        .filter(|value| *value <= 3)
                        .ok_or_else(|| {
                            SpriteSpawnEditScriptError::InvalidRange(line, (*vertical_range).into())
                        })?,
                    smart_spawn: match *smart_spawn {
                        "true" => true,
                        "false" => false,
                        value => {
                            return Err(SpriteSpawnEditScriptError::InvalidBoolean(
                                line,
                                value.into(),
                            ));
                        }
                    },
                },
            ),
            ["boundary-air", enabled] => (
                "boundary-air",
                SpriteSpawnEdit::BoundaryInteractionAir(boolean(line, enabled)?),
            ),
            [command, ..] if !matches!(*command, "settings" | "boundary-air") => {
                return Err(SpriteSpawnEditScriptError::UnknownCommand(
                    line,
                    (*command).into(),
                ));
            }
            _ => return Err(SpriteSpawnEditScriptError::WrongArity(line)),
        };
        let seen = if command == "settings" {
            &mut properties_seen
        } else {
            &mut boundary_seen
        };
        if std::mem::replace(seen, true) {
            return Err(SpriteSpawnEditScriptError::DuplicateCommand(
                line,
                command.into(),
            ));
        }
        edits.push(edit);
    }
    if edits.is_empty() {
        return Err(SpriteSpawnEditScriptError::NoCommands);
    }
    Ok(edits)
}

fn boolean(line: usize, value: &str) -> Result<bool, SpriteSpawnEditScriptError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(SpriteSpawnEditScriptError::InvalidBoolean(
            line,
            value.into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_semantic_edits_and_rejects_noncanonical_values() {
        assert_eq!(
            parse("LMSPAWN1\nsettings 3 true # scroll-triggered\nboundary-air false\n").unwrap(),
            vec![
                SpriteSpawnEdit::Properties {
                    vertical_range: 3,
                    smart_spawn: true,
                },
                SpriteSpawnEdit::BoundaryInteractionAir(false),
            ]
        );
        assert!(matches!(
            parse("LMSPAWN1\nsettings 4 false\n"),
            Err(SpriteSpawnEditScriptError::InvalidRange(2, _))
        ));
        assert!(matches!(
            parse("LMSPAWN1\nsettings 1 yes\n"),
            Err(SpriteSpawnEditScriptError::InvalidBoolean(2, _))
        ));
        assert!(matches!(
            parse("LMSPAWN1\nsettings 1 true\nsettings 2 false\n"),
            Err(SpriteSpawnEditScriptError::DuplicateCommand(3, _))
        ));
        assert!(matches!(
            parse("LMSPAWN1\nboundary-air true\nboundary-air false\n"),
            Err(SpriteSpawnEditScriptError::DuplicateCommand(3, _))
        ));
    }

    #[test]
    fn rejects_framing_commands_arity_and_limits_before_an_edit_escapes() {
        assert!(matches!(
            parse("OLD\nsettings 1 true\n"),
            Err(SpriteSpawnEditScriptError::UnsupportedVersion(_))
        ));
        assert_eq!(
            parse("LMSPAWN1\n"),
            Err(SpriteSpawnEditScriptError::NoCommands)
        );
        assert!(matches!(
            parse("LMSPAWN1\nspawn 1 true\n"),
            Err(SpriteSpawnEditScriptError::UnknownCommand(2, _))
        ));
        assert_eq!(
            parse("LMSPAWN1\nsettings 1\n"),
            Err(SpriteSpawnEditScriptError::WrongArity(2))
        );
        assert_eq!(
            parse(&"x".repeat(MAX_SCRIPT_LEN + 1)),
            Err(SpriteSpawnEditScriptError::TooLarge)
        );
        let too_many_lines = format!("LMSPAWN1\n{}", "#\n".repeat(16));
        assert_eq!(
            parse(&too_many_lines),
            Err(SpriteSpawnEditScriptError::TooManyLines)
        );
    }
}
