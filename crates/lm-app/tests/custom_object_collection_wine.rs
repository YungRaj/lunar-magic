use lm_level::{
    CustomObjectEntry, CustomObjectLibrary, CustomSpriteLibrary, MwlFile, ObjectRecord,
    SpriteLengthTable, SpriteToken, SscSidecar,
};
use lm_project::MwlNativeLevel;
use lm_rom::RomImage;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LUNAR_MAGIC_363_SHA256: &str =
    "b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff";
const PRISTINE_HEADERED_SMW_US_SHA256: &str =
    "5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf";
const DESCRIPTION: &str = "Rust multi-object placement oracle";
const RETAINED_PREVIEW_SHA256: &str =
    "cd248183f65b1efbd6eea42714ee0464e23c08e1310f5d17ed00e5da48a9adb5";
const RETAINED_SPRITE_PREVIEW_SHA256: &str =
    "e6a042be9e2192cc0d0b7413f299ab9857c33a96a6acf17da25d4da490fad3a6";

struct OracleFixtureCleanup {
    directory: PathBuf,
    rom: PathBuf,
    keep: bool,
}

impl Drop for OracleFixtureCleanup {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        let _ = fs::remove_dir_all(&self.directory);
        let _ = fs::remove_file(self.rom.with_extension("mw0"));
        let _ = fs::remove_file(self.rom.with_extension("mw0t"));
        let _ = fs::remove_file(self.rom.with_extension("mw2"));
        let _ = fs::remove_file(self.rom.with_extension("mwt"));
        let _ = fs::remove_file(self.rom.with_extension("ssc"));
        let _ = fs::remove_file(&self.rom);
    }
}

fn run(prefix: &Path, program: &str, arguments: &[&str]) -> std::process::Output {
    let mut command = Command::new(program);
    command.env("WINEDEBUG", "-all").args(arguments);
    if !prefix.as_os_str().is_empty() {
        command.env("WINEPREFIX", prefix);
    }
    command
        .output()
        .unwrap_or_else(|error| panic!("cannot run {program}: {error}"))
}

fn successful(prefix: &Path, program: &str, arguments: &[&str]) -> std::process::Output {
    let output = run(prefix, program, arguments);
    assert!(
        output.status.success(),
        "{program} {arguments:?} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn wine_path(prefix: &Path, path: &Path) -> String {
    let host = path.to_str().expect("oracle paths are UTF-8");
    String::from_utf8(successful(prefix, "winepath", &["-w", host]).stdout)
        .expect("winepath output is UTF-8")
        .trim()
        .to_owned()
}

fn compile_helper(root: &Path, source: &str, output: &Path, libraries: &[&str]) {
    let mut command = Command::new("i686-w64-mingw32-gcc");
    command.args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"]);
    if source == "tools/wine-lm-open-rom.c" {
        command.arg("-municode");
    }
    command.arg(root.join(source)).arg("-o").arg(output);
    for library in libraries {
        command.arg(library);
    }
    let output = command.output().expect("cannot launch MinGW compiler");
    assert!(
        output.status.success(),
        "{source} compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn compile_macos_capture_helper(root: &Path, output: &Path) {
    let result = Command::new("xcrun")
        .args(["swiftc"])
        .arg(root.join("tools/capture-macos-window.swift"))
        .arg("-o")
        .arg(output)
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
        .expect("cannot launch Swift compiler");
    assert!(
        result.status.success(),
        "macOS capture-helper compilation failed:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn run_lunar_magic(executable: &Path, operation: &str, rom: &Path, mwl: &Path) {
    let executable = executable.to_str().unwrap();
    let rom = format!(
        r"Z:\{}",
        rom.display()
            .to_string()
            .trim_start_matches('/')
            .replace('/', r"\")
    );
    let mwl = format!(
        r"Z:\{}",
        mwl.display()
            .to_string()
            .trim_start_matches('/')
            .replace('/', r"\")
    );
    let output = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .args([executable, operation, &rom, &mwl, "105"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Lunar Magic {operation} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
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

fn wait_for_lunar_magic_exit(prefix: &Path) {
    for _ in 0..40 {
        let processes = run(prefix, "wine", &["tasklist"]);
        if processes.status.success()
            && !String::from_utf8_lossy(&processes.stdout).contains("Lunar Magic.exe")
        {
            return;
        }
        thread::sleep(Duration::from_millis(250));
    }
    panic!("Lunar Magic did not close after the saved oracle transaction");
}

fn added_object_records(before: &MwlNativeLevel, after: &MwlNativeLevel) -> Vec<Vec<u8>> {
    let mut remaining = before.layer1.objects.records.iter().fold(
        BTreeMap::<Vec<u8>, usize>::new(),
        |mut counts, record| {
            *counts.entry(record.encoded().to_vec()).or_default() += 1;
            counts
        },
    );
    after
        .layer1
        .objects
        .records
        .iter()
        .filter_map(|record| {
            let encoded = record.encoded().to_vec();
            match remaining.get_mut(&encoded) {
                Some(count) if *count > 0 => {
                    *count -= 1;
                    None
                }
                _ => Some(encoded),
            }
        })
        .collect()
}

fn added_sprite_records(before: &MwlNativeLevel, after: &MwlNativeLevel) -> Vec<Vec<u8>> {
    fn records(level: &MwlNativeLevel) -> impl Iterator<Item = &lm_level::SpriteRecord> {
        level.sprites.tokens.iter().filter_map(|token| match token {
            SpriteToken::Record(record) => Some(record),
            SpriteToken::Screen(_) | SpriteToken::Control(_) => None,
        })
    }
    let mut remaining =
        records(before).fold(BTreeMap::<Vec<u8>, usize>::new(), |mut counts, record| {
            *counts.entry(record.encoded.clone()).or_default() += 1;
            counts
        });
    records(after)
        .filter_map(|record| match remaining.get_mut(&record.encoded) {
            Some(count) if *count > 0 => {
                *count -= 1;
                None
            }
            _ => Some(record.encoded.clone()),
        })
        .collect()
}

fn assert_sprite_preview_is_rendered(path: &Path) {
    let bytes = fs::read(path).unwrap();
    assert!(bytes.len() >= 54, "preview BMP is truncated");
    assert_eq!(&bytes[..2], b"BM");
    let pixel_offset = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let width = i32::from_le_bytes(bytes[18..22].try_into().unwrap());
    let height = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
    let bits = u16::from_le_bytes(bytes[28..30].try_into().unwrap());
    assert!(width > 0 && height != 0 && width <= 4096 && height.unsigned_abs() <= 4096);
    assert!(
        bits == 24 || bits == 32,
        "unexpected preview BMP depth {bits}"
    );
    let pixels = bytes.get(pixel_offset..).expect("missing preview pixels");
    let bytes_per_pixel = usize::from(bits / 8);
    let row_bytes = (width as usize * bytes_per_pixel + 3) & !3;
    let required = row_bytes * height.unsigned_abs() as usize;
    assert!(pixels.len() >= required, "preview BMP pixels are truncated");
    let first = &pixels[..bytes_per_pixel];
    let mut different = 0;
    let mut custom_pixels = 0;
    let mut custom_columns = vec![false; width as usize];
    for (y, row) in pixels[..required].chunks_exact(row_bytes).enumerate() {
        for (x, pixel) in row[..width as usize * bytes_per_pixel]
            .chunks_exact(bytes_per_pixel)
            .enumerate()
        {
            different += usize::from(pixel != first);
            if y >= height.unsigned_abs() as usize / 3
                && y <= height.unsigned_abs() as usize / 2
                && x >= width as usize / 4
                && x <= width as usize * 3 / 4
                && pixel[0] > 140
                && pixel[1] < 150
                && pixel[2] < 150
            {
                custom_pixels += 1;
                custom_columns[x] = true;
            }
        }
    }
    assert!(
        different >= 16 * 16,
        "Lunar Magic's custom-sprite preview did not render sprite artwork"
    );
    let occupied = custom_columns
        .iter()
        .enumerate()
        .filter_map(|(x, occupied)| occupied.then_some(x))
        .collect::<Vec<_>>();
    assert!(
        custom_pixels >= 40
            && occupied
                .first()
                .zip(occupied.last())
                .is_some_and(|(first, last)| last - first >= 64),
        "Lunar Magic's preview did not contain the complete SSC-defined two-sprite artwork"
    );
}

fn assert_preview_is_rendered(path: &Path) {
    let bytes = fs::read(path).unwrap();
    assert!(bytes.len() >= 54, "preview BMP is truncated");
    assert_eq!(&bytes[..2], b"BM");
    let pixel_offset = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let width = i32::from_le_bytes(bytes[18..22].try_into().unwrap());
    let height = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
    let bits = u16::from_le_bytes(bytes[28..30].try_into().unwrap());
    assert!(width > 0 && height != 0 && width <= 4096 && height.unsigned_abs() <= 4096);
    assert!(
        bits == 24 || bits == 32,
        "unexpected preview BMP depth {bits}"
    );
    let pixels = bytes.get(pixel_offset..).expect("missing preview pixels");
    let bytes_per_pixel = usize::from(bits / 8);
    let row_bytes = (width as usize * bytes_per_pixel + 3) & !3;
    let required = row_bytes * height.unsigned_abs() as usize;
    assert!(pixels.len() >= required, "preview BMP pixels are truncated");
    let first = &pixels[..bytes_per_pixel];
    let mut different = 0;
    let mut gold_columns = vec![0_usize; width as usize];
    for (y, row) in pixels[..required].chunks_exact(row_bytes).enumerate() {
        for (x, pixel) in row[..width as usize * bytes_per_pixel]
            .chunks_exact(bytes_per_pixel)
            .enumerate()
        {
            different += usize::from(pixel != first);
            if y >= height.unsigned_abs() as usize / 2
                && pixel[2] > 140
                && pixel[1] > 70
                && pixel[0] < 100
            {
                gold_columns[x] += 1;
            }
        }
    }
    assert!(
        different >= 16 * 16,
        "Lunar Magic's custom-object preview did not render object artwork"
    );
    let occupied_columns = gold_columns
        .iter()
        .enumerate()
        .filter_map(|(x, count)| (*count >= 2).then_some(x))
        .collect::<Vec<_>>();
    let mut substantial_groups = 0;
    let mut group_start = None;
    let mut previous = 0;
    for x in occupied_columns.into_iter().chain([usize::MAX]) {
        if let Some(start) = group_start
            && (x == usize::MAX || x - previous > 8)
        {
            substantial_groups += usize::from(previous - start + 1 >= 20);
            group_start = None;
        }
        if x != usize::MAX {
            group_start.get_or_insert(x);
            previous = x;
        }
    }
    assert!(
        substantial_groups >= 2,
        "Lunar Magic's preview did not contain both separated custom-object artworks"
    );
}

#[test]
fn retained_custom_object_preview_is_hash_and_structure_bound() {
    let root = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    let bytes = fs::read(
        root.join("docs/oracle-work/lm363/pristine-us/custom-object-collection/preview.png"),
    )
    .unwrap();
    assert_eq!(lm_oracle::sha256_hex(&bytes), RETAINED_PREVIEW_SHA256);
    assert_eq!(bytes.get(..8), Some(b"\x89PNG\r\n\x1a\n".as_slice()));
    assert_eq!(u32::from_be_bytes(bytes[8..12].try_into().unwrap()), 13);
    assert_eq!(&bytes[12..16], b"IHDR");
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 520);
    assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 520);
    assert_eq!(&bytes[24..29], &[8, 6, 0, 0, 0]);
}

#[test]
fn retained_custom_sprite_preview_is_hash_and_structure_bound() {
    let root = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    let bytes = fs::read(
        root.join("docs/oracle-work/lm363/pristine-us/custom-sprite-collection/preview.png"),
    )
    .unwrap();
    assert_eq!(
        lm_oracle::sha256_hex(&bytes),
        RETAINED_SPRITE_PREVIEW_SHA256
    );
    assert_eq!(bytes.get(..8), Some(b"\x89PNG\r\n\x1a\n".as_slice()));
    assert_eq!(u32::from_be_bytes(bytes[8..12].try_into().unwrap()), 13);
    assert_eq!(&bytes[12..16], b"IHDR");
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 520);
    assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 520);
    assert_eq!(&bytes[24..29], &[8, 6, 0, 0, 0]);
}

#[test]
#[ignore = "requires Wine, MinGW, local Lunar Magic 3.63, and the verified pristine SMW-US ROM"]
fn lunar_magic_hides_a_custom_description_without_its_final_newline() {
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
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = root.join("target/custom-object-wine-oracle").join(format!(
        "lm-custom-object-wine-rejection-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let prefix = PathBuf::new();
    let running = successful(&prefix, "wine", &["tasklist"]);
    assert!(!String::from_utf8_lossy(&running.stdout).contains("Lunar Magic.exe"));

    let rom = pristine.parent().unwrap().join(format!(
        "custom-object-rejection-{}-{nonce}.smc",
        std::process::id()
    ));
    let _cleanup = OracleFixtureCleanup {
        directory: directory.clone(),
        rom: rom.clone(),
        keep: std::env::var_os("LM_KEEP_CUSTOM_OBJECT_ORACLE").is_some(),
    };
    fs::copy(&pristine, &rom).unwrap();
    let data = [0, 0, 0, 0, 0, 1, 0, 4, 0xff];
    let decoded = CustomObjectLibrary::decode(&data, b"incomplete").unwrap();
    assert_eq!(decoded.entries().len(), 1);
    assert!(decoded.lunar_magic_picker_entries().is_empty());
    fs::write(rom.with_extension("mw0"), data).unwrap();
    fs::write(rom.with_extension("mw0t"), b"incomplete").unwrap();

    let oracle_helper = directory.join("wine-custom-object-oracle.exe");
    compile_helper(
        &root,
        "tools/wine-custom-object-oracle.c",
        &oracle_helper,
        &["-luser32"],
    );
    let startup_rom = wine_path(&prefix, &rom);
    let mut launcher = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .current_dir(&root)
        .arg(&lunar_magic)
        .arg(&startup_rom)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_secs(5));
    successful(
        &prefix,
        "wine",
        &[
            oracle_helper.to_str().unwrap(),
            "Lunar Magic.exe",
            "--expect-empty",
            "0",
            "0",
            "unused",
            "unused",
        ],
    );
    wait_for_lunar_magic_exit(&prefix);
    let _ = launcher.wait();
    assert_eq!(fs::read(&rom).unwrap(), pristine_bytes);
}

/// Loads a Rust-authored two-object `.mw0`/`.mw0t` collection in Lunar Magic 3.63, binds its
/// description to the live picker entry, retains its rendered preview, pastes the complete group,
/// saves the ROM, and proves both objects survive Lunar Magic's own MWL exporter.
#[test]
#[ignore = "requires Wine, MinGW, local Lunar Magic 3.63, and the verified pristine SMW-US ROM"]
fn rust_multi_object_collection_reloads_renders_and_places_in_lunar_magic() {
    let root = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
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
    let directory = root.join("target/custom-object-wine-oracle").join(format!(
        "lm-custom-object-wine-oracle-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let prefix = PathBuf::new();
    let running = successful(&prefix, "wine", &["tasklist"]);
    assert!(
        !String::from_utf8_lossy(&running.stdout).contains("Lunar Magic.exe"),
        "the custom-object oracle requires no pre-existing default-prefix Lunar Magic process"
    );

    let executable = lunar_magic.clone();
    let fixture_stem = format!("custom-object-oracle-{}-{nonce}", std::process::id());
    let rom_name = format!("{fixture_stem}.smc");
    let rom = pristine.parent().unwrap().join(&rom_name);
    let _cleanup = OracleFixtureCleanup {
        directory: directory.clone(),
        rom: rom.clone(),
        keep: std::env::var_os("LM_KEEP_CUSTOM_OBJECT_ORACLE").is_some(),
    };
    let before_mwl = directory.join("before.mwl");
    let after_mwl = directory.join("after.mwl");
    let preview_png = directory.join("preview.png");
    let preview = directory.join("preview.bmp");
    let preview_ready = directory.join("preview-ready.txt");
    let preview_continue = directory.join("preview-continue");
    fs::copy(&pristine, &rom).unwrap();

    let objects = vec![
        ObjectRecord::new(vec![0x01, 0x00, 0x03]).unwrap(),
        ObjectRecord::new(vec![0x01, 0x00, 0x04]).unwrap(),
        ObjectRecord::new(vec![0x02, 0x08, 0x04]).unwrap(),
    ];
    assert!(objects[0].screen_jump().is_some());
    assert!(objects[1..].iter().all(ObjectRecord::is_positioned_object));
    let expected_placed = [vec![0x06, 0x06, 0x10], vec![0x07, 0x0e, 0x10]];
    let mut library = CustomObjectLibrary::default();
    library
        .push(CustomObjectEntry::new_group(objects, DESCRIPTION.to_owned()).unwrap())
        .unwrap();
    let (data, descriptions) = library.encode().unwrap();
    fs::write(rom.with_extension("mw0"), data).unwrap();
    fs::write(rom.with_extension("mw0t"), descriptions).unwrap();
    let oracle_helper = directory.join("wine-custom-object-oracle.exe");
    compile_helper(
        &root,
        "tools/wine-custom-object-oracle.c",
        &oracle_helper,
        &["-luser32"],
    );
    let capture_helper = directory.join("capture-macos-window");
    compile_macos_capture_helper(&root, &capture_helper);

    let startup_rom = wine_path(&prefix, &rom);
    let mut launcher = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .current_dir(&root)
        .arg(&executable)
        .arg(&startup_rom)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_secs(5));
    let ready_wine = wine_path(&prefix, &preview_ready);
    let continue_wine = wine_path(&prefix, &preview_continue);
    let mut oracle = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .args([
            oracle_helper.to_str().unwrap(),
            "Lunar Magic.exe",
            DESCRIPTION,
            "96",
            "96",
            &ready_wine,
            &continue_wine,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    for _ in 0..1600 {
        if preview_ready.is_file() {
            break;
        }
        if let Some(status) = oracle.try_wait().unwrap() {
            let output = oracle.wait_with_output().unwrap();
            panic!(
                "custom-object oracle exited before preview capture: {status}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
    if !preview_ready.is_file() {
        let output = oracle.wait_with_output().unwrap();
        panic!(
            "oracle did not publish preview bounds: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let rectangle = fs::read_to_string(&preview_ready).unwrap();
    let values = rectangle
        .split_whitespace()
        .map(|value| value.parse::<i32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 4);
    assert!(values[0] >= 0 && values[1] >= 0 && values[2] > 0 && values[3] > 0);
    let coordinates = values.iter().map(i32::to_string).collect::<Vec<_>>();
    let capture = Command::new(&capture_helper)
        .args(["wine", "Add Objects Window"])
        .args(&coordinates)
        .arg(&preview_png)
        .output()
        .expect("cannot launch window-specific compositor capture");
    assert!(
        capture.status.success(),
        "screencapture failed: {}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let conversion = Command::new("sips")
        .args(["-s", "format", "bmp"])
        .arg(&preview_png)
        .arg("--out")
        .arg(&preview)
        .output()
        .expect("cannot launch preview conversion");
    assert!(
        conversion.status.success(),
        "sips failed: {}",
        String::from_utf8_lossy(&conversion.stderr)
    );
    assert_preview_is_rendered(&preview);
    fs::write(&preview_continue, b"captured\n").unwrap();
    let oracle_output = oracle.wait_with_output().unwrap();
    assert!(
        oracle_output.status.success(),
        "custom-object oracle failed with {}\nstdout:\n{}\nstderr:\n{}",
        oracle_output.status,
        String::from_utf8_lossy(&oracle_output.stdout),
        String::from_utf8_lossy(&oracle_output.stderr)
    );
    wait_for_lunar_magic_exit(&prefix);
    let _ = launcher.wait();

    run_lunar_magic(&executable, "-ExportLevel", &pristine, &before_mwl);
    run_lunar_magic(&executable, "-ExportLevel", &rom, &after_mwl);
    let before = decode_level(&before_mwl);
    let after = decode_level(&after_mwl);
    assert_eq!(
        after.layer1.objects.records.len(),
        before.layer1.objects.records.len() + 2,
        "Lunar Magic did not paste the complete collection group"
    );
    let added = added_object_records(&before, &after);
    assert_eq!(
        added, expected_placed,
        "Lunar Magic did not apply its recovered custom-selector and coordinate transformation",
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
fn lunar_magic_hides_a_custom_sprite_description_without_its_final_newline() {
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
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = root.join("target/custom-sprite-wine-oracle").join(format!(
        "lm-custom-sprite-wine-rejection-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let prefix = PathBuf::new();
    let running = successful(&prefix, "wine", &["tasklist"]);
    assert!(!String::from_utf8_lossy(&running.stdout).contains("Lunar Magic.exe"));

    let rom = pristine.parent().unwrap().join(format!(
        "custom-sprite-rejection-{}-{nonce}.smc",
        std::process::id()
    ));
    let _cleanup = OracleFixtureCleanup {
        directory: directory.clone(),
        rom: rom.clone(),
        keep: std::env::var_os("LM_KEEP_CUSTOM_SPRITE_ORACLE").is_some(),
    };
    fs::copy(&pristine, &rom).unwrap();
    let data = [0, 1, 0x20, 0, 0, 0x30, 0x0f, 0xff];
    let decoded =
        CustomSpriteLibrary::decode(&data, b"incomplete", &SpriteLengthTable::standard()).unwrap();
    assert_eq!(decoded.entries().len(), 1);
    assert!(decoded.lunar_magic_picker_entries().is_empty());
    fs::write(rom.with_extension("mw2"), data).unwrap();
    fs::write(rom.with_extension("mwt"), b"incomplete").unwrap();

    let oracle_helper = directory.join("wine-custom-sprite-oracle.exe");
    compile_helper(
        &root,
        "tools/wine-custom-sprite-oracle.c",
        &oracle_helper,
        &["-luser32"],
    );
    let startup_rom = wine_path(&prefix, &rom);
    let mut launcher = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .current_dir(&root)
        .arg(&lunar_magic)
        .arg(&startup_rom)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_secs(5));
    successful(
        &prefix,
        "wine",
        &[
            oracle_helper.to_str().unwrap(),
            "Lunar Magic.exe",
            "--expect-empty",
            "0",
            "0",
            "unused",
            "unused",
        ],
    );
    wait_for_lunar_magic_exit(&prefix);
    let _ = launcher.wait();
    assert_eq!(fs::read(&rom).unwrap(), pristine_bytes);
}

/// Loads a Rust-authored two-sprite `.mw2`/`.mwt` placement in Lunar Magic 3.63, binds its
/// description to the live custom picker, captures its rendered preview, places the complete
/// group, saves, and proves both sprites survive Lunar Magic's own MWL exporter.
#[test]
#[ignore = "requires Wine, MinGW, local Lunar Magic 3.63, and the verified pristine SMW-US ROM"]
fn rust_multi_sprite_collection_reloads_renders_and_places_in_lunar_magic() {
    const SPRITE_DESCRIPTION: &str = "Rust multi-sprite placement oracle";
    let root = fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
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
    let directory = root.join("target/custom-sprite-wine-oracle").join(format!(
        "lm-custom-sprite-wine-oracle-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let prefix = PathBuf::new();
    let running = successful(&prefix, "wine", &["tasklist"]);
    assert!(
        !String::from_utf8_lossy(&running.stdout).contains("Lunar Magic.exe"),
        "the custom-sprite oracle requires no pre-existing default-prefix Lunar Magic process"
    );

    let fixture_stem = format!("custom-sprite-oracle-{}-{nonce}", std::process::id());
    let rom = pristine
        .parent()
        .unwrap()
        .join(format!("{fixture_stem}.smc"));
    let _cleanup = OracleFixtureCleanup {
        directory: directory.clone(),
        rom: rom.clone(),
        keep: std::env::var_os("LM_KEEP_CUSTOM_SPRITE_ORACLE").is_some(),
    };
    let before_mwl = directory.join("before.mwl");
    let after_mwl = directory.join("after.mwl");
    let preview_png = directory.join("preview.png");
    let preview_bmp = directory.join("preview.bmp");
    let preview_ready = directory.join("preview-ready.txt");
    let preview_continue = directory.join("preview-continue");
    fs::copy(&pristine, &rom).unwrap();

    // Green Koopa and Goomba, separated by one 16-pixel X cell in a single placement group.
    let data = [0, 1, 0x20, 0, 0, 0x30, 0x0f, 0xff];
    let descriptions = format!("{SPRITE_DESCRIPTION}\n").into_bytes();
    let display_metadata = b"00\t2\t-8,0,3c10;0,0,3c11\n0f\t2\t8,0,3c20;16,0,3c21\n";
    let decoded =
        CustomSpriteLibrary::decode(&data, &descriptions, &SpriteLengthTable::standard()).unwrap();
    assert_eq!(decoded.entries().len(), 1);
    assert_eq!(decoded.entries()[0].sprites.len(), 2);
    assert_eq!(
        decoded
            .encode_checked(&SpriteLengthTable::standard())
            .unwrap()
            .0,
        data
    );
    let ssc = SscSidecar::decode(display_metadata).unwrap();
    assert_eq!(ssc.entries().len(), 2);
    assert_eq!(ssc.encode_lossless(), display_metadata);
    fs::write(rom.with_extension("mw2"), data).unwrap();
    fs::write(rom.with_extension("mwt"), descriptions).unwrap();
    fs::write(rom.with_extension("ssc"), display_metadata).unwrap();

    let oracle_helper = directory.join("wine-custom-sprite-oracle.exe");
    compile_helper(
        &root,
        "tools/wine-custom-sprite-oracle.c",
        &oracle_helper,
        &["-luser32"],
    );
    let capture_helper = directory.join("capture-macos-window");
    compile_macos_capture_helper(&root, &capture_helper);

    let startup_rom = wine_path(&prefix, &rom);
    let mut launcher = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .current_dir(&root)
        .arg(&lunar_magic)
        .arg(&startup_rom)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_secs(5));
    let ready_wine = wine_path(&prefix, &preview_ready);
    let continue_wine = wine_path(&prefix, &preview_continue);
    let mut oracle = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .args([
            oracle_helper.to_str().unwrap(),
            "Lunar Magic.exe",
            SPRITE_DESCRIPTION,
            "96",
            "96",
            &ready_wine,
            &continue_wine,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    for _ in 0..1600 {
        if preview_ready.is_file() {
            break;
        }
        if let Some(status) = oracle.try_wait().unwrap() {
            let output = oracle.wait_with_output().unwrap();
            panic!(
                "custom-sprite oracle exited before preview capture: {status}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        preview_ready.is_file(),
        "oracle did not publish preview bounds"
    );
    let values = fs::read_to_string(&preview_ready)
        .unwrap()
        .split_whitespace()
        .map(|value| value.parse::<i32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 4);
    assert!(values[0] >= 0 && values[1] >= 0 && values[2] > 0 && values[3] > 0);
    let coordinates = values.iter().map(i32::to_string).collect::<Vec<_>>();
    let capture = Command::new(&capture_helper)
        .args(["wine", "Add Sprites Window"])
        .args(&coordinates)
        .arg(&preview_png)
        .output()
        .expect("cannot launch window-specific compositor capture");
    assert!(
        capture.status.success(),
        "screencapture failed: {}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let conversion = Command::new("sips")
        .args(["-s", "format", "bmp"])
        .arg(&preview_png)
        .arg("--out")
        .arg(&preview_bmp)
        .output()
        .expect("cannot launch preview conversion");
    assert!(
        conversion.status.success(),
        "sips failed: {}",
        String::from_utf8_lossy(&conversion.stderr)
    );
    assert_sprite_preview_is_rendered(&preview_bmp);
    fs::write(&preview_continue, b"captured\n").unwrap();
    let oracle_output = oracle.wait_with_output().unwrap();
    assert!(
        oracle_output.status.success(),
        "custom-sprite oracle failed with {}\nstdout:\n{}\nstderr:\n{}",
        oracle_output.status,
        String::from_utf8_lossy(&oracle_output.stdout),
        String::from_utf8_lossy(&oracle_output.stderr)
    );
    wait_for_lunar_magic_exit(&prefix);
    let _ = launcher.wait();

    run_lunar_magic(&lunar_magic, "-ExportLevel", &pristine, &before_mwl);
    run_lunar_magic(&lunar_magic, "-ExportLevel", &rom, &after_mwl);
    let before = decode_level(&before_mwl);
    let after = decode_level(&after_mwl);
    let before_sprite_count = before
        .sprites
        .tokens
        .iter()
        .filter(|token| matches!(token, SpriteToken::Record(_)))
        .count();
    let after_sprite_count = after
        .sprites
        .tokens
        .iter()
        .filter(|token| matches!(token, SpriteToken::Record(_)))
        .count();
    assert_eq!(
        after_sprite_count,
        before_sprite_count + 2,
        "Lunar Magic did not paste the complete custom-sprite group"
    );
    let added = added_sprite_records(&before, &after);
    assert_eq!(
        added,
        [vec![0x60, 0x60, 0x00], vec![0x60, 0x70, 0x0f]],
        "Lunar Magic did not apply its custom-sprite group coordinate transformation"
    );
    assert_eq!(after.header, before.header);
    assert_eq!(after.layer1, before.layer1);
    assert_eq!(after.layer2, before.layer2);
    assert_eq!(after.palette, before.palette);
    assert_eq!(after.secondary_exits, before.secondary_exits);
    assert_eq!(after.exanimation, before.exanimation);
    assert_eq!(after.expanded_settings, before.expanded_settings);
    let image = RomImage::from_bytes(fs::read(&rom).unwrap()).unwrap();
    assert!(lm_rom::detect_identity(&image).unwrap().checksum_matches());
}
