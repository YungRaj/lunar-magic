use lm_app::EntityAppearanceDocumentEdit;
use lm_level::{AppearanceSource, EntityAppearanceRecord};
use std::fmt;

const MAGIC: &str = "LMENTED1";
pub const MAX_SCRIPT_LEN: usize = 4 * 1024 * 1024;
const MAX_LINE_LEN: usize = 512;
const MAX_COMMANDS: usize = 0x1_0000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntityAppearanceEditScriptError {
    TooLarge(usize),
    MissingMagic,
    LineTooLong { line: usize, bytes: usize },
    TooManyCommands,
    WrongArity { line: usize },
    UnknownCommand { line: usize, command: String },
    InvalidSource { line: usize, source: String },
    InvalidNumber { line: usize, value: String },
    InvalidBoolean { line: usize, value: String },
}

impl fmt::Display for EntityAppearanceEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid entity appearance edit script: {self:?}")
    }
}

impl std::error::Error for EntityAppearanceEditScriptError {}

pub fn parse(
    input: &str,
) -> Result<Vec<EntityAppearanceDocumentEdit>, EntityAppearanceEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(EntityAppearanceEditScriptError::TooLarge(input.len()));
    }
    let mut lines = input.lines();
    if lines.next() != Some(MAGIC) {
        return Err(EntityAppearanceEditScriptError::MissingMagic);
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if raw.len() > MAX_LINE_LEN {
            return Err(EntityAppearanceEditScriptError::LineTooLong {
                line,
                bytes: raw.len(),
            });
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        if edits.len() == MAX_COMMANDS {
            return Err(EntityAppearanceEditScriptError::TooManyCommands);
        }
        edits.push(parse_command(line, content)?);
    }
    Ok(edits)
}

fn parse_command(
    line: usize,
    content: &str,
) -> Result<EntityAppearanceDocumentEdit, EntityAppearanceEditScriptError> {
    let fields: Vec<_> = content.split_ascii_whitespace().collect();
    match fields.as_slice() {
        [
            operation @ ("insert" | "replace"),
            index,
            source,
            source_id,
            tile,
            palette,
            x,
            y,
            x_flip,
            y_flip,
        ] => {
            let value = record(
                line,
                RecordFields {
                    source,
                    source_id,
                    tile,
                    palette,
                    x,
                    y,
                    x_flip,
                    y_flip,
                },
            )?;
            let index = decimal(line, index)?;
            Ok(if *operation == "insert" {
                EntityAppearanceDocumentEdit::Insert { index, value }
            } else {
                EntityAppearanceDocumentEdit::Replace { index, value }
            })
        }
        ["remove", index] => Ok(EntityAppearanceDocumentEdit::Remove {
            index: decimal(line, index)?,
        }),
        ["move", from, before] => Ok(EntityAppearanceDocumentEdit::MoveBefore {
            from: decimal(line, from)?,
            before: decimal(line, before)?,
        }),
        [command, ..] if !matches!(*command, "insert" | "replace" | "remove" | "move") => {
            Err(EntityAppearanceEditScriptError::UnknownCommand {
                line,
                command: (*command).to_owned(),
            })
        }
        _ => Err(EntityAppearanceEditScriptError::WrongArity { line }),
    }
}

#[derive(Clone, Copy)]
struct RecordFields<'a> {
    source: &'a str,
    source_id: &'a str,
    tile: &'a str,
    palette: &'a str,
    x: &'a str,
    y: &'a str,
    x_flip: &'a str,
    y_flip: &'a str,
}

fn record(
    line: usize,
    fields: RecordFields<'_>,
) -> Result<EntityAppearanceRecord, EntityAppearanceEditScriptError> {
    let id = hex::<u32>(line, fields.source_id)?;
    let source = match fields.source {
        "layer1" => AppearanceSource::Layer1Object(id),
        "layer2" => AppearanceSource::Layer2Object(id),
        "sprite" => AppearanceSource::Sprite(id),
        _ => {
            return Err(EntityAppearanceEditScriptError::InvalidSource {
                line,
                source: fields.source.to_owned(),
            });
        }
    };
    Ok(EntityAppearanceRecord {
        source,
        tile_index: hex(line, fields.tile)?,
        palette_index: decimal(line, fields.palette)?,
        x: signed(line, fields.x)?,
        y: signed(line, fields.y)?,
        x_flip: boolean(line, fields.x_flip)?,
        y_flip: boolean(line, fields.y_flip)?,
    })
}

fn decimal<T>(line: usize, value: &str) -> Result<T, EntityAppearanceEditScriptError>
where
    T: std::str::FromStr,
{
    value.parse().map_err(|_| invalid_number(line, value))
}

fn signed(line: usize, value: &str) -> Result<i32, EntityAppearanceEditScriptError> {
    decimal(line, value)
}

fn hex<T>(line: usize, value: &str) -> Result<T, EntityAppearanceEditScriptError>
where
    T: TryFrom<u64>,
{
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let parsed = u64::from_str_radix(value, 16).map_err(|_| invalid_number(line, value))?;
    T::try_from(parsed).map_err(|_| invalid_number(line, value))
}

fn boolean(line: usize, value: &str) -> Result<bool, EntityAppearanceEditScriptError> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(EntityAppearanceEditScriptError::InvalidBoolean {
            line,
            value: value.to_owned(),
        }),
    }
}

fn invalid_number(line: usize, value: &str) -> EntityAppearanceEditScriptError {
    EntityAppearanceEditScriptError::InvalidNumber {
        line,
        value: value.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_edit_and_source_kind() {
        let edits = parse("LMENTED1\ninsert 0 layer1 10 20 3 -4 5 1 0\nreplace 0 layer2 11 21 4 6 -7 0 1\ninsert 1 sprite 12 22 5 8 9 0 0\nmove 1 0\nremove 2\n").unwrap();
        assert_eq!(edits.len(), 5);
        assert!(matches!(
            &edits[0],
            EntityAppearanceDocumentEdit::Insert {
                value: EntityAppearanceRecord {
                    source: AppearanceSource::Layer1Object(0x10),
                    x: -4,
                    x_flip: true,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            &edits[1],
            EntityAppearanceDocumentEdit::Replace {
                value: EntityAppearanceRecord {
                    source: AppearanceSource::Layer2Object(0x11),
                    y: -7,
                    y_flip: true,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            &edits[2],
            EntityAppearanceDocumentEdit::Insert {
                value: EntityAppearanceRecord {
                    source: AppearanceSource::Sprite(0x12),
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn malformed_framing_arity_source_numbers_and_booleans_fail() {
        for text in [
            "bad\n",
            "LMENTED1\nunknown 0\n",
            "LMENTED1\ninsert 0 bad 1 2 3 4 5 0 0\n",
            "LMENTED1\ninsert 0 sprite nope 2 3 4 5 0 0\n",
            "LMENTED1\ninsert 0 sprite 1 2 3 4 5 yes 0\n",
            "LMENTED1\nremove\n",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
    }
}
