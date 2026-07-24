use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "oracle_release_gate_process/fixtures.rs"]
mod fixtures;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn invoke(arguments: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .args(arguments)
        .output()
        .unwrap()
}

fn complete_requirements(root: &std::path::Path) -> Vec<String> {
    let mut arguments = vec![
        "oracle-release-gate".into(),
        root.display().to_string(),
        "version:3.63".into(),
    ];
    arguments.extend(
        [
            "open-save",
            "render-level",
            "level-edit",
            "lunar-magic-reopen",
            "emulator-boot",
        ]
        .map(|operation| format!("operation:{operation}")),
    );
    arguments.extend(
        [
            ("mapper", "lorom"),
            ("header", "headerless"),
            ("region", "us"),
            ("revision", "smw-us-v1"),
            ("rom_size", "expanded"),
            ("fixture_family", "clean"),
        ]
        .map(|(name, value)| format!("argument:{name}={value}")),
    );
    arguments.extend(
        [
            "rom",
            "codecs",
            "rats",
            "levels",
            "map16",
            "sprites",
            "graphics",
            "palettes",
            "exanimation",
            "overworld",
            "rendering",
            "application",
        ]
        .map(|subsystem| format!("argument:subsystem={subsystem}")),
    );
    arguments
}

#[test]
fn built_release_gate_rejects_incomplete_policy_and_empty_corpus() {
    let root = std::env::temp_dir().join(format!(
        "lm-release-gate-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();

    let incomplete = invoke(&[
        "oracle-release-gate".into(),
        root.display().to_string(),
        "version:3.63".into(),
    ]);
    assert!(!incomplete.status.success());
    assert!(
        String::from_utf8_lossy(&incomplete.stderr)
            .contains("release gate requires version, all workflow operations")
    );

    let empty = invoke(&complete_requirements(&root));
    assert!(!empty.status.success());
    assert!(
        String::from_utf8_lossy(&empty.stderr)
            .contains("oracle suite contains no case.manifest files")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn built_release_gate_accepts_complete_bound_corpus_and_rejects_corruption() {
    let root = std::env::temp_dir().join(format!(
        "lm-release-corpus-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    fixtures::write_release_corpus(&root);

    let accepted = invoke(&complete_requirements(&root));
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(String::from_utf8_lossy(&accepted.stdout).contains("oracle-release-gate: PASS"));

    fs::write(root.join("emulator-boot/emulator.png"), [0]).unwrap();
    let rejected = invoke(&complete_requirements(&root));
    assert!(!rejected.status.success());
    assert!(!String::from_utf8_lossy(&rejected.stdout).contains("oracle-release-gate: PASS"));
    fs::remove_dir_all(root).unwrap();
}
