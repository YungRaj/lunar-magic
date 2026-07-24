use lm_app::MwlDocumentEdit;
use lm_level::{MwlFile, MwlSectionKind};
use std::collections::HashSet;
use std::fmt;

pub const MAGIC: &str = "LMWLEDT1";
pub const MAX_SCRIPT_LEN: usize = MwlFile::MAX_SECTION_BYTES + 4096;
const MAX_COMMANDS: usize = MwlFile::SECTION_COUNT + 3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MwlEditScriptError {
    TooLarge(usize),
    MissingMagic,
    TooManyCommands(usize),
    LineTooLong { line: usize, bytes: usize },
    Malformed { line: usize },
    UnknownCommand { line: usize, command: String },
    UnknownSection { line: usize, section: String },
    DuplicateTarget { line: usize, target: String },
    InvalidNumber { line: usize, value: String },
    InvalidHex { line: usize },
    AttributionLength(usize),
    SectionTooLarge(usize),
}

impl fmt::Display for MwlEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid MWL edit script: {self:?}")
    }
}

impl std::error::Error for MwlEditScriptError {}

pub fn parse(text: &str) -> Result<Vec<MwlDocumentEdit>, MwlEditScriptError> {
    if text.len() > MAX_SCRIPT_LEN {
        return Err(MwlEditScriptError::TooLarge(text.len()));
    }
    let mut lines = text.lines();
    if lines.next() != Some(MAGIC) {
        return Err(MwlEditScriptError::MissingMagic);
    }
    let mut edits = Vec::new();
    let mut targets = HashSet::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        if raw.len() > MwlFile::MAX_SECTION_BYTES * 2 + 32 {
            return Err(MwlEditScriptError::LineTooLong {
                line,
                bytes: raw.len(),
            });
        }
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        if edits.len() == MAX_COMMANDS {
            return Err(MwlEditScriptError::TooManyCommands(edits.len() + 1));
        }
        let mut fields = raw.split_ascii_whitespace();
        let command = fields
            .next()
            .ok_or(MwlEditScriptError::Malformed { line })?;
        let edit = match command {
            "flags" => {
                unique(&mut targets, line, "flags")?;
                let value = one(fields, line)?;
                MwlDocumentEdit::SetFlags(parse_hex_number(value, line)?)
            }
            "level" => {
                unique(&mut targets, line, "level")?;
                let value = one(fields, line)?;
                MwlDocumentEdit::SetLevelNumber(parse_hex_number(value, line)?)
            }
            "attribution" => {
                unique(&mut targets, line, "attribution")?;
                let value = one(fields, line)?;
                let bytes = decode_hex(value, line)?;
                let len = bytes.len();
                let attribution = bytes
                    .try_into()
                    .map_err(|_| MwlEditScriptError::AttributionLength(len))?;
                MwlDocumentEdit::SetAttribution(attribution)
            }
            "section" => {
                let name = fields
                    .next()
                    .ok_or(MwlEditScriptError::Malformed { line })?;
                let value = one(fields, line)?;
                let section = parse_section(name, line)?;
                unique(&mut targets, line, &format!("section/{name}"))?;
                let bytes = if value == "-" {
                    Vec::new()
                } else {
                    decode_hex(value, line)?
                };
                if bytes.len() > MwlFile::MAX_SECTION_BYTES {
                    return Err(MwlEditScriptError::SectionTooLarge(bytes.len()));
                }
                MwlDocumentEdit::ReplaceSection { section, bytes }
            }
            command => {
                return Err(MwlEditScriptError::UnknownCommand {
                    line,
                    command: command.to_owned(),
                });
            }
        };
        edits.push(edit);
    }
    Ok(edits)
}

fn one<'a>(
    mut fields: impl Iterator<Item = &'a str>,
    line: usize,
) -> Result<&'a str, MwlEditScriptError> {
    let value = fields
        .next()
        .ok_or(MwlEditScriptError::Malformed { line })?;
    if fields.next().is_some() {
        return Err(MwlEditScriptError::Malformed { line });
    }
    Ok(value)
}

fn unique(
    targets: &mut HashSet<String>,
    line: usize,
    target: &str,
) -> Result<(), MwlEditScriptError> {
    if !targets.insert(target.to_owned()) {
        return Err(MwlEditScriptError::DuplicateTarget {
            line,
            target: target.to_owned(),
        });
    }
    Ok(())
}

fn parse_hex_number<T>(value: &str, line: usize) -> Result<T, MwlEditScriptError>
where
    T: TryFrom<u64>,
{
    let value = value.strip_prefix("0x").unwrap_or(value);
    let parsed = u64::from_str_radix(value, 16).map_err(|_| MwlEditScriptError::InvalidNumber {
        line,
        value: value.to_owned(),
    })?;
    T::try_from(parsed).map_err(|_| MwlEditScriptError::InvalidNumber {
        line,
        value: value.to_owned(),
    })
}

fn decode_hex(value: &str, line: usize) -> Result<Vec<u8>, MwlEditScriptError> {
    if value.len() % 2 != 0 {
        return Err(MwlEditScriptError::InvalidHex { line });
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text =
                std::str::from_utf8(pair).map_err(|_| MwlEditScriptError::InvalidHex { line })?;
            u8::from_str_radix(text, 16).map_err(|_| MwlEditScriptError::InvalidHex { line })
        })
        .collect()
}

fn parse_section(name: &str, line: usize) -> Result<MwlSectionKind, MwlEditScriptError> {
    match name {
        "header" => Ok(MwlSectionKind::LevelHeader),
        "layer1" => Ok(MwlSectionKind::Layer1),
        "layer2" => Ok(MwlSectionKind::Layer2),
        "sprites" => Ok(MwlSectionKind::Sprites),
        "palette" => Ok(MwlSectionKind::Palette),
        "secondary-exits" => Ok(MwlSectionKind::SecondaryExits),
        "exanimation" => Ok(MwlSectionKind::ExAnimation),
        "expanded-header" => Ok(MwlSectionKind::ExpandedHeader),
        _ => Err(MwlEditScriptError::UnknownSection {
            line,
            section: name.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_edit_kind_and_empty_sections() {
        let attribution = "11".repeat(MwlFile::ATTRIBUTION_LEN);
        let text = format!(
            "{MAGIC}\nflags 89abcdef\nlevel 1ab\nattribution {attribution}\nsection layer1 0102ff\nsection layer2 -\n"
        );
        let edits = parse(&text).unwrap();
        assert_eq!(edits.len(), 5);
        assert_eq!(edits[0], MwlDocumentEdit::SetFlags(0x89ab_cdef));
        assert_eq!(edits[1], MwlDocumentEdit::SetLevelNumber(0x01ab));
        assert!(matches!(
            &edits[3],
            MwlDocumentEdit::ReplaceSection { section: MwlSectionKind::Layer1, bytes }
                if bytes == &[1, 2, 0xff]
        ));
        assert!(matches!(
            &edits[4],
            MwlDocumentEdit::ReplaceSection { section: MwlSectionKind::Layer2, bytes }
                if bytes.is_empty()
        ));
    }

    #[test]
    fn malformed_unknown_duplicate_and_bounds_are_rejected() {
        assert_eq!(parse("bad\n"), Err(MwlEditScriptError::MissingMagic));
        assert!(matches!(
            parse(&format!("{MAGIC}\nflags 1\nflags 2\n")),
            Err(MwlEditScriptError::DuplicateTarget { line: 3, .. })
        ));
        assert!(matches!(
            parse(&format!("{MAGIC}\nsection mystery 00\n")),
            Err(MwlEditScriptError::UnknownSection { .. })
        ));
        assert!(matches!(
            parse(&format!("{MAGIC}\nattribution 00\n")),
            Err(MwlEditScriptError::AttributionLength(1))
        ));
        assert!(matches!(
            parse(&format!("{MAGIC}\nsection layer1 0\n")),
            Err(MwlEditScriptError::InvalidHex { .. })
        ));
    }
}
