use crate::read_bounded_bytes;
use std::fs;
use std::path::Path;

pub const MAX_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_LINES: usize = 65_536;
pub const MAX_LINE_BYTES: usize = 64 * 1024;

pub fn load(path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("command script must be a regular file".into());
    }
    if usize::try_from(metadata.len()).unwrap_or(usize::MAX) > MAX_FILE_BYTES {
        return Err("command script exceeds the bounded file limit".into());
    }
    let bytes = read_bounded_bytes(path, MAX_FILE_BYTES, "command script")?;
    let text = std::str::from_utf8(&bytes)?;
    parse(text)
}

fn parse(text: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if text.len() > MAX_FILE_BYTES {
        return Err("command script exceeds the bounded file limit".into());
    }
    let lines: Vec<_> = text.lines().map(str::to_owned).collect();
    if lines.len() > MAX_LINES {
        return Err("command script exceeds the line-count limit".into());
    }
    if lines.iter().any(|line| line.len() > MAX_LINE_BYTES) {
        return Err("command script exceeds the per-line byte limit".into());
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_blank_unicode_and_confirmation_lines() {
        assert_eq!(
            parse("open My Hack 日本語.smc\n\nyes\nquit\n").unwrap(),
            ["open My Hack 日本語.smc", "", "yes", "quit"]
        );
    }

    #[test]
    fn enforces_file_line_and_line_length_bounds_before_dispatch() {
        assert!(parse(&"x".repeat(MAX_FILE_BYTES + 1)).is_err());
        assert!(parse(&"\n".repeat(MAX_LINES + 1)).is_err());
        assert!(parse(&"x".repeat(MAX_LINE_BYTES + 1)).is_err());
    }
}
