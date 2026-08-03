//! Strict compressed Layer 2 tilemap scripts for word painting and native remapping.

use lm_app::NativeLevelAssetsControllerEdit;
use lm_level::{NativeLayer2RemapProgram, NATIVE_LAYER2_TILEMAP_LEN};
use std::collections::BTreeSet;
use std::fmt;

pub const MAX_SCRIPT_LEN: usize = 64 * 1024;
const MAX_COMMANDS: usize = 4096;
const MAGIC: &str = "LML2TIL1";

#[derive(Debug)]
pub enum Layer2TilemapEditScriptError {
    TooLarge,
    MissingMagic,
    UnsupportedVersion(String),
    TooManyCommands,
    UnknownCommand {
        line: usize,
        command: String,
    },
    WrongArity {
        line: usize,
    },
    InvalidIndex {
        line: usize,
        value: String,
    },
    IndexOutOfRange {
        line: usize,
        index: usize,
    },
    InvalidWord {
        line: usize,
        value: String,
    },
    InvalidOffset {
        line: usize,
        value: String,
    },
    InvalidSelection {
        line: usize,
        value: String,
    },
    DuplicateSelectionIndex {
        line: usize,
        index: usize,
    },
    Remap {
        line: usize,
        error: lm_level::NativeLayer2RemapError,
    },
}

impl fmt::Display for Layer2TilemapEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid Layer 2 tilemap-edit script: ")?;
        match self {
            Self::TooLarge => formatter.write_str("file exceeds the size limit"),
            Self::MissingMagic => formatter.write_str("missing format header"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported format header {version:?}")
            }
            Self::TooManyCommands => formatter.write_str("too many commands"),
            Self::UnknownCommand { line, command } => {
                write!(formatter, "unknown command {command:?} on line {line}")
            }
            Self::WrongArity { line } => write!(formatter, "wrong command arity on line {line}"),
            Self::InvalidIndex { line, value } => {
                write!(
                    formatter,
                    "invalid decimal tile index {value:?} on line {line}"
                )
            }
            Self::IndexOutOfRange { line, index } => {
                write!(
                    formatter,
                    "tile index {index} is out of range on line {line}"
                )
            }
            Self::InvalidWord { line, value } => {
                write!(
                    formatter,
                    "invalid hexadecimal tile word {value:?} on line {line}"
                )
            }
            Self::InvalidOffset { line, value } => {
                write!(
                    formatter,
                    "invalid signed decimal offset {value:?} on line {line}"
                )
            }
            Self::InvalidSelection { line, value } => {
                write!(formatter, "invalid tile selection {value:?} on line {line}")
            }
            Self::DuplicateSelectionIndex { line, index } => {
                write!(
                    formatter,
                    "duplicate selection index {index} on line {line}"
                )
            }
            Self::Remap { line, error } => write!(formatter, "remap on line {line}: {error}"),
        }
    }
}

impl std::error::Error for Layer2TilemapEditScriptError {}

/// Parses `word INDEX WORD` and `remap OFFSET SELECTION PROGRAM` commands.
///
/// Indexes and offsets are decimal, words are four hexadecimal digits, and a selection is `all`
/// or a comma-separated decimal index list. The remap program uses Lunar Magic's native syntax.
pub fn parse(
    input: &str,
) -> Result<Vec<NativeLevelAssetsControllerEdit>, Layer2TilemapEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(Layer2TilemapEditScriptError::TooLarge);
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(Layer2TilemapEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(Layer2TilemapEditScriptError::UnsupportedVersion(
            magic.into(),
        ));
    }
    let mut edits = Vec::new();
    for (offset, raw) in lines.enumerate() {
        let line = offset + 2;
        let content = raw.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        if edits.len() == MAX_COMMANDS {
            return Err(Layer2TilemapEditScriptError::TooManyCommands);
        }
        let command = content.split_whitespace().next().unwrap_or_default();
        match command {
            "word" => edits.push(parse_word(line, content)?),
            "remap" => edits.push(parse_remap(line, content)?),
            _ => {
                return Err(Layer2TilemapEditScriptError::UnknownCommand {
                    line,
                    command: command.into(),
                });
            }
        }
    }
    Ok(edits)
}

fn parse_word(
    line: usize,
    content: &str,
) -> Result<NativeLevelAssetsControllerEdit, Layer2TilemapEditScriptError> {
    let parts: Vec<_> = content.split_whitespace().collect();
    let ["word", index, word] = parts.as_slice() else {
        return Err(Layer2TilemapEditScriptError::WrongArity { line });
    };
    let index = decimal_index(line, index)?;
    if word.len() != 4 || !word.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Layer2TilemapEditScriptError::InvalidWord {
            line,
            value: (*word).into(),
        });
    }
    let word =
        u16::from_str_radix(word, 16).map_err(|_| Layer2TilemapEditScriptError::InvalidWord {
            line,
            value: (*word).into(),
        })?;
    Ok(NativeLevelAssetsControllerEdit::Layer2TilemapWords(vec![(
        index, word,
    )]))
}

fn parse_remap(
    line: usize,
    content: &str,
) -> Result<NativeLevelAssetsControllerEdit, Layer2TilemapEditScriptError> {
    let mut parts = content.split_whitespace();
    let _ = parts.next();
    let offset = parts
        .next()
        .ok_or(Layer2TilemapEditScriptError::WrongArity { line })?;
    let selection = parts
        .next()
        .ok_or(Layer2TilemapEditScriptError::WrongArity { line })?;
    let program = parts.collect::<Vec<_>>().join(" ");
    if program.is_empty() {
        return Err(Layer2TilemapEditScriptError::WrongArity { line });
    }
    let global_offset =
        offset
            .parse::<i32>()
            .map_err(|_| Layer2TilemapEditScriptError::InvalidOffset {
                line,
                value: offset.into(),
            })?;
    if !(-0x7fff..=0x7fff).contains(&global_offset) {
        return Err(Layer2TilemapEditScriptError::InvalidOffset {
            line,
            value: offset.into(),
        });
    }
    let selection = parse_selection(line, selection)?;
    NativeLayer2RemapProgram::parse(&program)
        .map_err(|error| Layer2TilemapEditScriptError::Remap { line, error })?;
    Ok(NativeLevelAssetsControllerEdit::Layer2TilemapRemap {
        script: program,
        global_offset,
        selection,
    })
}

fn parse_selection(
    line: usize,
    value: &str,
) -> Result<Option<Vec<usize>>, Layer2TilemapEditScriptError> {
    if value == "all" {
        return Ok(None);
    }
    if value.is_empty() {
        return Err(Layer2TilemapEditScriptError::InvalidSelection {
            line,
            value: value.into(),
        });
    }
    let mut seen = BTreeSet::new();
    let mut indexes = Vec::new();
    for value in value.split(',') {
        if value.is_empty() {
            return Err(Layer2TilemapEditScriptError::InvalidSelection {
                line,
                value: value.into(),
            });
        }
        let index = decimal_index(line, value)?;
        if !seen.insert(index) {
            return Err(Layer2TilemapEditScriptError::DuplicateSelectionIndex { line, index });
        }
        indexes.push(index);
    }
    Ok(Some(indexes))
}

fn decimal_index(line: usize, value: &str) -> Result<usize, Layer2TilemapEditScriptError> {
    let index = value
        .parse::<usize>()
        .map_err(|_| Layer2TilemapEditScriptError::InvalidIndex {
            line,
            value: value.into(),
        })?;
    let tile_count = NATIVE_LAYER2_TILEMAP_LEN / 2;
    if index >= tile_count {
        return Err(Layer2TilemapEditScriptError::IndexOutOfRange { line, index });
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_word_and_selection_scoped_native_remap_commands() {
        assert_eq!(
            parse("LML2TIL1\nword 0 1234\nword 1023 abcd\nremap -2 0,16 9234,9235\n").unwrap(),
            vec![
                NativeLevelAssetsControllerEdit::Layer2TilemapWords(vec![(0, 0x1234)]),
                NativeLevelAssetsControllerEdit::Layer2TilemapWords(vec![(0x3ff, 0xabcd)]),
                NativeLevelAssetsControllerEdit::Layer2TilemapRemap {
                    script: "9234,9235".into(),
                    global_offset: -2,
                    selection: Some(vec![0, 16]),
                },
            ]
        );
        assert!(matches!(
            parse("LML2TIL1\nremap 0 all 8000,8001\n").unwrap()[0],
            NativeLevelAssetsControllerEdit::Layer2TilemapRemap {
                selection: None,
                ..
            }
        ));
    }

    #[test]
    fn rejects_bad_framing_bounds_selections_and_remap_programs() {
        for script in [
            "LML2OBJ1\nword 0 1234\n",
            "LML2TIL1\nword 1024 1234\n",
            "LML2TIL1\nword 0 12345\n",
            "LML2TIL1\nword 0 123\n",
            "LML2TIL1\nremap 32768 all 8000,8001\n",
            "LML2TIL1\nremap 0 1,1 8000,8001\n",
            "LML2TIL1\nremap 0 all 8000\n",
            "LML2TIL1\nobject remove 0\n",
        ] {
            assert!(parse(script).is_err(), "accepted {script:?}");
        }
    }
}
