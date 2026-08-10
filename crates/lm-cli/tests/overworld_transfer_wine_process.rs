use lm_oracle::Observation;
use lm_overworld::EventRevealTable;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[test]
fn wine_transfer_map16_allocation_decodes_as_recovered_interleaved_rle() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rom =
        fs::read(root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc"))
            .unwrap();
    let payload = &rom[0x80_208..0x83_130];
    let decoded = lm_codec::decode_interleaved_sized_rle_prefix(payload, 0x4000).unwrap();
    assert_eq!(decoded.first_stream_len, 6514);
    assert_eq!(decoded.consumed, 0x2f28);
    assert_eq!(
        &decoded.bytes[..8],
        &[0x75, 0x1c, 0x75, 0x1c, 0x75, 0x1c, 0x75, 0x1c]
    );
    assert_eq!(
        lm_codec::encode_interleaved_sized_rle(&decoded.bytes).unwrap(),
        payload
    );
}

#[test]
fn built_cli_semantically_observes_transferred_map16_tables() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive");
    let directory = std::env::temp_dir().join(format!("lm-cli-map16-wine-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let output = directory.join("Transferred Map16.obs");
    let process = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("smw-transferred-map16-observe")
        .arg(fixture.join("after.smc"))
        .arg(&output)
        .output()
        .unwrap();
    assert_success(&process);
    let observation = Observation::from_text(&fs::read_to_string(output).unwrap()).unwrap();
    assert_eq!(
        observation.get("map16/transferred/definition-words"),
        Some("8192")
    );
    assert_eq!(
        observation.get("map16/transferred/acts-like-count"),
        Some("2884")
    );
    assert_eq!(
        observation.get("map16/transferred/definitions/0000"),
        Some("1c75")
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn built_cli_semantically_observes_installed_map16_remaps() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive");
    let directory =
        std::env::temp_dir().join(format!("lm-cli-map16-remaps-wine-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let output = directory.join("Map16 Remaps.obs");
    let process = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("smw-installed-map16-remaps-observe")
        .arg(fixture.join("after.smc"))
        .arg(&output)
        .output()
        .unwrap();
    assert_success(&process);
    let observation = Observation::from_text(&fs::read_to_string(output).unwrap()).unwrap();
    assert_eq!(
        observation.get("map16/remap/range-group-count"),
        Some("120")
    );
    assert_eq!(
        observation.get("map16/remap/record-group-count"),
        Some("120")
    );
    fs::remove_dir_all(directory).unwrap();
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn verify_oracle(
    fixture: &Path,
    manifest: &str,
    before_observation: &str,
    after_observation: &str,
) {
    let verification = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("oracle-verify")
        .arg(fixture.join(manifest))
        .arg(fixture.join("before.smc"))
        .arg(fixture.join("after.smc"))
        .arg(fixture.join(before_observation))
        .arg(fixture.join(after_observation))
        .output()
        .unwrap();
    assert_success(&verification);
}

fn verify_full_observation(fixture: &Path, output: &Path) {
    let observe = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("smw-overworld-transfer-full-observe")
        .arg(fixture.join("after.smc"))
        .arg(output)
        .output()
        .unwrap();
    assert_success(&observe);
    let actual_text = fs::read_to_string(output).unwrap();
    let expected_text = fs::read_to_string(fixture.join("after-full.obs")).unwrap();
    let full = Observation::from_text(&actual_text).unwrap();
    let expected = Observation::from_text(&expected_text).unwrap();
    let differences = expected.differences(&full);
    assert!(
        differences.is_empty(),
        "full overworld observation differs at up to the first 16 paths: {:?}",
        &differences[..differences.len().min(16)]
    );
    assert_eq!(actual_text, full.to_text(), "generated observation is not canonical");
    assert_eq!(
        expected_text,
        expected.to_text(),
        "retained observation is not canonical"
    );
    for (path, expected) in [
        ("map16/transferred/definition-words", "8192"),
        ("map16/transferred/acts-like-count", "2884"),
        ("overworld/native-path-links/count", "14"),
        ("overworld/native-warp-links/count", "27"),
        ("overworld/native-level-names/count", "96"),
        ("overworld/native-player-starts/count", "2"),
        ("overworld/expanded-settings/count", "7"),
        ("overworld/messages/count", "194"),
        ("overworld/boss-sequence/message-count", "7"),
        (
            "overworld/expanded-settings/00/layer3/mode-enabled",
            "false",
        ),
        ("overworld/expanded-settings/00/layer3/mode-packed", "0"),
        (
            "overworld/expanded-settings/00/layer3/alternate-source-route",
            "none",
        ),
        (
            "overworld/expanded-settings/00/layer3/primary-additive-input",
            "none",
        ),
    ] {
        assert_eq!(full.get(path), Some(expected), "{path}");
    }
}

#[test]
fn built_cli_replays_lunar_magic_overworld_transfer_event_domain() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive");
    let directory =
        std::env::temp_dir().join(format!("lm-cli-overworld-wine-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let exported = directory.join("Transferred Events 日本語.lmevt");
    let normalized = directory.join("Normalized Events.lmevt");
    let observation = directory.join("Event Observation.obs");
    let transfer_observation = directory.join("Transfer Event Domains.obs");
    let full_transfer_observation = directory.join("Complete Transfer Domains.obs");

    let export = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("smw-overworld-event-export")
        .arg(fixture.join("after.smc"))
        .arg(&exported)
        .output()
        .unwrap();
    assert_success(&export);
    assert_eq!(
        fs::read(&exported).unwrap(),
        fs::read(fixture.join("after.lmevt")).unwrap()
    );
    let inspect = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("overworld-event-file")
        .arg(&exported)
        .arg(&normalized)
        .arg(&observation)
        .output()
        .unwrap();
    assert_success(&inspect);
    let decoded =
        EventRevealTable::decode_native_event_file(&fs::read(&normalized).unwrap()).unwrap();
    assert_eq!(decoded.entries.len(), 120);
    let observed = Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
    assert_eq!(observed.get("overworld/event-reveals/count"), Some("120"));
    assert_eq!(
        fs::read_to_string(&observation).unwrap(),
        fs::read_to_string(fixture.join("after.obs")).unwrap()
    );

    let observe_transfer = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("smw-overworld-transfer-observe")
        .arg(fixture.join("after.smc"))
        .arg(&transfer_observation)
        .output()
        .unwrap();
    assert_success(&observe_transfer);
    assert_eq!(
        fs::read_to_string(&transfer_observation).unwrap(),
        fs::read_to_string(fixture.join("after-events.obs")).unwrap()
    );
    let all_events =
        Observation::from_text(&fs::read_to_string(&transfer_observation).unwrap()).unwrap();
    assert_eq!(
        all_events.get("overworld/event-number-map/count"),
        Some("96")
    );
    assert_eq!(
        all_events.get("overworld/special-event-reveals/count"),
        Some("24")
    );
    assert_eq!(
        all_events.get("overworld/event-tilemap/primary-bytes"),
        Some("4096")
    );

    verify_full_observation(&fixture, &full_transfer_observation);

    verify_oracle(&fixture, "oracle.manifest", "before.obs", "after.obs");
    verify_oracle(
        &fixture,
        "oracle-events.manifest",
        "before-events.obs",
        "after-events.obs",
    );
    verify_oracle(
        &fixture,
        "oracle-full.manifest",
        "before-full.obs",
        "after-full.obs",
    );
    fs::remove_dir_all(directory).unwrap();
}
