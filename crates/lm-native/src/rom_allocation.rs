use crate::level_editor_forms;
use std::ops::Range;

pub(crate) fn parse_search_range(start: &str, end: &str) -> Result<Range<usize>, String> {
    let start = level_editor_forms::parse_hex_u32(start, "allocation start")? as usize;
    let end = level_editor_forms::parse_hex_u32(end, "allocation end")? as usize;
    if start >= end {
        return Err("allocation search start must be below its end".into());
    }
    Ok(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_range_is_hexadecimal_ordered_and_end_exclusive() {
        assert_eq!(
            parse_search_range("008000", "010000").unwrap(),
            0x8000..0x10000
        );
        assert!(parse_search_range("100", "100").is_err());
        assert!(parse_search_range("200", "100").is_err());
        assert!(parse_search_range("not-hex", "100").is_err());
    }
}
