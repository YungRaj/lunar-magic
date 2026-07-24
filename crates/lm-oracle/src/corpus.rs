use crate::{ManifestError, OracleManifest};
use std::collections::{BTreeMap, BTreeSet};

/// One independently required dimension of an external Lunar Magic fixture corpus.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CorpusRequirement {
    LunarMagicVersion(String),
    Operation(String),
    Argument { name: String, value: String },
}

impl CorpusRequirement {
    #[must_use]
    pub fn is_satisfied_by(&self, manifest: &OracleManifest) -> bool {
        match self {
            Self::LunarMagicVersion(version) => manifest.lunar_magic_version == *version,
            Self::Operation(operation) => manifest.operation.name == *operation,
            Self::Argument { name, value } => manifest
                .operation
                .arguments
                .iter()
                .any(|(actual_name, actual_value)| actual_name == name && actual_value == value),
        }
    }
}

/// Coverage requirements intentionally supplied by the fixture owner.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CorpusPolicy {
    pub requirements: BTreeSet<CorpusRequirement>,
}

/// Coverage evidence independent from whether individual byte/semantic comparisons pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusCoverageReport {
    pub case_count: usize,
    pub valid_case_count: usize,
    pub missing: Vec<CorpusRequirement>,
    pub duplicate_case_ids: Vec<String>,
    pub invalid_cases: Vec<InvalidCorpusCase>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidCorpusCase {
    pub index: usize,
    pub case_id: String,
    pub error: ManifestError,
}

impl CorpusCoverageReport {
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        self.valid_case_count != 0
            && self.missing.is_empty()
            && self.duplicate_case_ids.is_empty()
            && self.invalid_cases.is_empty()
    }
}

/// Audits corpus breadth without opening or embedding copyrighted ROMs.
///
/// Each requirement is existential: at least one manifest must provide that version, operation, or
/// argument tag. Duplicate case IDs are rejected because they make reports and fixture provenance
/// ambiguous even when stored in different directories.
#[must_use]
pub fn audit_corpus(policy: &CorpusPolicy, manifests: &[OracleManifest]) -> CorpusCoverageReport {
    let mut valid = Vec::with_capacity(manifests.len());
    let mut invalid_cases = Vec::new();
    for (index, manifest) in manifests.iter().enumerate() {
        match manifest.validate() {
            Ok(()) => valid.push(manifest),
            Err(error) => invalid_cases.push(InvalidCorpusCase {
                index,
                case_id: manifest.case_id.clone(),
                error,
            }),
        }
    }
    let missing = policy
        .requirements
        .iter()
        .filter(|requirement| {
            !valid
                .iter()
                .any(|manifest| requirement.is_satisfied_by(manifest))
        })
        .cloned()
        .collect();
    let mut counts = BTreeMap::<&str, usize>::new();
    for manifest in &valid {
        *counts.entry(&manifest.case_id).or_default() += 1;
    }
    let duplicate_case_ids = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(case_id, _)| case_id.to_owned())
        .collect();
    CorpusCoverageReport {
        case_count: manifests.len(),
        valid_case_count: valid.len(),
        missing,
        duplicate_case_ids,
        invalid_cases,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Operation;

    fn manifest(
        id: &str,
        version: &str,
        operation: &str,
        arguments: &[(&str, &str)],
    ) -> OracleManifest {
        OracleManifest {
            case_id: id.into(),
            lunar_magic_version: version.into(),
            input_sha256: "00".repeat(32),
            output_sha256: "11".repeat(32),
            operation: Operation {
                name: operation.into(),
                arguments: arguments
                    .iter()
                    .map(|(name, value)| ((*name).into(), (*value).into()))
                    .collect(),
            },
            changed_ranges: vec![],
            decoded_before: String::new(),
            decoded_after: String::new(),
            owned_allocations_before: vec![],
            owned_allocations_after: vec![],
            warnings: vec![],
            errors: vec![],
        }
    }

    #[test]
    fn multi_axis_requirements_can_be_satisfied_by_distinct_cases() {
        let policy = CorpusPolicy {
            requirements: [
                CorpusRequirement::LunarMagicVersion("3.40".into()),
                CorpusRequirement::Operation("level-save".into()),
                CorpusRequirement::Argument {
                    name: "mapper".into(),
                    value: "sa1".into(),
                },
                CorpusRequirement::Argument {
                    name: "fixture_family".into(),
                    value: "ecosystem-modified".into(),
                },
            ]
            .into_iter()
            .collect(),
        };
        let manifests = [
            manifest("clean", "3.40", "level-save", &[("mapper", "lorom")]),
            manifest(
                "patched",
                "3.33",
                "map16-save",
                &[("mapper", "sa1"), ("fixture_family", "ecosystem-modified")],
            ),
        ];
        let report = audit_corpus(&policy, &manifests);
        assert!(report.is_satisfied());
        assert_eq!(report.case_count, 2);
        assert_eq!(report.valid_case_count, 2);
    }

    #[test]
    fn missing_dimensions_and_duplicate_case_ids_are_reported_stably() {
        let policy = CorpusPolicy {
            requirements: [
                CorpusRequirement::Operation("overworld-save".into()),
                CorpusRequirement::Argument {
                    name: "header".into(),
                    value: "copier".into(),
                },
            ]
            .into_iter()
            .collect(),
        };
        let manifests = [
            manifest("same", "3.40", "level-save", &[]),
            manifest("same", "3.40", "map16-save", &[]),
        ];
        let report = audit_corpus(&policy, &manifests);
        assert!(!report.is_satisfied());
        assert_eq!(report.missing.len(), 2);
        assert_eq!(report.duplicate_case_ids, ["same"]);
        assert!(!audit_corpus(&CorpusPolicy::default(), &[]).is_satisfied());
    }

    #[test]
    fn invalid_programmatic_manifests_cannot_satisfy_coverage() {
        let policy = CorpusPolicy {
            requirements: [CorpusRequirement::Operation("level-save".into())]
                .into_iter()
                .collect(),
        };
        let mut invalid = manifest("bad", "3.40", "level-save", &[("mapper", "lorom")]);
        invalid
            .operation
            .arguments
            .push(("mapper".into(), "sa1".into()));
        let report = audit_corpus(&policy, &[invalid]);
        assert_eq!(report.case_count, 1);
        assert_eq!(report.valid_case_count, 0);
        assert_eq!(
            report.missing,
            [CorpusRequirement::Operation("level-save".into())]
        );
        assert_eq!(
            report.invalid_cases,
            [InvalidCorpusCase {
                index: 0,
                case_id: "bad".into(),
                error: ManifestError::DuplicateArgumentName("mapper".into()),
            }]
        );
        assert!(!report.is_satisfied());
    }
}
