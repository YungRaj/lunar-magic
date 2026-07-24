use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_graphics::{
    CompactExAnimationFile, ExAnimationFrame, ExAnimationFrameEdit, edit_exanimation_frames,
};
use std::fmt;
#[cfg(test)]
use std::fs;
use std::path::Path;

const MAX_SCRIPT_BYTES: usize = 1024 * 1024;
const MAX_LINE_BYTES: usize = 256;
const MAX_EDITS: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
enum ScriptError {
    TooLarge(usize),
    InvalidUtf8,
    LineTooLong { line: usize, bytes: usize },
    TooManyEdits(usize),
    InvalidLine { line: usize, text: String },
    InvalidNumber { line: usize, value: String },
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid ExAnimation frame script: {self:?}")
    }
}

impl std::error::Error for ScriptError {}

pub fn execute(
    input: &Path,
    size_modes: &Path,
    maximum_records: usize,
    record_index: usize,
    edits: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    require_distinct(&[input, size_modes, edits, output])?;
    let modes = crate::size_mode_file::read(size_modes)?;
    let mut file = CompactExAnimationFile::decode(
        &read_bounded(input, CompactExAnimationFile::MAX_FILE_LEN)?,
        maximum_records,
        &modes,
    )?;
    let record_count = file.animation.records.len();
    let record = file.animation.records.get(record_index).ok_or_else(|| {
        format!("ExAnimation record {record_index} is outside record count {record_count}")
    })?;
    let double_size = modes[usize::from(record.size_mode())];
    let parsed = parse_script(&read_bounded(edits, MAX_SCRIPT_BYTES)?)?;
    let edited = edit_exanimation_frames(record, double_size, &parsed)?;
    file.animation.records[record_index] = edited;
    write_new(output, file.encode(&modes)?)?;
    println!("edited-exanimation-record: {record_index:#04x}");
    println!("frame-edits: {}", parsed.len());
    println!("output: {}", output.display());
    Ok(())
}

fn parse_script(bytes: &[u8]) -> Result<Vec<ExAnimationFrameEdit>, ScriptError> {
    if bytes.len() > MAX_SCRIPT_BYTES {
        return Err(ScriptError::TooLarge(bytes.len()));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ScriptError::InvalidUtf8)?;
    let mut edits = Vec::new();
    for (line_index, raw) in text.lines().enumerate() {
        let line_number = line_index + 1;
        if raw.len() > MAX_LINE_BYTES {
            return Err(ScriptError::LineTooLong {
                line: line_number,
                bytes: raw.len(),
            });
        }
        let line = raw.split_once('#').map_or(raw, |(prefix, _)| prefix).trim();
        if line.is_empty() {
            continue;
        }
        edits.push(parse_line(line_number, line)?);
        if edits.len() > MAX_EDITS {
            return Err(ScriptError::TooManyEdits(edits.len()));
        }
    }
    Ok(edits)
}

fn parse_line(line: usize, text: &str) -> Result<ExAnimationFrameEdit, ScriptError> {
    let fields = text.split_ascii_whitespace().collect::<Vec<_>>();
    match fields.as_slice() {
        ["insert", index, words] => Ok(ExAnimationFrameEdit::Insert {
            index: parse_number(line, index)?,
            frame: parse_words(line, words)?,
        }),
        ["replace", index, words] => Ok(ExAnimationFrameEdit::Replace {
            index: parse_number(line, index)?,
            frame: parse_words(line, words)?,
        }),
        ["remove", index] => Ok(ExAnimationFrameEdit::Remove {
            index: parse_number(line, index)?,
        }),
        ["move", from, before] => Ok(ExAnimationFrameEdit::MoveBefore {
            from: parse_number(line, from)?,
            before: parse_number(line, before)?,
        }),
        _ => Err(ScriptError::InvalidLine {
            line,
            text: text.into(),
        }),
    }
}

fn parse_words(line: usize, value: &str) -> Result<ExAnimationFrame, ScriptError> {
    let words = value
        .split(',')
        .map(|word| {
            u16::try_from(parse_hex(line, word)?).map_err(|_| ScriptError::InvalidNumber {
                line,
                value: word.into(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !(1..=2).contains(&words.len()) {
        return Err(ScriptError::InvalidLine {
            line,
            text: value.into(),
        });
    }
    Ok(ExAnimationFrame {
        source_words: words,
    })
}

fn parse_number(line: usize, value: &str) -> Result<usize, ScriptError> {
    usize::try_from(parse_hex(line, value)?).map_err(|_| ScriptError::InvalidNumber {
        line,
        value: value.into(),
    })
}

fn parse_hex(line: usize, value: &str) -> Result<u32, ScriptError> {
    let digits = value.strip_prefix("0x").unwrap_or(value);
    u32::from_str_radix(digits, 16).map_err(|_| ScriptError::InvalidNumber {
        line,
        value: value.into(),
    })
}

fn require_distinct(paths: &[&Path]) -> Result<(), Box<dyn std::error::Error>> {
    if paths
        .iter()
        .enumerate()
        .any(|(index, path)| paths[..index].contains(path))
    {
        return Err("ExAnimation frame input, modes, script, and output paths must differ".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{CompactExAnimation, ExAnimationRecord, exanimation_frames};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-exanimation-frames-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn parses_all_operations_comments_and_hex_words() {
        assert_eq!(
            parse_script(b"# edit frames\nreplace 0 1234\ninsert 1 0xabcd\nmove 1 0\nremove 1\n")
                .unwrap(),
            [
                ExAnimationFrameEdit::Replace {
                    index: 0,
                    frame: ExAnimationFrame {
                        source_words: vec![0x1234]
                    }
                },
                ExAnimationFrameEdit::Insert {
                    index: 1,
                    frame: ExAnimationFrame {
                        source_words: vec![0xabcd]
                    }
                },
                ExAnimationFrameEdit::MoveBefore { from: 1, before: 0 },
                ExAnimationFrameEdit::Remove { index: 1 }
            ]
        );
        assert!(parse_script(b"replace nope 1").is_err());
        assert!(parse_script(b"insert 0 1,2,3").is_err());
        assert!(parse_script(&[0xff]).is_err());
    }

    #[test]
    fn edits_lmexan_file_and_failure_does_not_publish() {
        let directory = directory();
        let input = directory.join("input.lmexan");
        let modes = directory.join("modes.bin");
        let script = directory.join("edits.txt");
        let output = directory.join("output.lmexan");
        let size_modes = [false; 256];
        let file = CompactExAnimationFile {
            source_slot: 0x105,
            animation: CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: vec![
                    ExAnimationRecord::new(1, 1, 0, 0x20, false, &[1, 0, 2, 0], false).unwrap(),
                ],
            },
        };
        fs::write(&input, file.encode(&size_modes).unwrap()).unwrap();
        fs::write(&modes, [0; 256]).unwrap();
        fs::write(&script, b"replace 0 1234\ninsert 2 abcd\n").unwrap();
        execute(&input, &modes, 32, 0, &script, &output).unwrap();
        let edited =
            CompactExAnimationFile::decode(&fs::read(&output).unwrap(), 32, &size_modes).unwrap();
        assert_eq!(edited.source_slot, 0x105);
        assert_eq!(
            exanimation_frames(&edited.animation.records[0], false).unwrap(),
            [
                ExAnimationFrame {
                    source_words: vec![0x1234]
                },
                ExAnimationFrame {
                    source_words: vec![2]
                },
                ExAnimationFrame {
                    source_words: vec![0xabcd]
                }
            ]
        );
        let failed = directory.join("failed.lmexan");
        fs::write(&script, b"replace 0 1234\nremove ff\n").unwrap();
        assert!(execute(&input, &modes, 32, 0, &script, &failed).is_err());
        assert!(!failed.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
