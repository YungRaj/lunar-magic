use crate::Observation;
use lm_project::RatsOwnershipManifest;

/// Produces a header-addressable snapshot of explicit RATS ownership and retention authority.
#[must_use]
pub fn observe_rats_manifest(manifest: &RatsOwnershipManifest) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "rats-manifest/owned-count",
        manifest.owned.len(),
    );
    put(
        &mut result,
        "rats-manifest/retained-count",
        manifest.retained.len(),
    );
    let mut blocks: Vec<_> = manifest.owned.iter().collect();
    blocks.sort_unstable_by_key(|block| block.header_offset);
    for block in blocks {
        let base = format!("rats-manifest/blocks/{:016x}", block.header_offset);
        put(
            &mut result,
            &format!("{base}/payload-start"),
            block.payload.start,
        );
        put(
            &mut result,
            &format!("{base}/payload-end"),
            block.payload.end,
        );
        put(
            &mut result,
            &format!("{base}/payload-length"),
            block.payload.len(),
        );
        put(
            &mut result,
            &format!("{base}/disposition"),
            if manifest.retained.contains(block) {
                "retain"
            } else {
                "reclaim"
            },
        );
    }
    result
}

fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_value())
        .expect("observation paths are unique");
}

trait ObservationValue {
    fn into_value(self) -> String;
}
impl<T: ToString> ObservationValue for T {
    fn into_value(self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::RatsBlock;

    #[test]
    fn observes_exact_ranges_and_retention_by_header_identity() {
        let retained = RatsBlock {
            header_offset: 0x200,
            payload: 0x208..0x20b,
        };
        let reclaimed = RatsBlock {
            header_offset: 0x100,
            payload: 0x108..0x10a,
        };
        let manifest = RatsOwnershipManifest {
            owned: vec![retained.clone(), reclaimed],
            retained: vec![retained],
        };
        let observed = observe_rats_manifest(&manifest);
        assert_eq!(
            observed.get("rats-manifest/blocks/0000000000000100/disposition"),
            Some("reclaim")
        );
        assert_eq!(
            observed.get("rats-manifest/blocks/0000000000000200/payload-length"),
            Some("3")
        );
    }
}
