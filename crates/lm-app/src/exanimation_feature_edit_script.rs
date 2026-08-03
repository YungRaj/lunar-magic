//! Strict semantic scripts for Lunar Magic's four per-level animation switches.

use std::fmt;

pub const MAX_SCRIPT_LEN: usize = 4 * 1024;
const MAGIC: &str = "LMEXFT1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExAnimationFeatureEdit {
    pub palette: bool,
    pub vanilla: bool,
    pub global: bool,
    pub level: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationFeatureEditScriptError {
    TooLarge,
    MissingMagic,
    UnsupportedVersion(String),
    TooManyLines,
    UnknownCommand(usize, String),
    WrongArity(usize),
    InvalidBoolean(usize, String),
    DuplicateFeatures(usize),
    MissingFeatures,
}

impl fmt::Display for ExAnimationFeatureEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid animation-feature edit script: {self:?}")
    }
}

impl std::error::Error for ExAnimationFeatureEditScriptError {}

pub fn parse(input: &str) -> Result<ExAnimationFeatureEdit, ExAnimationFeatureEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(ExAnimationFeatureEditScriptError::TooLarge);
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(ExAnimationFeatureEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(ExAnimationFeatureEditScriptError::UnsupportedVersion(
            magic.into(),
        ));
    }
    let mut edit = None;
    for (offset, raw) in lines.enumerate() {
        let line = offset + 2;
        if line > 32 {
            return Err(ExAnimationFeatureEditScriptError::TooManyLines);
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        let words: Vec<_> = content.split_whitespace().collect();
        let next = match words.as_slice() {
            ["features", palette, vanilla, global, level] => ExAnimationFeatureEdit {
                palette: boolean(line, palette)?,
                vanilla: boolean(line, vanilla)?,
                global: boolean(line, global)?,
                level: boolean(line, level)?,
            },
            [command, ..] if *command != "features" => {
                return Err(ExAnimationFeatureEditScriptError::UnknownCommand(
                    line,
                    (*command).into(),
                ));
            }
            _ => return Err(ExAnimationFeatureEditScriptError::WrongArity(line)),
        };
        if edit.replace(next).is_some() {
            return Err(ExAnimationFeatureEditScriptError::DuplicateFeatures(line));
        }
    }
    edit.ok_or(ExAnimationFeatureEditScriptError::MissingFeatures)
}

fn boolean(line: usize, value: &str) -> Result<bool, ExAnimationFeatureEditScriptError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ExAnimationFeatureEditScriptError::InvalidBoolean(
            line,
            value.into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_four_named_positive_states() {
        assert_eq!(
            parse("LMEXFT1\nfeatures true false true false\n").unwrap(),
            ExAnimationFeatureEdit {
                palette: true,
                vanilla: false,
                global: true,
                level: false,
            }
        );
    }

    #[test]
    fn rejects_missing_duplicate_malformed_and_unbounded_inputs() {
        assert_eq!(
            parse("LMEXFT1\n"),
            Err(ExAnimationFeatureEditScriptError::MissingFeatures)
        );
        assert!(matches!(
            parse("LMEXFT1\nfeatures true false true false\nfeatures false false false false\n"),
            Err(ExAnimationFeatureEditScriptError::DuplicateFeatures(3))
        ));
        assert!(matches!(
            parse("LMEXFT1\nfeatures yes false true false\n"),
            Err(ExAnimationFeatureEditScriptError::InvalidBoolean(2, _))
        ));
        assert!(matches!(
            parse(&"x".repeat(MAX_SCRIPT_LEN + 1)),
            Err(ExAnimationFeatureEditScriptError::TooLarge)
        ));
    }
}
