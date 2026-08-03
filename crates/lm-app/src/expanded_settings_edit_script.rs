//! Strict, bounded raw and semantic expanded-settings edit scripts.

use lm_level::{
    ExpandedLevelSettingsRecord, Layer3ExpandedModeFlags, Layer3TilemapGraphicsDescriptor,
    Layer3TilemapGraphicsDescriptorError,
};
use std::fmt;

pub const MAX_SCRIPT_LEN: usize = 16 * 1024;
const MAGIC: &str = "LMXSETED1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsScriptEdit {
    Word {
        index: usize,
        value: u16,
    },
    Layer3Tilemap {
        enabled: bool,
        descriptor: Layer3TilemapGraphicsDescriptor,
    },
    Layer3ExpandedMode(Layer3ExpandedModeFlags),
    SpriteBoundaryInteractionAir(bool),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedSettingsEditScript {
    pub edits: Vec<ExpandedSettingsScriptEdit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedSettingsEditScriptError {
    TooLarge,
    MissingMagic,
    UnsupportedVersion(String),
    TooManyLines,
    WrongArity(usize),
    UnknownCommand(usize, String),
    InvalidNumber(usize, String),
    InvalidBoolean(usize, String),
    Layer3Descriptor(usize, Layer3TilemapGraphicsDescriptorError),
    WordOutOfRange(usize, usize),
    DuplicateWord(usize, usize),
}

impl fmt::Display for ExpandedSettingsEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid expanded-settings edit script: {self:?}")
    }
}

impl std::error::Error for ExpandedSettingsEditScriptError {}

pub fn parse(input: &str) -> Result<ExpandedSettingsEditScript, ExpandedSettingsEditScriptError> {
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
    let mut seen = [false; ExpandedLevelSettingsRecord::WORD_COUNT];
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
        let (edit, owned): (ExpandedSettingsScriptEdit, Vec<usize>) = match words.as_slice() {
            ["word", index, value] => {
                let index = number(line, index)?;
                (
                    ExpandedSettingsScriptEdit::Word {
                        index,
                        value: number(line, value)?,
                    },
                    vec![index],
                )
            }
            ["layer3-tilemap", enabled, file, length, destination] => {
                let descriptor = Layer3TilemapGraphicsDescriptor::new(
                    number(line, file)?,
                    number(line, length)?,
                    number(line, destination)?,
                )
                .map_err(|error| ExpandedSettingsEditScriptError::Layer3Descriptor(line, error))?;
                (
                    ExpandedSettingsScriptEdit::Layer3Tilemap {
                        enabled: boolean(line, enabled)?,
                        descriptor,
                    },
                    vec![0, 1],
                )
            }
            ["layer3-mode", packed] => (
                ExpandedSettingsScriptEdit::Layer3ExpandedMode(
                    Layer3ExpandedModeFlags::from_packed(number(line, packed)?),
                ),
                (8..16).collect(),
            ),
            ["boundary-air", enabled] => (
                ExpandedSettingsScriptEdit::SpriteBoundaryInteractionAir(boolean(line, enabled)?),
                vec![8],
            ),
            [command, ..]
                if !matches!(
                    *command,
                    "word" | "layer3-tilemap" | "layer3-mode" | "boundary-air"
                ) =>
            {
                return Err(ExpandedSettingsEditScriptError::UnknownCommand(
                    line,
                    (*command).into(),
                ));
            }
            _ => return Err(ExpandedSettingsEditScriptError::WrongArity(line)),
        };
        for index in owned {
            if index >= seen.len() {
                return Err(ExpandedSettingsEditScriptError::WordOutOfRange(line, index));
            }
            if std::mem::replace(&mut seen[index], true) {
                return Err(ExpandedSettingsEditScriptError::DuplicateWord(line, index));
            }
        }
        edits.push(edit);
    }
    Ok(ExpandedSettingsEditScript { edits })
}

impl ExpandedSettingsEditScript {
    pub fn resolve(
        &self,
        source: &ExpandedLevelSettingsRecord,
    ) -> Result<Vec<(usize, u16)>, ExpandedSettingsEditScriptError> {
        let mut staged = source.clone();
        let mut touched = [false; ExpandedLevelSettingsRecord::WORD_COUNT];
        for edit in &self.edits {
            match *edit {
                ExpandedSettingsScriptEdit::Word { index, value } => {
                    staged
                        .set_word(index, value)
                        .map_err(|_| ExpandedSettingsEditScriptError::WordOutOfRange(0, index))?;
                    touched[index] = true;
                }
                ExpandedSettingsScriptEdit::Layer3Tilemap {
                    enabled,
                    descriptor,
                } => {
                    staged
                        .set_layer3_tilemap_enabled(enabled)
                        .expect("fixed word zero");
                    staged
                        .set_layer3_tilemap_graphics_descriptor(descriptor)
                        .expect("fixed word one");
                    touched[0] = true;
                    touched[1] = true;
                }
                ExpandedSettingsScriptEdit::Layer3ExpandedMode(flags) => {
                    staged
                        .set_layer3_expanded_mode_flags(flags)
                        .expect("fixed mode words");
                    touched[8..16].fill(true);
                }
                ExpandedSettingsScriptEdit::SpriteBoundaryInteractionAir(enabled) => {
                    let mut header = lm_level::ExpandedLevelHeader::from(&staged);
                    header.set_sprites_beyond_boundaries_use_air(enabled);
                    staged = header.into();
                    touched[8] = true;
                }
            }
        }
        Ok(touched
            .into_iter()
            .enumerate()
            .filter_map(|(index, touched)| touched.then_some((index, staged.word(index).unwrap())))
            .collect())
    }
}

fn boolean(line: usize, value: &str) -> Result<bool, ExpandedSettingsEditScriptError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ExpandedSettingsEditScriptError::InvalidBoolean(
            line,
            value.into(),
        )),
    }
}

fn number<T: TryFrom<u64>>(line: usize, value: &str) -> Result<T, ExpandedSettingsEditScriptError> {
    let value_without_prefix = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(value_without_prefix, 16)
        .ok()
        .and_then(|value| T::try_from(value).ok())
        .ok_or_else(|| ExpandedSettingsEditScriptError::InvalidNumber(line, value.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_and_semantic_commands_and_resolves_shared_words_losslessly() {
        let script =
            parse("LMXSETED1\nword 2 a5a5\nlayer3-tilemap true abc 2 3\nboundary-air false\n")
                .unwrap();
        assert_eq!(script.edits.len(), 3);
        let mut source = ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap();
        source.set_word(8, 0xf123).unwrap();
        let edits = script.resolve(&source).unwrap();
        assert_eq!(
            edits,
            vec![(0, 0x7a5a), (1, 0xeabc), (2, 0xa5a5), (8, 0xb123)]
        );
    }

    #[test]
    fn resolves_layer3_mode_without_changing_any_shared_low_bits() {
        let script = parse("LMXSETED1\nlayer3-mode 89abcdef\n").unwrap();
        let source = ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap();
        let edits = script.resolve(&source).unwrap();
        assert_eq!(edits.len(), 8);
        let mut staged = source.clone();
        for (word, value) in edits {
            assert_eq!(value & 0x0fff, source.word(word).unwrap() & 0x0fff);
            staged.set_word(word, value).unwrap();
        }
        assert_eq!(staged.layer3_expanded_mode_flags().packed(), 0x89ab_cdef);
    }

    #[test]
    fn rejects_bad_framing_values_overlaps_and_owned_word_bounds() {
        assert!(matches!(
            parse("LMXSETED1\nlayer3-tilemap true 1000 0 0\n"),
            Err(ExpandedSettingsEditScriptError::Layer3Descriptor(2, _))
        ));
        assert!(matches!(
            parse("LMXSETED1\nboundary-air yes\n"),
            Err(ExpandedSettingsEditScriptError::InvalidBoolean(2, _))
        ));
        assert_eq!(
            parse("LMXSETED1\nword 0 1\nlayer3-tilemap true abc 2 3\n"),
            Err(ExpandedSettingsEditScriptError::DuplicateWord(3, 0))
        );
        assert_eq!(
            parse("LMXSETED1\nword 10 1\n"),
            Err(ExpandedSettingsEditScriptError::WordOutOfRange(2, 0x10))
        );
        assert_eq!(
            parse("LMXSETED1\nlayer3-mode 89abcdef\nboundary-air true\n"),
            Err(ExpandedSettingsEditScriptError::DuplicateWord(3, 8))
        );
        assert!(parse("OLD\n").is_err());
        assert!(matches!(
            parse("LMXSETED1\nunknown 1\n"),
            Err(ExpandedSettingsEditScriptError::UnknownCommand(2, _))
        ));
        assert_eq!(
            parse("LMXSETED1\nword 1\n"),
            Err(ExpandedSettingsEditScriptError::WrongArity(2))
        );
        assert!(matches!(
            parse("LMXSETED1\nword xyz 1\n"),
            Err(ExpandedSettingsEditScriptError::InvalidNumber(2, _))
        ));
        assert_eq!(
            parse(&"x".repeat(MAX_SCRIPT_LEN + 1)),
            Err(ExpandedSettingsEditScriptError::TooLarge)
        );
        let too_many_lines = format!("LMXSETED1\n{}", "#\n".repeat(256));
        assert_eq!(
            parse(&too_many_lines),
            Err(ExpandedSettingsEditScriptError::TooManyLines)
        );
    }
}
