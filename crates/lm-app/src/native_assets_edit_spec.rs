//! Bounded aggregate edit specifications for the runnable application shell.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

const MAGIC: &str = "LMNATED1";
const MAX_LEN: usize = 16 * 1024;
const MAX_LINES: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeAssetsEditSpec {
    pub level: Option<PathBuf>,
    pub palette: Option<PathBuf>,
    pub exanimation: Option<PathBuf>,
    pub expanded_settings: Option<PathBuf>,
}

#[derive(Debug)]
pub enum NativeAssetsEditSpecError {
    Io(std::io::Error),
    Utf8(std::string::FromUtf8Error),
    TooLarge,
    MissingMagic,
    UnsupportedVersion(String),
    TooManyLines,
    WrongAssignment(usize),
    UnknownField(usize, String),
    DuplicateField(usize, String),
    EmptyPath(usize),
    NoDomains,
}

impl fmt::Display for NativeAssetsEditSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid native-assets edit specification: ")?;
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Utf8(error) => write!(formatter, "invalid UTF-8: {error}"),
            Self::TooLarge => formatter.write_str("file exceeds the size limit"),
            Self::MissingMagic => formatter.write_str("missing format header"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported format header {version:?}")
            }
            Self::TooManyLines => formatter.write_str("too many lines"),
            Self::WrongAssignment(line) => {
                write!(formatter, "malformed assignment on line {line}")
            }
            Self::UnknownField(line, field) => {
                write!(formatter, "unknown field {field:?} on line {line}")
            }
            Self::DuplicateField(line, field) => {
                write!(formatter, "duplicate field {field:?} on line {line}")
            }
            Self::EmptyPath(line) => write!(formatter, "empty path on line {line}"),
            Self::NoDomains => formatter.write_str("no edit domains were declared"),
        }
    }
}
impl std::error::Error for NativeAssetsEditSpecError {}

pub fn read(path: &Path) -> Result<NativeAssetsEditSpec, NativeAssetsEditSpecError> {
    let bytes = crate::read_bounded_bytes(path, MAX_LEN, "native-assets edit specification")
        .map_err(|error| NativeAssetsEditSpecError::Io(std::io::Error::other(error.to_string())))?;
    let text = String::from_utf8(bytes).map_err(NativeAssetsEditSpecError::Utf8)?;
    parse(&text, path.parent().unwrap_or_else(|| Path::new("")))
}

pub fn parse(input: &str, base: &Path) -> Result<NativeAssetsEditSpec, NativeAssetsEditSpecError> {
    if input.len() > MAX_LEN {
        return Err(NativeAssetsEditSpecError::TooLarge);
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(NativeAssetsEditSpecError::MissingMagic)?;
    if magic != MAGIC {
        return Err(NativeAssetsEditSpecError::UnsupportedVersion(magic.into()));
    }
    let mut fields = BTreeMap::new();
    for (offset, raw) in lines.enumerate() {
        let line = offset + 2;
        if line > MAX_LINES {
            return Err(NativeAssetsEditSpecError::TooManyLines);
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        let (key, value) = content
            .split_once('=')
            .ok_or(NativeAssetsEditSpecError::WrongAssignment(line))?;
        let key = key.trim();
        if !matches!(
            key,
            "level" | "palette" | "exanimation" | "expanded-settings"
        ) {
            return Err(NativeAssetsEditSpecError::UnknownField(line, key.into()));
        }
        let value = value.trim();
        if value.is_empty() {
            return Err(NativeAssetsEditSpecError::EmptyPath(line));
        }
        let path = PathBuf::from(value);
        let path = if path.is_absolute() {
            path
        } else {
            base.join(path)
        };
        if fields.insert(key.to_owned(), path).is_some() {
            return Err(NativeAssetsEditSpecError::DuplicateField(line, key.into()));
        }
    }
    if fields.is_empty() {
        return Err(NativeAssetsEditSpecError::NoDomains);
    }
    Ok(NativeAssetsEditSpec {
        level: fields.remove("level"),
        palette: fields.remove("palette"),
        exanimation: fields.remove("exanimation"),
        expanded_settings: fields.remove("expanded-settings"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_unicode_paths_and_rejects_duplicates() {
        let spec = parse(
            "LMNATED1\nlevel=Scripts/Level edits.txt\nexpanded-settings=設定.txt\n",
            Path::new("project"),
        )
        .unwrap();
        assert_eq!(
            spec.level,
            Some(PathBuf::from("project/Scripts/Level edits.txt"))
        );
        assert_eq!(
            spec.expanded_settings,
            Some(PathBuf::from("project/設定.txt"))
        );
        assert!(parse("LMNATED1\nlevel=a\nlevel=b\n", Path::new(".")).is_err());
        assert!(parse("LMNATED1\n", Path::new(".")).is_err());
    }
}
