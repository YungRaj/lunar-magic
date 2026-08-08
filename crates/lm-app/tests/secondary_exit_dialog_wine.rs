use lm_level::{SecondaryExit, SecondaryExitTable};
use lm_project::Project;
use lm_rom::{RomImage, detect_identity};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TARGET_EXIT_INDEX: usize = 0x1ffe;
const LUNAR_MAGIC_363_SHA256: &str =
    "b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff";
const PRISTINE_HEADERED_SMW_US_SHA256: &str =
    "5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf";

fn wine_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .env("WINEDEBUG", "-all")
        .env("WINEDLLOVERRIDES", "d3d9=");
    command
}

fn successful(mut command: Command, description: &str) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("cannot launch {description}: {error}"));
    assert!(
        output.status.success(),
        "{description} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn wine_path(path: &Path) -> String {
    let mut command = wine_command("winepath");
    command.args(["-w"]).arg(path);
    String::from_utf8(successful(command, "winepath").stdout)
        .unwrap()
        .trim()
        .to_owned()
}

fn compile_helpers(root: &Path, directory: &Path) -> (PathBuf, PathBuf) {
    let oracle = directory.join("wine-secondary-exit-oracle.exe");
    let output = Command::new("i686-w64-mingw32-gcc")
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(root.join("tools/wine-secondary-exit-oracle.c"))
        .arg("-o")
        .arg(&oracle)
        .arg("-luser32")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "secondary-exit helper compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture = directory.join("capture-macos-window");
    let output = Command::new("xcrun")
        .args(["swiftc"])
        .arg(root.join("tools/capture-macos-window.swift"))
        .arg("-o")
        .arg(&capture)
        .args([
            "-framework",
            "AppKit",
            "-framework",
            "CoreGraphics",
            "-framework",
            "ImageIO",
            "-framework",
            "ScreenCaptureKit",
            "-framework",
            "UniformTypeIdentifiers",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "capture helper compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (oracle, capture)
}

fn wait_for_file_or_exit(path: &Path, child: &mut Child) {
    for _ in 0..800 {
        if path.is_file() {
            return;
        }
        if let Some(status) = child.try_wait().unwrap() {
            let stdout = child.stdout.take().map(|mut stdout| {
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut stdout, &mut bytes).unwrap();
                bytes
            });
            panic!(
                "secondary-exit oracle exited before publishing dialog state: {status}\n{}",
                String::from_utf8_lossy(stdout.as_deref().unwrap_or_default())
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("secondary-exit oracle did not publish dialog state");
}

fn wait_for_launcher(mut launcher: Child) {
    for _ in 0..100 {
        if launcher.try_wait().unwrap().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = launcher.kill();
    panic!("Lunar Magic did not close after the secondary-exit oracle");
}

fn drive_dialog(
    root: &Path,
    lunar_magic: &Path,
    helper: &Path,
    capture_helper: &Path,
    directory: &Path,
    rom: &Path,
    mode: &str,
    capture: Option<(&str, &Path)>,
) -> String {
    let ready = directory.join(format!("{mode}-ready"));
    let continue_path = directory.join(format!("{mode}-continue"));
    let mut launcher_command = wine_command("wine");
    let launcher = launcher_command
        .current_dir(root)
        .arg(lunar_magic)
        .arg(wine_path(rom))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_secs(3));
    let mut oracle_command = wine_command("wine");
    let mut oracle = oracle_command
        .args([
            helper.to_str().unwrap(),
            "Lunar Magic.exe",
            mode,
            &wine_path(&ready),
            &wine_path(&continue_path),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for_file_or_exit(&ready, &mut oracle);
    if let Some((title, capture_path)) = capture {
        let values = fs::read_to_string(&ready)
            .unwrap()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 4);
        let output = Command::new(capture_helper)
            .args(["wine", title])
            .arg(capture_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "dialog capture failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(fs::metadata(capture_path).unwrap().len() > 10_000);
    }
    fs::write(&continue_path, []).unwrap();
    let output = oracle.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{mode} oracle failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    wait_for_launcher(launcher);
    String::from_utf8(output.stdout).unwrap()
}

fn decoded_table(path: &Path) -> SecondaryExitTable {
    Project::new(RomImage::from_bytes(fs::read(path).unwrap()).unwrap())
        .load_secondary_exit_table_detected(lm_profile::smw_us_v1_secondary_exit_locator())
        .unwrap()
        .table
}

fn assert_valid_checksum(path: &Path) {
    let image = RomImage::from_bytes(fs::read(path).unwrap()).unwrap();
    assert!(detect_identity(&image).unwrap().checksum_matches());
}

/// Authenticates Lunar Magic 3.63's original secondary-exit dialog across edit, OK, reopen,
/// Cancel rollback, Clear Slot, and both branches of the Clear All confirmation prompt.
#[test]
#[ignore = "requires macOS ScreenCaptureKit, Wine, MinGW, Lunar Magic 3.63, and pristine SMW-US"]
fn original_secondary_exit_dialog_applies_clears_rejects_and_cancels_losslessly() {
    let root = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let pristine = root.join("sysLMRestore/smwOrig.smc");
    let pristine_bytes = fs::read(&pristine).unwrap();
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&lunar_magic).unwrap()),
        LUNAR_MAGIC_363_SHA256
    );
    assert_eq!(
        lm_oracle::sha256_hex(&pristine_bytes),
        PRISTINE_HEADERED_SMW_US_SHA256
    );
    let mut tasklist_command = wine_command("wine");
    tasklist_command.arg("tasklist");
    let tasklist = successful(tasklist_command, "wine tasklist");
    assert!(
        !String::from_utf8_lossy(&tasklist.stdout).contains("Lunar Magic.exe"),
        "secondary-exit oracle requires no pre-existing Lunar Magic process"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = root
        .join("target/secondary-exit-dialog-oracle")
        .join(format!("{}-{nonce}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let (helper, capture_helper) = compile_helpers(&root, &directory);

    let cancelled_rom = directory.join("cancelled.smc");
    fs::write(&cancelled_rom, &pristine_bytes).unwrap();
    let cancel_output = drive_dialog(
        &root,
        &lunar_magic,
        &helper,
        &capture_helper,
        &directory,
        &cancelled_rom,
        "cancel-reopen",
        None,
    );
    assert!(cancel_output.contains("cancel-reopen-destination=0"));
    assert_eq!(fs::read(&cancelled_rom).unwrap(), pristine_bytes);

    let applied_rom = directory.join("applied.smc");
    let configured_capture = directory.join("configured.png");
    fs::write(&applied_rom, &pristine_bytes).unwrap();
    drive_dialog(
        &root,
        &lunar_magic,
        &helper,
        &capture_helper,
        &directory,
        &applied_rom,
        "apply",
        Some(("Modify Secondary Entrances (in hex)", &configured_capture)),
    );
    let expected = SecondaryExit {
        destination_level: 0x1ab,
        position_and_method: 0xeb,
        screen: 0x1d,
        x: 0,
        y: 6,
        destination_flags: 0x84,
        x_and_overworld_flags: 0,
        additional_flags: 0x60,
    };
    assert_eq!(
        decoded_table(&applied_rom).entries[TARGET_EXIT_INDEX],
        expected
    );
    assert_valid_checksum(&applied_rom);

    let applied_bytes = fs::read(&applied_rom).unwrap();
    let reopened = drive_dialog(
        &root,
        &lunar_magic,
        &helper,
        &capture_helper,
        &directory,
        &applied_rom,
        "reopen",
        None,
    );
    for state in [
        "control=006d class=ComboBox enabled=1 visible=1 checked=0 selection=8190 count=8192",
        "control=01a0 class=ComboBox enabled=1 visible=1 checked=0 selection=6 count=7",
        "control=01a1 class=ComboBox enabled=1 visible=1 checked=0 selection=11 count=16",
        "control=01c1 class=Button enabled=1 visible=1 checked=1",
        "control=01c0 class=Button enabled=1 visible=1 checked=1",
        "control=028e class=Button enabled=1 visible=1 checked=1",
    ] {
        assert!(reopened.contains(state), "missing reopened state: {state}");
    }
    assert_eq!(fs::read(&applied_rom).unwrap(), applied_bytes);

    let rejected_rom = directory.join("clear-all-rejected.smc");
    let confirmation_capture = directory.join("clear-all-confirmation.png");
    fs::write(&rejected_rom, &applied_bytes).unwrap();
    let rejected = drive_dialog(
        &root,
        &lunar_magic,
        &helper,
        &capture_helper,
        &directory,
        &rejected_rom,
        "clear-all-no",
        Some(("Modify Secondary Entrances (in hex)", &confirmation_capture)),
    );
    assert!(rejected.contains("clear-all-prompt=Really clear all slots?"));
    assert!(rejected.contains("clear-all-no-destination=1AB"));
    assert_eq!(fs::read(&rejected_rom).unwrap(), applied_bytes);

    let slot_cleared_rom = directory.join("slot-cleared.smc");
    fs::write(&slot_cleared_rom, &applied_bytes).unwrap();
    drive_dialog(
        &root,
        &lunar_magic,
        &helper,
        &capture_helper,
        &directory,
        &slot_cleared_rom,
        "clear-slot",
        None,
    );
    assert_eq!(
        decoded_table(&slot_cleared_rom).entries[TARGET_EXIT_INDEX],
        SecondaryExit::default()
    );
    assert_valid_checksum(&slot_cleared_rom);

    let all_cleared_rom = directory.join("all-cleared.smc");
    fs::write(&all_cleared_rom, &applied_bytes).unwrap();
    let cleared = drive_dialog(
        &root,
        &lunar_magic,
        &helper,
        &capture_helper,
        &directory,
        &all_cleared_rom,
        "clear-all-yes",
        None,
    );
    assert!(cleared.contains("clear-all-yes-destination=0"));
    assert!(
        decoded_table(&all_cleared_rom)
            .entries
            .iter()
            .all(|entry| *entry == SecondaryExit::default())
    );
    assert_valid_checksum(&all_cleared_rom);

    let retained = root.join("docs/oracle-work/lm363/pristine-us/secondary-exit-dialog");
    if std::env::var_os("LM_UPDATE_SECONDARY_EXIT_DIALOG_ORACLE").is_some() {
        fs::create_dir_all(&retained).unwrap();
        fs::copy(&configured_capture, retained.join("configured.png")).unwrap();
        fs::copy(
            &confirmation_capture,
            retained.join("clear-all-confirmation.png"),
        )
        .unwrap();
    }
    assert_eq!(
        fs::read(&configured_capture).unwrap(),
        fs::read(retained.join("configured.png")).unwrap()
    );
    assert_eq!(
        fs::read(&confirmation_capture).unwrap(),
        fs::read(retained.join("clear-all-confirmation.png")).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}
