use lm_app::OverworldAppearanceDocumentEdit;
use lm_overworld::SpriteAppearancePart;
use std::fmt;

const MAGIC: &str = "LMOWAED1";
pub const MAX_SCRIPT_LEN: usize = 4 * 1024 * 1024;
const MAX_LINE_LEN: usize = 512;
const MAX_COMMANDS: usize = 0x1_0000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldAppearanceEditScriptError {
    TooLarge(usize),
    MissingMagic,
    LineTooLong { line: usize, bytes: usize },
    TooManyCommands,
    WrongArity { line: usize },
    UnknownCommand { line: usize, command: String },
    InvalidNumber { line: usize, value: String },
    InvalidBoolean { line: usize, value: String },
}

impl fmt::Display for OverworldAppearanceEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid overworld appearance edit script: {self:?}"
        )
    }
}

impl std::error::Error for OverworldAppearanceEditScriptError {}

pub fn parse(
    input: &str,
) -> Result<Vec<OverworldAppearanceDocumentEdit>, OverworldAppearanceEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(OverworldAppearanceEditScriptError::TooLarge(input.len()));
    }
    let mut lines = input.lines();
    if lines.next() != Some(MAGIC) {
        return Err(OverworldAppearanceEditScriptError::MissingMagic);
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if raw.len() > MAX_LINE_LEN {
            return Err(OverworldAppearanceEditScriptError::LineTooLong {
                line,
                bytes: raw.len(),
            });
        }
        let content = raw.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        if edits.len() == MAX_COMMANDS {
            return Err(OverworldAppearanceEditScriptError::TooManyCommands);
        }
        edits.push(parse_command(line, content)?);
    }
    Ok(edits)
}

fn parse_command(
    line: usize,
    content: &str,
) -> Result<OverworldAppearanceDocumentEdit, OverworldAppearanceEditScriptError> {
    let fields: Vec<_> = content.split_ascii_whitespace().collect();
    match fields.as_slice() {
        ["definition", "insert", index, id] => {
            Ok(OverworldAppearanceDocumentEdit::InsertDefinition {
                index: decimal(line, index)?,
                sprite_id: hex(line, id)?,
            })
        }
        ["definition", "remove", id] => Ok(OverworldAppearanceDocumentEdit::RemoveDefinition {
            sprite_id: hex(line, id)?,
        }),
        ["definition", "move", id, before] => {
            Ok(OverworldAppearanceDocumentEdit::MoveDefinitionBefore {
                sprite_id: hex(line, id)?,
                before: if *before == "end" {
                    None
                } else {
                    Some(hex(line, before)?)
                },
            })
        }
        [
            "part",
            operation @ ("insert" | "replace"),
            id,
            index,
            tile,
            palette,
            x,
            y,
            x_flip,
            y_flip,
        ] => {
            let sprite_id = hex(line, id)?;
            let index = decimal(line, index)?;
            let value = SpriteAppearancePart {
                tile_index: hex(line, tile)?,
                palette_index: decimal(line, palette)?,
                x_offset: signed(line, x)?,
                y_offset: signed(line, y)?,
                x_flip: boolean(line, x_flip)?,
                y_flip: boolean(line, y_flip)?,
            };
            Ok(if *operation == "insert" {
                OverworldAppearanceDocumentEdit::InsertPart {
                    sprite_id,
                    index,
                    value,
                }
            } else {
                OverworldAppearanceDocumentEdit::ReplacePart {
                    sprite_id,
                    index,
                    value,
                }
            })
        }
        ["part", "remove", id, index] => Ok(OverworldAppearanceDocumentEdit::RemovePart {
            sprite_id: hex(line, id)?,
            index: decimal(line, index)?,
        }),
        ["part", "move", id, index, before] => {
            Ok(OverworldAppearanceDocumentEdit::MovePartBefore {
                sprite_id: hex(line, id)?,
                index: decimal(line, index)?,
                before: if *before == "end" {
                    None
                } else {
                    Some(decimal(line, before)?)
                },
            })
        }
        [command, ..] if !matches!(*command, "definition" | "part") => {
            Err(OverworldAppearanceEditScriptError::UnknownCommand {
                line,
                command: (*command).to_owned(),
            })
        }
        _ => Err(OverworldAppearanceEditScriptError::WrongArity { line }),
    }
}

fn decimal<T: std::str::FromStr>(
    line: usize,
    value: &str,
) -> Result<T, OverworldAppearanceEditScriptError> {
    value.parse().map_err(|_| invalid(line, value))
}
fn signed(line: usize, value: &str) -> Result<i16, OverworldAppearanceEditScriptError> {
    decimal(line, value)
}
fn hex<T: TryFrom<u64>>(line: usize, value: &str) -> Result<T, OverworldAppearanceEditScriptError> {
    let stripped = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let parsed = u64::from_str_radix(stripped, 16).map_err(|_| invalid(line, value))?;
    T::try_from(parsed).map_err(|_| invalid(line, value))
}
fn boolean(line: usize, value: &str) -> Result<bool, OverworldAppearanceEditScriptError> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(OverworldAppearanceEditScriptError::InvalidBoolean {
            line,
            value: value.to_owned(),
        }),
    }
}
fn invalid(line: usize, value: &str) -> OverworldAppearanceEditScriptError {
    OverworldAppearanceEditScriptError::InvalidNumber {
        line,
        value: value.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_definition_and_part_operations() {
        let edits = parse("LMOWAED1\ndefinition insert 0 10\npart insert 10 0 123 4 -8 16 1 0\npart replace 10 0 124 5 9 -10 0 1\ndefinition move 10 end\npart move 10 0 end\npart remove 10 0\ndefinition remove 10\n").unwrap();
        assert_eq!(edits.len(), 7);
        assert!(matches!(
            edits[0],
            OverworldAppearanceDocumentEdit::InsertDefinition {
                index: 0,
                sprite_id: 0x10
            }
        ));
        assert!(matches!(
            edits[1],
            OverworldAppearanceDocumentEdit::InsertPart {
                value: SpriteAppearancePart {
                    x_offset: -8,
                    x_flip: true,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            edits[3],
            OverworldAppearanceDocumentEdit::MoveDefinitionBefore { before: None, .. }
        ));
        assert!(matches!(
            edits[4],
            OverworldAppearanceDocumentEdit::MovePartBefore { before: None, .. }
        ));
    }
    #[test]
    fn malformed_inputs_fail() {
        for text in [
            "bad\n",
            "LMOWAED1\nunknown x\n",
            "LMOWAED1\ndefinition insert no 1\n",
            "LMOWAED1\npart insert 1 0 2 3 4 5 yes 0\n",
            "LMOWAED1\npart remove 1\n",
        ] {
            assert!(parse(text).is_err(), "accepted {text:?}");
        }
    }
}
