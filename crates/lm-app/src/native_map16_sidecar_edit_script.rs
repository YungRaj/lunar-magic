use lm_app::NativeMap16SidecarEdit;
use std::fmt;

const MAGIC: &str = "LMN16ED1";
pub const MAX_SCRIPT_LEN: usize = 64 * 1024;
const MAX_COMMANDS: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeMap16SidecarEditScriptError {
    line: usize,
    message: String,
}

impl fmt::Display for NativeMap16SidecarEditScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid native Map16 sidecar edit at line {}: {}",
            self.line, self.message
        )
    }
}

impl std::error::Error for NativeMap16SidecarEditScriptError {}

pub fn parse(
    input: &str,
) -> Result<Vec<NativeMap16SidecarEdit>, NativeMap16SidecarEditScriptError> {
    if input.len() > MAX_SCRIPT_LEN {
        return Err(error(0, "script is too large"));
    }
    let mut lines = input.lines();
    if lines.next() != Some(MAGIC) {
        return Err(error(1, "wrong or missing LMN16ED1 magic"));
    }
    let mut edits = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = index + 2;
        let content = raw.split('#').next().unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        let words: Vec<_> = content.split_whitespace().collect();
        let ["set", entry, value] = words.as_slice() else {
            return Err(error(line, "expected: set HEX_ENTRY HEX_DWORD"));
        };
        edits.push(NativeMap16SidecarEdit {
            entry: parse_hex(line, entry)?,
            value: u32::try_from(parse_hex(line, value)?)
                .map_err(|_| error(line, "dword exceeds 32 bits"))?,
        });
        if edits.len() > MAX_COMMANDS {
            return Err(error(line, "too many commands"));
        }
    }
    Ok(edits)
}

fn parse_hex(line: usize, value: &str) -> Result<usize, NativeMap16SidecarEditScriptError> {
    usize::from_str_radix(value.strip_prefix("0x").unwrap_or(value), 16)
        .map_err(|_| error(line, format!("invalid hexadecimal value {value:?}")))
}

fn error(line: usize, message: impl Into<String>) -> NativeMap16SidecarEditScriptError {
    NativeMap16SidecarEditScriptError {
        line,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ordered_dword_edits_and_comments() {
        assert_eq!(
            parse("LMN16ED1\nset 0 44332211\nset 0x200 2 # boundary\n").unwrap(),
            [
                NativeMap16SidecarEdit {
                    entry: 0,
                    value: 0x4433_2211
                },
                NativeMap16SidecarEdit {
                    entry: 0x200,
                    value: 2
                }
            ]
        );
    }

    #[test]
    fn malformed_and_oversized_scripts_fail() {
        assert!(parse("bad\n").is_err());
        assert!(parse("LMN16ED1\nset x 1\n").is_err());
        assert!(parse("LMN16ED1\nremove 0\n").is_err());
        assert!(parse(&"x".repeat(MAX_SCRIPT_LEN + 1)).is_err());
    }
}
