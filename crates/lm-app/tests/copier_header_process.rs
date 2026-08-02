use lm_rom::{COPIER_HEADER_LEN, CopierHeader, RomImage};
use std::fs;
use std::process::Command;

const PRISTINE_SMW_US_SHA256: &str =
    "0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b";

fn pristine_smw_us_logical_bytes() -> Vec<u8> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for path in [
        root.join("Super Mario World (USA).sfc"),
        root.join("SMW-working.sfc"),
        root.join("sysLMRestore/smwOrig.smc"),
    ] {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        let Ok(image) = RomImage::from_bytes(bytes) else {
            continue;
        };
        if lm_oracle::sha256_hex(image.logical_bytes()) == PRISTINE_SMW_US_SHA256 {
            return image.logical_bytes().to_vec();
        }
    }
    panic!("verified pristine SMW-US fixture not found");
}

#[test]
fn scripted_binary_adds_and_removes_copier_header_without_logical_changes() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-header-process-日本語-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let logical = vec![0x5a; 0x8000];
    fs::write(directory.join("Plain ROM.smc"), &logical).unwrap();
    let add_spec = directory.join("Add header spec.txt");
    let remove_spec = directory.join("Remove header spec.txt");
    fs::write(
        &add_spec,
        "LMHDRAD1\ninput Plain ROM.smc\noutput Headered ROM.smc\nfill 165\n",
    )
    .unwrap();
    fs::write(
        &remove_spec,
        "LMHDRRM1\ninput Headered ROM.smc\noutput Restored ROM.smc\n",
    )
    .unwrap();
    let script = directory.join("commands.txt");
    fs::write(
        &script,
        format!(
            "copier-header-add {}\ncopier-header-remove {}\nquit\n",
            add_spec.display(),
            remove_spec.display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .args(["--script", script.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let headered =
        RomImage::from_bytes(fs::read(directory.join("Headered ROM.smc")).unwrap()).unwrap();
    assert_eq!(headered.copier_header(), CopierHeader::Present);
    assert_eq!(headered.logical_bytes(), logical);
    assert!(
        headered.as_file_bytes()[..COPIER_HEADER_LEN]
            .iter()
            .all(|byte| *byte == 0xa5)
    );
    assert_eq!(
        fs::read(directory.join("Restored ROM.smc")).unwrap(),
        logical
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn scripted_binary_creates_the_exact_lunar_magic_canonical_header() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-canonical-header-process-日本語-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let logical = pristine_smw_us_logical_bytes();
    fs::write(directory.join("Plain SMW.sfc"), &logical).unwrap();
    let spec = directory.join("Canonical header spec.txt");
    fs::write(
        &spec,
        "LMHDRAD1\ninput Plain SMW.sfc\noutput Canonical SMW.smc\nmode lunar-magic-smw-us-v1\n",
    )
    .unwrap();
    let script = directory.join("commands.txt");
    fs::write(
        &script,
        format!("copier-header-add {}\nquit\n", spec.display()),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .args(["--script", script.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let headered = fs::read(directory.join("Canonical SMW.smc")).unwrap();
    assert_eq!(
        &headered[..COPIER_HEADER_LEN],
        &lm_profile::smw_us_v1_lunar_magic_copier_header()
    );
    assert_eq!(&headered[COPIER_HEADER_LEN..], logical);
    fs::remove_dir_all(directory).unwrap();
}
