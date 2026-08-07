use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SETTINGS_KEY: &str = r"HKCU\Software\LunarianConcepts\LunarMagic\Settings";
const LEVEL_LIMIT_ADDRESS: &str = "0x005e7734,4";
const OVERWORLD_LIMIT_ADDRESS: &str = "0x005e477c,4";
const LUNAR_MAGIC_363_SHA256: &str =
    "b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff";
const PRISTINE_HEADERED_SMW_US_SHA256: &str =
    "5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf";

fn run(prefix: &Path, program: &str, arguments: &[&str]) -> std::process::Output {
    Command::new(program)
        .env("WINEPREFIX", prefix)
        .env("WINEDEBUG", "-all")
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("cannot run {program}: {error}"))
}

fn successful(prefix: &Path, program: &str, arguments: &[&str]) -> std::process::Output {
    let output = run(prefix, program, arguments);
    assert!(
        output.status.success(),
        "{program} {arguments:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn wine_path(prefix: &Path, path: &Path) -> String {
    let host = path.to_str().expect("oracle paths are UTF-8");
    let output = successful(prefix, "winepath", &["-w", host]);
    String::from_utf8(output.stdout)
        .expect("winepath output is UTF-8")
        .trim()
        .to_owned()
}

fn read_u32(prefix: &Path, helper: &Path, address: &str) -> Option<u32> {
    let helper = helper.to_str().expect("helper path is UTF-8");
    let output = run(
        prefix,
        "wine",
        &[helper, "LMUndoOracle.exe", "read", address],
    );
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let bytes = text.trim().as_bytes();
    if bytes.len() != 8 {
        return None;
    }
    let mut value = [0_u8; 4];
    for (index, byte) in value.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(std::str::from_utf8(&bytes[start..start + 2]).ok()?, 16).ok()?;
    }
    Some(u32::from_le_bytes(value))
}

fn stop_isolated_wine(prefix: &Path, child: &mut Child) {
    let _ = run(prefix, "wineserver", &["-k"]);
    let _ = child.wait();
}

fn observe_limits(
    prefix: &Path,
    executable: &Path,
    helper: &Path,
    rom: &str,
    expected: u32,
) -> Option<(u32, u32)> {
    let mut child = Command::new("wine")
        .env("WINEPREFIX", prefix)
        .env("WINEDEBUG", "-all")
        .arg(executable)
        .arg(rom)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Do not let the zero boundary match globals before startup has loaded the ROM and settings.
    thread::sleep(Duration::from_millis(500));
    let mut observed = None;
    for _ in 0..100 {
        let level = read_u32(prefix, helper, LEVEL_LIMIT_ADDRESS);
        let overworld = read_u32(prefix, helper, OVERWORLD_LIMIT_ADDRESS);
        if level == Some(expected) && overworld == Some(expected) {
            observed = Some((level.unwrap(), overworld.unwrap()));
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    stop_isolated_wine(prefix, &mut child);
    observed
}

#[test]
#[ignore = "requires Wine, MinGW, local Lunar Magic 3.63, and the verified pristine SMW-US ROM"]
fn original_lunar_magic_shares_and_clamps_every_undo_history_boundary() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let pristine = root.join("sysLMRestore/smwOrig.smc");
    assert!(lunar_magic.is_file(), "missing {}", lunar_magic.display());
    assert!(pristine.is_file(), "missing {}", pristine.display());
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&lunar_magic).unwrap()),
        LUNAR_MAGIC_363_SHA256,
        "unexpected Lunar Magic executable"
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&pristine).unwrap()),
        PRISTINE_HEADERED_SMW_US_SHA256,
        "unexpected pristine ROM fixture"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-undo-history-wine-{}-{nonce}",
        std::process::id(),
    ));
    fs::create_dir(&directory).unwrap();
    let prefix = directory.join("prefix");
    successful(&prefix, "wineboot", &["-u"]);

    let oracle_executable = directory.join("LMUndoOracle.exe");
    fs::copy(&lunar_magic, &oracle_executable).unwrap();
    let helper = directory.join("wine-window-command.exe");
    let compiler_output = Command::new("i686-w64-mingw32-gcc")
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(root.join("tools/wine-window-command.c"))
        .args(["-lcomctl32", "-lgdi32", "-o"])
        .arg(&helper)
        .output()
        .expect("cannot launch MinGW compiler");
    assert!(
        compiler_output.status.success(),
        "helper compilation failed:\n{}",
        String::from_utf8_lossy(&compiler_output.stderr)
    );
    let rom = wine_path(&prefix, &pristine);

    assert_eq!(
        observe_limits(&prefix, &oracle_executable, &helper, &rom, 33),
        Some((33, 33)),
        "a fresh Lunar Magic 3.63 prefix did not apply its default to both editors"
    );
    for configured in [0_u32, 1, 2, 9, 33, 51, 52] {
        successful(
            &prefix,
            "wine",
            &[
                "reg",
                "add",
                SETTINGS_KEY,
                "/v",
                "UndoMain",
                "/t",
                "REG_DWORD",
                "/d",
                &configured.to_string(),
                "/f",
            ],
        );
        let expected = configured.min(51);
        let observed = observe_limits(&prefix, &oracle_executable, &helper, &rom, expected);
        assert_eq!(
            observed,
            Some((expected, expected)),
            "configured UndoMain {configured} did not reach both editor histories"
        );
    }

    let _ = fs::remove_dir_all(directory);
}
