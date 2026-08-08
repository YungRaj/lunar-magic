use lm_level::{MwlFile, MwlLevelHeaderSection, MwlSectionKind};
use lm_rom::{RomImage, detect_identity};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    let oracle = directory.join("wine-main-midway-entrance-oracle.exe");
    let output = Command::new("i686-w64-mingw32-gcc")
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(root.join("tools/wine-main-midway-entrance-oracle.c"))
        .arg("-o")
        .arg(&oracle)
        .arg("-luser32")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "entrance helper compilation failed:\n{}",
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
            panic!("entrance oracle exited before publishing dialog state: {status}");
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("entrance oracle did not publish dialog state");
}

fn wait_for_launcher(mut launcher: Child) {
    for _ in 0..80 {
        if launcher.try_wait().unwrap().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = launcher.kill();
    panic!("Lunar Magic did not close after the entrance-dialog oracle");
}

fn drive_dialog(
    root: &Path,
    lunar_magic: &Path,
    helper: &Path,
    capture_helper: &Path,
    directory: &Path,
    rom: &Path,
    mode: &str,
    capture: Option<&Path>,
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
    if let Some(capture) = capture {
        let output = Command::new(capture_helper)
            .args(["wine", "Modify Main and Midway Entrance (in hex)"])
            .arg(capture)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "dialog capture failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(fs::metadata(capture).unwrap().len() > 10_000);
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

fn export_header(lunar_magic: &Path, rom: &Path, output: &Path) -> MwlLevelHeaderSection {
    let mut command = wine_command("wine");
    command
        .arg(lunar_magic)
        .args(["-ExportLevel", &wine_path(rom), &wine_path(output), "105"]);
    successful(command, "Lunar Magic level export");
    let mwl = MwlFile::decode(&fs::read(output).unwrap()).unwrap();
    MwlLevelHeaderSection::decode(mwl.section(MwlSectionKind::LevelHeader)).unwrap()
}

/// Drives every mutable group in Lunar Magic 3.63's original main/midway entrance dialog,
/// proves Cancel is byte-atomic, proves OK installs and persists separate-midway settings, and
/// reopens the original dialog to authenticate its persisted checkbox/combo state.
#[test]
#[ignore = "requires macOS ScreenCaptureKit, Wine, MinGW, Lunar Magic 3.63, and pristine SMW-US"]
fn original_main_midway_dialog_applies_reopens_and_cancels_losslessly() {
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
        "entrance oracle requires no pre-existing Lunar Magic process"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = root
        .join("target/main-midway-entrance-oracle")
        .join(format!("{}-{nonce}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let (helper, capture_helper) = compile_helpers(&root, &directory);

    let cancelled_rom = directory.join("cancelled.smc");
    fs::write(&cancelled_rom, &pristine_bytes).unwrap();
    drive_dialog(
        &root,
        &lunar_magic,
        &helper,
        &capture_helper,
        &directory,
        &cancelled_rom,
        "cancel",
        None,
    );
    assert_eq!(fs::read(&cancelled_rom).unwrap(), pristine_bytes);

    let applied_rom = directory.join("applied.smc");
    let dialog_capture = directory.join("dialog.png");
    fs::write(&applied_rom, &pristine_bytes).unwrap();
    drive_dialog(
        &root,
        &lunar_magic,
        &helper,
        &capture_helper,
        &directory,
        &applied_rom,
        "apply",
        Some(&dialog_capture),
    );
    let header = export_header(&lunar_magic, &applied_rom, &directory.join("applied.mwl"));
    let main = header.main_entrance();
    assert_eq!(
        [
            main.position,
            main.vertical_settings,
            main.screen_and_method,
            main.level_mode_and_screen,
            main.flags,
            main.high_position,
            main.additional_flags,
        ],
        [0x54, 0x13, 0xb7, 0x1a, 0xc0, 0x00, 0x5a]
    );
    let midway = header.midway_entrance();
    assert_eq!(
        [
            midway.position,
            midway.flags,
            midway.high_position,
            midway.additional_flags,
        ],
        [0x00, 0xe9, 0x0a, 0x4b]
    );
    let image = RomImage::from_bytes(fs::read(&applied_rom).unwrap()).unwrap();
    assert!(detect_identity(&image).unwrap().checksum_matches());
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
        "control=01e0 class=Button enabled=1 visible=1 checked=1",
        "control=01a0 class=ComboBox enabled=1 visible=1 checked=0 selection=3",
        "control=01a1 class=ComboBox enabled=1 visible=1 checked=0 selection=4",
        "control=01e4 class=ComboBox enabled=1 visible=1 checked=0 selection=2",
        "control=01e5 class=ComboBox enabled=1 visible=1 checked=0 selection=3",
    ] {
        assert!(reopened.contains(state), "missing reopened state: {state}");
    }
    assert_eq!(
        fs::read(&applied_rom).unwrap(),
        applied_bytes,
        "reopen followed by Cancel must not mutate the applied ROM"
    );

    let retained =
        root.join("docs/oracle-work/lm363/pristine-us/main-midway-entrance/dialog-configured.png");
    if std::env::var_os("LM_UPDATE_MAIN_MIDWAY_ORACLE").is_some() {
        fs::create_dir_all(retained.parent().unwrap()).unwrap();
        fs::copy(&dialog_capture, &retained).unwrap();
    }
    assert_eq!(
        fs::read(&dialog_capture).unwrap(),
        fs::read(&retained).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}
