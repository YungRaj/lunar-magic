use std::fmt;
use std::fmt::Write as _;

/// One external payload descriptor in Lunar Magic's legacy text MWL format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyMwlSidecar {
    pub flags: u8,
    pub source_address: u32,
    pub file_name: String,
}

/// The four fields retained for one secondary exit by the legacy text format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyMwlSecondaryExit {
    pub index: u16,
    pub position_and_method: u8,
    pub screen_and_y: u8,
    pub destination_high_and_flags: u8,
}

/// Lossless modeled content of Lunar Magic's legacy text level manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyMwlManifest {
    pub version: u16,
    pub attribution: String,
    pub level_number: u16,
    /// The five bytes following the level number in a current manifest.
    pub header: [u8; 5],
    pub layer1: LegacyMwlSidecar,
    pub layer2: LegacyMwlSidecar,
    pub sprites: LegacyMwlSidecar,
    pub secondary_exits: Vec<LegacyMwlSecondaryExit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyMwlDecodeReport {
    pub manifest: LegacyMwlManifest,
    pub diagnostics: Vec<LegacyMwlDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyMwlDiagnostic {
    LevelNumberClamped { source: u16, target: u16 },
    IgnoredSecondaryExit { line: usize },
    ReplacedSecondaryExit { index: u16 },
}

impl fmt::Display for LegacyMwlDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LevelNumberClamped { source, target } => {
                write!(
                    formatter,
                    "source level ${source:03X} was clamped to ${target:03X}"
                )
            }
            Self::IgnoredSecondaryExit { line } => {
                write!(formatter, "ignored malformed secondary-exit row {line}")
            }
            Self::ReplacedSecondaryExit { index } => {
                write!(
                    formatter,
                    "later secondary exit ${index:03X} replaced an earlier row"
                )
            }
        }
    }
}

impl LegacyMwlManifest {
    pub const CURRENT_VERSION: u16 = 0x0363;
    pub const MAX_FILE_BYTES: usize = 1024 * 1024;
    pub const MAX_LINE_BYTES: usize = 0x410;
    pub const MAX_SECONDARY_EXITS: usize = 0x2000;

    /// Decodes a legacy text MWL manifest, accepting CRLF or LF framing and `//` comments.
    ///
    /// # Errors
    ///
    /// Rejects invalid UTF-8, unsupported framing, malformed hexadecimal fields, unsafe sidecar
    /// names, and duplicate/oversized exit tables. Lunar Magic defaults an unparsable legacy
    /// header version to 1.32 and accepts later parseable versions through its current branch;
    /// this decoder deliberately retains that compatibility behavior.
    pub fn decode(bytes: &[u8]) -> Result<Self, LegacyMwlError> {
        Self::decode_with_diagnostics(bytes).map(|report| report.manifest)
    }

    /// Decodes with the non-fatal compatibility diagnostics Lunar Magic presents while
    /// continuing the legacy import.
    pub fn decode_with_diagnostics(bytes: &[u8]) -> Result<LegacyMwlDecodeReport, LegacyMwlError> {
        if bytes.len() > Self::MAX_FILE_BYTES {
            return Err(LegacyMwlError::FileTooLarge(bytes.len()));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| LegacyMwlError::Utf8)?;
        let mut lines = LogicalLines::new(text)?;
        let header_line = lines.next_header()?.ok_or(LegacyMwlError::MissingHeader)?;
        let version = parse_version(header_line)?;
        let attribution = lines
            .next_data()?
            .ok_or(LegacyMwlError::MissingAttribution)?
            .to_string();
        let level_fields = fields(
            lines
                .next_data()?
                .ok_or(LegacyMwlError::MissingLevelFields)?,
        );
        let expected = if version < 0x0132 { 5 } else { 6 };
        if level_fields.len() != expected {
            return Err(LegacyMwlError::LevelFieldCount {
                expected,
                actual: level_fields.len(),
            });
        }
        let source_level_number = parse_hex_u16(level_fields[0], 0x0fff, "level number")?;
        let level_number = source_level_number.min(0x01ff);
        let mut diagnostics = Vec::new();
        if source_level_number != level_number {
            diagnostics.push(LegacyMwlDiagnostic::LevelNumberClamped {
                source: source_level_number,
                target: level_number,
            });
        }
        let mut header = [0; 5];
        for (destination, field) in header.iter_mut().zip(&level_fields[1..]) {
            *destination = parse_hex_u8(field, "level header")?;
        }
        let layer1 = parse_sidecar(
            lines
                .next_data()?
                .ok_or(LegacyMwlError::MissingSidecar("Layer 1"))?,
        )?;
        let layer2 = parse_sidecar(
            lines
                .next_data()?
                .ok_or(LegacyMwlError::MissingSidecar("Layer 2"))?,
        )?;
        let sprites = parse_sidecar(
            lines
                .next_data()?
                .ok_or(LegacyMwlError::MissingSidecar("sprites"))?,
        )?;

        let mut secondary_exits: Vec<LegacyMwlSecondaryExit> = Vec::new();
        let mut exit_lines = 0_usize;
        while let Some(line) = lines.next_data()? {
            exit_lines += 1;
            let fields = fields(line);
            if fields.len() != 4 {
                diagnostics.push(LegacyMwlDiagnostic::IgnoredSecondaryExit { line: exit_lines });
                continue;
            }
            let index_maximum = if version < 0x0132 { 0x00ff } else { 0x0fff };
            let Ok(mut index) = parse_hex_u16(fields[0], index_maximum, "secondary exit index")
            else {
                diagnostics.push(LegacyMwlDiagnostic::IgnoredSecondaryExit { line: exit_lines });
                continue;
            };
            if version < 0x0132 && level_number & 0x100 != 0 {
                index = index
                    .checked_add(0x100)
                    .ok_or(LegacyMwlError::SecondaryExitIndex(index))?;
            }
            let parsed = [fields[1], fields[2], fields[3]]
                .map(|field| parse_hex_u8(field, "secondary exit field"));
            let [
                Ok(position_and_method),
                Ok(screen_and_y),
                Ok(destination_high_and_flags),
            ] = parsed
            else {
                diagnostics.push(LegacyMwlDiagnostic::IgnoredSecondaryExit { line: exit_lines });
                continue;
            };
            let exit = LegacyMwlSecondaryExit {
                index,
                position_and_method,
                screen_and_y,
                destination_high_and_flags,
            };
            if let Some(existing) = secondary_exits
                .iter_mut()
                .find(|value| value.index == index)
            {
                *existing = exit;
                diagnostics.push(LegacyMwlDiagnostic::ReplacedSecondaryExit { index });
            } else {
                secondary_exits.push(exit);
            }
        }
        let manifest = Self {
            version,
            attribution,
            level_number,
            header,
            layer1,
            layer2,
            sprites,
            secondary_exits,
        };
        manifest.validate()?;
        Ok(LegacyMwlDecodeReport {
            manifest,
            diagnostics,
        })
    }

    /// Validates a decoded or programmatically constructed legacy manifest.
    ///
    /// Unlike [`Self::encode`], this accepts every historical version understood by the importer.
    ///
    /// # Errors
    ///
    /// Rejects versions outside the legacy header's three hexadecimal digits, invalid text or
    /// filenames, out-of-range values, and duplicate secondary exits.
    pub fn validate(&self) -> Result<(), LegacyMwlError> {
        if self.version > 0x0fff {
            return Err(LegacyMwlError::UnsupportedVersion(self.version));
        }
        if self.level_number > 0x01ff {
            return Err(LegacyMwlError::LevelNumber(self.level_number));
        }
        validate_text_line(&self.attribution, "attribution")?;
        validate_sidecar(&self.layer1)?;
        validate_sidecar(&self.layer2)?;
        validate_sidecar(&self.sprites)?;
        if self.secondary_exits.len() > Self::MAX_SECONDARY_EXITS {
            return Err(LegacyMwlError::TooManySecondaryExits(
                self.secondary_exits.len(),
            ));
        }
        let mut seen = [false; Self::MAX_SECONDARY_EXITS];
        for exit in &self.secondary_exits {
            if exit.index >= 0x2000 {
                return Err(LegacyMwlError::SecondaryExitIndex(exit.index));
            }
            if seen[usize::from(exit.index)] {
                return Err(LegacyMwlError::DuplicateSecondaryExit(exit.index));
            }
            seen[usize::from(exit.index)] = true;
        }
        Ok(())
    }

    /// Encodes Lunar Magic 3.63's canonical CRLF text representation.
    ///
    /// # Errors
    ///
    /// Rejects values outside the current native level/exit namespace and text that cannot be
    /// represented on one bounded manifest line.
    pub fn encode(&self) -> Result<Vec<u8>, LegacyMwlError> {
        self.validate()?;
        if self.version != Self::CURRENT_VERSION {
            return Err(LegacyMwlError::UnsupportedEncodeVersion(self.version));
        }
        let mut output = String::new();
        output.push_str("Lunar Magic Version 3.63\r\n");
        output.push_str(&self.attribution);
        output.push_str("\r\n");
        write!(
            output,
            "{:03X} {:02X} {:02X} {:02X} {:02X} {:02X}\r\n",
            self.level_number,
            self.header[0],
            self.header[1],
            self.header[2],
            self.header[3],
            self.header[4]
        )
        .expect("writing to a String cannot fail");
        encode_sidecar(&mut output, &self.layer1);
        encode_sidecar(&mut output, &self.layer2);
        encode_sidecar(&mut output, &self.sprites);
        for exit in &self.secondary_exits {
            write!(
                output,
                "{:03X} {:02X} {:02X} {:02X}\r\n",
                exit.index,
                exit.position_and_method,
                exit.screen_and_y,
                exit.destination_high_and_flags
            )
            .expect("writing to a String cannot fail");
        }
        if output.len() > Self::MAX_FILE_BYTES {
            return Err(LegacyMwlError::FileTooLarge(output.len()));
        }
        Ok(output.into_bytes())
    }

    /// Derives the optional palette filename exactly as Lunar Magic does on legacy import.
    ///
    /// The final byte of the Layer 1 sidecar name is replaced with `3`.
    ///
    /// # Errors
    ///
    /// Rejects an empty Layer 1 filename.
    pub fn palette_file_name(&self) -> Result<String, LegacyMwlError> {
        let mut bytes = self.layer1.file_name.as_bytes().to_vec();
        let Some(last) = bytes.last_mut() else {
            return Err(LegacyMwlError::UnsafeSidecarName(
                self.layer1.file_name.clone(),
            ));
        };
        *last = b'3';
        String::from_utf8(bytes).map_err(|_| LegacyMwlError::Utf8)
    }
}

fn encode_sidecar(output: &mut String, sidecar: &LegacyMwlSidecar) {
    write!(
        output,
        "{:02X} {:06X} {}\r\n",
        sidecar.flags, sidecar.source_address, sidecar.file_name
    )
    .expect("writing to a String cannot fail");
}

fn parse_sidecar(line: &str) -> Result<LegacyMwlSidecar, LegacyMwlError> {
    let mut fields = line.splitn(3, char::is_whitespace);
    let flags = fields.next().ok_or(LegacyMwlError::SidecarFieldCount)?;
    let source_address = fields.next().ok_or(LegacyMwlError::SidecarFieldCount)?;
    let file_name = fields
        .next()
        .map(str::trim_start)
        .filter(|name| !name.is_empty())
        .ok_or(LegacyMwlError::SidecarFieldCount)?;
    let sidecar = LegacyMwlSidecar {
        flags: parse_hex_u8(flags, "sidecar flags")?,
        source_address: parse_hex_u32(source_address, 0x00ff_ffff, "sidecar address")?,
        file_name: file_name.to_string(),
    };
    validate_sidecar(&sidecar)?;
    Ok(sidecar)
}

fn validate_sidecar(sidecar: &LegacyMwlSidecar) -> Result<(), LegacyMwlError> {
    if sidecar.source_address > 0x00ff_ffff {
        return Err(LegacyMwlError::SidecarAddress(sidecar.source_address));
    }
    validate_text_line(&sidecar.file_name, "sidecar filename")?;
    let path = std::path::Path::new(&sidecar.file_name);
    if path.is_absolute()
        || path.components().count() != 1
        || matches!(
            path.components().next(),
            Some(std::path::Component::ParentDir | std::path::Component::CurDir)
        )
    {
        return Err(LegacyMwlError::UnsafeSidecarName(sidecar.file_name.clone()));
    }
    Ok(())
}

fn validate_text_line(value: &str, field: &'static str) -> Result<(), LegacyMwlError> {
    if value.is_empty()
        || value.len() > LegacyMwlManifest::MAX_LINE_BYTES
        || value.contains(['\r', '\n', '\0'])
    {
        return Err(LegacyMwlError::InvalidTextLine(field));
    }
    Ok(())
}

fn parse_version(line: &str) -> Result<u16, LegacyMwlError> {
    if !line.starts_with("Lunar Magic ") {
        return Err(LegacyMwlError::MissingHeader);
    }
    let bytes = line.as_bytes();
    let parsed = bytes
        .get(20)
        .and_then(|major| std::str::from_utf8(std::slice::from_ref(major)).ok())
        .and_then(|major| u16::from_str_radix(major, 16).ok())
        .zip(
            bytes
                .get(22..24)
                .and_then(|minor| std::str::from_utf8(minor).ok())
                .and_then(|minor| u16::from_str_radix(minor, 16).ok()),
        )
        .map(|(major, minor)| major << 8 | minor);
    Ok(parsed.unwrap_or(0x0132))
}

fn fields(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

fn parse_hex_u8(value: &str, field: &'static str) -> Result<u8, LegacyMwlError> {
    let parsed = parse_hex_u32(value, u32::from(u8::MAX), field)?;
    u8::try_from(parsed).map_err(|_| LegacyMwlError::HexField {
        field,
        value: value.into(),
    })
}

fn parse_hex_u16(value: &str, maximum: u16, field: &'static str) -> Result<u16, LegacyMwlError> {
    let parsed = parse_hex_u32(value, u32::from(maximum), field)?;
    u16::try_from(parsed).map_err(|_| LegacyMwlError::HexField {
        field,
        value: value.into(),
    })
}

fn parse_hex_u32(value: &str, maximum: u32, field: &'static str) -> Result<u32, LegacyMwlError> {
    let parsed = u32::from_str_radix(value, 16).map_err(|_| LegacyMwlError::HexField {
        field,
        value: value.into(),
    })?;
    if parsed > maximum {
        return Err(LegacyMwlError::HexField {
            field,
            value: value.into(),
        });
    }
    Ok(parsed)
}

struct LogicalLines<'a> {
    lines: std::str::Lines<'a>,
}

impl<'a> LogicalLines<'a> {
    fn new(text: &'a str) -> Result<Self, LegacyMwlError> {
        if text.contains('\0') {
            return Err(LegacyMwlError::Utf8);
        }
        Ok(Self {
            lines: text.lines(),
        })
    }

    fn next_header(&mut self) -> Result<Option<&'a str>, LegacyMwlError> {
        let Some(raw) = self.lines.next() else {
            return Ok(None);
        };
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.len() > LegacyMwlManifest::MAX_LINE_BYTES {
            return Err(LegacyMwlError::LineTooLong(line.len()));
        }
        Ok(Some(line))
    }

    fn next_data(&mut self) -> Result<Option<&'a str>, LegacyMwlError> {
        for raw in self.lines.by_ref() {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            if line.len() > LegacyMwlManifest::MAX_LINE_BYTES {
                return Err(LegacyMwlError::LineTooLong(line.len()));
            }
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            return Ok(Some(line));
        }
        Ok(None)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyMwlError {
    FileTooLarge(usize),
    LineTooLong(usize),
    Utf8,
    MissingHeader,
    MissingAttribution,
    MissingLevelFields,
    MissingSidecar(&'static str),
    Version(String),
    UnsupportedVersion(u16),
    UnsupportedEncodeVersion(u16),
    LevelFieldCount { expected: usize, actual: usize },
    LevelNumber(u16),
    SidecarFieldCount,
    SidecarAddress(u32),
    UnsafeSidecarName(String),
    InvalidTextLine(&'static str),
    HexField { field: &'static str, value: String },
    SecondaryExitFieldCount(usize),
    SecondaryExitIndex(u16),
    DuplicateSecondaryExit(u16),
    TooManySecondaryExits(usize),
}

impl fmt::Display for LegacyMwlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid legacy MWL manifest: {self:?}")
    }
}

impl std::error::Error for LegacyMwlError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> &'static [u8] {
        b"Lunar Magic Version 3.63\r\n\
          \xC2\xA92025 FuSoYa, Defender of Relm\r\n\
          105 5B 00 9A 00 00\r\n\
          00 0688DD Level 105.mw0\r\n\
          0C FFD900 Level 105.mw1\r\n\
          00 07C4CA Level 105.mw2\r\n\
          1CB A9 08 0E\r\n"
    }

    #[test]
    fn live_lunar_magic_fixture_round_trips_exactly() {
        let manifest = LegacyMwlManifest::decode(fixture()).unwrap();
        assert_eq!(manifest.version, 0x0363);
        assert_eq!(manifest.level_number, 0x105);
        assert_eq!(manifest.header, [0x5b, 0, 0x9a, 0, 0]);
        assert_eq!(manifest.layer2.flags, 0x0c);
        assert_eq!(manifest.layer2.source_address, 0xff_d900);
        assert_eq!(manifest.secondary_exits[0].index, 0x1cb);
        assert_eq!(manifest.palette_file_name().unwrap(), "Level 105.mw3");
        assert_eq!(manifest.encode().unwrap(), fixture());
    }

    #[test]
    fn comments_lf_and_pre_132_shapes_are_accepted_without_weakening_bounds() {
        let bytes = b"Lunar Magic Version 1.31\n\
                      // comment\n\
                      attribution\n\
                      105 01 02 03 04\n\
                      00 000001 a.mw0\n\
                      00 000002 a.mw1\n\
                      00 000003 a.mw2\n\
                      CB 01 02 03\n";
        let manifest = LegacyMwlManifest::decode(bytes).unwrap();
        assert_eq!(manifest.header, [1, 2, 3, 4, 0]);
        assert_eq!(manifest.secondary_exits[0].index, 0x1cb);
        assert!(matches!(
            manifest.encode(),
            Err(LegacyMwlError::UnsupportedEncodeVersion(0x0131))
        ));
    }

    #[test]
    fn malformed_version_defaults_to_132_and_future_versions_use_the_current_shape() {
        let malformed = std::str::from_utf8(fixture())
            .unwrap()
            .replace("Lunar Magic Version 3.63", "Lunar Magic Version nope");
        let decoded = LegacyMwlManifest::decode(malformed.as_bytes()).unwrap();
        assert_eq!(decoded.version, 0x0132);
        assert_eq!(decoded.header, [0x5b, 0, 0x9a, 0, 0]);
        assert_eq!(decoded.secondary_exits[0].index, 0x1cb);

        let future = std::str::from_utf8(fixture())
            .unwrap()
            .replace("Lunar Magic Version 3.63", "Lunar Magic Version 9.99");
        let decoded = LegacyMwlManifest::decode(future.as_bytes()).unwrap();
        assert_eq!(decoded.version, 0x0999);
        assert!(matches!(
            decoded.encode(),
            Err(LegacyMwlError::UnsupportedEncodeVersion(0x0999))
        ));

        let wrong_prefix = std::str::from_utf8(fixture())
            .unwrap()
            .replace("Lunar Magic ", "Lunar Tragic ");
        assert!(matches!(
            LegacyMwlManifest::decode(wrong_prefix.as_bytes()),
            Err(LegacyMwlError::MissingHeader)
        ));

        let leading_comment = [b"// comment before signature\n".as_slice(), fixture()].concat();
        assert!(matches!(
            LegacyMwlManifest::decode(&leading_comment),
            Err(LegacyMwlError::MissingHeader)
        ));
    }

    #[test]
    fn malformed_fields_and_unsafe_names_are_rejected() {
        let unsafe_name = std::str::from_utf8(fixture())
            .unwrap()
            .replace("Level 105.mw0", "../evil.mw0");
        assert!(matches!(
            LegacyMwlManifest::decode(unsafe_name.as_bytes()),
            Err(LegacyMwlError::UnsafeSidecarName(_))
        ));
        let trailing = std::str::from_utf8(fixture())
            .unwrap()
            .replace("105 5B 00 9A 00 00", "105 5B 00 9A 00 00 FF");
        assert!(matches!(
            LegacyMwlManifest::decode(trailing.as_bytes()),
            Err(LegacyMwlError::LevelFieldCount { .. })
        ));
    }

    #[test]
    fn source_level_and_secondary_exit_recovery_match_prompt_and_continue_import() {
        let mut input = std::str::from_utf8(fixture())
            .unwrap()
            .replace("105 5B", "FFF 5B")
            .into_bytes();
        input.extend_from_slice(b"not an exit\r\n1CB 11 22 33\r\n");
        let report = LegacyMwlManifest::decode_with_diagnostics(&input).unwrap();
        assert_eq!(report.manifest.level_number, 0x01ff);
        assert_eq!(report.manifest.secondary_exits.len(), 1);
        assert_eq!(
            report.manifest.secondary_exits[0],
            LegacyMwlSecondaryExit {
                index: 0x1cb,
                position_and_method: 0x11,
                screen_and_y: 0x22,
                destination_high_and_flags: 0x33,
            }
        );
        assert_eq!(
            report.diagnostics,
            [
                LegacyMwlDiagnostic::LevelNumberClamped {
                    source: 0x0fff,
                    target: 0x01ff,
                },
                LegacyMwlDiagnostic::IgnoredSecondaryExit { line: 2 },
                LegacyMwlDiagnostic::ReplacedSecondaryExit { index: 0x1cb },
            ]
        );
    }
}
