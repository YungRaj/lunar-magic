//! Bounded scripts for compact native `ExAnimation` edits.

use lm_app::ExAnimationControllerEdit;
use lm_graphics::{ExAnimationFrame, ExAnimationFrameEdit, ExAnimationRecord};
use std::fmt;

const MAGIC: &str = "LMEXAED1";
pub const MAX_SCRIPT_LEN: usize = 128 * 1024;
const MAX_LINE_LEN: usize = 8192;
const MAX_LINES: usize = 8192;
const MAX_COMMANDS: usize = 2048;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationEditScriptError {
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
    InvalidBoolean {
        line: usize,
        value: String,
    },
    InvalidWidth {
        line: usize,
        value: String,
    },
    Record {
        line: usize,
        message: String,
    },
}

impl fmt::Display for ExAnimationEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid native ExAnimation edit script: {self:?}"
        )
    }
}

impl std::error::Error for ExAnimationEditScriptError {}

pub fn parse(input: &str) -> Result<Vec<ExAnimationControllerEdit>, ExAnimationEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(ExAnimationEditScriptError::TooLarge {
            actual: input.len(),
            maximum: MAX_SCRIPT_LEN,
        });
    }
    let mut lines = input.lines();
    let magic = lines
        .next()
        .ok_or(ExAnimationEditScriptError::MissingMagic)?;
    if magic != MAGIC {
        return Err(ExAnimationEditScriptError::UnsupportedVersion(magic.into()));
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        validate_line(line, raw)?;
        let content = raw.split('#').next().unwrap_or_default().trim();
        if !content.is_empty() {
            edits.push(parse_command(line, content)?);
            if edits.len() > MAX_COMMANDS {
                return Err(ExAnimationEditScriptError::TooManyCommands {
                    maximum: MAX_COMMANDS,
                });
            }
        }
    }
    Ok(edits)
}

fn validate_line(line: usize, raw: &str) -> Result<(), ExAnimationEditScriptError> {
    if line > MAX_LINES {
        return Err(ExAnimationEditScriptError::TooManyLines { maximum: MAX_LINES });
    }
    if raw.len() > MAX_LINE_LEN {
        return Err(ExAnimationEditScriptError::LineTooLong {
            line,
            actual: raw.len(),
            maximum: MAX_LINE_LEN,
        });
    }
    Ok(())
}

pub(crate) fn parse_command(
    line: usize,
    content: &str,
) -> Result<ExAnimationControllerEdit, ExAnimationEditScriptError> {
    let words: Vec<_> = content.split_whitespace().collect();
    match words.as_slice() {
        ["setting", value] => Ok(ExAnimationControllerEdit::SetSetting(hex_u8(line, value)?)),
        ["header", value] => Ok(ExAnimationControllerEdit::SetHeaderValue(hex_u32(
            line, value,
        )?)),
        ["trigger", trigger, "clear"] => Ok(ExAnimationControllerEdit::SetTrigger {
            trigger: hex_usize(line, trigger)?,
            value: None,
        }),
        ["trigger", trigger, value] => Ok(ExAnimationControllerEdit::SetTrigger {
            trigger: hex_usize(line, trigger)?,
            value: Some(hex_u8(line, value)?),
        }),
        ["record", "insert", index, fields @ ..] => Ok(ExAnimationControllerEdit::InsertRecord {
            index: hex_usize(line, index)?,
            record: parse_record(line, fields)?,
        }),
        ["record", "replace", index, fields @ ..] => Ok(ExAnimationControllerEdit::ReplaceRecord {
            index: hex_usize(line, index)?,
            record: parse_record(line, fields)?,
        }),
        ["record", "remove", index] => Ok(ExAnimationControllerEdit::RemoveRecord {
            index: hex_usize(line, index)?,
        }),
        ["record", "move", from, before] => Ok(ExAnimationControllerEdit::MoveRecordBefore {
            from: hex_usize(line, from)?,
            before: hex_usize(line, before)?,
        }),
        ["frame", operation, record, values @ ..] => parse_frame(line, operation, record, values),
        [command, ..]
            if !matches!(
                *command,
                "setting" | "header" | "trigger" | "record" | "frame"
            ) =>
        {
            Err(ExAnimationEditScriptError::UnknownCommand {
                line,
                command: (*command).into(),
            })
        }
        _ => Err(ExAnimationEditScriptError::WrongArity { line }),
    }
}

fn parse_record(
    line: usize,
    fields: &[&str],
) -> Result<ExAnimationRecord, ExAnimationEditScriptError> {
    let [kind, size_mode, destination, flag, width, frames @ ..] = fields else {
        return Err(ExAnimationEditScriptError::WrongArity { line });
    };
    let double = parse_width(line, width)?;
    let words = frames
        .iter()
        .map(|word| hex_u16(line, word))
        .collect::<Result<Vec<_>, _>>()?;
    let words_per_frame = if double { 2 } else { 1 };
    if words.is_empty() || words.len() % words_per_frame != 0 || words.len() / words_per_frame > 256
    {
        return Err(ExAnimationEditScriptError::WrongArity { line });
    }
    let count = u8::try_from(words.len() / words_per_frame - 1)
        .map_err(|_| ExAnimationEditScriptError::WrongArity { line })?;
    let bytes = words
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    ExAnimationRecord::new(
        hex_u8(line, kind)?,
        count,
        hex_u8(line, size_mode)?,
        hex_u16(line, destination)?,
        parse_bool(line, flag)?,
        &bytes,
        double,
    )
    .map_err(|error| ExAnimationEditScriptError::Record {
        line,
        message: error.to_string(),
    })
}

fn parse_frame(
    line: usize,
    operation: &str,
    record: &str,
    values: &[&str],
) -> Result<ExAnimationControllerEdit, ExAnimationEditScriptError> {
    let record = hex_usize(line, record)?;
    let edit = match (operation, values) {
        ("insert", [index, words @ ..]) if !words.is_empty() => ExAnimationFrameEdit::Insert {
            index: hex_usize(line, index)?,
            frame: parse_frame_words(line, words)?,
        },
        ("replace", [index, words @ ..]) if !words.is_empty() => ExAnimationFrameEdit::Replace {
            index: hex_usize(line, index)?,
            frame: parse_frame_words(line, words)?,
        },
        ("remove", [index]) => ExAnimationFrameEdit::Remove {
            index: hex_usize(line, index)?,
        },
        ("move", [from, before]) => ExAnimationFrameEdit::MoveBefore {
            from: hex_usize(line, from)?,
            before: hex_usize(line, before)?,
        },
        _ => return Err(ExAnimationEditScriptError::WrongArity { line }),
    };
    Ok(ExAnimationControllerEdit::EditRecordFrames {
        record,
        edits: vec![edit],
    })
}

fn parse_frame_words(
    line: usize,
    words: &[&str],
) -> Result<ExAnimationFrame, ExAnimationEditScriptError> {
    if words.len() > 2 {
        return Err(ExAnimationEditScriptError::WrongArity { line });
    }
    Ok(ExAnimationFrame {
        source_words: words
            .iter()
            .map(|word| hex_u16(line, word))
            .collect::<Result<_, _>>()?,
    })
}

fn parse_width(line: usize, value: &str) -> Result<bool, ExAnimationEditScriptError> {
    match value {
        "single" => Ok(false),
        "double" => Ok(true),
        _ => Err(ExAnimationEditScriptError::InvalidWidth {
            line,
            value: value.into(),
        }),
    }
}

fn parse_bool(line: usize, value: &str) -> Result<bool, ExAnimationEditScriptError> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(ExAnimationEditScriptError::InvalidBoolean {
            line,
            value: value.into(),
        }),
    }
}

fn hex_usize(line: usize, value: &str) -> Result<usize, ExAnimationEditScriptError> {
    parse_hex(line, value)
}
fn hex_u8(line: usize, value: &str) -> Result<u8, ExAnimationEditScriptError> {
    parse_hex(line, value)
}
fn hex_u16(line: usize, value: &str) -> Result<u16, ExAnimationEditScriptError> {
    parse_hex(line, value)
}
fn hex_u32(line: usize, value: &str) -> Result<u32, ExAnimationEditScriptError> {
    parse_hex(line, value)
}

fn parse_hex<T>(line: usize, value: &str) -> Result<T, ExAnimationEditScriptError>
where
    T: TryFrom<u64>,
{
    let normalized = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(normalized, 16)
        .ok()
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| ExAnimationEditScriptError::InvalidNumber {
            line,
            value: value.into(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_collection_and_frame_surface() {
        let edits = parse("LMEXAED1\nsetting 03\nheader deadbeef\ntrigger 02 aa\ntrigger 03 clear\nrecord insert 0 01 00 1234 1 single 0001 0002\nframe replace 0 1 2222\nrecord move 0 1\nrecord remove 0\n").unwrap();
        assert_eq!(edits.len(), 8);
        let ExAnimationControllerEdit::InsertRecord { record, .. } = &edits[4] else {
            panic!()
        };
        assert_eq!(record.frame_count_minus_one(), 1);
        assert_eq!(record.destination(), 0x1234);
    }

    #[test]
    fn rejects_bad_width_limits_and_versions() {
        assert!(parse("OLD\n").is_err());
        assert!(parse("LMEXAED1\nrecord insert 0 1 0 1 0 triple 1\n").is_err());
        assert!(parse(&format!("LMEXAED1\n{}", "x".repeat(MAX_LINE_LEN + 1))).is_err());
    }
}
