use crate::OracleManifest;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A deterministic decoded-model snapshot independent of physical ROM allocation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Observation {
    entries: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationError {
    InvalidHeader,
    InvalidLine(String),
    InvalidHex(String),
    InvalidUtf8(String),
    DuplicatePath(String),
    InputTooLarge(usize),
    ComponentTooLarge(usize),
    TooManyEntries(usize),
}

impl fmt::Display for ObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid semantic observation: {self:?}")
    }
}

impl std::error::Error for ObservationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationDifference {
    pub path: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticVerificationReport {
    pub before: Vec<ObservationDifference>,
    pub after: Vec<ObservationDifference>,
}

impl SemanticVerificationReport {
    #[must_use]
    pub fn is_match(&self) -> bool {
        self.before.is_empty() && self.after.is_empty()
    }
}

impl Observation {
    const HEADER: &'static str = "LMOBS1";
    pub const MAX_TEXT_BYTES: usize = 128 * 1024 * 1024;
    pub const MAX_COMPONENT_BYTES: usize = 16 * 1024 * 1024;
    pub const MAX_ENTRIES: usize = 1_000_000;

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one unique semantic path.
    ///
    /// # Errors
    ///
    /// Returns [`ObservationError::DuplicatePath`] if the path was already recorded.
    pub fn insert(
        &mut self,
        path: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), ObservationError> {
        let path = path.into();
        if self.entries.contains_key(&path) {
            return Err(ObservationError::DuplicatePath(path));
        }
        self.entries.insert(path, value.into());
        Ok(())
    }

    #[must_use]
    pub fn get(&self, path: &str) -> Option<&str> {
        self.entries.get(path).map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|(path, value)| (path.as_str(), value.as_str()))
    }

    /// Emits entries in canonical path order with hex-encoded UTF-8 path and value fields.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut result = String::from(Self::HEADER);
        result.push('\n');
        for (path, value) in &self.entries {
            result.push_str(&encode_hex(path));
            result.push('=');
            result.push_str(&encode_hex(value));
            result.push('\n');
        }
        result
    }

    /// Parses one canonical observation. Input order is accepted but output is always sorted.
    ///
    /// # Errors
    ///
    /// Returns [`ObservationError`] for malformed, non-UTF-8, or duplicate entries.
    pub fn from_text(text: &str) -> Result<Self, ObservationError> {
        Self::from_text_with_limits(
            text,
            ParseLimits {
                text_bytes: Self::MAX_TEXT_BYTES,
                component_bytes: Self::MAX_COMPONENT_BYTES,
                entries: Self::MAX_ENTRIES,
            },
        )
    }

    fn from_text_with_limits(text: &str, limits: ParseLimits) -> Result<Self, ObservationError> {
        if text.len() > limits.text_bytes {
            return Err(ObservationError::InputTooLarge(text.len()));
        }
        let mut lines = text.lines();
        if lines.next() != Some(Self::HEADER) {
            return Err(ObservationError::InvalidHeader);
        }
        let mut observation = Self::new();
        for line in lines {
            let count = observation.len().saturating_add(1);
            if count > limits.entries {
                return Err(ObservationError::TooManyEntries(count));
            }
            let (path, value) = line
                .split_once('=')
                .ok_or_else(|| ObservationError::InvalidLine(line.to_owned()))?;
            observation.insert(
                decode_hex(path, limits.component_bytes)?,
                decode_hex(value, limits.component_bytes)?,
            )?;
        }
        Ok(observation)
    }

    #[must_use]
    pub fn differences(&self, actual: &Self) -> Vec<ObservationDifference> {
        self.entries
            .keys()
            .chain(actual.entries.keys())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|path| {
                let expected = self.entries.get(path);
                let actual = actual.entries.get(path);
                (expected != actual).then(|| ObservationDifference {
                    path: path.clone(),
                    expected: expected.cloned(),
                    actual: actual.cloned(),
                })
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
struct ParseLimits {
    text_bytes: usize,
    component_bytes: usize,
    entries: usize,
}

/// Compares decoded before/after observations against those embedded in a manifest.
///
/// # Errors
///
/// Returns [`ObservationError`] if either expected manifest snapshot is malformed.
pub fn verify_semantic_observations(
    manifest: &OracleManifest,
    actual_before: &Observation,
    actual_after: &Observation,
) -> Result<SemanticVerificationReport, ObservationError> {
    let expected_before = Observation::from_text(&manifest.decoded_before)?;
    let expected_after = Observation::from_text(&manifest.decoded_after)?;
    Ok(SemanticVerificationReport {
        before: expected_before.differences(actual_before),
        after: expected_after.differences(actual_after),
    })
}

fn encode_hex(value: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn decode_hex(encoded: &str, maximum_bytes: usize) -> Result<String, ObservationError> {
    if encoded.len() % 2 != 0 {
        return Err(ObservationError::InvalidHex(encoded.to_owned()));
    }
    let decoded_len = encoded.len() / 2;
    if decoded_len > maximum_bytes {
        return Err(ObservationError::ComponentTooLarge(decoded_len));
    }
    let bytes = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair)
                .map_err(|_| ObservationError::InvalidHex(encoded.to_owned()))?;
            u8::from_str_radix(pair, 16)
                .map_err(|_| ObservationError::InvalidHex(encoded.to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|error| ObservationError::InvalidUtf8(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Operation;

    #[test]
    fn observations_are_canonical_and_lossless() {
        let mut observation = Observation::new();
        observation.insert("level/105/music", "雪=3\n").unwrap();
        observation.insert("level/105/object-count", "12").unwrap();
        let text = observation.to_text();
        assert_eq!(Observation::from_text(&text).unwrap(), observation);
        assert!(text.lines().nth(1).unwrap() < text.lines().nth(2).unwrap());
    }

    #[test]
    fn differences_report_missing_added_and_changed_paths() {
        let mut expected = Observation::new();
        expected.insert("a", "1").unwrap();
        expected.insert("b", "2").unwrap();
        let mut actual = Observation::new();
        actual.insert("b", "3").unwrap();
        actual.insert("c", "4").unwrap();
        let differences = expected.differences(&actual);
        assert_eq!(differences.len(), 3);
        assert_eq!(differences[0].expected.as_deref(), Some("1"));
        assert_eq!(differences[2].actual.as_deref(), Some("4"));
    }

    #[test]
    fn manifest_semantics_are_checked_on_both_sides() {
        let mut before = Observation::new();
        before.insert("tile", "1").unwrap();
        let mut after = Observation::new();
        after.insert("tile", "2").unwrap();
        let manifest = OracleManifest {
            case_id: "semantic".into(),
            lunar_magic_version: "3.40".into(),
            input_sha256: String::new(),
            output_sha256: String::new(),
            operation: Operation {
                name: "edit".into(),
                arguments: Vec::new(),
            },
            changed_ranges: Vec::new(),
            decoded_before: before.to_text(),
            decoded_after: after.to_text(),
            owned_allocations_before: Vec::new(),
            owned_allocations_after: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        };
        assert!(
            verify_semantic_observations(&manifest, &before, &after)
                .unwrap()
                .is_match()
        );
        after.insert("extra", "unexpected").unwrap();
        assert!(
            !verify_semantic_observations(&manifest, &before, &after)
                .unwrap()
                .is_match()
        );
    }

    #[test]
    fn duplicates_and_invalid_utf8_are_rejected() {
        assert!(matches!(Observation::from_text("LMOBS1\n61=31\n61=32\n"),
            Err(ObservationError::DuplicatePath(path)) if path == "a"));
        assert!(matches!(
            Observation::from_text("LMOBS1\nff=31\n"),
            Err(ObservationError::InvalidUtf8(_))
        ));
    }

    #[test]
    fn observation_parser_bounds_text_components_and_entry_count() {
        let limits = ParseLimits {
            text_bytes: 100,
            component_bytes: 2,
            entries: 1,
        };
        assert_eq!(
            Observation::from_text_with_limits("LMOBS1\n616263=31\n", limits),
            Err(ObservationError::ComponentTooLarge(3))
        );
        assert_eq!(
            Observation::from_text_with_limits("LMOBS1\n61=31\n62=32\n", limits),
            Err(ObservationError::TooManyEntries(2))
        );
        assert_eq!(
            Observation::from_text_with_limits(
                "LMOBS1\n",
                ParseLimits {
                    text_bytes: 4,
                    ..limits
                }
            ),
            Err(ObservationError::InputTooLarge(7))
        );
    }
}
