use crate::{
    ManifestError, Observation, Operation, OracleManifest, compare_bytes,
    compare_tagged_allocations, sha256_hex,
};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AllocationOwnershipPolicy {
    /// Do not claim ownership of any tagged allocation.
    #[default]
    None,
    /// Claim complete validated blocks that were added, removed, changed, or relocated.
    ChangedTagged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureMetadata {
    pub case_id: String,
    pub lunar_magic_version: String,
    pub operation: Operation,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub allocation_ownership: AllocationOwnershipPolicy,
}

/// Captures a deterministic, replayable oracle manifest without retaining ROM bytes.
///
/// # Errors
///
/// Returns [`ManifestError`] when capture metadata cannot identify an unambiguous replay case.
pub fn capture_oracle_case(
    metadata: CaptureMetadata,
    before: &[u8],
    after: &[u8],
    decoded_before: &Observation,
    decoded_after: &Observation,
) -> Result<OracleManifest, ManifestError> {
    let (owned_allocations_before, owned_allocations_after) = match metadata.allocation_ownership {
        AllocationOwnershipPolicy::None => (Vec::new(), Vec::new()),
        AllocationOwnershipPolicy::ChangedTagged => changed_tagged_ranges(before, after),
    };
    let manifest = OracleManifest {
        case_id: metadata.case_id,
        lunar_magic_version: metadata.lunar_magic_version,
        input_sha256: sha256_hex(before),
        output_sha256: sha256_hex(after),
        operation: metadata.operation,
        changed_ranges: compare_bytes(before, after)
            .into_iter()
            .map(|difference| difference.range)
            .collect(),
        decoded_before: decoded_before.to_text(),
        decoded_after: decoded_after.to_text(),
        owned_allocations_before,
        owned_allocations_after,
        warnings: metadata.warnings,
        errors: metadata.errors,
    };
    manifest.validate()?;
    Ok(manifest)
}

fn changed_tagged_ranges(before: &[u8], after: &[u8]) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let diff = compare_tagged_allocations(before, after);
    let mut before_ranges: Vec<_> = diff
        .removed
        .into_iter()
        .map(|identity| identity.full_range)
        .collect();
    let mut after_ranges: Vec<_> = diff
        .added
        .into_iter()
        .map(|identity| identity.full_range)
        .collect();
    for matched in diff
        .matched
        .into_iter()
        .filter(crate::TaggedPayloadMatch::relocated)
    {
        before_ranges.push(matched.before.full_range);
        after_ranges.push(matched.after.full_range);
    }
    canonicalize_ranges(&mut before_ranges);
    canonicalize_ranges(&mut after_ranges);
    (before_ranges, after_ranges)
}

fn canonicalize_ranges(ranges: &mut Vec<Range<usize>>) {
    ranges.sort_by_key(|range| (range.start, range.end));
    ranges.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{verify_oracle_case, verify_oracle_case_with_observations};
    use lm_rats::make_header;

    fn image(offset: usize, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0xff; 0x100];
        bytes[offset..offset + 8].copy_from_slice(&make_header(payload.len()).unwrap());
        bytes[offset + 8..offset + 8 + payload.len()].copy_from_slice(payload);
        bytes
    }

    fn observation(value: &str) -> Observation {
        let mut observation = Observation::new();
        observation.insert("level/105/music", value).unwrap();
        observation
    }

    #[test]
    fn captured_manifest_replays_through_exact_and_semantic_verifiers() {
        let before = image(0x20, &[1, 2, 3]);
        let after = image(0x60, &[1, 2, 3]);
        let decoded_before = observation("1");
        let decoded_after = observation("2");
        let manifest = capture_oracle_case(
            CaptureMetadata {
                case_id: "move-object".into(),
                lunar_magic_version: "3.63".into(),
                operation: Operation {
                    name: "move".into(),
                    arguments: vec![("level".into(), "105".into())],
                },
                warnings: vec!["fixture warning".into()],
                errors: Vec::new(),
                allocation_ownership: AllocationOwnershipPolicy::ChangedTagged,
            },
            &before,
            &after,
            &decoded_before,
            &decoded_after,
        )
        .unwrap();
        assert_eq!(manifest.owned_allocations_before.len(), 1);
        assert_eq!(manifest.owned_allocations_before[0], 0x20..0x2b);
        assert_eq!(manifest.owned_allocations_after.len(), 1);
        assert_eq!(manifest.owned_allocations_after[0], 0x60..0x6b);
        assert!(verify_oracle_case(&manifest, &before, &after, &[]).is_match());
        assert!(
            verify_oracle_case_with_observations(
                &manifest,
                &before,
                &after,
                &[],
                &decoded_before,
                &decoded_after,
            )
            .unwrap()
            .is_match()
        );
        assert_eq!(
            OracleManifest::from_text(&manifest.to_text()).unwrap(),
            manifest
        );
    }

    #[test]
    fn ownership_is_explicit_and_changed_payloads_claim_both_blocks() {
        let before = image(0x20, &[1, 2, 3]);
        let after = image(0x20, &[1, 9, 3]);
        let capture = |policy| {
            capture_oracle_case(
                CaptureMetadata {
                    case_id: "case".into(),
                    lunar_magic_version: "3.63".into(),
                    operation: Operation {
                        name: "edit".into(),
                        arguments: Vec::new(),
                    },
                    warnings: Vec::new(),
                    errors: Vec::new(),
                    allocation_ownership: policy,
                },
                &before,
                &after,
                &Observation::new(),
                &Observation::new(),
            )
        };
        let none = capture(AllocationOwnershipPolicy::None).unwrap();
        assert!(none.owned_allocations_before.is_empty());
        let owned = capture(AllocationOwnershipPolicy::ChangedTagged).unwrap();
        assert_eq!(owned.owned_allocations_before.len(), 1);
        assert_eq!(owned.owned_allocations_before[0], 0x20..0x2b);
        assert_eq!(owned.owned_allocations_after.len(), 1);
        assert_eq!(owned.owned_allocations_after[0], 0x20..0x2b);
    }

    #[test]
    fn capture_refuses_ambiguous_fixture_metadata() {
        let result = capture_oracle_case(
            CaptureMetadata {
                case_id: "case".into(),
                lunar_magic_version: "3.63".into(),
                operation: Operation {
                    name: "edit".into(),
                    arguments: vec![
                        ("mapper".into(), "lorom".into()),
                        ("mapper".into(), "sa1".into()),
                    ],
                },
                warnings: Vec::new(),
                errors: Vec::new(),
                allocation_ownership: AllocationOwnershipPolicy::None,
            },
            &[1],
            &[2],
            &Observation::new(),
            &Observation::new(),
        );
        assert_eq!(
            result,
            Err(ManifestError::DuplicateArgumentName("mapper".into()))
        );
    }
}
