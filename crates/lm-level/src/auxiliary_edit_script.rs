use crate::{
    Entrance, EntranceKind, LevelAuxiliaryEdit, Map16OverrideEdit, Map16Tile, ScreenExit,
    SecondaryExit, SequenceEdit, Subtile,
};
use std::fmt;

pub const AUXILIARY_EDIT_SCRIPT_MAGIC: &str = "LMAUXED1";
pub const MAX_AUXILIARY_EDIT_SCRIPT_BYTES: usize = 1024 * 1024;
pub const MAX_AUXILIARY_EDIT_COMMANDS: usize = 65_536;
pub const MAX_AUXILIARY_EDIT_LINE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuxiliaryEditScriptError(String);

impl fmt::Display for AuxiliaryEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AuxiliaryEditScriptError {}

/// Parses one bounded `LMAUXED1` script into an ordered atomic edit batch.
///
/// # Errors
///
/// Rejects excessive input, incorrect magic, excessive lines/commands, unknown operations,
/// incorrect arity, malformed numbers, and values outside their destination field type.
pub fn parse_auxiliary_edit_script(
    text: &str,
) -> Result<Vec<LevelAuxiliaryEdit>, AuxiliaryEditScriptError> {
    if text.len() > MAX_AUXILIARY_EDIT_SCRIPT_BYTES {
        return Err(error(
            "level auxiliary edit script exceeds its bounded file limit",
        ));
    }
    let mut lines = text.lines();
    if lines.next() != Some(AUXILIARY_EDIT_SCRIPT_MAGIC) {
        return Err(error("level auxiliary edit script has the wrong magic"));
    }
    let mut edits = Vec::new();
    for (line_index, line) in lines.enumerate() {
        if line.len() > MAX_AUXILIARY_EDIT_LINE_BYTES {
            return Err(error(format!(
                "level auxiliary edit line {} is too long",
                line_index + 2
            )));
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.is_empty() {
            continue;
        }
        if edits.len() == MAX_AUXILIARY_EDIT_COMMANDS {
            return Err(error("level auxiliary edit script has too many commands"));
        }
        edits.push(
            parse_command(&fields)
                .map_err(|message| error(format!("line {}: {message}", line_index + 2)))?,
        );
    }
    Ok(edits)
}

fn parse_command(fields: &[&str]) -> Result<LevelAuxiliaryEdit, String> {
    let operation = fields.first().ok_or("empty command")?;
    if operation.starts_with("entrance-") {
        return parse_entrance(fields);
    }
    if operation.starts_with("screen-exit-") {
        return parse_screen_exit(fields);
    }
    if operation.starts_with("secondary-exit-") {
        return parse_secondary_exit(fields);
    }
    if operation.starts_with("map16-") {
        return parse_map16_override(fields);
    }
    Err("unknown command or wrong arity".into())
}

fn parse_entrance(fields: &[&str]) -> Result<LevelAuxiliaryEdit, String> {
    match fields {
        [operation, index, kind, x, y, screen, action, flags]
            if matches!(*operation, "entrance-insert" | "entrance-replace") =>
        {
            Ok(LevelAuxiliaryEdit::Entrance(sequence_value(
                operation,
                number(index)?,
                Entrance {
                    kind: parse_kind(kind)?,
                    x: number(x)?,
                    y: number(y)?,
                    screen: number(screen)?,
                    action: number(action)?,
                    raw_flags: number(flags)?,
                },
            )))
        }
        ["entrance-remove", index] => Ok(LevelAuxiliaryEdit::Entrance(SequenceEdit::Remove {
            index: number(index)?,
        })),
        ["entrance-move", from, before] => {
            Ok(LevelAuxiliaryEdit::Entrance(SequenceEdit::MoveBefore {
                from: number(from)?,
                before: number(before)?,
            }))
        }
        _ => Err("unknown entrance command or wrong arity".into()),
    }
}

fn parse_screen_exit(fields: &[&str]) -> Result<LevelAuxiliaryEdit, String> {
    match fields {
        [operation, index, encoded]
            if matches!(*operation, "screen-exit-insert" | "screen-exit-replace") =>
        {
            Ok(LevelAuxiliaryEdit::ScreenExit(sequence_value(
                operation,
                number(index)?,
                ScreenExit {
                    encoded: number(encoded)?,
                },
            )))
        }
        ["screen-exit-remove", index] => Ok(LevelAuxiliaryEdit::ScreenExit(SequenceEdit::Remove {
            index: number(index)?,
        })),
        ["screen-exit-move", from, before] => {
            Ok(LevelAuxiliaryEdit::ScreenExit(SequenceEdit::MoveBefore {
                from: number(from)?,
                before: number(before)?,
            }))
        }
        _ => Err("unknown screen-exit command or wrong arity".into()),
    }
}

fn parse_secondary_exit(fields: &[&str]) -> Result<LevelAuxiliaryEdit, String> {
    match fields {
        [
            operation,
            index,
            destination,
            position,
            screen,
            x,
            y,
            destination_flags,
            x_flags,
            additional,
        ] if matches!(
            *operation,
            "secondary-exit-insert" | "secondary-exit-replace"
        ) =>
        {
            Ok(LevelAuxiliaryEdit::SecondaryExit(sequence_value(
                operation,
                number(index)?,
                SecondaryExit {
                    destination_level: number(destination)?,
                    position_and_method: number(position)?,
                    screen: number(screen)?,
                    x: number(x)?,
                    y: number(y)?,
                    destination_flags: number(destination_flags)?,
                    x_and_overworld_flags: number(x_flags)?,
                    additional_flags: number(additional)?,
                },
            )))
        }
        ["secondary-exit-remove", index] => {
            Ok(LevelAuxiliaryEdit::SecondaryExit(SequenceEdit::Remove {
                index: number(index)?,
            }))
        }
        ["secondary-exit-move", from, before] => Ok(LevelAuxiliaryEdit::SecondaryExit(
            SequenceEdit::MoveBefore {
                from: number(from)?,
                before: number(before)?,
            },
        )),
        _ => Err("unknown secondary-exit command or wrong arity".into()),
    }
}

fn parse_map16_override(fields: &[&str]) -> Result<LevelAuxiliaryEdit, String> {
    match fields {
        [
            "map16-upsert",
            index,
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            acts_like,
        ] => Ok(LevelAuxiliaryEdit::Map16Override(
            Map16OverrideEdit::Upsert {
                index: number(index)?,
                tile: Map16Tile {
                    top_left: Subtile(number(top_left)?),
                    top_right: Subtile(number(top_right)?),
                    bottom_left: Subtile(number(bottom_left)?),
                    bottom_right: Subtile(number(bottom_right)?),
                    acts_like: number(acts_like)?,
                },
            },
        )),
        ["map16-remove", index] => Ok(LevelAuxiliaryEdit::Map16Override(
            Map16OverrideEdit::Remove {
                index: number(index)?,
            },
        )),
        _ => Err("unknown Map16 override command or wrong arity".into()),
    }
}

fn sequence_value<T>(operation: &str, index: usize, value: T) -> SequenceEdit<T> {
    if operation.ends_with("insert") {
        SequenceEdit::Insert { index, value }
    } else {
        SequenceEdit::Replace { index, value }
    }
}

fn parse_kind(value: &str) -> Result<EntranceKind, String> {
    match value {
        "main" => Ok(EntranceKind::Main),
        "midway" => Ok(EntranceKind::Midway),
        "secondary" => Ok(EntranceKind::Secondary),
        _ => Err("entrance kind must be main, midway, or secondary".into()),
    }
}

fn number<T>(value: &str) -> Result<T, String>
where
    T: TryFrom<u64>,
{
    let parsed = value
        .strip_prefix("0x")
        .map_or_else(|| value.parse::<u64>(), |hex| u64::from_str_radix(hex, 16))
        .map_err(|_| format!("invalid number {value:?}"))?;
    T::try_from(parsed).map_err(|_| "number is out of range".into())
}

fn error(message: impl Into<String>) -> AuxiliaryEditScriptError {
    AuxiliaryEditScriptError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_domain_and_numeric_form() {
        let edits = parse_auxiliary_edit_script("LMAUXED1\nentrance-insert 0 secondary 1 2 3 4 0x500\nscreen-exit-insert 0 0x1234\nsecondary-exit-insert 0 0x105 2 3 4 5 0x20 0x80 7\nmap16-upsert 0x20 1 2 3 4 5\n").unwrap();
        assert_eq!(edits.len(), 4);
    }

    #[test]
    fn reports_magic_line_arity_number_and_range_errors() {
        assert!(parse_auxiliary_edit_script("wrong\n").is_err());
        assert!(parse_auxiliary_edit_script("LMAUXED1\nbad\n").is_err());
        assert!(parse_auxiliary_edit_script("LMAUXED1\nentrance-remove\n").is_err());
        assert!(parse_auxiliary_edit_script("LMAUXED1\nentrance-remove nope\n").is_err());
        assert!(
            parse_auxiliary_edit_script("LMAUXED1\nscreen-exit-insert 0 0x100000000\n").is_err()
        );
    }

    #[test]
    fn enforces_all_bounds() {
        assert!(
            parse_auxiliary_edit_script(&"x".repeat(MAX_AUXILIARY_EDIT_SCRIPT_BYTES + 1)).is_err()
        );
        let long_line = format!(
            "LMAUXED1\n{}\n",
            "x".repeat(MAX_AUXILIARY_EDIT_LINE_BYTES + 1)
        );
        assert!(parse_auxiliary_edit_script(&long_line).is_err());
        let commands = format!(
            "LMAUXED1\n{}",
            "entrance-remove 0\n".repeat(MAX_AUXILIARY_EDIT_COMMANDS + 1)
        );
        assert!(parse_auxiliary_edit_script(&commands).is_err());
    }
}
