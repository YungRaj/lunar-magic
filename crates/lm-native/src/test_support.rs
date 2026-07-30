use lm_rom::RomImage;
use std::path::PathBuf;

const PRISTINE_SMW_US_SHA256: &str =
    "0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b";

/// Loads the verified pristine SMW-US fixture without assuming that Lunar Magic's live working
/// ROM still has its original bytes.
pub(crate) fn pristine_smw_us_rom_bytes() -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let candidates = [
        root.join("Super Mario World (USA).sfc"),
        root.join("SMW-working.sfc"),
        root.join("sysLMRestore/smwOrig.smc"),
    ];
    let mut observed = Vec::new();
    for path in candidates {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(image) = RomImage::from_bytes(bytes.clone()) else {
            observed.push(format!("{} (invalid ROM)", path.display()));
            continue;
        };
        let digest = lm_oracle::sha256_hex(image.logical_bytes());
        if digest == PRISTINE_SMW_US_SHA256 {
            // Tests need one stable logical-ROM representation regardless of whether the
            // legally supplied fixture happens to have a 512-byte copier prefix. Returning the
            // container bytes made header-sensitive tests depend on which candidate existed
            // first and made a later `resize` preserve the prefix at offset zero as if it were
            // ROM data.
            return image.logical_bytes().to_vec();
        }
        observed.push(format!("{} ({digest})", path.display()));
    }
    panic!(
        "verified pristine SMW-US fixture not found; checked {}",
        observed.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_skips_the_live_expanded_rom_and_finds_pristine_logical_bytes() {
        let bytes = pristine_smw_us_rom_bytes();
        let image = RomImage::from_bytes(bytes).unwrap();
        assert_eq!(
            lm_oracle::sha256_hex(image.logical_bytes()),
            PRISTINE_SMW_US_SHA256
        );
    }
}
