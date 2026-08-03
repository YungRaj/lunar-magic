//! Strict semantic scripts for Lunar Magic's installed per-level sprite-spawn controls.

use std::fmt;

pub const MAX_SCRIPT_LEN: usize = 1024;
const MAGIC: &str = "LMSPAWN1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpriteSpawnEdit {
    pub vertical_range: u8,
    pub smart_spawn: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpriteSpawnEditScriptError {
    TooLarge,
    MissingMagic,
    UnsupportedVersion(String),
    TooManyLines,
    MissingSettings,
    DuplicateSettings(usize),
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

pub fn parse(input: &str) -> Result<SpriteSpawnEdit, SpriteSpawnEditScriptError> {
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
    let mut settings = None;
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
        let edit = match words.as_slice() {
            ["settings", vertical_range, smart_spawn] => SpriteSpawnEdit {
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
            [command, ..] if *command != "settings" => {
                return Err(SpriteSpawnEditScriptError::UnknownCommand(
                    line,
                    (*command).into(),
                ));
            }
            _ => return Err(SpriteSpawnEditScriptError::WrongArity(line)),
        };
        if settings.replace(edit).is_some() {
            return Err(SpriteSpawnEditScriptError::DuplicateSettings(line));
        }
    }
    settings.ok_or(SpriteSpawnEditScriptError::MissingSettings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_bounded_semantic_edit_and_rejects_noncanonical_values() {
        assert_eq!(
            parse("LMSPAWN1\nsettings 3 true # scroll-triggered\n").unwrap(),
            SpriteSpawnEdit {
                vertical_range: 3,
                smart_spawn: true,
            }
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
            Err(SpriteSpawnEditScriptError::DuplicateSettings(3))
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
            Err(SpriteSpawnEditScriptError::MissingSettings)
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
