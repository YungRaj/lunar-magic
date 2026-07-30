use std::collections::BTreeMap;
use std::fmt::Write as _;

use lm_level::{NativeSpriteFieldError, NativeSpriteStream, SpriteToken};

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

/// Domain counters used while Lunar Magic-compatible level loading walks every slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelUsageAccumulator {
    level_count: usize,
    map16_tiles: BTreeMap<u32, UsageCounter>,
    graphics_files: BTreeMap<u32, UsageCounter>,
    sprites: BTreeMap<u32, UsageCounter>,
    music_tracks: BTreeMap<u32, UsageCounter>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UsageCounter {
    count: u64,
    levels: Vec<bool>,
    name: Option<String>,
}

impl LevelUsageAccumulator {
    /// Creates the bounded counter planes used by an all-level scan.
    ///
    /// # Errors
    ///
    /// Rejects a level namespace larger than Lunar Magic's recovered bitmap limit.
    pub fn new(level_count: usize) -> Result<Self, LevelUsageAnalysisError> {
        if level_count > LevelUsageReport::MAX_LEVELS {
            return Err(LevelUsageAnalysisError::TooManyLevels(level_count));
        }
        Ok(Self {
            level_count,
            map16_tiles: BTreeMap::new(),
            graphics_files: BTreeMap::new(),
            sprites: BTreeMap::new(),
            music_tracks: BTreeMap::new(),
        })
    }

    /// Counts Lunar Magic's final Layer 1 cache and optional raw Layer 2 tilemap.
    ///
    /// Layer 1 words at or above `$8000` are ignored. Layer 2 words are first offset by the active
    /// `$1000`-tile bank and then stored in Lunar Magic's separate `$8000..$FFFF` report namespace.
    ///
    /// # Errors
    ///
    /// Rejects a level outside the configured scan namespace or counter overflow.
    pub fn observe_map16(
        &mut self,
        level: usize,
        layer1_cache: &[u16],
        layer2: Option<(&[u16], u8)>,
    ) -> Result<(), LevelUsageAnalysisError> {
        self.validate_level(level)?;
        for &tile in layer1_cache {
            if tile < 0x8000 {
                observe(
                    &mut self.map16_tiles,
                    u32::from(tile),
                    level,
                    self.level_count,
                )?;
            }
        }
        if let Some((words, bank)) = layer2 {
            for &tile in words {
                let banked = u32::from(tile) + u32::from(bank) * 0x1000;
                if banked < 0x8000 {
                    observe(
                        &mut self.map16_tiles,
                        banked + 0x8000,
                        level,
                        self.level_count,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Counts every loaded graphics-file assignment, including duplicates between slots.
    ///
    /// # Errors
    ///
    /// Rejects an out-of-range level or counter overflow.
    pub fn observe_graphics(
        &mut self,
        level: usize,
        files: impl IntoIterator<Item = u16>,
    ) -> Result<(), LevelUsageAnalysisError> {
        self.validate_level(level)?;
        for file in files {
            if file < 0x1000 {
                observe(
                    &mut self.graphics_files,
                    u32::from(file),
                    level,
                    self.level_count,
                )?;
            }
        }
        Ok(())
    }

    /// Counts native sprite records using `extra_bits << 8 | sprite_number`.
    ///
    /// # Errors
    ///
    /// Rejects an out-of-range level, malformed record, or counter overflow.
    pub fn observe_sprites(
        &mut self,
        level: usize,
        sprites: &NativeSpriteStream,
    ) -> Result<(), LevelUsageAnalysisError> {
        self.validate_level(level)?;
        for token in &sprites.tokens {
            let SpriteToken::Record(record) = token else {
                continue;
            };
            let fields = record
                .native_fields()
                .map_err(LevelUsageAnalysisError::Sprite)?;
            let resource = u32::from(fields.sprite_number) | u32::from(fields.extra_bits) << 8;
            observe(&mut self.sprites, resource, level, self.level_count)?;
        }
        Ok(())
    }

    /// Counts the single resolved music track for a successfully loaded level.
    ///
    /// # Errors
    ///
    /// Rejects an out-of-range level, invalid name text, or counter overflow.
    pub fn observe_music(
        &mut self,
        level: usize,
        track: u8,
        name: impl Into<String>,
    ) -> Result<(), LevelUsageAnalysisError> {
        self.validate_level(level)?;
        let name = name.into();
        validate_name(&name).map_err(LevelUsageAnalysisError::Report)?;
        let counter = counter(&mut self.music_tracks, u32::from(track), self.level_count);
        counter.count = counter
            .count
            .checked_add(1)
            .ok_or(LevelUsageAnalysisError::CountOverflow)?;
        counter.levels[level] = true;
        counter.name = Some(name);
        Ok(())
    }

    /// Materializes ascending resource entries, optionally retaining defined-but-unused resources.
    #[must_use]
    pub fn finish(
        mut self,
        defined_map16: impl IntoIterator<Item = u32>,
        inserted_graphics: impl IntoIterator<Item = u32>,
    ) -> LevelUsageReport {
        for resource in defined_map16 {
            let _ = counter(&mut self.map16_tiles, resource, self.level_count);
        }
        for resource in inserted_graphics {
            let _ = counter(&mut self.graphics_files, resource, self.level_count);
        }
        LevelUsageReport {
            map16_tiles: entries(self.map16_tiles),
            graphics_files: entries(self.graphics_files),
            sprites: entries(self.sprites),
            music_tracks: entries(self.music_tracks),
        }
    }

    fn validate_level(&self, level: usize) -> Result<(), LevelUsageAnalysisError> {
        if level >= self.level_count {
            return Err(LevelUsageAnalysisError::LevelOutOfRange {
                level,
                count: self.level_count,
            });
        }
        Ok(())
    }
}

fn counter(
    counters: &mut BTreeMap<u32, UsageCounter>,
    resource: u32,
    level_count: usize,
) -> &mut UsageCounter {
    counters.entry(resource).or_insert_with(|| UsageCounter {
        count: 0,
        levels: vec![false; level_count],
        name: None,
    })
}

fn observe(
    counters: &mut BTreeMap<u32, UsageCounter>,
    resource: u32,
    level: usize,
    level_count: usize,
) -> Result<(), LevelUsageAnalysisError> {
    let counter = counter(counters, resource, level_count);
    counter.count = counter
        .count
        .checked_add(1)
        .ok_or(LevelUsageAnalysisError::CountOverflow)?;
    counter.levels[level] = true;
    Ok(())
}

fn entries(counters: BTreeMap<u32, UsageCounter>) -> Vec<LevelUsageEntry> {
    counters
        .into_iter()
        .map(|(resource, counter)| LevelUsageEntry {
            resource,
            count: counter.count,
            levels: counter.levels,
            name: counter.name,
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelUsageAnalysisError {
    TooManyLevels(usize),
    LevelOutOfRange { level: usize, count: usize },
    Sprite(NativeSpriteFieldError),
    Report(LevelUsageReportError),
    CountOverflow,
}

impl std::fmt::Display for LevelUsageAnalysisError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "level-usage analysis failed: {self:?}")
    }
}

impl std::error::Error for LevelUsageAnalysisError {}

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

    #[test]
    fn accumulator_preserves_native_map16_namespaces_duplicates_and_membership() {
        let mut analysis = LevelUsageAccumulator::new(2).unwrap();
        analysis
            .observe_map16(0, &[0x25, 0x25, 0x8000], Some((&[1, 1, 0x7000], 2)))
            .unwrap();
        analysis.observe_graphics(0, [0x14, 0x14, 0x1000]).unwrap();
        analysis.observe_map16(1, &[0x25], None).unwrap();
        let report = analysis.finish([6], [0x7f]);
        assert_eq!(
            report
                .map16_tiles
                .iter()
                .map(|entry| (entry.resource, entry.count, entry.levels.clone()))
                .collect::<Vec<_>>(),
            [
                (6, 0, vec![false, false]),
                (0x25, 3, vec![true, true]),
                (0xa001, 2, vec![true, false]),
            ]
        );
        assert_eq!(
            report
                .graphics_files
                .iter()
                .map(|entry| (entry.resource, entry.count))
                .collect::<Vec<_>>(),
            [(0x14, 2), (0x7f, 0)]
        );
    }

    #[test]
    fn accumulator_counts_extra_bit_sprite_namespace_and_resolved_music() {
        let sprites = NativeSpriteStream {
            header: 0,
            expanded: false,
            tokens: vec![
                SpriteToken::Record(lm_level::SpriteRecord {
                    encoded: vec![0x08, 0, 0x42],
                }),
                SpriteToken::Record(lm_level::SpriteRecord {
                    encoded: vec![0x0c, 0, 0x42],
                }),
            ],
        };
        let mut analysis = LevelUsageAccumulator::new(1).unwrap();
        analysis.observe_sprites(0, &sprites).unwrap();
        analysis.observe_music(0, 3, "Here We Go!").unwrap();
        let report = analysis.finish([], []);
        assert_eq!(
            report
                .sprites
                .iter()
                .map(|entry| (entry.resource, entry.count))
                .collect::<Vec<_>>(),
            [(0x242, 1), (0x342, 1)]
        );
        assert_eq!(report.music_tracks[0].resource, 3);
        assert_eq!(report.music_tracks[0].name.as_deref(), Some("Here We Go!"));
    }
}
