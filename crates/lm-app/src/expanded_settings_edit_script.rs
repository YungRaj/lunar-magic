use std::fmt;

pub const MAX_SCRIPT_LEN: usize = 16 * 1024;
const MAGIC: &str = "LMXSETED1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsEditScriptError {
    TooLarge,
    MissingMagic,
    UnsupportedVersion(String),
    TooManyLines,
    WrongArity(usize),
    UnknownCommand(usize, String),
    InvalidNumber(usize, String),
}
impl fmt::Display for ExpandedSettingsEditScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid expanded-settings edit script: {self:?}")
    }
}
impl std::error::Error for ExpandedSettingsEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<(usize, u16)>, ExpandedSettingsEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(ExpandedSettingsEditScriptError::TooLarge);
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(ExpandedSettingsEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(ExpandedSettingsEditScriptError::UnsupportedVersion(
            magic.into(),
        ));
    }
    let mut edits = Vec::new();
    for (offset, raw) in lines.enumerate() {
        let line = offset + 2;
        if line > 256 {
            return Err(ExpandedSettingsEditScriptError::TooManyLines);
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        let words: Vec<_> = content.split_whitespace().collect();
        match words.as_slice() {
            ["word", index, value] => edits.push((number(line, index)?, number(line, value)?)),
            [command, ..] if *command != "word" => {
                return Err(ExpandedSettingsEditScriptError::UnknownCommand(
                    line,
                    (*command).into(),
                ));
            }
            _ => return Err(ExpandedSettingsEditScriptError::WrongArity(line)),
        }
    }
    Ok(edits)
}

fn number<T: TryFrom<u64>>(line: usize, value: &str) -> Result<T, ExpandedSettingsEditScriptError> {
    let value_without_prefix = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(value_without_prefix, 16)
        .ok()
        .and_then(|v| T::try_from(v).ok())
        .ok_or_else(|| ExpandedSettingsEditScriptError::InvalidNumber(line, value.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_bounded_word_edits() {
        assert_eq!(
            parse("LMXSETED1\nword 0 a5a5\nword f 0x1234 # hi\n").unwrap(),
            vec![(0, 0xa5a5), (15, 0x1234)]
        );
        assert!(parse("OLD\n").is_err());
    }
}
