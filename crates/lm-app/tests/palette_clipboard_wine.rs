use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LUNAR_MAGIC_363_SHA256: &str =
    "b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff";
const PRISTINE_HEADERED_SMW_US_SHA256: &str =
    "5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf";
const SHOW_PALETTE_EDITOR: &str = "0x2528";

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

fn compile(root: &Path, source: &str, output: &Path, libraries: &[&str]) {
    let mut command = Command::new("i686-w64-mingw32-gcc");
    command
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(root.join("tools").join(source));
    command.args(libraries).arg("-o").arg(output);
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "cannot compile {source}:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn normalized(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .replace("\r\n", "\n")
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
fn original_palette_editor_publishes_exact_color_and_row_clipboard_records() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let source_rom = root.join("sysLMRestore/smwOrig.smc");
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&lunar_magic).unwrap()),
        LUNAR_MAGIC_363_SHA256
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&source_rom).unwrap()),
        PRISTINE_HEADERED_SMW_US_SHA256
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-palette-clipboard-wine-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let window_helper = directory.join("window.exe");
    let palette_helper = directory.join("palette.exe");
    compile(
        &root,
        "wine-window-command.c",
        &window_helper,
        &["-lcomctl32", "-lgdi32"],
    );
    compile(
        &root,
        "wine-palette-clipboard-oracle.c",
        &palette_helper,
        &["-lgdi32"],
    );

    let prefix = directory.join("prefix");
    successful(&prefix, "wineboot", &["-u"]);
    let executable_name = "LMPal.exe";
    let executable = directory.join(executable_name);
    let working_rom = directory.join("working.smc");
    fs::copy(&lunar_magic, &executable).unwrap();
    fs::copy(&source_rom, &working_rom).unwrap();
    let windows_rom = normalized(
        &successful(
            &prefix,
            "winepath",
            &["-w", working_rom.to_str().expect("ROM path is UTF-8")],
        )
        .stdout,
    );
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
    // A fresh Wine prefix needs an undisturbed graphics-driver startup interval before helper
    // processes can safely query its Toolhelp snapshot.
    thread::sleep(Duration::from_secs(10));
    successful(
        &prefix,
        "wine",
        &[
            window_helper.to_str().unwrap(),
            executable_name,
            "post-command",
            SHOW_PALETTE_EDITOR,
        ],
    );
    thread::sleep(Duration::from_secs(1));

    let color = normalized(
        &successful(
            &prefix,
            "wine",
            &[
                palette_helper.to_str().unwrap(),
                executable_name,
                "invoke-color",
            ],
        )
        .stdout,
    );
    let row = normalized(
        &successful(
            &prefix,
            "wine",
            &[
                palette_helper.to_str().unwrap(),
                executable_name,
                "invoke-row",
            ],
        )
        .stdout,
    );
    let decode_color = normalized(
        &successful(
            &prefix,
            "wine",
            &[
                palette_helper.to_str().unwrap(),
                executable_name,
                "decode-color",
            ],
        )
        .stdout,
    );
    let reject_color = normalized(
        &successful(
            &prefix,
            "wine",
            &[
                palette_helper.to_str().unwrap(),
                executable_name,
                "reject-color",
            ],
        )
        .stdout,
    );
    let decode_row = normalized(
        &successful(
            &prefix,
            "wine",
            &[
                palette_helper.to_str().unwrap(),
                executable_name,
                "decode-row",
            ],
        )
        .stdout,
    );
    let reject_row = normalized(
        &successful(
            &prefix,
            "wine",
            &[
                palette_helper.to_str().unwrap(),
                executable_name,
                "reject-row",
            ],
        )
        .stdout,
    );
    let oracle = fs::read_to_string(
        root.join("docs/oracle-work/lm363/pristine-us/palette-clipboard/oracle.tsv"),
    )
    .unwrap();
    let fields = oracle
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once('\t'))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        color,
        format!(
            "name={} size={} bytes={}\nname={} size={} bytes={}\n",
            fields["color_v2_format"],
            fields["color_v2_size"],
            fields["color_v2_bytes"],
            fields["color_legacy_format"],
            fields["color_legacy_size"],
            fields["color_legacy_bytes"]
        )
    );
    assert_eq!(
        row,
        format!(
            "name={} size={} bytes={}\nname={} size={} bytes={}\n",
            fields["row_v2_format"],
            fields["row_v2_size"],
            fields["row_v2_bytes"],
            fields["row_legacy_format"],
            fields["row_legacy_size"],
            fields["row_legacy_bytes"]
        )
    );
    assert_eq!(
        decode_color,
        format!("result={}\n", fields["color_v2_decode_result"])
    );
    assert_eq!(
        reject_color,
        format!("result={}\n", fields["color_v2_short_result"])
    );
    assert_eq!(
        decode_row,
        format!(
            "result={} colors={}\n",
            fields["row_v2_decode_result"], fields["row_v2_decode_colors"]
        )
    );
    assert_eq!(
        reject_row,
        format!("result={}\n", fields["row_v2_short_result"])
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&working_rom).unwrap()),
        PRISTINE_HEADERED_SMW_US_SHA256
    );

    wine.stop();
    fs::remove_dir_all(directory).unwrap();
}
