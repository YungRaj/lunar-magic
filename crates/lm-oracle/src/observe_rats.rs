use crate::{Observation, sha256_hex};
use lm_rats::scan;

/// Produces a content-addressed inventory of every validated RATS payload in a logical ROM.
#[must_use]
pub fn observe_rats(bytes: &[u8]) -> Observation {
    let mut result = Observation::new();
    let blocks = scan(bytes);
    put(&mut result, "rats/block-count", blocks.len());
    for block in blocks {
        let base = format!("rats/blocks/{:08x}", block.header_offset);
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
            &format!("{base}/payload-sha256"),
            sha256_hex(&bytes[block.payload]),
        );
    }
    result
}

fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_value())
        .expect("RATS observation paths are unique");
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
    use lm_rats::make_header;

    #[test]
    fn observes_valid_blocks_by_location_and_payload_identity() {
        let mut bytes = vec![0xff; 0x80];
        let payload = [1, 2, 3];
        bytes[0x20..0x28].copy_from_slice(&make_header(payload.len()).unwrap());
        bytes[0x28..0x2b].copy_from_slice(&payload);
        let observation = observe_rats(&bytes);
        assert_eq!(observation.get("rats/block-count"), Some("1"));
        assert_eq!(
            observation.get("rats/blocks/00000020/payload-length"),
            Some("3")
        );
        let expected = sha256_hex(&payload);
        assert_eq!(
            observation.get("rats/blocks/00000020/payload-sha256"),
            Some(expected.as_str())
        );
    }
}
