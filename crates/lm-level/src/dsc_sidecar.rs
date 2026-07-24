use std::fmt;

/// Defensive bound for an external text sidecar. The native loader itself reads incrementally.
pub const MAX_DSC_SOURCE_LEN: usize = 4 * 1024 * 1024;

/// One valid directive recovered from Lunar Magic's `.dsc` reader.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DscDirective {
    Description(DscDescription),
    DisplayMapping(u16),
    AlternateMapping(u16),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DscEntry {
    pub key: u16,
    /// Complete source flags, including bits not interpreted by this version.
    pub flags: u32,
    pub directive: DscDirective,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DscDescription {
    pub text: String,
    /// Values supplied by `\b`, `\d`, `\f`, and `\m`, respectively.
    /// Missing values inherit editor UI defaults when the table is resolved.
    pub background: Option<u32>,
    pub detail: Option<u32>,
    pub foreground: Option<u32>,
    pub mode: Option<u32>,
}

/// Lossless source plus Lunar Magic-compatible interpretations of valid records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DscSidecar {
    source: Vec<u8>,
    entries: Vec<DscEntry>,
}

impl DscSidecar {
    /// Parses the optional UTF-8 BOM and tab-separated hexadecimal records used by Lunar Magic.
    /// Malformed records are preserved in `source()` but omitted from `entries()`, matching the
    /// native reader's skip-to-next-line behavior.
    ///
    /// # Errors
    ///
    /// Rejects an input larger than [`MAX_DSC_SOURCE_LEN`].
    pub fn decode(source: &[u8]) -> Result<Self, DscSidecarError> {
        if source.len() > MAX_DSC_SOURCE_LEN {
            return Err(DscSidecarError::SourceTooLarge(source.len()));
        }
        let body = source.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(source);
        let entries = body
            .split(|byte| *byte == b'\n')
            .filter_map(parse_line)
            .collect();
        Ok(Self {
            source: source.to_vec(),
            entries,
        })
    }

    #[must_use]
    pub fn source(&self) -> &[u8] {
        &self.source
    }

    #[must_use]
    pub fn encode_lossless(&self) -> Vec<u8> {
        self.source.clone()
    }

    #[must_use]
    pub fn entries(&self) -> &[DscEntry] {
        &self.entries
    }
}

fn parse_line(line: &[u8]) -> Option<DscEntry> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let mut fields = line.splitn(3, |byte| *byte == b'\t');
    let key = parse_hex(fields.next()?)?;
    let flags = parse_hex(fields.next()?)?;
    let payload = fields.next().unwrap_or_default();
    if key >= 0x8000 {
        return None;
    }

    let directive = if flags & 0x16 == 0 {
        DscDirective::Description(parse_description(payload))
    } else {
        let value = u16::try_from(parse_leading_hex(payload)? & 0x7fff).ok()?;
        if flags & 0x10 != 0 {
            if value >= 0x3d00 {
                return None;
            }
            DscDirective::AlternateMapping(value)
        } else {
            DscDirective::DisplayMapping(value)
        }
    };
    Some(DscEntry {
        key: u16::try_from(key).ok()?,
        flags,
        directive,
    })
}

fn parse_description(payload: &[u8]) -> DscDescription {
    let mut output = Vec::with_capacity(payload.len());
    let mut background = None;
    let mut detail = None;
    let mut foreground = None;
    let mut mode = None;
    let mut index = 0;
    while index < payload.len() && output.len() < 0x5ff {
        let byte = payload[index];
        index += 1;
        if byte != b'\\' || index >= payload.len() {
            output.push(byte);
            continue;
        }
        let escape = payload[index];
        index += 1;
        match escape {
            b'\\' => output.push(b'\\'),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b'b' | b'd' | b'f' | b'm' => {
                let (value, consumed) = parse_six_hex(&payload[index..]);
                index += consumed;
                match escape {
                    b'b' => background = value,
                    b'd' => detail = value,
                    b'f' => foreground = value,
                    b'm' => mode = value,
                    _ => unreachable!(),
                }
            }
            _ => output.push(b' '),
        }
    }
    DscDescription {
        text: String::from_utf8_lossy(&output).into_owned(),
        background,
        detail,
        foreground,
        mode,
    }
}

fn parse_six_hex(input: &[u8]) -> (Option<u32>, usize) {
    let length = input
        .iter()
        .take(6)
        .take_while(|byte| byte.is_ascii_hexdigit())
        .count();
    (parse_hex(&input[..length]), length)
}

fn parse_leading_hex(input: &[u8]) -> Option<u32> {
    let input = input.strip_prefix(b" ").unwrap_or(input);
    let length = input.iter().take_while(|b| b.is_ascii_hexdigit()).count();
    parse_hex(&input[..length])
}

fn parse_hex(input: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(input).ok()?.trim();
    (!text.is_empty()).then(|| u32::from_str_radix(text, 16).ok())?
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DscSidecarError {
    SourceTooLarge(usize),
}

impl fmt::Display for DscSidecarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid DSC sidecar: {self:?}")
    }
}

impl std::error::Error for DscSidecarError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_source_and_parses_bom_crlf_and_escapes() {
        let source =
            b"\xef\xbb\xbf0123\t28\tHello\\nworld\\\\!\\b112233\\d445566\\f778899\\mABCDEF\r\n";
        let file = DscSidecar::decode(source).unwrap();
        assert_eq!(file.encode_lossless(), source);
        let DscDirective::Description(description) = &file.entries()[0].directive else {
            panic!("expected description")
        };
        assert_eq!(description.text, "Hello\nworld\\!");
        assert_eq!(description.background, Some(0x0011_2233));
        assert_eq!(description.detail, Some(0x0044_5566));
        assert_eq!(description.foreground, Some(0x0077_8899));
        assert_eq!(description.mode, Some(0x00ab_cdef));
    }

    #[test]
    fn distinguishes_mapping_forms_and_masks_the_high_bit() {
        let file = DscSidecar::decode(b"10\t2\tFFFF\n11\t10\tBCFF\n12\t10\tBD00\n").unwrap();
        assert_eq!(file.entries().len(), 2);
        assert_eq!(
            file.entries()[0].directive,
            DscDirective::DisplayMapping(0x7fff)
        );
        assert_eq!(
            file.entries()[1].directive,
            DscDirective::AlternateMapping(0x3cff)
        );
    }

    #[test]
    fn ignores_malformed_and_out_of_range_lines() {
        let file = DscSidecar::decode(b"nope\n8000\t0\tbad\n7fff\t0\tgood\n").unwrap();
        assert_eq!(file.entries().len(), 1);
        assert_eq!(file.entries()[0].key, 0x7fff);
    }
}
