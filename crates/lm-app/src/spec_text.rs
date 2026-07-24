use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

pub(crate) const MAX_SPEC_BYTES: usize = 64 * 1024;
const MAX_LINES: usize = 32;
const MAX_LINE_BYTES: usize = 4096;

pub(crate) type Fields = BTreeMap<String, String>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SpecError(String);

impl fmt::Display for SpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SpecError {}

pub(crate) fn parse_fields(text: &str, expected_magic: &str) -> Result<Fields, SpecError> {
    if text.len() > MAX_SPEC_BYTES {
        return Err(error("portable specification is too large"));
    }
    let mut lines = text.lines();
    if lines.next() != Some(expected_magic) {
        return Err(error(format!(
            "portable specification must begin with {expected_magic}"
        )));
    }
    let mut fields = Fields::new();
    for (index, raw) in lines.enumerate() {
        let line_number = index + 2;
        if line_number > MAX_LINES {
            return Err(error("portable specification has too many lines"));
        }
        if raw.len() > MAX_LINE_BYTES {
            return Err(error(format!(
                "portable specification line {line_number} is too long"
            )));
        }
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).ok_or_else(|| {
            error(format!(
                "portable specification line {line_number} has no value"
            ))
        })?;
        let value = value.trim();
        if value.is_empty() {
            return Err(error(format!(
                "portable specification line {line_number} has an empty value"
            )));
        }
        if fields.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(error(format!("portable specification repeats {key}")));
        }
    }
    Ok(fields)
}

pub(crate) fn take_path(fields: &mut Fields, key: &str, base: &Path) -> Result<PathBuf, SpecError> {
    let value = fields
        .remove(key)
        .ok_or_else(|| error(format!("portable specification is missing {key}")))?;
    Ok(base.join(value))
}

pub(crate) fn take_optional_path(fields: &mut Fields, key: &str, base: &Path) -> Option<PathBuf> {
    fields.remove(key).map(|value| base.join(value))
}

pub(crate) fn take_usize(fields: &mut Fields, key: &str) -> Result<usize, SpecError> {
    let value = fields
        .remove(key)
        .ok_or_else(|| error(format!("portable specification is missing {key}")))?;
    value.parse().map_err(|_| {
        error(format!(
            "portable specification has invalid decimal {key}: {value:?}"
        ))
    })
}

pub(crate) fn take_string(fields: &mut Fields, key: &str) -> Result<String, SpecError> {
    fields
        .remove(key)
        .ok_or_else(|| error(format!("portable specification is missing {key}")))
}

pub(crate) fn reject_unknown(fields: &Fields) -> Result<(), SpecError> {
    fields.keys().next().map_or(Ok(()), |key| {
        Err(error(format!(
            "portable specification has unknown field {key}"
        )))
    })
}

pub(crate) fn error(message: impl Into<String>) -> SpecError {
    SpecError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_rejects_bad_magic_duplicates_and_unknown_fields() {
        assert!(parse_fields("wrong\n", "MAGIC").is_err());
        assert!(parse_fields("MAGIC\npage a\npage b\n", "MAGIC").is_err());
        let fields = parse_fields("MAGIC\nunknown x\n", "MAGIC").unwrap();
        assert!(reject_unknown(&fields).is_err());
    }
}
