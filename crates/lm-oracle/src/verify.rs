use crate::{
    ByteDifference, Observation, ObservationError, OracleManifest, OwnedTaggedRangeReport,
    SemanticVerificationReport, TaggedAllocationDiff, compare_bytes, compare_tagged_allocations,
    sha256_hex, unexpected_differences, verify_owned_tagged_ranges, verify_semantic_observations,
};
use std::ops::Range;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VerificationReport {
    pub actual_changed_ranges: Vec<ByteDifference>,
    pub expected_changed_ranges: Vec<Range<usize>>,
    pub unexpected_changes: Vec<ByteDifference>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleCaseReport {
    pub manifest_valid: bool,
    pub input_hash_matches: bool,
    pub output_hash_matches: bool,
    pub recorded_errors: Vec<String>,
    pub changes: VerificationReport,
    pub tagged_allocations: TaggedAllocationDiff,
    pub owned_tagged_ranges: OwnedTaggedRangeReport,
    pub semantics: Option<SemanticVerificationReport>,
}

impl OracleCaseReport {
    #[must_use]
    pub fn is_match(&self) -> bool {
        self.input_hash_matches
            && self.manifest_valid
            && self.output_hash_matches
            && self.recorded_errors.is_empty()
            && self.changes.is_match()
            && self.owned_tagged_ranges.is_valid()
            && self
                .semantics
                .as_ref()
                .is_none_or(SemanticVerificationReport::is_match)
    }
}

impl VerificationReport {
    #[must_use]
    pub fn is_match(&self) -> bool {
        self.unexpected_changes.is_empty()
            && self
                .actual_changed_ranges
                .iter()
                .map(|difference| difference.range.clone())
                .eq(self.expected_changed_ranges.iter().cloned())
    }
}

/// Verifies exact changed ranges and the unchanged-region invariant for one oracle case.
///
/// Owned allocations before and after are permitted because a legal save may relocate a payload.
/// Callers can supply additional checksum or pointer-table ranges owned by the save workflow.
#[must_use]
pub fn verify_manifest_change(
    manifest: &OracleManifest,
    before: &[u8],
    after: &[u8],
    additional_owned_ranges: &[Range<usize>],
) -> VerificationReport {
    let actual_changed_ranges = compare_bytes(before, after);
    let mut allowed = manifest.changed_ranges.clone();
    allowed.extend(manifest.owned_allocations_before.iter().cloned());
    allowed.extend(manifest.owned_allocations_after.iter().cloned());
    allowed.extend(additional_owned_ranges.iter().cloned());
    let unexpected_changes = unexpected_differences(before, after, &allowed);
    VerificationReport {
        actual_changed_ranges,
        expected_changed_ranges: manifest.changed_ranges.clone(),
        unexpected_changes,
    }
}

/// Verifies fixture hashes and all byte ownership assertions in a replayable oracle case.
#[must_use]
pub fn verify_oracle_case(
    manifest: &OracleManifest,
    before: &[u8],
    after: &[u8],
    additional_owned_ranges: &[Range<usize>],
) -> OracleCaseReport {
    OracleCaseReport {
        manifest_valid: manifest.validate().is_ok(),
        input_hash_matches: sha256_hex(before) == manifest.input_sha256,
        output_hash_matches: sha256_hex(after) == manifest.output_sha256,
        recorded_errors: manifest.errors.clone(),
        changes: verify_manifest_change(manifest, before, after, additional_owned_ranges),
        tagged_allocations: compare_tagged_allocations(before, after),
        owned_tagged_ranges: verify_owned_tagged_ranges(
            before,
            after,
            &manifest.owned_allocations_before,
            &manifest.owned_allocations_after,
        ),
        semantics: None,
    }
}

/// Verifies fixture bytes and requires decoded observations to match the manifest snapshots.
///
/// # Errors
///
/// Returns [`ObservationError`] when the manifest's decoded snapshots are not valid observations.
pub fn verify_oracle_case_with_observations(
    manifest: &OracleManifest,
    before: &[u8],
    after: &[u8],
    additional_owned_ranges: &[Range<usize>],
    decoded_before: &Observation,
    decoded_after: &Observation,
) -> Result<OracleCaseReport, ObservationError> {
    let mut report = verify_oracle_case(manifest, before, after, additional_owned_ranges);
    report.semantics = Some(verify_semantic_observations(
        manifest,
        decoded_before,
        decoded_after,
    )?);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Observation, Operation};

    fn manifest(changed_ranges: Vec<Range<usize>>) -> OracleManifest {
        OracleManifest {
            case_id: "case".into(),
            lunar_magic_version: "3.40".into(),
            input_sha256: "in".into(),
            output_sha256: "out".into(),
            operation: Operation {
                name: "edit".into(),
                arguments: Vec::new(),
            },
            changed_ranges,
            decoded_before: "before".into(),
            decoded_after: "after".into(),
            owned_allocations_before: Vec::new(),
            owned_allocations_after: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    #[test]
    fn exact_change_matches_manifest() {
        let before = [0, 0, 0, 0];
        let after = [0, 1, 2, 0];
        assert!(
            verify_manifest_change(&manifest(vec![1..3, 9..9]), &before, &after, &[])
                .unexpected_changes
                .is_empty()
        );
        assert!(
            verify_manifest_change(
                &manifest(std::iter::once(1..3).collect()),
                &before,
                &after,
                &[]
            )
            .is_match()
        );
    }

    #[test]
    fn unrelated_change_is_reported() {
        let report = verify_manifest_change(
            &manifest(vec![1..2, 9..9]),
            &[0, 0, 0, 0],
            &[0, 1, 0, 3],
            &[],
        );
        assert_eq!(
            report.unexpected_changes,
            vec![ByteDifference { range: 3..4 }]
        );
    }

    #[test]
    fn complete_case_checks_hashes_and_ranges() {
        let before = [0, 0, 0];
        let after = [0, 1, 0];
        let mut case = manifest(std::iter::once(1..2).collect());
        case.input_sha256 = sha256_hex(&before);
        case.output_sha256 = sha256_hex(&after);
        assert!(verify_oracle_case(&case, &before, &after, &[]).is_match());
        case.output_sha256 = "wrong".into();
        assert!(!verify_oracle_case(&case, &before, &after, &[]).is_match());
    }

    #[test]
    fn invalid_owned_allocation_claim_fails_the_complete_case() {
        let bytes = [0; 32];
        let mut case = manifest(Vec::new());
        case.input_sha256 = sha256_hex(&bytes);
        case.output_sha256 = sha256_hex(&bytes);
        case.owned_allocations_before.push(4..12);
        let report = verify_oracle_case(&case, &bytes, &bytes, &[]);
        assert!(!report.is_match());
        let expected = 4..12;
        assert_eq!(
            report.owned_tagged_ranges.missing_before.as_slice(),
            std::slice::from_ref(&expected)
        );
    }

    #[test]
    fn recorded_oracle_errors_invalidate_an_otherwise_exact_case() {
        let bytes = [0; 8];
        let mut case = manifest(Vec::new());
        case.input_sha256 = sha256_hex(&bytes);
        case.output_sha256 = sha256_hex(&bytes);
        case.errors
            .push("Lunar Magic rejected the operation".into());
        let report = verify_oracle_case(&case, &bytes, &bytes, &[]);
        assert!(report.input_hash_matches && report.output_hash_matches);
        assert!(report.changes.is_match());
        assert_eq!(report.recorded_errors, case.errors);
        assert!(!report.is_match());
    }

    #[test]
    fn invalid_programmatic_range_claims_cannot_pass_replay() {
        let bytes = [0; 8];
        for ranges in [
            std::iter::once(2..2).collect(),
            vec![3..5, 1..2],
            vec![1..4, 3..5],
        ] {
            let mut case = manifest(ranges);
            case.input_sha256 = sha256_hex(&bytes);
            case.output_sha256 = sha256_hex(&bytes);
            let report = verify_oracle_case(&case, &bytes, &bytes, &[]);
            assert!(!report.manifest_valid);
            assert!(!report.is_match());
        }
    }

    #[test]
    fn semantic_mismatch_fails_an_otherwise_exact_case() {
        let bytes = [0; 8];
        let mut expected = Observation::new();
        expected.insert("level/105/music", "1").unwrap();
        let mut actual = Observation::new();
        actual.insert("level/105/music", "2").unwrap();
        let mut case = manifest(Vec::new());
        case.input_sha256 = sha256_hex(&bytes);
        case.output_sha256 = sha256_hex(&bytes);
        case.decoded_before = expected.to_text();
        case.decoded_after = expected.to_text();
        let report =
            verify_oracle_case_with_observations(&case, &bytes, &bytes, &[], &expected, &actual)
                .unwrap();
        assert!(report.input_hash_matches && report.output_hash_matches);
        assert!(report.changes.is_match());
        assert!(!report.is_match());
        assert_eq!(report.semantics.unwrap().after.len(), 1);
    }
}
