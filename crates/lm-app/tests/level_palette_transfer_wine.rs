use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LUNAR_MAGIC_363_SHA256: &str =
    "b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff";
const PRISTINE_HEADERED_SMW_US_SHA256: &str =
    "5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf";
const PRISTINE_LEVEL_105_RGB_SHA256: &str =
    "88586ad377c5501476d93a820387c58312df9d05a64dd68af8f3131d71d10afa";
const PRISTINE_LEVEL_105_TPL_SHA256: &str =
    "d4da32140cc2994b332e2bfd86579a7002868d692a4c6779ae99adedc6182201";
const PRISTINE_LEVEL_105_RAW_SHA256: &str =
    "8a50127cc38c0f39120687e3b4c2fa3067ded7dfbddf49c88a1d431003640c8f";
const WORKING_PALETTE: &str = "0x008634c0,514";
const TRANSFER_MASK: &str = "0x0086b6e0,257";
const EXPORT_LEVEL_PALETTE: &str = "0x239f";
const IMPORT_LEVEL_PALETTE: &str = "0x23a0";
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

fn wine_path(prefix: &Path, path: &Path) -> String {
    String::from_utf8(
        successful(
            prefix,
            "winepath",
            &["-w", path.to_str().expect("oracle paths are UTF-8")],
        )
        .stdout,
    )
    .expect("winepath output is UTF-8")
    .trim()
    .to_owned()
}

fn parse_hex_bytes(output: &[u8], expected: usize) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(output).ok()?.trim();
    if text.len() != expected.checked_mul(2)? {
        return None;
    }
    (0..expected)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok())
        .collect()
}

fn read_bytes(prefix: &Path, helper: &Path, executable: &str, request: &str) -> Option<Vec<u8>> {
    let output = run(
        prefix,
        "wine",
        &[
            helper.to_str().expect("helper path is UTF-8"),
            executable,
            "read",
            request,
        ],
    );
    output
        .status
        .success()
        .then(|| {
            let expected = request.rsplit_once(',')?.1.parse().ok()?;
            parse_hex_bytes(&output.stdout, expected)
        })
        .flatten()
}

fn wait_for_palette(prefix: &Path, helper: &Path, executable: &str) -> (Vec<u8>, Vec<u8>) {
    // A new macOS Wine prefix needs an undisturbed interval to create its graphics driver and
    // main window. Starting Toolhelp helper processes immediately can otherwise starve startup.
    thread::sleep(Duration::from_secs(5));
    for _ in 0..200 {
        if let (Some(palette), Some(mask)) = (
            read_bytes(prefix, helper, executable, WORKING_PALETTE),
            read_bytes(prefix, helper, executable, TRANSFER_MASK),
        ) && mask == vec![1; 257]
            && palette.iter().any(|byte| *byte != 0)
        {
            return (palette, mask);
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("Lunar Magic did not publish its working palette within twenty-five seconds");
}

fn wait_for_mask(
    prefix: &Path,
    helper: &Path,
    executable: &str,
    expected_mask: &[u8],
) -> (Vec<u8>, Vec<u8>) {
    for _ in 0..100 {
        if let (Some(palette), Some(mask)) = (
            read_bytes(prefix, helper, executable, WORKING_PALETTE),
            read_bytes(prefix, helper, executable, TRANSFER_MASK),
        ) && mask == expected_mask
        {
            return (palette, mask);
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("Lunar Magic did not publish the imported palette selector");
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

fn submit_file_dialog(prefix: &Path, helper: &Path, executable: &str, path: &Path) {
    let windows_path = wine_path(prefix, path);
    for _ in 0..200 {
        let output = run(
            prefix,
            "wine",
            &[
                helper.to_str().expect("helper path is UTF-8"),
                executable,
                "save",
                &windows_path,
            ],
        );
        if output.status.success() {
            return;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("file-name control not found"),
            "unexpected file-dialog failure:\nstdout:\n{}\nstderr:\n{stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        thread::sleep(Duration::from_millis(25));
    }
    panic!("Lunar Magic file dialog did not become ready within five seconds");
}

fn submit_file_dialog_and_acknowledge_message(
    prefix: &Path,
    helper: &Path,
    executable: &str,
    path: &Path,
    expected_message: &str,
) {
    let windows_path = wine_path(prefix, path);
    let helper_path = helper.to_str().expect("helper path is UTF-8");
    let submit = Command::new("wine")
        .env("WINEPREFIX", prefix)
        .env("WINEDEBUG", "-all")
        .args([helper_path, executable, "save", &windows_path])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut observed = None;
    for _ in 0..200 {
        let output = run(prefix, "wine", &[helper_path, executable, "children"]);
        let text = String::from_utf8_lossy(&output.stdout);
        if output.status.success() && text.contains(expected_message) {
            observed = Some(text.into_owned());
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        observed.is_some(),
        "Lunar Magic did not display the expected rejection: {expected_message}"
    );
    successful(prefix, "wine", &[helper_path, executable, "click", "1"]);
    let output = submit.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "file-dialog helper failed after message acknowledgement:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn export_palette(prefix: &Path, helper: &Path, executable: &str, path: &Path) -> Vec<u8> {
    post_command(prefix, helper, executable, EXPORT_LEVEL_PALETTE);
    submit_file_dialog(prefix, helper, executable, path);
    for _ in 0..200 {
        if let Ok(bytes) = fs::read(path) {
            return bytes;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("Lunar Magic did not publish {}", path.display());
}

fn stop_isolated_wine(prefix: &Path, child: &mut Child) {
    let _ = run(prefix, "wineserver", &["-k"]);
    let _ = child.kill();
    let _ = child.wait();
}

fn words(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks_exact(2)
        .map(|word| u16::from_le_bytes([word[0], word[1]]))
        .collect()
}

#[test]
#[ignore = "requires Wine, MinGW, local Lunar Magic 3.63, and the verified pristine SMW-US ROM"]
fn original_lunar_magic_level_palette_transfer_formats_masks_and_auto_enable_match() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let pristine = root.join("sysLMRestore/smwOrig.smc");
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&lunar_magic).unwrap()),
        LUNAR_MAGIC_363_SHA256
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&pristine).unwrap()),
        PRISTINE_HEADERED_SMW_US_SHA256
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-level-palette-wine-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let prefix = directory.join("prefix");
    successful(&prefix, "wineboot", &["-u"]);

    // Keep the basename below the legacy 16-character process-name boundary used by
    // Wine's Toolhelp snapshot implementation on macOS.
    let executable_name = "LMPal.exe";
    let executable = directory.join(executable_name);
    fs::copy(&lunar_magic, &executable).unwrap();
    let helper = directory.join("wine-window-command.exe");
    let compiler = Command::new("i686-w64-mingw32-gcc")
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(root.join("tools/wine-window-command.c"))
        .args(["-lcomctl32", "-lgdi32", "-o"])
        .arg(&helper)
        .output()
        .expect("cannot launch MinGW compiler");
    assert!(
        compiler.status.success(),
        "helper compilation failed:\n{}",
        String::from_utf8_lossy(&compiler.stderr)
    );

    let rom = wine_path(&prefix, &pristine);
    let mut child = Command::new("wine")
        .env("WINEPREFIX", &prefix)
        .env("WINEDEBUG", "-all")
        .arg(&executable)
        .arg(&rom)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    eprintln!("palette oracle: waiting for original working buffers");
    let (baseline_bytes, baseline_mask) = wait_for_palette(&prefix, &helper, executable_name);
    assert_eq!(baseline_bytes.len(), 514);
    assert_eq!(baseline_mask, vec![1; 257]);
    let baseline = words(&baseline_bytes);
    assert_eq!(baseline.len(), 257);

    let rgb_path = directory.join("level.pal");
    let tpl_path = directory.join("level.tpl");
    let raw_path = directory.join("level.mw3");
    eprintln!("palette oracle: exporting RGB, TPL v2, and raw palettes");
    let rgb = export_palette(&prefix, &helper, executable_name, &rgb_path);
    let tpl = export_palette(&prefix, &helper, executable_name, &tpl_path);
    let raw = export_palette(&prefix, &helper, executable_name, &raw_path);
    eprintln!(
        "palette oracle hashes: pal={} tpl={} raw={}",
        lm_oracle::sha256_hex(&rgb),
        lm_oracle::sha256_hex(&tpl),
        lm_oracle::sha256_hex(&raw)
    );
    assert_eq!(lm_oracle::sha256_hex(&rgb), PRISTINE_LEVEL_105_RGB_SHA256);
    assert_eq!(lm_oracle::sha256_hex(&tpl), PRISTINE_LEVEL_105_TPL_SHA256);
    assert_eq!(lm_oracle::sha256_hex(&raw), PRISTINE_LEVEL_105_RAW_SHA256);
    assert_eq!(rgb.len(), 0x300);
    assert_eq!(&tpl[..4], b"TPL\x02");
    assert_eq!(tpl.len(), 0x204);
    assert_eq!(raw, baseline_bytes);
    assert!(!directory.join("level.palmask").exists());

    let tpl_words = words(&tpl[4..]);
    for index in 0..256 {
        let expected = if index % 16 == 0 {
            baseline[256]
        } else {
            baseline[index]
        };
        assert_eq!(tpl_words[index], expected, "TPL color {index}");
        let [red, green, blue] = [rgb[index * 3], rgb[index * 3 + 1], rgb[index * 3 + 2]];
        assert_eq!(u16::from(red >> 3), expected & 0x1f, "PAL red {index}");
        assert_eq!(
            u16::from(green >> 3),
            expected >> 5 & 0x1f,
            "PAL green {index}"
        );
        assert_eq!(
            u16::from(blue >> 3),
            expected >> 10 & 0x1f,
            "PAL blue {index}"
        );
    }

    let import_path = directory.join("masked.tpl");
    let mut imported = tpl.clone();
    let changed_selected = (baseline[1] ^ 0x001f) & 0x7fff;
    let changed_unselected = (baseline[2] ^ 0x03e0) & 0x7fff;
    imported[6..8].copy_from_slice(&changed_selected.to_le_bytes());
    imported[8..10].copy_from_slice(&changed_unselected.to_le_bytes());
    fs::write(&import_path, imported).unwrap();
    let mut selector = vec![0_u8; 257];
    selector[1] = 1;
    fs::write(directory.join("masked.palmask"), &selector).unwrap();

    eprintln!("palette oracle: importing a selective mask");
    post_command(&prefix, &helper, executable_name, IMPORT_LEVEL_PALETTE);
    submit_file_dialog(&prefix, &helper, executable_name, &import_path);
    let (after_bytes, after_mask) = wait_for_mask(&prefix, &helper, executable_name, &selector);
    let after = words(&after_bytes);
    assert_eq!(after_mask, selector);
    assert_eq!(after[1], changed_selected);
    assert_eq!(after[2], baseline[2]);
    for index in 0..257 {
        if index != 1 {
            assert_eq!(after[index], baseline[index], "masked color {index}");
        }
    }

    let masked_export = directory.join("masked-export.tpl");
    eprintln!("palette oracle: exporting the retained selective mask");
    export_palette(&prefix, &helper, executable_name, &masked_export);
    assert_eq!(
        fs::read(directory.join("masked-export.palmask")).unwrap(),
        selector
    );

    let malformed_path = directory.join("malformed.tpl");
    fs::write(&malformed_path, b"TPL\x01invalid-version").unwrap();
    post_command(&prefix, &helper, executable_name, IMPORT_LEVEL_PALETTE);
    submit_file_dialog_and_acknowledge_message(
        &prefix,
        &helper,
        executable_name,
        &malformed_path,
        "not saved in SNES format",
    );
    let rejected_palette = read_bytes(&prefix, &helper, executable_name, WORKING_PALETTE).unwrap();
    let rejected_mask = read_bytes(&prefix, &helper, executable_name, TRANSFER_MASK).unwrap();
    assert_eq!(rejected_palette, after_bytes);
    assert_eq!(
        rejected_mask,
        vec![1; 257],
        "the original resets its transient selector before rejecting an invalid TPL"
    );

    post_command(&prefix, &helper, executable_name, SHOW_PALETTE_EDITOR);
    let values = successful(
        &prefix,
        "wine",
        &[
            helper.to_str().expect("helper path is UTF-8"),
            executable_name,
            "dialog-values",
        ],
    );
    let values = String::from_utf8(values.stdout).unwrap();
    assert!(
        values.contains("button=0x0068 check=1"),
        "palette import did not auto-enable the per-level custom palette:\n{values}"
    );

    eprintln!("palette oracle: stopping isolated Wine");
    stop_isolated_wine(&prefix, &mut child);
    fs::remove_dir_all(directory).unwrap();
}
