use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn built_cli_normalizes_and_observes_exact_record_through_unicode_paths() {
    let directory = std::env::temp_dir().join(format!(
        "lm-expanded-settings-process-{}-{}-日本語",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("source record.bin");
    let normalized = directory.join("normalized record.bin");
    let observation = directory.join("semantic record.obs");
    let bytes =
        std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap().wrapping_mul(13));
    fs::write(&input, bytes).unwrap();

    let run = || {
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args([
                "expanded-settings-file",
                input.to_str().unwrap(),
                normalized.to_str().unwrap(),
                observation.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(fs::read(&normalized).unwrap(), bytes);
    let observed = lm_oracle::Observation::from_text(
        std::str::from_utf8(&fs::read(&observation).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(observed.get("expanded-settings/words/00"), Some("3328"));
    let before_observation = fs::read(&observation).unwrap();

    let second = run();
    assert!(!second.status.success());
    assert_eq!(fs::read(&normalized).unwrap(), bytes);
    assert_eq!(fs::read(&observation).unwrap(), before_observation);
    fs::remove_dir_all(directory).unwrap();
}
