use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn directory() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "lm codec oracle 日本語 {} {}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn built_binary_observes_codec_semantics_and_refuses_collisions() {
    let directory = directory();
    let literal = directory.join("literal stream.lz2");
    let fill = directory.join("fill stream.lz2");
    let literal_observation = directory.join("literal semantic.obs");
    let fill_observation = directory.join("fill semantic.obs");
    fs::write(&literal, [0x02, b'A', b'A', b'A', 0xff]).unwrap();
    fs::write(&fill, [0x22, b'A', 0xff]).unwrap();

    for (input, output) in [(&literal, &literal_observation), (&fill, &fill_observation)] {
        assert!(
            Command::new(env!("CARGO_BIN_EXE_lm-cli"))
                .args(["codec-observe", "lz2"])
                .arg(input)
                .args(["10"])
                .arg(output)
                .status()
                .unwrap()
                .success()
        );
    }
    assert_eq!(
        fs::read(&literal_observation).unwrap(),
        fs::read(&fill_observation).unwrap()
    );

    assert!(
        !Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args(["codec-observe", "lz2"])
            .arg(&literal)
            .args(["10"])
            .arg(&literal_observation)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args(["codec-observe", "lz2"])
            .arg(&literal)
            .args(["2"])
            .arg(directory.join("too small.obs"))
            .output()
            .unwrap()
            .status
            .success()
    );

    let rle_bytes = vec![0xff; 128];
    for (kind, packed, bound) in [
        (
            "rle-terminated",
            lm_codec::encode_terminated_rle(&rle_bytes),
            "80",
        ),
        ("rle-sized", lm_codec::encode_sized_rle(&rle_bytes), "80"),
    ] {
        let input = directory.join(format!("{kind} packed.bin"));
        let output = directory.join(format!("{kind} semantic.obs"));
        fs::write(&input, packed).unwrap();
        assert!(
            Command::new(env!("CARGO_BIN_EXE_lm-cli"))
                .args(["codec-observe", kind])
                .arg(input)
                .arg(bound)
                .arg(&output)
                .status()
                .unwrap()
                .success()
        );
        let observation =
            lm_oracle::Observation::from_text(&fs::read_to_string(output).unwrap()).unwrap();
        assert_eq!(observation.get("codec/kind"), Some(kind));
        assert_eq!(observation.get("codec/decoded-bytes"), Some("128"));
    }
    fs::remove_dir_all(directory).unwrap();
}
