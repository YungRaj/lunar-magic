use std::collections::BTreeSet;
use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Operation {
    pub name: String,
    pub arguments: Vec<(String, String)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleManifest {
    pub case_id: String,
    pub lunar_magic_version: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub operation: Operation,
    pub changed_ranges: Vec<Range<usize>>,
    pub decoded_before: String,
    pub decoded_after: String,
    pub owned_allocations_before: Vec<Range<usize>>,
    pub owned_allocations_after: Vec<Range<usize>>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestError {
    InvalidHeader,
    InvalidLine(String),
    InvalidHex(String),
    MissingField(&'static str),
    DuplicateField(&'static str),
    EmptyField(&'static str),
    InvalidSha256(&'static str),
    EmptyArgumentName(usize),
    DuplicateArgumentName(String),
    InvalidRange {
        field: &'static str,
        index: usize,
        start: usize,
        end: usize,
    },
    NonCanonicalRanges {
        field: &'static str,
        index: usize,
    },
    InputTooLarge(usize),
    ValueTooLarge(usize),
    TooManyRecords(usize),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid oracle manifest: {self:?}")
    }
}

impl std::error::Error for ManifestError {}

impl OracleManifest {
    const HEADER: &'static str = "LMORACLE1";
    pub const MAX_TEXT_BYTES: usize = 64 * 1024 * 1024;
    pub const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;
    pub const MAX_RECORDS: usize = 1_000_000;

    /// Validates stable fixture identity and unambiguous operation arguments.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] for empty identity fields, noncanonical SHA-256 values, or empty
    /// and duplicate argument names.
    pub fn validate(&self) -> Result<(), ManifestError> {
        for (field, value) in [
            ("case_id", self.case_id.as_str()),
            ("version", self.lunar_magic_version.as_str()),
            ("operation", self.operation.name.as_str()),
        ] {
            if value.is_empty() {
                return Err(ManifestError::EmptyField(field));
            }
        }
        for (field, value) in [
            ("input_sha256", self.input_sha256.as_str()),
            ("output_sha256", self.output_sha256.as_str()),
        ] {
            if value.len() != 64
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(ManifestError::InvalidSha256(field));
            }
        }
        let mut argument_names = BTreeSet::new();
        for (index, (name, _)) in self.operation.arguments.iter().enumerate() {
            if name.is_empty() {
                return Err(ManifestError::EmptyArgumentName(index));
            }
            if !argument_names.insert(name) {
                return Err(ManifestError::DuplicateArgumentName(name.clone()));
            }
        }
        validate_ranges("changed_range", &self.changed_ranges)?;
        validate_ranges("owned_before", &self.owned_allocations_before)?;
        validate_ranges("owned_after", &self.owned_allocations_after)?;
        Ok(())
    }

    /// Serializes the manifest into a deterministic, line-oriented interchange format.
    /// Values are hex-encoded UTF-8, so newlines and delimiters remain lossless.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut output = String::from(Self::HEADER);
        output.push('\n');
        append(&mut output, "case_id", &self.case_id);
        append(&mut output, "version", &self.lunar_magic_version);
        append(&mut output, "input_sha256", &self.input_sha256);
        append(&mut output, "output_sha256", &self.output_sha256);
        append(&mut output, "operation", &self.operation.name);
        for (name, value) in &self.operation.arguments {
            append(&mut output, "argument_name", name);
            append(&mut output, "argument_value", value);
        }
        append_ranges(&mut output, "changed_range", &self.changed_ranges);
        append(&mut output, "decoded_before", &self.decoded_before);
        append(&mut output, "decoded_after", &self.decoded_after);
        append_ranges(&mut output, "owned_before", &self.owned_allocations_before);
        append_ranges(&mut output, "owned_after", &self.owned_allocations_after);
        for warning in &self.warnings {
            append(&mut output, "warning", warning);
        }
        for error in &self.errors {
            append(&mut output, "error", error);
        }
        output
    }

    /// Parses the deterministic manifest format emitted by [`Self::to_text`].
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] for malformed, incomplete, or invalid UTF-8 input.
    pub fn from_text(text: &str) -> Result<Self, ManifestError> {
        Self::from_text_with_limits(
            text,
            ParseLimits {
                text_bytes: Self::MAX_TEXT_BYTES,
                value_bytes: Self::MAX_VALUE_BYTES,
                records: Self::MAX_RECORDS,
            },
        )
    }

    fn from_text_with_limits(text: &str, limits: ParseLimits) -> Result<Self, ManifestError> {
        if text.len() > limits.text_bytes {
            return Err(ManifestError::InputTooLarge(text.len()));
        }
        let mut lines = text.lines();
        if lines.next() != Some(Self::HEADER) {
            return Err(ManifestError::InvalidHeader);
        }
        let mut case_id = None;
        let mut version = None;
        let mut input = None;
        let mut output = None;
        let mut operation = None;
        let mut arguments = Vec::new();
        let mut pending_argument = None;
        let mut changed_ranges = Vec::new();
        let mut decoded_before = None;
        let mut decoded_after = None;
        let mut owned_before = Vec::new();
        let mut owned_after = Vec::new();
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let mut records = 0_usize;
        for line in lines {
            records = records.saturating_add(1);
            if records > limits.records {
                return Err(ManifestError::TooManyRecords(records));
            }
            let (kind, encoded) = line
                .split_once('=')
                .ok_or_else(|| ManifestError::InvalidLine(line.to_owned()))?;
            let value = decode_hex(encoded, limits.value_bytes)?;
            match kind {
                "case_id" => set_once(&mut case_id, value, "case_id")?,
                "version" => set_once(&mut version, value, "version")?,
                "input_sha256" => set_once(&mut input, value, "input_sha256")?,
                "output_sha256" => set_once(&mut output, value, "output_sha256")?,
                "operation" => set_once(&mut operation, value, "operation")?,
                "argument_name" if pending_argument.is_none() => pending_argument = Some(value),
                "argument_value" => {
                    let name = pending_argument
                        .take()
                        .ok_or_else(|| ManifestError::InvalidLine(line.to_owned()))?;
                    arguments.push((name, value));
                }
                "changed_range" => changed_ranges.push(parse_range(&value)?),
                "decoded_before" => set_once(&mut decoded_before, value, "decoded_before")?,
                "decoded_after" => set_once(&mut decoded_after, value, "decoded_after")?,
                "owned_before" => owned_before.push(parse_range(&value)?),
                "owned_after" => owned_after.push(parse_range(&value)?),
                "warning" => warnings.push(value),
                "error" => errors.push(value),
                _ => return Err(ManifestError::InvalidLine(line.to_owned())),
            }
        }
        if pending_argument.is_some() {
            return Err(ManifestError::MissingField("argument_value"));
        }
        let manifest = Self {
            case_id: case_id.ok_or(ManifestError::MissingField("case_id"))?,
            lunar_magic_version: version.ok_or(ManifestError::MissingField("version"))?,
            input_sha256: input.ok_or(ManifestError::MissingField("input_sha256"))?,
            output_sha256: output.ok_or(ManifestError::MissingField("output_sha256"))?,
            operation: Operation {
                name: operation.ok_or(ManifestError::MissingField("operation"))?,
                arguments,
            },
            changed_ranges,
            decoded_before: decoded_before.ok_or(ManifestError::MissingField("decoded_before"))?,
            decoded_after: decoded_after.ok_or(ManifestError::MissingField("decoded_after"))?,
            owned_allocations_before: owned_before,
            owned_allocations_after: owned_after,
            warnings,
            errors,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

fn validate_ranges(field: &'static str, ranges: &[Range<usize>]) -> Result<(), ManifestError> {
    for (index, range) in ranges.iter().enumerate() {
        if range.start >= range.end {
            return Err(ManifestError::InvalidRange {
                field,
                index,
                start: range.start,
                end: range.end,
            });
        }
        if index > 0 && ranges[index - 1].end > range.start {
            return Err(ManifestError::NonCanonicalRanges { field, index });
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ParseLimits {
    text_bytes: usize,
    value_bytes: usize,
    records: usize,
}

fn set_once(
    target: &mut Option<String>,
    value: String,
    field: &'static str,
) -> Result<(), ManifestError> {
    if target.is_some() {
        return Err(ManifestError::DuplicateField(field));
    }
    *target = Some(value);
    Ok(())
}

fn append_ranges(output: &mut String, kind: &str, ranges: &[Range<usize>]) {
    for range in ranges {
        append(output, kind, &format!("{:x}:{:x}", range.start, range.end));
    }
}

fn parse_range(value: &str) -> Result<Range<usize>, ManifestError> {
    let (start, end) = value
        .split_once(':')
        .ok_or_else(|| ManifestError::InvalidLine(value.to_owned()))?;
    let start = usize::from_str_radix(start, 16)
        .map_err(|_| ManifestError::InvalidLine(value.to_owned()))?;
    let end =
        usize::from_str_radix(end, 16).map_err(|_| ManifestError::InvalidLine(value.to_owned()))?;
    if start > end {
        return Err(ManifestError::InvalidLine(value.to_owned()));
    }
    Ok(start..end)
}

fn append(output: &mut String, kind: &str, value: &str) {
    output.push_str(kind);
    output.push('=');
    for byte in value.as_bytes() {
        use std::fmt::Write;
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output.push('\n');
}

fn decode_hex(encoded: &str, maximum_bytes: usize) -> Result<String, ManifestError> {
    if encoded.len() % 2 != 0 {
        return Err(ManifestError::InvalidHex(encoded.to_owned()));
    }
    let decoded_len = encoded.len() / 2;
    if decoded_len > maximum_bytes {
        return Err(ManifestError::ValueTooLarge(decoded_len));
    }
    let bytes = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair)
                .map_err(|_| ManifestError::InvalidHex(encoded.to_owned()))?;
            u8::from_str_radix(text, 16).map_err(|_| ManifestError::InvalidHex(encoded.to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|error| ManifestError::InvalidHex(error.to_string()))
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
