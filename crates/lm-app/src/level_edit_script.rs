//! Strict, bounded terminal scripts for the complete native level-controller edit surface.

use lm_app::NativeLevelEdit;
use lm_level::{
    LegacyHeaderEdit, ObjectCoordinateNibbles, ObjectEdit, ObjectRecord, SpriteRecord, SpriteToken,
};
use std::fmt;

const MAGIC: &str = "LMLEDIT1";
pub const MAX_SCRIPT_LEN: usize = 64 * 1024;
const MAX_LINE_LEN: usize = 4096;
const MAX_LINES: usize = 8192;
const MAX_COMMANDS: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelEditScriptError {
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
    InvalidField {
        line: usize,
        field: String,
    },
    InvalidNumber {
        line: usize,
        value: String,
    },
    InvalidHexBytes {
        line: usize,
    },
    InvalidObjectRecord {
        line: usize,
    },
    InvalidSpriteKind {
        line: usize,
        kind: String,
    },
}

impl fmt::Display for LevelEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native level-edit script: {self:?}")
    }
}

impl std::error::Error for LevelEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<NativeLevelEdit>, LevelEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(LevelEditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines.next().ok_or(LevelEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(LevelEditScriptError::UnsupportedVersion(magic.into()));
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if line > MAX_LINES {
            return Err(LevelEditScriptError::TooManyLines { maximum: MAX_LINES });
        }
        if raw.len() > MAX_LINE_LEN {
            return Err(LevelEditScriptError::LineTooLong {
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
            return Err(LevelEditScriptError::TooManyCommands {
                maximum: MAX_COMMANDS,
            });
        }
        edits.push(parse_command(line, content)?);
    }
    Ok(edits)
}

fn parse_command(line: usize, content: &str) -> Result<NativeLevelEdit, LevelEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["header", field, value] => parse_header(line, field, value),
        ["object", "insert", index, bytes] => {
            Ok(NativeLevelEdit::Objects(vec![ObjectEdit::Insert {
                index: decimal(line, index)?,
                record: object_record(line, bytes)?,
            }]))
        }
        ["object", "replace", index, bytes] => {
            Ok(NativeLevelEdit::Objects(vec![ObjectEdit::Replace {
                index: decimal(line, index)?,
                record: object_record(line, bytes)?,
            }]))
        }
        ["object", "remove", index] => Ok(NativeLevelEdit::Objects(vec![ObjectEdit::Remove {
            index: decimal(line, index)?,
        }])),
        ["object", "move", from, before] => {
            Ok(NativeLevelEdit::Objects(vec![ObjectEdit::MoveBefore {
                from: decimal(line, from)?,
                before: decimal(line, before)?,
            }]))
        }
        ["object", "command", index, command_id] => {
            Ok(NativeLevelEdit::Objects(vec![ObjectEdit::SetCommandId {
                index: decimal(line, index)?,
                command_id: hex_byte(line, command_id)?,
            }]))
        }
        ["object", "parameter", index, parameter] => {
            Ok(NativeLevelEdit::Objects(vec![ObjectEdit::SetParameter {
                index: decimal(line, index)?,
                parameter: hex_byte(line, parameter)?,
            }]))
        }
        ["object", "coordinates", index, first, second] => Ok(NativeLevelEdit::Objects(vec![
            ObjectEdit::SetCoordinateNibbles {
                index: decimal(line, index)?,
                coordinates: ObjectCoordinateNibbles {
                    first: hex_byte(line, first)?,
                    second: hex_byte(line, second)?,
                },
            },
        ])),
        ["object", "screen-advance", index, advances] => Ok(NativeLevelEdit::Objects(vec![
            ObjectEdit::SetAdvancesScreen {
                index: decimal(line, index)?,
                advances: boolean(line, advances)?,
            },
        ])),
        ["object", "screen-jump-target", index, packed_target] => {
            Ok(NativeLevelEdit::Objects(vec![
                ObjectEdit::SetScreenJumpTarget {
                    index: decimal(line, index)?,
                    packed_target: hex_word(line, packed_target)?,
                },
            ]))
        }
        ["object", "relocate", index, screen, first, second] => Ok(NativeLevelEdit::Objects(vec![
            ObjectEdit::RelocateOrdinary {
                index: decimal(line, index)?,
                screen: hex_word(line, screen)?,
                coordinates: ObjectCoordinateNibbles {
                    first: hex_byte(line, first)?,
                    second: hex_byte(line, second)?,
                },
            },
        ])),
        ["sprite-header", value] => Ok(NativeLevelEdit::SetSpriteHeader(hex_byte(line, value)?)),
        ["sprite", command @ ..] => parse_sprite_command(line, command),
        [command, ..] if !matches!(*command, "header" | "object" | "sprite-header" | "sprite") => {
            Err(LevelEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(LevelEditScriptError::WrongArity { line }),
    }
}

fn parse_sprite_command(
    line: usize,
    command: &[&str],
) -> Result<NativeLevelEdit, LevelEditScriptError> {
    match command {
        ["insert", index, kind, value] => Ok(NativeLevelEdit::InsertSprite {
            index: decimal(line, index)?,
            token: sprite_token(line, kind, value)?,
        }),
        ["replace", index, kind, value] => Ok(NativeLevelEdit::ReplaceSprite {
            index: decimal(line, index)?,
            token: sprite_token(line, kind, value)?,
        }),
        ["remove", index] => Ok(NativeLevelEdit::RemoveSprite {
            index: decimal(line, index)?,
        }),
        ["move", from, before] => Ok(NativeLevelEdit::MoveSpriteBefore {
            from: decimal(line, from)?,
            before: decimal(line, before)?,
        }),
        ["sort-screen", selected] => Ok(NativeLevelEdit::SortLegacySpritesByScreen {
            selected: decimal(line, selected)?,
        }),
        ["relocate-expanded", selected, screen, x, y] => {
            Ok(NativeLevelEdit::RelocateExpandedSprite {
                selected: decimal(line, selected)?,
                screen: hex_byte(line, screen)?,
                x: hex_byte(line, x)?,
                y: hex_word(line, y)?,
            })
        }
        _ => Err(LevelEditScriptError::WrongArity { line }),
    }
}

fn parse_header(
    line: usize,
    field: &str,
    value: &str,
) -> Result<NativeLevelEdit, LevelEditScriptError> {
    let value = hex_byte(line, value)?;
    let edit = match field {
        "background-palette" => LegacyHeaderEdit::BackgroundPalette(value),
        "mode" => LegacyHeaderEdit::LevelMode(value),
        "background-color" => LegacyHeaderEdit::BackgroundColor(value),
        "sprite-tileset" => LegacyHeaderEdit::SpriteTileset(value),
        "music" => LegacyHeaderEdit::DefaultMusicSelector(value),
        "sprite-palette" => LegacyHeaderEdit::SpritePalette(value),
        "foreground-palette" => LegacyHeaderEdit::ForegroundPalette(value),
        "object-tileset" => LegacyHeaderEdit::ObjectTileset(value),
        _ => {
            return Err(LevelEditScriptError::InvalidField {
                line,
                field: field.into(),
            });
        }
    };
    Ok(NativeLevelEdit::LegacyHeader(edit))
}

fn object_record(line: usize, value: &str) -> Result<ObjectRecord, LevelEditScriptError> {
    ObjectRecord::new(hex_bytes(line, value)?)
        .map_err(|_| LevelEditScriptError::InvalidObjectRecord { line })
}

fn sprite_token(line: usize, kind: &str, value: &str) -> Result<SpriteToken, LevelEditScriptError> {
    match kind {
        "record" => Ok(SpriteToken::Record(SpriteRecord {
            encoded: hex_bytes(line, value)?,
        })),
        "screen" => {
            let value = hex_byte(line, value)?;
            if value > 0x7f {
                return Err(LevelEditScriptError::InvalidNumber {
                    line,
                    value: value.to_string(),
                });
            }
            Ok(SpriteToken::Screen(value))
        }
        "control" => {
            let value = hex_byte(line, value)?;
            if value < 0x80 || value == 0xff {
                return Err(LevelEditScriptError::InvalidNumber {
                    line,
                    value: value.to_string(),
                });
            }
            Ok(SpriteToken::Control(value))
        }
        _ => Err(LevelEditScriptError::InvalidSpriteKind {
            line,
            kind: kind.into(),
        }),
    }
}

fn decimal(line: usize, value: &str) -> Result<usize, LevelEditScriptError> {
    value
        .parse()
        .map_err(|_| LevelEditScriptError::InvalidNumber {
            line,
            value: value.into(),
        })
}

fn hex_byte(line: usize, value: &str) -> Result<u8, LevelEditScriptError> {
    u8::from_str_radix(strip_hex_prefix(value), 16).map_err(|_| {
        LevelEditScriptError::InvalidNumber {
            line,
            value: value.into(),
        }
    })
}

fn hex_word(line: usize, value: &str) -> Result<u16, LevelEditScriptError> {
    u16::from_str_radix(strip_hex_prefix(value), 16).map_err(|_| {
        LevelEditScriptError::InvalidNumber {
            line,
            value: value.into(),
        }
    })
}

fn boolean(line: usize, value: &str) -> Result<bool, LevelEditScriptError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(LevelEditScriptError::InvalidNumber {
            line,
            value: value.into(),
        }),
    }
}

fn hex_bytes(line: usize, value: &str) -> Result<Vec<u8>, LevelEditScriptError> {
    let value = strip_hex_prefix(value);
    if value.is_empty() || value.len() % 2 != 0 || value.len() > 512 {
        return Err(LevelEditScriptError::InvalidHexBytes { line });
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair)
                .map_err(|_| LevelEditScriptError::InvalidHexBytes { line })?;
            u8::from_str_radix(text, 16).map_err(|_| LevelEditScriptError::InvalidHexBytes { line })
        })
        .collect()
}

fn strip_hex_prefix(value: &str) -> &str {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_native_controller_edit_shape() {
        let script = "LMLEDIT1\n\
            header mode 03\n\
            object insert 0 010001\n\
            object replace 0 020001\n\
            object move 0 1\n\
            object remove 0\n\
            object command 0 22\n\
            object parameter 0 7f\n\
            object coordinates 0 0e 0d\n\
            object screen-advance 0 true\n\
            object screen-jump-target 0 0f1e\n\
            object relocate 0 001f 0c 0b\n\
            sprite-header 10\n\
            sprite insert 0 record 000001\n\
            sprite insert 1 screen 12\n\
            sprite insert 2 control 90\n\
            sprite replace 0 record 040005\n\
            sprite move 2 0\n\
            sprite sort-screen 0\n\
            sprite relocate-expanded 0 04 03 00A7\n\
            sprite remove 1\n";
        let edits = parse(script).unwrap();
        assert_eq!(edits.len(), 20);
        assert!(matches!(edits[0], NativeLevelEdit::LegacyHeader(_)));
        assert!(matches!(edits[4], NativeLevelEdit::Objects(_)));
        assert!(matches!(
            edits[5],
            NativeLevelEdit::Objects(ref edits)
                if edits == &[ObjectEdit::SetCommandId { index: 0, command_id: 0x22 }]
        ));
        assert!(matches!(
            edits[6],
            NativeLevelEdit::Objects(ref edits)
                if edits == &[ObjectEdit::SetParameter { index: 0, parameter: 0x7f }]
        ));
        assert!(matches!(
            edits[7],
            NativeLevelEdit::Objects(ref edits)
                if edits == &[ObjectEdit::SetCoordinateNibbles {
                    index: 0,
                    coordinates: ObjectCoordinateNibbles { first: 0x0e, second: 0x0d }
                }]
        ));
        assert!(matches!(
            edits[8],
            NativeLevelEdit::Objects(ref edits)
                if edits == &[ObjectEdit::SetAdvancesScreen { index: 0, advances: true }]
        ));
        assert!(matches!(
            edits[9],
            NativeLevelEdit::Objects(ref edits)
                if edits == &[ObjectEdit::SetScreenJumpTarget { index: 0, packed_target: 0x0f1e }]
        ));
        assert!(matches!(
            edits[10],
            NativeLevelEdit::Objects(ref edits)
                if edits == &[ObjectEdit::RelocateOrdinary {
                    index: 0,
                    screen: 0x1f,
                    coordinates: ObjectCoordinateNibbles { first: 0x0c, second: 0x0b }
                }]
        ));
        assert!(matches!(edits[11], NativeLevelEdit::SetSpriteHeader(0x10)));
        assert!(matches!(
            edits[17],
            NativeLevelEdit::SortLegacySpritesByScreen { selected: 0 }
        ));
        assert!(matches!(
            edits[18],
            NativeLevelEdit::RelocateExpandedSprite {
                selected: 0,
                screen: 4,
                x: 3,
                y: 0xa7
            }
        ));
        assert!(matches!(
            edits[14],
            NativeLevelEdit::InsertSprite {
                token: SpriteToken::Control(0x90),
                ..
            }
        ));
    }

    #[test]
    fn rejects_bad_framing_records_tokens_and_limits() {
        for script in [
            "wrong\n",
            "LMLEDIT1\nobject insert 0 01\n",
            "LMLEDIT1\nobject command 0 xyz\n",
            "LMLEDIT1\nobject screen-advance 0 yes\n",
            "LMLEDIT1\nsprite insert 0 screen 80\n",
            "LMLEDIT1\nsprite insert 0 control 7f\n",
            "LMLEDIT1\nsprite insert 0 mystery 00\n",
            "LMLEDIT1\nunknown x\n",
        ] {
            assert!(parse(script).is_err(), "accepted {script:?}");
        }
        let oversized = "x".repeat(MAX_SCRIPT_LEN + 1);
        assert!(matches!(
            parse(&oversized),
            Err(LevelEditScriptError::TooLarge { .. })
        ));
        let too_many_commands = format!("LMLEDIT1\n{}", "header mode 0\n".repeat(MAX_COMMANDS + 1));
        assert!(matches!(
            parse(&too_many_commands),
            Err(LevelEditScriptError::TooManyCommands { .. })
        ));
        let long_line = format!("LMLEDIT1\n{}", "x".repeat(MAX_LINE_LEN + 1));
        assert!(matches!(
            parse(&long_line),
            Err(LevelEditScriptError::LineTooLong { .. })
        ));
    }

    #[test]
    fn parses_recovered_tileset_and_music_header_fields() {
        let edits = parse(
            "LMLEDIT1\nheader sprite-tileset 0f\nheader music 06\nheader object-tileset 0a\n",
        )
        .unwrap();
        assert_eq!(
            edits,
            [
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpriteTileset(0x0f)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::DefaultMusicSelector(6)),
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ObjectTileset(0x0a)),
            ]
        );
    }
}
