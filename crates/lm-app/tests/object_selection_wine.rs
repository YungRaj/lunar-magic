use lm_level::{MwlFile, ObjectRecord, SpriteLengthTable};
use lm_project::MwlNativeLevel;
use lm_rom::RomImage;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LUNAR_MAGIC_363_SHA256: &str =
    "b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff";
const PRISTINE_HEADERED_SMW_US_SHA256: &str =
    "5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf";

struct Cleanup {
    directory: PathBuf,
    rom: PathBuf,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if std::env::var_os("LM_KEEP_OBJECT_SELECTION_ORACLE").is_none() {
            let _ = fs::remove_dir_all(&self.directory);
            let _ = fs::remove_file(&self.rom);
        }
    }
}

fn wine_path(path: &Path) -> String {
    let output = Command::new("winepath")
        .args(["-w", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn export_level(executable: &Path, rom: &Path, output: &Path) {
    let result = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(executable)
        .arg("-ExportLevel")
        .arg(wine_path(rom))
        .arg(wine_path(output))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "Lunar Magic export failed:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn decode_level(path: &Path) -> MwlNativeLevel {
    MwlNativeLevel::decode(
        &MwlFile::decode(&fs::read(path).unwrap()).unwrap(),
        &SpriteLengthTable::standard(),
        32,
        &[false; 256],
    )
    .unwrap()
}

fn launch_lunar_magic(root: &Path, executable: &Path, rom: &Path) -> std::process::Child {
    for _ in 0..3 {
        let mut candidate = Command::new("wine")
            .env("WINEDEBUG", "-all")
            .current_dir(root)
            .arg(executable)
            .arg(wine_path(rom))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        thread::sleep(Duration::from_secs(3));
        if candidate.try_wait().unwrap().is_none() {
            thread::sleep(Duration::from_secs(3));
            return candidate;
        }
        thread::sleep(Duration::from_secs(1));
    }
    panic!("Lunar Magic failed to remain open after three attempts");
}

fn positioned(records: &[ObjectRecord]) -> Vec<Vec<u8>> {
    records
        .iter()
        .filter(|record| record.is_positioned_object())
        .map(|record| record.encoded().to_vec())
        .collect()
}

fn controls(records: &[ObjectRecord]) -> Vec<Vec<u8>> {
    records
        .iter()
        // Screen jumps are structural cursor records. Lunar Magic correctly excludes them from
        // selection, then drops now-redundant jumps while rebuilding an empty positioned stream.
        .filter(|record| !record.is_positioned_object() && record.screen_jump().is_none())
        .map(|record| record.encoded().to_vec())
        .collect()
}

#[test]
#[ignore = "requires Wine, MinGW, local Lunar Magic 3.63, and the verified pristine SMW-US ROM"]
fn lunar_magic_select_all_deletes_every_positioned_object_and_preserves_controls() {
    let root = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    let executable = root.join("lm363/Lunar Magic.exe");
    let pristine = root.join("sysLMRestore/smwOrig.smc");
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&executable).unwrap()),
        LUNAR_MAGIC_363_SHA256
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&pristine).unwrap()),
        PRISTINE_HEADERED_SMW_US_SHA256
    );
    let tasklist = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg("tasklist")
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&tasklist.stdout).contains("Lunar Magic.exe"),
        "object-selection oracle requires no pre-existing Lunar Magic process"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = root
        .join("target/object-selection-wine-oracle")
        .join(format!(
            "lm-object-selection-{}-{nonce}",
            std::process::id()
        ));
    fs::create_dir_all(&directory).unwrap();
    let rom = pristine
        .parent()
        .unwrap()
        .join(format!("object-select-{}-{nonce}.smc", std::process::id()));
    let _cleanup = Cleanup {
        directory: directory.clone(),
        rom: rom.clone(),
    };
    let before_mwl = directory.join("before.mwl");
    let after_mwl = directory.join("after.mwl");
    let helper = directory.join("wine-object-selection-oracle.exe");
    fs::copy(&pristine, &rom).unwrap();
    let compile = Command::new("i686-w64-mingw32-gcc")
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(root.join("tools/wine-object-selection-oracle.c"))
        .arg("-o")
        .arg(&helper)
        .arg("-luser32")
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "oracle helper compilation failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    export_level(&executable, &rom, &before_mwl);

    let mut launcher = launch_lunar_magic(&root, &executable, &rom);
    let oracle = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&helper)
        .args(["Lunar Magic.exe", "delete"])
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "object-selection oracle failed with {}:\nstdout:\n{}\nstderr:\n{}",
        oracle.status,
        String::from_utf8_lossy(&oracle.stdout),
        String::from_utf8_lossy(&oracle.stderr)
    );
    assert!(String::from_utf8_lossy(&oracle.stdout).contains("gesture=ctrl-a,delete"));
    let _ = launcher.wait();
    export_level(&executable, &rom, &after_mwl);

    let before = decode_level(&before_mwl);
    let after = decode_level(&after_mwl);
    assert!(!positioned(&before.layer1.objects.records).is_empty());
    assert!(positioned(&after.layer1.objects.records).is_empty());
    assert_eq!(
        controls(&after.layer1.objects.records),
        controls(&before.layer1.objects.records),
        "Select All must exclude opaque control records"
    );
    assert_eq!(after.header, before.header);
    assert_eq!(after.layer2, before.layer2);
    assert_eq!(after.sprites, before.sprites);
    assert_eq!(after.palette, before.palette);
    assert_eq!(after.secondary_exits, before.secondary_exits);
    assert_eq!(after.exanimation, before.exanimation);
    assert_eq!(after.expanded_settings, before.expanded_settings);
    let image = RomImage::from_bytes(fs::read(&rom).unwrap()).unwrap();
    assert!(lm_rom::detect_identity(&image).unwrap().checksum_matches());
}

#[test]
#[ignore = "requires Wine, MinGW, local Lunar Magic 3.63, and the verified pristine SMW-US ROM"]
fn lunar_magic_ctrl_selection_right_duplicates_and_drags_the_complete_object_group() {
    let root = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    let executable = root.join("lm363/Lunar Magic.exe");
    let pristine = root.join("sysLMRestore/smwOrig.smc");
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&executable).unwrap()),
        LUNAR_MAGIC_363_SHA256
    );
    assert_eq!(
        lm_oracle::sha256_hex(&fs::read(&pristine).unwrap()),
        PRISTINE_HEADERED_SMW_US_SHA256
    );
    let tasklist = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg("tasklist")
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&tasklist.stdout).contains("Lunar Magic.exe"),
        "object-selection oracle requires no pre-existing Lunar Magic process"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // Lunar Magic's legacy startup path buffer crashes on sufficiently long absolute ROM names.
    let directory = root
        .join("target/object-selection-wine-oracle")
        .join(format!("d-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let rom = pristine
        .parent()
        .unwrap()
        .join(format!("object-drag-{}-{nonce}.smc", std::process::id()));
    let _cleanup = Cleanup {
        directory: directory.clone(),
        rom: rom.clone(),
    };
    let before_mwl = directory.join("before.mwl");
    let after_mwl = directory.join("after.mwl");
    let helper = directory.join("wine-object-selection-oracle.exe");
    fs::copy(&pristine, &rom).unwrap();
    let compile = Command::new("i686-w64-mingw32-gcc")
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(root.join("tools/wine-object-selection-oracle.c"))
        .arg("-o")
        .arg(&helper)
        .arg("-luser32")
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "oracle helper compilation failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    export_level(&executable, &rom, &before_mwl);
    thread::sleep(Duration::from_secs(2));

    let mut launcher = launch_lunar_magic(&root, &executable, &rom);
    let oracle = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&helper)
        .args(["Lunar Magic.exe", "right-duplicate-drag"])
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "object-selection oracle failed with {}:\nstdout:\n{}\nstderr:\n{}",
        oracle.status,
        String::from_utf8_lossy(&oracle.stdout),
        String::from_utf8_lossy(&oracle.stderr)
    );
    let stdout = String::from_utf8_lossy(&oracle.stdout);
    assert!(stdout.contains("phase=selected count=2"));
    assert!(stdout.contains("gesture=ctrl-select,right-duplicate,drag"));
    let _ = launcher.wait();
    export_level(&executable, &rom, &after_mwl);

    let before = decode_level(&before_mwl);
    let after = decode_level(&after_mwl);
    let mut expected = before.layer1.objects.clone();
    assert_eq!(
        expected
            .duplicate_ordinary_object_group(&[1, 2], 1, -13)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        after.layer1.objects, expected,
        "Lunar Magic's complete selected-group clone/drag stream differs from Rust"
    );
    assert_eq!(after.header, before.header);
    assert_eq!(after.layer2, before.layer2);
    assert_eq!(after.sprites, before.sprites);
    assert_eq!(after.palette, before.palette);
    assert_eq!(after.secondary_exits, before.secondary_exits);
    assert_eq!(after.exanimation, before.exanimation);
    assert_eq!(after.expanded_settings, before.expanded_settings);
    let image = RomImage::from_bytes(fs::read(&rom).unwrap()).unwrap();
    assert!(lm_rom::detect_identity(&image).unwrap().checksum_matches());
}
