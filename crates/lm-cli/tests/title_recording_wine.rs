use lm_title::{TitleScreenRecording, encode_zsnes_title_recording};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

const LUNAR_MAGIC_363_SHA256: &str =
    "b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff";
const INPUT_ROM_SHA256: &str = "7300346506c982766ed3ae370c56a31e30ad7a9603706bc3c6b18051e70f41c7";
const VANILLA_ROM_SHA256: &str = "5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf";
const INSTALLED_ROM_SHA256: &str =
    "758c41d8f849d2a96efa76f789f471b37e2843981f0b759e34c6a670cc936676";
const ZSNES_STATE_SHA256: &str = "958059ec938e651410f01f6b692176c5037adc854f4fc218bbd051de782f0964";
const VANILLA_INSTALLED_ROM_SHA256: &str =
    "2002afa81216a4530b2b8074acdb66f606d92687a4de56d2e32cdbadda272421";
const VANILLA_UPDATED_ROM_SHA256: &str =
    "46079b7e14c90d89cc7b46a797bd05a48fabacaec7fc6d7e63134bc405d36bb0";

fn run(prefix: Option<&Path>, arguments: &[&Path]) -> Output {
    let mut command = Command::new("wine");
    command.env("WINEDEBUG", "-all");
    if let Some(prefix) = prefix {
        command.env("WINEPREFIX", prefix);
    }
    command.args(arguments).output().unwrap()
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
#[ignore = "requires Wine, local Lunar Magic 3.63, and the authenticated 2 MiB SMW-US ROM"]
fn lunar_magic_batch_import_export_and_rejections_match_rust() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = root.join("lm363/Lunar Magic.exe");
    let baseline = root.join("Super Mario World (USA).sfc");
    let vanilla = root.join("sysLMRestore/smwOrig.smc");
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&executable).unwrap()),
        LUNAR_MAGIC_363_SHA256
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&baseline).unwrap()),
        INPUT_ROM_SHA256
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&vanilla).unwrap()),
        VANILLA_ROM_SHA256
    );
    let prefix = std::env::var_os("LUNAR_MAGIC_WINEPREFIX").map(PathBuf::from);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-title-recording-wine-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();

    let recording = TitleScreenRecording::from_bytes(vec![0x12, 0x34, 0x56, 0xff]).unwrap();
    let state = encode_zsnes_title_recording(&recording);
    assert_eq!(lm_oracle::sha256_hex(&state), ZSNES_STATE_SHA256);
    let state_path = directory.join("input.zst");
    fs::write(&state_path, &state).unwrap();

    let imported_rom = directory.join("imported.smc");
    fs::copy(&baseline, &imported_rom).unwrap();
    let import = run(
        prefix.as_deref(),
        &[
            &executable,
            Path::new("-ImportTitleMoves"),
            &imported_rom,
            &state_path,
        ],
    );
    assert!(import.status.success(), "{}", output_text(&import));
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&imported_rom).unwrap()),
        INSTALLED_ROM_SHA256
    );

    let exported_state = directory.join("exported.zst");
    let export = run(
        prefix.as_deref(),
        &[
            &executable,
            Path::new("-ExportTitleMoves"),
            &imported_rom,
            &exported_state,
        ],
    );
    assert!(export.status.success(), "{}", output_text(&export));
    assert_eq!(fs::read(&exported_state).unwrap(), state);

    let vanilla_imported = directory.join("vanilla-imported.smc");
    let vanilla_state = directory.join("vanilla-seven.zst");
    fs::write(
        &vanilla_state,
        encode_zsnes_title_recording(
            &TitleScreenRecording::from_bytes(vec![0x12, 0x12, 0x12, 0x12, 0x12, 0x12, 0xff])
                .unwrap(),
        ),
    )
    .unwrap();
    fs::copy(&vanilla, &vanilla_imported).unwrap();
    let vanilla_import = run(
        prefix.as_deref(),
        &[
            &executable,
            Path::new("-ImportTitleMoves"),
            &vanilla_imported,
            &vanilla_state,
        ],
    );
    assert!(
        vanilla_import.status.success(),
        "{}",
        output_text(&vanilla_import)
    );
    let vanilla_installed = fs::read(&vanilla_imported).unwrap();
    assert_eq!(vanilla_installed.len(), 0x10_0200);
    assert_eq!(
        lm_oracle::sha256_hex(&vanilla_installed),
        VANILLA_INSTALLED_ROM_SHA256
    );
    let updated_state = directory.join("vanilla-updated.zst");
    let mut updated_bytes = vec![0x56; 0x101];
    *updated_bytes.last_mut().unwrap() = 0xff;
    fs::write(
        &updated_state,
        encode_zsnes_title_recording(&TitleScreenRecording::from_bytes(updated_bytes).unwrap()),
    )
    .unwrap();
    let vanilla_update = run(
        prefix.as_deref(),
        &[
            &executable,
            Path::new("-ImportTitleMoves"),
            &vanilla_imported,
            &updated_state,
        ],
    );
    assert!(
        vanilla_update.status.success(),
        "{}",
        output_text(&vanilla_update)
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&vanilla_imported).unwrap()),
        VANILLA_UPDATED_ROM_SHA256
    );

    let malformed = directory.join("malformed.zst");
    fs::write(&malformed, [0; 12]).unwrap();
    let rejected_rom = directory.join("rejected.smc");
    fs::copy(&baseline, &rejected_rom).unwrap();
    let rejection = run(
        prefix.as_deref(),
        &[
            &executable,
            Path::new("-ImportTitleMoves"),
            &rejected_rom,
            &malformed,
        ],
    );
    assert!(!rejection.status.success());
    let rejection_text = output_text(&rejection);
    assert!(rejection_text.contains("Not a ZSNES Savestate!"));
    assert!(rejection_text.contains("This does not appear to be a valid ZSNES savestate."));
    assert_eq!(
        fs::read(&rejected_rom).unwrap(),
        fs::read(&baseline).unwrap()
    );

    let absent_output = directory.join("absent.zst");
    let absent = run(
        prefix.as_deref(),
        &[
            &executable,
            Path::new("-ExportTitleMoves"),
            &baseline,
            &absent_output,
        ],
    );
    assert!(!absent.status.success());
    let absent_text = output_text(&absent);
    assert!(absent_text.contains("ASM code not detected!"));
    assert!(absent_text.contains("there is nothing to export"));
    assert!(!absent_output.exists());

    fs::remove_dir_all(directory).unwrap();
}
