use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn scripted_binary_creates_and_applies_ips_through_unicode_specs() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-ips-process-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("Before image.smc"), b"0123456789").unwrap();
    fs::write(directory.join("After image.smc"), b"01AAAA6789-more").unwrap();
    let create_spec = directory.join("Create patch spec.txt");
    let apply_spec = directory.join("Apply patch spec.txt");
    fs::write(
        &create_spec,
        "LMIPSC01\nbefore Before image.smc\nafter After image.smc\noutput Change 日本語.ips\n",
    )
    .unwrap();
    fs::write(
        &apply_spec,
        "LMIPSA01\nsource Before image.smc\npatch Change 日本語.ips\noutput Patched image.smc\n",
    )
    .unwrap();
    let script = directory.join("commands.txt");
    fs::write(
        &script,
        format!(
            "ips-create {}\nips-apply {}\nquit\n",
            create_spec.display(),
            apply_spec.display()
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
    assert_eq!(
        fs::read(directory.join("Patched image.smc")).unwrap(),
        fs::read(directory.join("After image.smc")).unwrap()
    );
    let repeated = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .args(["--script", script.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!repeated.status.success());
    fs::remove_dir_all(directory).unwrap();
}
