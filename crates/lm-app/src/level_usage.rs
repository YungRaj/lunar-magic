use std::fmt::Write as _;

/// One counted resource and the levels in which Lunar Magic observed it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelUsageEntry {
    pub resource: u32,
    pub count: u64,
    pub levels: Vec<bool>,
    pub name: Option<String>,
}

/// The four resource domains emitted by Lunar Magic's level-usage analysis.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LevelUsageReport {
    pub map16_tiles: Vec<LevelUsageEntry>,
    pub graphics_files: Vec<LevelUsageEntry>,
    pub sprites: Vec<LevelUsageEntry>,
    pub music_tracks: Vec<LevelUsageEntry>,
}

/// Local timestamp fields used by the original report header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelUsageTimestamp {
    pub month: u8,
    pub day: u8,
    pub year: u16,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl LevelUsageReport {
    pub const MAX_LEVELS: usize = 0x2000;
    pub const MAX_REPORT_BYTES: usize = 64 * 1024 * 1024;

    /// Encodes the English Lunar Magic report framing recovered from
    /// `GenerateLevelUsageAnalysisReport` at `00486A40`.
    ///
    /// Entries are emitted in caller-provided order. The semantic scanner supplies ascending
    /// resource IDs, matching Lunar Magic's fixed-domain loops.
    ///
    /// # Errors
    ///
    /// Rejects invalid timestamps, inconsistent level vectors, line-breaking text, and output
    /// larger than the bounded report limit.
    pub fn encode_lunar_magic_363(
        &self,
        timestamp: LevelUsageTimestamp,
    ) -> Result<Vec<u8>, LevelUsageReportError> {
        validate_timestamp(timestamp)?;
        let levels = self.level_count()?;
        let mut output = String::new();
        output.push_str("Lunar Magic 3.63\r\n");
        let (hour, suffix) = display_hour(timestamp.hour);
        let _ = writeln!(
            output,
            "{:02}/{:02}/{:04}  {hour:02}:{:02}:{:02} {suffix}M\r",
            timestamp.month, timestamp.day, timestamp.year, timestamp.minute, timestamp.second
        );
        output.push_str("\r\n");
        encode_section(
            &mut output,
            "Map16 Tile Usage Report (Non-covered, Non-conditional)",
            "Tile",
            &self.map16_tiles,
            levels,
            false,
        );
        encode_section(
            &mut output,
            "Graphics Usage Report",
            "Graphics File",
            &self.graphics_files,
            levels,
            false,
        );
        encode_section(
            &mut output,
            "Sprite Usage Report (extra bits are upper part of sprite number)",
            "Sprite",
            &self.sprites,
            levels,
            false,
        );
        encode_section(
            &mut output,
            "Music Usage Report",
            "Music Track",
            &self.music_tracks,
            levels,
            true,
        );
        if output.len() > Self::MAX_REPORT_BYTES {
            return Err(LevelUsageReportError::ReportTooLarge(output.len()));
        }
        Ok(output.into_bytes())
    }

    fn level_count(&self) -> Result<usize, LevelUsageReportError> {
        let mut count = None;
        for entry in self
            .map16_tiles
            .iter()
            .chain(&self.graphics_files)
            .chain(&self.sprites)
            .chain(&self.music_tracks)
        {
            if entry.levels.len() > Self::MAX_LEVELS {
                return Err(LevelUsageReportError::TooManyLevels(entry.levels.len()));
            }
            match count {
                Some(expected) if expected != entry.levels.len() => {
                    return Err(LevelUsageReportError::LevelCountMismatch {
                        expected,
                        actual: entry.levels.len(),
                    });
                }
                None => count = Some(entry.levels.len()),
                _ => {}
            }
            if let Some(name) = &entry.name {
                validate_name(name)?;
            }
        }
        Ok(count.unwrap_or(0))
    }
}

fn encode_section(
    output: &mut String,
    title: &str,
    item: &str,
    entries: &[LevelUsageEntry],
    level_count: usize,
    names: bool,
) {
    if !output.ends_with("\r\n\r\n") {
        output.push_str("\r\n\r\n");
    }
    output.push_str(title);
    output.push_str("\r\n\r\n");
    for entry in entries {
        let _ = writeln!(
            output,
            "{item} {:X} count: {:X}\r",
            entry.resource, entry.count
        );
        if names {
            output.push_str("\tName: ");
            output.push_str(entry.name.as_deref().unwrap_or(""));
            output.push_str("\r\n");
        }
        if entry.count != 0 {
            encode_level_ranges(output, &entry.levels[..level_count]);
        }
    }
}

fn encode_level_ranges(output: &mut String, levels: &[bool]) {
    output.push_str("\tLevels: ");
    let mut line_width = 0;
    let mut run_start = None;
    for index in 0..=levels.len() {
        let active = levels.get(index).copied().unwrap_or(false);
        match (run_start, active) {
            (None, true) => {
                if line_width >= 0x44 {
                    output.push_str("\r\n\t\t");
                    line_width = 0;
                } else if line_width != 0 {
                    output.push_str(", ");
                    line_width += 2;
                }
                let value = format!("{index:X}");
                line_width += value.len();
                output.push_str(&value);
                run_start = Some(index);
            }
            (Some(start), false) => {
                if index - start > 1 {
                    let suffix = format!("-{:X}", index - 1);
                    line_width += suffix.len();
                    output.push_str(&suffix);
                }
                run_start = None;
            }
            _ => {}
        }
    }
    output.push_str("\r\n");
}

const fn display_hour(hour: u8) -> (u8, char) {
    let display = if hour == 0 || hour == 12 {
        12
    } else {
        hour % 12
    };
    (display, if hour < 12 { 'A' } else { 'P' })
}

fn validate_timestamp(timestamp: LevelUsageTimestamp) -> Result<(), LevelUsageReportError> {
    if !(1..=12).contains(&timestamp.month)
        || !(1..=31).contains(&timestamp.day)
        || timestamp.hour > 23
        || timestamp.minute > 59
        || timestamp.second > 59
    {
        return Err(LevelUsageReportError::Timestamp(timestamp));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), LevelUsageReportError> {
    if name.contains(['\r', '\n', '\0']) {
        return Err(LevelUsageReportError::InvalidName);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelUsageReportError {
    Timestamp(LevelUsageTimestamp),
    TooManyLevels(usize),
    LevelCountMismatch { expected: usize, actual: usize },
    InvalidName,
    ReportTooLarge(usize),
}

impl std::fmt::Display for LevelUsageReportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid level-usage report: {self:?}")
    }
}

impl std::error::Error for LevelUsageReportError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(resource: u32, count: u64, active: &[usize], levels: usize) -> LevelUsageEntry {
        let mut membership = vec![false; levels];
        for &level in active {
            membership[level] = true;
        }
        LevelUsageEntry {
            resource,
            count,
            levels: membership,
            name: None,
        }
    }

    #[test]
    fn compact_ranges_match_recovered_single_pair_and_run_rules() {
        let mut output = String::new();
        encode_level_ranges(
            &mut output,
            &entry(0, 0, &[0, 2, 3, 5, 6, 7, 0x10], 0x11).levels,
        );
        assert_eq!(output, "\tLevels: 0, 2-3, 5-7, 10\r\n");
    }

    #[test]
    fn range_writer_wraps_before_the_next_run_at_the_native_threshold() {
        let mut levels = vec![false; 0x100];
        for level in (0..=0x90).step_by(2) {
            levels[level] = true;
        }
        let mut output = String::new();
        encode_level_ranges(&mut output, &levels);
        let lines = output.split("\r\n").collect::<Vec<_>>();
        assert!(lines.len() > 2);
        assert!(lines[1].starts_with("\t\t"));
    }

    #[test]
    fn complete_report_uses_exact_headers_hex_counts_names_and_clock() {
        let report = LevelUsageReport {
            map16_tiles: vec![entry(0x25, 0x12, &[0x105], 0x106)],
            graphics_files: vec![entry(0x14, 1, &[0], 0x106)],
            sprites: vec![entry(0x2a, 2, &[0, 1], 0x106)],
            music_tracks: vec![LevelUsageEntry {
                name: Some("Here We Go!".into()),
                ..entry(3, 1, &[0x105], 0x106)
            }],
        };
        let bytes = report
            .encode_lunar_magic_363(LevelUsageTimestamp {
                month: 7,
                day: 30,
                year: 2026,
                hour: 0,
                minute: 5,
                second: 9,
            })
            .unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("Lunar Magic 3.63\r\n07/30/2026  12:05:09 AM\r\n\r\n"));
        assert!(text.contains("Tile 25 count: 12\r\n\tLevels: 105\r\n"));
        assert!(text.contains("Sprite 2A count: 2\r\n\tLevels: 0-1\r\n"));
        assert!(text.contains("Music Track 3 count: 1\r\n\tName: Here We Go!\r\n"));
    }

    #[test]
    fn malformed_aggregate_is_rejected_before_encoding() {
        let report = LevelUsageReport {
            map16_tiles: vec![entry(0, 1, &[0], 1)],
            sprites: vec![entry(0, 1, &[0], 2)],
            ..LevelUsageReport::default()
        };
        assert!(matches!(
            report.encode_lunar_magic_363(LevelUsageTimestamp {
                month: 1,
                day: 1,
                year: 2026,
                hour: 0,
                minute: 0,
                second: 0,
            }),
            Err(LevelUsageReportError::LevelCountMismatch {
                expected: 1,
                actual: 2
            })
        ));
    }
}
