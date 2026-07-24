use lm_overworld::{
    OverworldMessage, decode_native_overworld_message_file, encode_native_overworld_message_file,
};
use lm_profile::smw_us_v1_overworld_message_patch_locator;
use lm_project::Project;
use lm_rom::{RomImage, compute_snes_checksum};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn run(binary: &str, arguments: &[&str]) {
    assert!(
        Command::new(binary)
            .args(arguments)
            .status()
            .unwrap()
            .success()
    );
}

fn make_messages(count: usize, second: u8) -> Vec<OverworldMessage> {
    (0..count)
        .map(|index| {
            let mut bytes = [0x1f; OverworldMessage::ENCODED_LEN];
            bytes[0] = u8::try_from(index % 0xfd).unwrap();
            bytes[1] = second;
            OverworldMessage(bytes)
        })
        .collect()
}

#[test]
fn built_cli_installs_reopens_and_exports_expanded_messages() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("lm-message-process-{nonce}"));
    fs::create_dir(&directory).unwrap();
    let input = root.join("Super Mario World (USA).sfc");
    let original = fs::read(&input).unwrap();
    let artifact = directory.join("messages.lmowmsg");
    let output = directory.join("installed.sfc");
    let grown_artifact = directory.join("grown messages.lmowmsg");
    let grown_output = directory.join("grown installed.sfc");
    let exported = directory.join("exported.lmowmsg");
    let messages = make_messages(200, 0x55);
    fs::write(
        &artifact,
        encode_native_overworld_message_file(&messages).unwrap(),
    )
    .unwrap();

    let binary = env!("CARGO_BIN_EXE_lm-cli");
    run(
        binary,
        &[
            "smw-overworld-message-install",
            input.to_str().unwrap(),
            artifact.to_str().unwrap(),
            output.to_str().unwrap(),
        ],
    );
    let installed = fs::read(&output).unwrap();
    assert_eq!(
        &installed[0x7fdc..0x7fe0],
        compute_snes_checksum(&installed, 0x7fdc).unwrap().encoded()
    );
    let project = Project::open_supported(RomImage::from_bytes(installed).unwrap()).unwrap();
    assert_eq!(
        project
            .load_expanded_overworld_messages_detected(smw_us_v1_overworld_message_patch_locator())
            .unwrap()
            .messages,
        messages
    );

    let grown_messages = make_messages(400, 0x66);
    fs::write(
        &grown_artifact,
        encode_native_overworld_message_file(&grown_messages).unwrap(),
    )
    .unwrap();
    run(
        binary,
        &[
            "smw-overworld-message-install",
            output.to_str().unwrap(),
            grown_artifact.to_str().unwrap(),
            grown_output.to_str().unwrap(),
        ],
    );
    let grown_installed = fs::read(&grown_output).unwrap();
    assert_eq!(
        &grown_installed[0x7fdc..0x7fe0],
        compute_snes_checksum(&grown_installed, 0x7fdc)
            .unwrap()
            .encoded()
    );
    let grown_project =
        Project::open_supported(RomImage::from_bytes(grown_installed).unwrap()).unwrap();
    assert_eq!(
        grown_project
            .load_expanded_overworld_messages_detected(smw_us_v1_overworld_message_patch_locator())
            .unwrap()
            .messages,
        grown_messages
    );

    run(
        binary,
        &[
            "smw-overworld-message-export",
            grown_output.to_str().unwrap(),
            exported.to_str().unwrap(),
        ],
    );
    assert_eq!(
        decode_native_overworld_message_file(&fs::read(exported).unwrap()).unwrap(),
        grown_messages
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
