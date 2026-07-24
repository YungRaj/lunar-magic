use crate::{Observation, sha256_hex};
use lm_level::{M16Sidecar, S16Sidecar};

#[must_use]
pub fn observe_m16_sidecar(sidecar: &M16Sidecar) -> Observation {
    let bytes = sidecar.encode();
    observe_entries(
        "m16",
        M16Sidecar::ENTRY_COUNT,
        M16Sidecar::ENCODED_LEN,
        M16Sidecar::ENCODED_LEN,
        &bytes,
        |index| sidecar.entry(index),
    )
}

#[must_use]
pub fn observe_s16_sidecar(sidecar: &S16Sidecar) -> Observation {
    let canonical = sidecar.encode_canonical();
    let mut result = observe_entries(
        "s16",
        S16Sidecar::ENTRY_COUNT,
        sidecar.loaded_len(),
        canonical.len(),
        &canonical,
        |index| sidecar.entry(index),
    );
    put(&mut result, "s16/block-size", &S16Sidecar::BLOCK_LEN);
    result
}

fn observe_entries(
    prefix: &str,
    entry_count: usize,
    loaded_len: usize,
    canonical_len: usize,
    canonical: &[u8],
    entry: impl Fn(usize) -> Option<u32>,
) -> Observation {
    let mut result = Observation::new();
    put(&mut result, &format!("{prefix}/entry-count"), &entry_count);
    put(&mut result, &format!("{prefix}/loaded-length"), &loaded_len);
    put(
        &mut result,
        &format!("{prefix}/canonical-length"),
        &canonical_len,
    );
    put(
        &mut result,
        &format!("{prefix}/canonical-sha256"),
        &sha256_hex(canonical),
    );
    let nonzero: Vec<_> = (0..entry_count)
        .filter_map(|index| {
            entry(index)
                .filter(|value| *value != 0)
                .map(|value| (index, value))
        })
        .collect();
    put(
        &mut result,
        &format!("{prefix}/nonzero-count"),
        &nonzero.len(),
    );
    for (index, value) in nonzero {
        put(
            &mut result,
            &format!("{prefix}/entries/{index:04x}"),
            &format!("{value:08x}"),
        );
    }
    result
}

fn put(result: &mut Observation, path: &str, value: &impl ToString) {
    result
        .insert(path, value.to_string())
        .expect("native Map16 sidecar observation paths are unique");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_entries_and_s16_framing_are_observed() {
        let mut sidecar = S16Sidecar::decode(&[0; 3]).unwrap();
        sidecar.set_entry(0x200, 0x4433_2211).unwrap();
        let observed = observe_s16_sidecar(&sidecar);
        assert_eq!(observed.get("s16/loaded-length"), Some("3"));
        assert_eq!(observed.get("s16/canonical-length"), Some("4096"));
        assert_eq!(observed.get("s16/nonzero-count"), Some("1"));
        assert_eq!(observed.get("s16/entries/0200"), Some("44332211"));
    }

    #[test]
    fn m16_digest_and_entry_count_cover_fixed_buffer() {
        let mut bytes = vec![0; M16Sidecar::ENCODED_LEN];
        bytes[..4].copy_from_slice(&1_u32.to_le_bytes());
        let observed = observe_m16_sidecar(&M16Sidecar::decode(&bytes).unwrap());
        assert_eq!(observed.get("m16/entry-count"), Some("2048"));
        assert_eq!(observed.get("m16/loaded-length"), Some("8192"));
        assert_eq!(observed.get("m16/entries/0000"), Some("00000001"));
    }
}
