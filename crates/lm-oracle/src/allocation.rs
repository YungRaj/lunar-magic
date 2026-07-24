use crate::sha256;
use lm_rats::scan;
use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaggedPayloadIdentity {
    pub full_range: Range<usize>,
    pub payload_range: Range<usize>,
    pub payload_sha256: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaggedPayloadMatch {
    pub before: TaggedPayloadIdentity,
    pub after: TaggedPayloadIdentity,
}

impl TaggedPayloadMatch {
    #[must_use]
    pub fn relocated(&self) -> bool {
        self.before.full_range.start != self.after.full_range.start
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TaggedAllocationDiff {
    pub matched: Vec<TaggedPayloadMatch>,
    pub removed: Vec<TaggedPayloadIdentity>,
    pub added: Vec<TaggedPayloadIdentity>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OwnedTaggedRangeReport {
    pub missing_before: Vec<Range<usize>>,
    pub missing_after: Vec<Range<usize>>,
}

impl OwnedTaggedRangeReport {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.missing_before.is_empty() && self.missing_after.is_empty()
    }
}

impl TaggedAllocationDiff {
    /// Returns true when both images contain the same multiset of tagged payload bytes.
    #[must_use]
    pub fn semantically_equivalent(&self) -> bool {
        self.removed.is_empty() && self.added.is_empty()
    }

    #[must_use]
    pub fn relocated_count(&self) -> usize {
        self.matched
            .iter()
            .filter(|entry| entry.relocated())
            .count()
    }
}

/// Compares validated RATS allocations by payload content rather than physical address.
///
/// Duplicate payloads are paired deterministically in ascending ROM order. Header bytes and free
/// space are intentionally excluded from the identity, allowing legal allocator relocation to be
/// distinguished from semantic payload changes.
#[must_use]
pub fn compare_tagged_allocations(before: &[u8], after: &[u8]) -> TaggedAllocationDiff {
    let before = identities(before);
    let after = identities(after);
    let mut used_after = vec![false; after.len()];
    let mut report = TaggedAllocationDiff::default();
    for old in before {
        let candidate = after.iter().enumerate().position(|(index, new)| {
            !used_after[index]
                && old.payload_range.len() == new.payload_range.len()
                && old.payload_sha256 == new.payload_sha256
        });
        if let Some(index) = candidate {
            used_after[index] = true;
            report.matched.push(TaggedPayloadMatch {
                before: old,
                after: after[index].clone(),
            });
        } else {
            report.removed.push(old);
        }
    }
    report.added.extend(
        after
            .into_iter()
            .enumerate()
            .filter_map(|(index, identity)| (!used_after[index]).then_some(identity)),
    );
    report
}

/// Validates that every declared owned range is exactly one complete, validated RATS block.
#[must_use]
pub fn verify_owned_tagged_ranges(
    before: &[u8],
    after: &[u8],
    owned_before: &[Range<usize>],
    owned_after: &[Range<usize>],
) -> OwnedTaggedRangeReport {
    let before_ranges: Vec<_> = identities(before)
        .into_iter()
        .map(|identity| identity.full_range)
        .collect();
    let after_ranges: Vec<_> = identities(after)
        .into_iter()
        .map(|identity| identity.full_range)
        .collect();
    OwnedTaggedRangeReport {
        missing_before: owned_before
            .iter()
            .filter(|range| !before_ranges.contains(range))
            .cloned()
            .collect(),
        missing_after: owned_after
            .iter()
            .filter(|range| !after_ranges.contains(range))
            .cloned()
            .collect(),
    }
}

fn identities(bytes: &[u8]) -> Vec<TaggedPayloadIdentity> {
    scan(bytes)
        .into_iter()
        .map(|block| TaggedPayloadIdentity {
            full_range: block.full_range(),
            payload_sha256: sha256(&bytes[block.payload.clone()]),
            payload_range: block.payload,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::make_header;

    fn image(blocks: &[(usize, &[u8])]) -> Vec<u8> {
        let mut bytes = vec![0xff; 0x200];
        for (offset, payload) in blocks {
            bytes[*offset..*offset + 8].copy_from_slice(&make_header(payload.len()).unwrap());
            bytes[*offset + 8..*offset + 8 + payload.len()].copy_from_slice(payload);
        }
        bytes
    }

    #[test]
    fn relocation_is_semantically_equivalent() {
        let before = image(&[(0x20, &[1, 2, 3]), (0x80, &[4, 5])]);
        let after = image(&[(0x40, &[4, 5]), (0xa0, &[1, 2, 3])]);
        let diff = compare_tagged_allocations(&before, &after);
        assert!(diff.semantically_equivalent());
        assert_eq!(diff.matched.len(), 2);
        assert_eq!(diff.relocated_count(), 2);
    }

    #[test]
    fn changed_payload_is_one_removal_and_one_addition() {
        let before = image(&[(0x20, &[1, 2, 3])]);
        let after = image(&[(0x20, &[1, 9, 3])]);
        let diff = compare_tagged_allocations(&before, &after);
        assert!(!diff.semantically_equivalent());
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.added.len(), 1);
    }

    #[test]
    fn duplicate_payloads_are_counted_as_a_multiset() {
        let before = image(&[(0x20, &[7]), (0x40, &[7])]);
        let after = image(&[(0x80, &[7])]);
        let diff = compare_tagged_allocations(&before, &after);
        assert_eq!(diff.matched.len(), 1);
        assert_eq!(diff.removed.len(), 1);
        assert!(diff.added.is_empty());
    }

    #[test]
    fn ownership_ranges_must_be_complete_validated_blocks() {
        let before = image(&[(0x20, &[1, 2, 3])]);
        let after = image(&[(0x40, &[1, 2, 3])]);
        let valid_before = 0x20..0x2b;
        let valid_after = 0x40..0x4b;
        let valid = verify_owned_tagged_ranges(
            &before,
            &after,
            std::slice::from_ref(&valid_before),
            std::slice::from_ref(&valid_after),
        );
        assert!(valid.is_valid());
        let invalid_before = 0x28..0x2b;
        let invalid_after = 0x40..0x4a;
        let invalid = verify_owned_tagged_ranges(
            &before,
            &after,
            std::slice::from_ref(&invalid_before),
            std::slice::from_ref(&invalid_after),
        );
        assert_eq!(
            invalid.missing_before.as_slice(),
            std::slice::from_ref(&invalid_before)
        );
        assert_eq!(
            invalid.missing_after.as_slice(),
            std::slice::from_ref(&invalid_after)
        );
    }
}
