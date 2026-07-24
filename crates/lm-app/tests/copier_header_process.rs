use lm_rom::{COPIER_HEADER_LEN, CopierHeader, RomImage};
use std::fs;
use std::process::Command;

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
