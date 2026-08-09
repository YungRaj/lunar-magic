use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LUNAR_MAGIC_363_SHA256: &str =
    "b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff";
const INSTALLED_LEVEL_105_SMW_US_SHA256: &str =
    "69cc6693ccd83f67369479314466b53c50e57569d319d9f8078667cfc025928e";
const SHOW_GRAPHICS_EDITOR: &str = "0x232a";

fn run(prefix: &Path, program: &str, arguments: &[&str]) -> Output {
    Command::new(program)
        .env("WINEPREFIX", prefix)
        .env("WINEDEBUG", "-all")
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("cannot run {program}: {error}"))
}

fn successful(prefix: &Path, program: &str, arguments: &[&str]) -> Output {
    let output = run(prefix, program, arguments);
    assert!(
        output.status.success(),
        "{program} {arguments:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn compile_helper(root: &Path, source: &str, destination: &Path, libraries: &[&str]) {
    let mut compiler = Command::new("i686-w64-mingw32-gcc");
    compiler
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(root.join("tools").join(source));
    for library in libraries {
        compiler.arg(library);
    }
    let output = compiler.arg("-o").arg(destination).output().unwrap();
    assert!(
        output.status.success(),
        "cannot compile {source}:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn post_command(prefix: &Path, helper: &Path, executable: &str, command: &str) {
    successful(
        prefix,
        "wine",
        &[
            helper.to_str().expect("helper path is UTF-8"),
            executable,
            "post-command",
            command,
        ],
    );
}

fn normalized(output: &[u8]) -> String {
    String::from_utf8(output.to_vec())
        .expect("oracle output is UTF-8")
        .replace("\r\n", "\n")
}

fn wait_for_initial_clipboard_copy(
    prefix: &Path,
    clipboard_helper: &Path,
    executable: &str,
) -> Output {
    for _ in 0..100 {
        let output = run(
            prefix,
            "wine",
            &[
                clipboard_helper.to_str().expect("helper path is UTF-8"),
                executable,
                "copy",
            ],
        );
        if output.status.success() {
            return output;
        }
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Window8x8 not found"),
            "unexpected graphics-editor readiness failure:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        thread::sleep(Duration::from_millis(250));
    }
    panic!("Lunar Magic did not open Window8x8 within twenty-five seconds");
}

struct IsolatedWine {
    prefix: PathBuf,
    child: Option<Child>,
}

impl IsolatedWine {
    fn stop(&mut self) {
        let _ = run(&self.prefix, "wineserver", &["-k"]);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for IsolatedWine {
    fn drop(&mut self) {
        self.stop();
    }
}

#[test]
#[ignore = "requires Wine, MinGW, local Lunar Magic 3.63, and the verified pristine SMW-US ROM"]
fn original_graphics_editor_gestures_preserve_private_buffer_and_guard_sheet_paste() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let source_rom = root.join("oracle-work/lm363/pristine-us/level-save-105/after.smc");
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&lunar_magic).unwrap()),
        LUNAR_MAGIC_363_SHA256
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&source_rom).unwrap()),
        INSTALLED_LEVEL_105_SMW_US_SHA256
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-graphics-editor-wine-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let window_helper = directory.join("wine-window-command.exe");
    let pixel_helper = directory.join("wine-graphics-pixel-oracle.exe");
    let cache_helper = directory.join("wine-graphics-cache-oracle.exe");
    let clipboard_helper = directory.join("wine-graphics-clipboard-oracle.exe");
    compile_helper(
        &root,
        "wine-window-command.c",
        &window_helper,
        &["-lcomctl32", "-lgdi32"],
    );
    compile_helper(&root, "wine-graphics-pixel-oracle.c", &pixel_helper, &[]);
    compile_helper(&root, "wine-graphics-cache-oracle.c", &cache_helper, &[]);
    compile_helper(
        &root,
        "wine-graphics-clipboard-oracle.c",
        &clipboard_helper,
        &[],
    );

    let prefix = directory.join("prefix");
    successful(&prefix, "wineboot", &["-u"]);
    let executable_name = "LMGraphicsOracle.exe";
    let executable = directory.join(executable_name);
    let working_rom = directory.join("working.smc");
    fs::copy(&lunar_magic, &executable).unwrap();
    fs::copy(&source_rom, &working_rom).unwrap();
    let windows_rom = String::from_utf8(
        successful(
            &prefix,
            "winepath",
            &["-w", working_rom.to_str().expect("ROM path is UTF-8")],
        )
        .stdout,
    )
    .unwrap();
    let child = Command::new("wine")
        .env("WINEPREFIX", &prefix)
        .env("WINEDEBUG", "-all")
        .arg(&executable)
        .arg(windows_rom.trim())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut wine = IsolatedWine {
        prefix: prefix.clone(),
        child: Some(child),
    };

    // Fresh macOS Wine prefixes need an undisturbed graphics-driver initialization interval.
    thread::sleep(Duration::from_secs(10));
    post_command(
        &prefix,
        &window_helper,
        executable_name,
        SHOW_GRAPHICS_EDITOR,
    );
    let copied = wait_for_initial_clipboard_copy(&prefix, &clipboard_helper, executable_name);
    successful(
        &prefix,
        "wine",
        &[
            window_helper.to_str().expect("helper path is UTF-8"),
            executable_name,
            "write-byte",
            "0x005e54f0,63",
        ],
    );
    let expected_zero = "00".repeat(64);
    assert_eq!(
        normalized(&copied.stdout),
        format!("format=Lunar Magic 8x8 Tile\nsize=64\nbytes={expected_zero}\n")
    );
    let roundtrip = successful(
        &prefix,
        "wine",
        &[
            clipboard_helper.to_str().expect("helper path is UTF-8"),
            executable_name,
            "roundtrip",
        ],
    );
    let expected_pixels = "000102030405060708090A0B0C0D0E0F".repeat(4);
    assert_eq!(
        normalized(&roundtrip.stdout),
        format!("format=Lunar Magic 8x8 Tile\nsize=64\nbytes={expected_pixels}\n")
    );

    let pixel = successful(
        &prefix,
        "wine",
        &[
            pixel_helper.to_str().expect("helper path is UTF-8"),
            executable_name,
        ],
    );
    assert_eq!(
        normalized(&pixel.stdout),
        fs::read_to_string(
            root.join("docs/oracle-work/lm363/pristine-us/graphics-pixel-buffer/oracle.tsv")
        )
        .unwrap()
    );
    let cache = successful(
        &prefix,
        "wine",
        &[
            cache_helper.to_str().expect("helper path is UTF-8"),
            executable_name,
        ],
    );
    assert_eq!(
        normalized(&cache.stdout),
        fs::read_to_string(
            root.join("docs/oracle-work/lm363/pristine-us/graphics-cache-paste/oracle.tsv")
        )
        .unwrap()
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&working_rom).unwrap()),
        INSTALLED_LEVEL_105_SMW_US_SHA256,
        "transient graphics gestures must not save the ROM"
    );

    wine.stop();
    fs::remove_dir_all(directory).unwrap();
}
