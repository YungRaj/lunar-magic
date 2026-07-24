//! Bounded scripts for lossless portable overworld metadata edits.

use lm_overworld::{MetadataEdit, OverworldLevelName, PlayerStart, Submap, SubmapSettings};
use std::fmt;

const MAGIC: &str = "LMOMEDT1";
pub const MAX_SCRIPT_LEN: usize = 128 * 1024;
const MAX_LINE_LEN: usize = 4096;
const MAX_LINES: usize = 4096;
const MAX_COMMANDS: usize = 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldMetadataEditScriptError {
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
    InvalidHex {
        line: usize,
        value: String,
    },
    InvalidSubmap {
        line: usize,
        value: String,
    },
}

impl fmt::Display for OverworldMetadataEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid overworld metadata edit script: {self:?}"
        )
    }
}

impl std::error::Error for OverworldMetadataEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<MetadataEdit>, OverworldMetadataEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(OverworldMetadataEditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(OverworldMetadataEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(OverworldMetadataEditScriptError::UnsupportedVersion(
            magic.into(),
        ));
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if line > MAX_LINES {
            return Err(OverworldMetadataEditScriptError::TooManyLines { maximum: MAX_LINES });
        }
        if raw.len() > MAX_LINE_LEN {
            return Err(OverworldMetadataEditScriptError::LineTooLong {
                line,
                actual: raw.len(),
                maximum: MAX_LINE_LEN,
            });
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if !content.is_empty() {
            edits.push(parse_line(line, content)?);
            if edits.len() > MAX_COMMANDS {
                return Err(OverworldMetadataEditScriptError::TooManyCommands {
                    maximum: MAX_COMMANDS,
                });
            }
        }
    }
    Ok(edits)
}

fn parse_line(
    line: usize,
    content: &str,
) -> Result<MetadataEdit, OverworldMetadataEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["name", "upsert", level, flags, tiles] => {
            Ok(MetadataEdit::UpsertLevelName(OverworldLevelName {
                level: hex(line, level)?,
                raw_flags: hex(line, flags)?,
                tiles: fixed_bytes(line, tiles)?,
            }))
        }
        ["name", "remove", level] => Ok(MetadataEdit::RemoveLevelName(hex(line, level)?)),
        ["start", "upsert", player, x, y, submap, flags] => {
            Ok(MetadataEdit::UpsertPlayerStart(PlayerStart {
                player: hex(line, player)?,
                x: hex(line, x)?,
                y: hex(line, y)?,
                submap: parse_submap(line, submap)?,
                raw_flags: hex(line, flags)?,
            }))
        }
        ["start", "remove", player] => Ok(MetadataEdit::RemovePlayerStart(hex(line, player)?)),
        [
            "settings",
            "upsert",
            submap,
            music,
            palette,
            layer1,
            layer2,
            flags,
            unknown,
        ] => Ok(MetadataEdit::UpsertSubmapSettings(SubmapSettings {
            submap: parse_submap(line, submap)?,
            music: hex(line, music)?,
            palette: hex(line, palette)?,
            layer1_scroll: hex(line, layer1)?,
            layer2_scroll: hex(line, layer2)?,
            raw_flags: hex(line, flags)?,
            unknown: fixed_bytes(line, unknown)?,
        })),
        ["settings", "remove", submap] => Ok(MetadataEdit::RemoveSubmapSettings(parse_submap(
            line, submap,
        )?)),
        [command, ..] if !matches!(*command, "name" | "start" | "settings") => {
            Err(OverworldMetadataEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(OverworldMetadataEditScriptError::WrongArity { line }),
    }
}

fn parse_submap(line: usize, value: &str) -> Result<Submap, OverworldMetadataEditScriptError> {
    let encoded = hex::<u8>(line, value)?;
    Submap::decode(encoded).ok_or_else(|| OverworldMetadataEditScriptError::InvalidSubmap {
        line,
        value: value.into(),
    })
}

fn fixed_bytes<const N: usize>(
    line: usize,
    value: &str,
) -> Result<[u8; N], OverworldMetadataEditScriptError> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(OverworldMetadataEditScriptError::InvalidHex {
            line,
            value: value.into(),
        });
    }
    let mut bytes = [0; N];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| {
            OverworldMetadataEditScriptError::InvalidHex {
                line,
                value: value.into(),
            }
        })?;
    }
    Ok(bytes)
}

fn hex<T>(line: usize, value: &str) -> Result<T, OverworldMetadataEditScriptError>
where
    T: TryFrom<u64>,
{
    let normalized = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(normalized, 16)
        .ok()
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| OverworldMetadataEditScriptError::InvalidNumber {
            line,
            value: value.into(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_metadata_domain_with_unknown_bytes() {
        let edits = parse("LMOMEDT1\nname upsert 105 80 000102030405060708090a0b0c0d0e0f101112\nstart upsert 0 1234 5678 6 a0\nsettings upsert 6 1 2 3 4 8123 0506070809\nname remove 106\nstart remove 1\nsettings remove 5\n").unwrap();
        assert_eq!(edits.len(), 6);
        let MetadataEdit::UpsertLevelName(name) = &edits[0] else {
            panic!()
        };
        assert_eq!(name.tiles[18], 0x12);
    }

    #[test]
    fn rejects_wrong_lengths_submaps_versions_and_commands() {
        assert!(parse("OLD\n").is_err());
        assert!(parse("LMOMEDT1\nname upsert 1 0 00\n").is_err());
        assert!(parse("LMOMEDT1\nstart upsert 0 0 0 7 0\n").is_err());
        assert!(parse("LMOMEDT1\nunknown x\n").is_err());
    }
}
