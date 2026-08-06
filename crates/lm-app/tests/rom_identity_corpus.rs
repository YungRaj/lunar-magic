use lm_rom::{CopierHeader, Mapper, RomImage, detect_identity};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires the retained local Lunar Magic 3.63 modified-ROM corpus"]
fn retained_lunar_magic_modified_rom_corpus_has_stable_identity() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixtures = [
        "sysLMRestore/smwOrig.smc",
        "oracle-work/lm363/pristine-us/level-save-000/after.smc",
        "oracle-work/lm363/pristine-us/level-save-105/after.smc",
        "oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc",
        "oracle-work/lm363/pristine-us/exanimation-install-positive/after.smc",
        "oracle-work/lm363/pristine-us/palette-install-positive/after.smc",
        "oracle-work/lm363/pristine-us/title-screen-transfer-positive/after.smc",
        "oracle-work/lm363/pristine-us/credits-transfer-positive/after.smc",
        "oracle-work/lm363/pristine-us/mwl-frame-edit-positive/after.smc",
        "oracle-work/lm363/pristine-us/mwl-semantic-edit-positive/after.smc",
        "oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc",
        "oracle-work/lm363/pristine-us/mwl-optional-transfer-positive/after.smc",
    ];
    for fixture in fixtures {
        let path = root.join(fixture);
        let image = RomImage::from_bytes(fs::read(&path).unwrap()).unwrap();
        let identity = detect_identity(&image).unwrap();
        assert_eq!(identity.mapper, Mapper::LoRom, "{}", path.display());
        assert!(identity.checksum_matches(), "{}", path.display());
        assert_eq!(
            image.copier_header(),
            CopierHeader::Present,
            "{}",
            path.display()
        );
    }

    let pristine = RomImage::from_bytes(fs::read(root.join(fixtures[0])).unwrap()).unwrap();
    let headerless = RomImage::from_bytes(pristine.logical_bytes().to_vec()).unwrap();
    let headerless_identity = detect_identity(&headerless).unwrap();
    assert_eq!(headerless.copier_header(), CopierHeader::Absent);
    assert!(headerless_identity.checksum_matches());

    let mut damaged = headerless.logical_bytes().to_vec();
    damaged[0] ^= 1;
    let damaged_identity = detect_identity(&RomImage::from_bytes(damaged).unwrap()).unwrap();
    assert!(!damaged_identity.checksum_matches());
}
