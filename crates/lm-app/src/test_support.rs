use lm_rom::RomImage;
use std::path::PathBuf;

const PRISTINE_SMW_US_SHA256: &str =
    "0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b";

/// Loads a verified pristine SMW-US fixture without assuming Lunar Magic's live working ROM
/// still has its original bytes.
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
            // Keep application tests independent of whether the supplied fixture uses a
            // 512-byte copier container. Callers that exercise header conversion add the
            // container explicitly; every other ROM mutation should start from identical
            // authenticated logical bytes.
            return image.logical_bytes().to_vec();
        }
        observed.push(format!("{} ({digest})", path.display()));
    }
    panic!(
        "verified pristine SMW-US fixture not found; checked {}",
        observed.join(", ")
    );
}

#[test]
fn resolver_skips_the_live_expanded_rom_and_finds_pristine_logical_bytes() {
    let bytes = pristine_smw_us_rom_bytes();
    let image = RomImage::from_bytes(bytes).unwrap();
    assert_eq!(
        lm_oracle::sha256_hex(image.logical_bytes()),
        PRISTINE_SMW_US_SHA256
    );
}
